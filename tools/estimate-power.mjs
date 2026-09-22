#!/usr/bin/env node
// Time-based power estimate for the Codex Status device (Plan C task-3).
//
// No current meter required: the firmware reports cumulative deep-cycle totals
// (deep.acc_cycles / acc_awake_ms / acc_ble_ms / acc_render_ms in /status.json)
// and we multiply the per-cycle time split by datasheet currents. Numbers are
// estimates, not measurements: verify the estimate against a USB power meter or
// PPK2/Joulescope before drawing final conclusions.
//
// Usage:
//   node tools/estimate-power.mjs [ip] [--period 60]
//
// Currents (ESP32-S3 datasheet v2.2, see power-plan-c/plan.md "依据"):
//   deep sleep        7-8 uA
//   light sleep       240 uA (idle CPU active estimate is 20-40 mA at 80-240 MHz)
//   BLE RX            93 mA
//   BLE TX 0 dBm      176 mA (advertising duty ~2-5% => phase average ~100-120 mA)
//   render/CPU active charged at the CPU rate (the panel/SPI draw is small here)

const args = process.argv.slice(2);
const ip = args.find((a) => !a.startsWith('--')) || '192.168.3.163';
const periodArg = args.indexOf('--period');
const periodS = periodArg >= 0 ? Number(args[periodArg + 1]) : null;

const I = {
  deep_uA: 8,
  cpu_mA: { low: 20, base: 30, high: 40 },
  ble_mA: { low: 93, base: 120, high: 176 },
};

const url = `http://${ip}/status.json`;
let status;
try {
  status = await (await fetch(url, { signal: AbortSignal.timeout(10000) })).json();
} catch (e) {
  console.error(`cannot fetch ${url}: ${e.message} (device may be in deep sleep; wake it with a plan or button)`);
  process.exit(1);
}

const deep = status.deep || {};
const acc = {
  cycles: deep.acc_cycles || 0,
  awakeMs: deep.acc_awake_ms || 0,
  bleMs: deep.acc_ble_ms || 0,
  renderMs: deep.acc_render_ms || 0,
};

if (!acc.cycles) {
  console.error('deep.acc_cycles = 0: no completed deep cycle since the last RTC reset.');
  console.error(`fw=${status.fw} mode=${status.mode} last_wake=${deep.last_wake_result} dur=${deep.last_awake_ms}ms`);
  process.exit(2);
}

const period = periodS || status.power?.rendezvous_period_s || 60;
const cyclesPerDay = 86400 / period;

const awakeS = acc.awakeMs / acc.cycles / 1000;
const bleS = acc.bleMs / acc.cycles / 1000;
const renderS = acc.renderMs / acc.cycles / 1000;
const cpuS = Math.max(0, awakeS - bleS);
const awakeDuty = (awakeS * cyclesPerDay) / 86400;

function estimate(cpu, ble) {
  const cycleMAs = bleS * ble + cpuS * cpu;
  const dailyMAs = cyclesPerDay * cycleMAs;
  const deepMAs = I.deep_uA / 1000 * (86400 - cyclesPerDay * awakeS);
  return {
    cycleMAs,
    dailyMah: (dailyMAs + deepMAs) / 3600,
    awakeAvgMa: cycleMAs / (awakeS || 1),
  };
}

const lo = estimate(I.cpu_mA.low, I.ble_mA.low);
const base = estimate(I.cpu_mA.base, I.ble_mA.base);
const hi = estimate(I.cpu_mA.high, I.ble_mA.high);

const f = (v, d = 1) => Number(v).toFixed(d);
console.log(`device ${ip} fw=${status.fw} mode=${status.mode}`);
console.log(`deep cycles sampled: ${acc.cycles}  (period ${period}s, ${f(cyclesPerDay)} cycles/day)`);
console.log(`per cycle: awake ${f(awakeS, 2)}s = BLE ${f(bleS, 2)}s + CPU/render ${f(cpuS, 2)}s (render ${f(renderS, 2)}s)`);
console.log(`awake duty: ${f(awakeDuty * 100, 2)}%   avg current during awake: ~${f(lo.awakeAvgMa)}-${f(hi.awakeAvgMa)} mA`);
console.log(`estimate: ${f(lo.dailyMah, 1)} / ${f(base.dailyMah, 1)} / ${f(hi.dailyMah, 1)} mAh/day (low/base/high)`);
console.log(`  1000 mAh battery => ${f(1000 / base.dailyMah, 1)} days at the base estimate`);
if (status.light_sleep_ms !== undefined) {
  const duty = (status.light_sleep_ms / status.awake_ms) * 100;
  console.log(`current boot light-sleep share: ${f(duty, 1)}% (${status.light_sleep_ms} of ${status.awake_ms} ms awake)`);
}

// Per-wake-type durations from the device history ring (ev=7 HIST_WAKE):
// the acc_* totals mix thin clock wakes with rendezvous/net cycles, so the
// rendezvous-only average is the number to use for the Plan C budget.
const WAKE_NAMES = { 0: 'light', 1: 'thin', 2: 'rendezvous-sleep', 3: 'rendezvous-light', 4: 'net' };
try {
  const hist = await (await fetch(`http://${ip}/history`, { signal: AbortSignal.timeout(10000) })).json();
  const groups = new Map();
  for (const rec of hist) {
    if (rec.ev !== 7) continue;
    const name = WAKE_NAMES[rec.aux] || `aux${rec.aux}`;
    const g = groups.get(name) || { n: 0, ms: 0, renders: 0, src: new Map() };
    g.n++;
    g.ms += rec.dur_ms;
    g.renders += rec.aux === 1 ? 1 : 0; // thin wakes are single-render
    g.src.set(rec.src, (g.src.get(rec.src) || 0) + 1);
    groups.set(name, g);
  }
  console.log('per wake result (ev=7):');
  for (const [name, g] of groups) {
    const src = [...g.src.entries()].map(([s, n]) => `${['none', 'ble', 'rtc'][s] || s}:${n}`).join(' ');
    console.log(`  ${name.padEnd(17)} n=${g.n}  avg ${f(g.ms / g.n / 1000, 2)}s  (src ${src})`);
  }
} catch {
  // history is optional; the acc_* estimate above still stands
}

console.log('note: time x datasheet current, not a measurement; cross-check with a USB power meter or PPK2.');
