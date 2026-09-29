// Cooperative Fake ROM runner. Input is an ignored local JSON file containing
// loopback addresses and control tokens; output contains no credentials.
import { readFileSync, writeFileSync } from 'node:fs';

const configPath = process.argv[2];
if (!configPath) throw new Error('usage: node tools/fake-rom-runner.mjs <config.json>');
const config = JSON.parse(readFileSync(configPath, 'utf8'));
const untilMs = config.until_ms ?? 86_400_000;
const maxEvents = config.max_events ?? 2000;
const actions = [...(config.actions ?? [])].sort((a, b) => a.at_ms - b.at_ms);
if (!Array.isArray(config.devices) || !config.devices.length || maxEvents < 1 || untilMs < 0) {
  throw new Error('devices, until_ms, or max_events invalid');
}
if (actions.some(action => !Number.isSafeInteger(action.at_ms) || action.at_ms < 0 ||
    action.at_ms > untilMs || action.kind !== 'light' ||
    !config.devices.some(device => device.mac === action.mac))) {
  throw new Error('invalid runner action');
}

async function call(endpoint, method, path, body, timeoutMs = 20_000) {
  const response = await fetch(`${endpoint.url}${path}`, {
    method,
    headers: { authorization: `Bearer ${endpoint.token}`,
      ...(body === undefined ? {} : { 'content-type': 'application/json' }) },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(timeoutMs),
  });
  const text = await response.text();
  if (!response.ok) throw new Error(`${method} ${path} returned ${response.status}: ${text.slice(0, 200)}`);
  return JSON.parse(text);
}

const bridge = config.bridge;
const deviceState = device => call(device, 'GET', '/sim/state');
const deviceStep = (device, delta) => call(device, 'POST', '/sim/time',
  { op: 'step', delta_ms: delta });
const bridgeStep = (mac, delta) => call(bridge, 'POST', '/sim/clock',
  { mac, op: 'step', delta_ms: delta });
const bridgeRun = (mac, kind) => call(bridge, 'POST', '/sim/run', { mac, kind }, 120_000);

function deadline(state) {
  const power = state.power;
  if (power.mode === 'deep') return power.ble_window_until_ms ?? power.next_contact_ms;
  if (state.plan.accepted && state.plan.mode === 'light') {
    return power.boot_ms + state.plan.accepted_at_ms + state.plan.granted_s * 1000;
  }
  if (power.provisional) return power.boot_ms + 300_000;
  return power.safety_deadline_ms || null;
}

function eventTime(states, devices) {
  let soonest = null;
  for (let index = 0; index < states.length; index++) {
    const next = deadline(states[index]);
    const scale = devices[index].scale_ppm ?? 1_000_000;
    if (next === null || scale <= 0) continue;
    const remaining = Math.max(0, next - states[index].clock.monotonic_ms);
    const globalDelta = Math.ceil(remaining * 1_000_000 / scale);
    soonest = soonest === null ? globalDelta : Math.min(soonest, globalDelta);
  }
  return soonest;
}

const trace = [];
let coordinate = 0;
let exhausted = true;
let nextAction = 0;
const carry = config.devices.map(() => 0);
const bridgeCarry = config.devices.map(() => 0);
for (const device of config.devices) {
  await call(device, 'POST', '/sim/time', { op: 'rate', rate_ppm: 0 });
  await call(bridge, 'POST', '/sim/clock', { mac: device.mac, op: 'rate', rate_ppm: 0 });
}

for (let event = 0; event < maxEvents && coordinate <= untilMs; event++) {
  while (nextAction < actions.length && actions[nextAction].at_ms === coordinate) {
    const action = actions[nextAction++];
    const reply = await call(bridge, 'POST', '/mcp', { jsonrpc: '2.0', id: `action-${nextAction}`,
      method: 'tools/call', params: { name: 'power_plan',
        arguments: { mac: action.mac, mode: 'light' } } });
    if (reply.result?.isError) throw new Error(`light action failed at ${coordinate}ms: ` +
      JSON.stringify(reply.result.content));
  }
  const states = await Promise.all(config.devices.map(deviceState));
  for (let index = 0; index < states.length; index++) {
    const device = config.devices[index];
    const state = states[index];
    if (state.power.ble_window_until_ms !== null) {
      const run = await bridgeRun(device.mac, 'ble').catch(error => {
        throw new Error(`BLE ${device.mac} at ${coordinate}ms: ${error}`);
      });
      if (run.outcome?.contact !== true || run.outcome?.error ||
          run.outcome?.result?.Err !== undefined) {
        throw new Error(`BLE rendezvous failed for ${device.mac} at ${coordinate}ms: ` +
          JSON.stringify(run.outcome));
      }
    }
    const afterBle = await deviceState(device);
    if (afterBle.power.mode === 'light' || afterBle.power.sync_open === true) {
      await bridgeRun(device.mac, 'http');
      if (afterBle.power.sync_open === true) {
        const drained = await deviceState(device);
        if (drained.sync?.pending_batch !== null || drained.sync?.rounds !== 0 ||
            drained.power.sync_open !== false) {
          throw new Error(`sync drain incomplete for ${device.mac} at ${coordinate}ms: ` +
            JSON.stringify(drained.sync));
        }
      }
    }
  }
  const after = await Promise.all(config.devices.map(deviceState));
  trace.push({ t: coordinate, devices: after.map((state, index) => ({
    mac: config.devices[index].mac, boot: state.boot_id,
    mode: state.power.mode, wakes: state.power.wake_count,
    job: state.bundle.job_id, seq: state.bundle.applied_seq,
    frame_crc: state.bundle.frame_crc,
    pre_sync_rounds: states[index].sync?.rounds, pre_sync_due: states[index].sync?.due,
    sync_rounds: state.sync?.rounds, sync_due: state.sync?.due,
    sync_serial: state.sync?.last_completed?.client_serial,
  })) });
  let delta = eventTime(after, config.devices);
  if (nextAction < actions.length) {
    const actionDelta = actions[nextAction].at_ms - coordinate;
    delta = delta === null ? actionDelta : Math.min(delta, actionDelta);
  }
  if (delta === null || coordinate >= untilMs) {
    exhausted = false;
    break;
  }
  const advance = Math.min(Math.max(delta, 1), untilMs - coordinate);
  if (advance <= 0) {
    exhausted = false;
    break;
  }
  for (let index = 0; index < config.devices.length; index++) {
    const device = config.devices[index];
    const scaled = advance * (device.scale_ppm ?? 1_000_000) + carry[index];
    const local = Math.floor(scaled / 1_000_000);
    carry[index] = scaled % 1_000_000;
    if (local) await deviceStep(device, local);
  }
  for (let index = 0; index < config.devices.length; index++) {
    const device = config.devices[index];
    const scaled = advance * (device.bridge_scale_ppm ?? 1_000_000) + bridgeCarry[index];
    const local = Math.floor(scaled / 1_000_000);
    bridgeCarry[index] = scaled % 1_000_000;
    if (local) await bridgeStep(device.mac, local);
  }
  coordinate += advance;
}

if (exhausted && coordinate < untilMs) {
  throw new Error(`max_events exhausted at ${coordinate}ms before ${untilMs}ms`);
}

if (config.trace) writeFileSync(config.trace, trace.map(row => JSON.stringify(row)).join('\n') + '\n');
const final = trace.at(-1);
console.log(JSON.stringify({ events: trace.length, coordinate_ms: coordinate,
  wakes: final?.devices.map(device => ({ mac: device.mac, count: device.wakes })) }));
