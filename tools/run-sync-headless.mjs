// Isolated sync-v1 smoke/virtual-day runner. No desktop automation or real MACs.
import { spawn, execFileSync } from 'node:child_process';
import { createWriteStream, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { createHash } from 'node:crypto';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(fileURLToPath(new URL('..', import.meta.url)));
const root = mkdtempSync(join(repo, 'artifacts/rollout-20260928/sync-headless-'));
const target = join(repo, 'bridge/target/rollout-20260928/debug');
const simExe = join(target, 'device-sim.exe');
const bridgeExe = join(target, 'bridge-app.exe');
const instance = `syncv1-${Date.now().toString(36).slice(-8)}`;
const host = 'sync-headless';
let crc = 0xffffffff;
for (const byte of Buffer.from(host)) {
  crc ^= byte;
  for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
}
const bridgeId = (((crc ^ 0xffffffff) >>> 0).toString(16).padStart(8, '0')).slice(0, 4) + '-' + instance;
const duration = Number(process.argv[2] ?? 600_000);
const scenario = process.argv[3] ?? 'periodic';
if (!Number.isSafeInteger(duration) || duration < 60_000 || duration > 86_400_000) {
  throw new Error('duration must be 60000..86400000 virtual milliseconds');
}
if (!['periodic', 'light', 'restart', 'device-restart'].includes(scenario) ||
    (scenario === 'light' && duration < 1_500_000)) {
  throw new Error('scenario must be periodic, light, restart or device-restart; light needs >=1500000ms');
}
const endpoint = 'sync-headless-endpoint';
const control = 'sync-headless-control';
const deviceToken = '0123456789abcdef0123456789abcdef';
const children = [];
const fake = [];

function freePort() {
  return new Promise((resolvePort, reject) => {
    const server = createServer();
    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const port = server.address().port;
      server.close(() => resolvePort(port));
    });
  });
}

function start(exe, args, env, label, readyLine = false) {
  const out = createWriteStream(join(root, `${label}.out`));
  const err = createWriteStream(join(root, `${label}.err`));
  const child = spawn(exe, args, { cwd: repo, env: { ...process.env, ...env },
    windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  children.push(child);
  child.stderr.pipe(err);
  let line = '';
  const ready = new Promise((resolveReady, reject) => {
    if (!readyLine) return resolveReady(null);
    child.stdout.on('data', chunk => {
      const text = chunk.toString();
      out.write(text);
      line += text;
      const end = line.indexOf('\n');
      if (end >= 0) {
        try { resolveReady(JSON.parse(line.slice(0, end))); }
        catch (error) { reject(error); }
        line = line.slice(end + 1);
      }
    });
    child.once('exit', code => reject(new Error(`${label} exited before ready: ${code}`)));
  });
  if (!readyLine) child.stdout.pipe(out);
  return { child, ready };
}

async function call(url, token, path, body, timeout = 20_000) {
  const response = await fetch(url + path, { method: body === undefined ? 'GET' : 'POST',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(timeout) });
  const raw = await response.text();
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status} ${raw.slice(0, 250)}`);
  return JSON.parse(raw);
}

async function mcp(url, name, args) {
  const reply = await call(url, control, '/mcp', { jsonrpc: '2.0', id: name,
    method: 'tools/call', params: { name, arguments: args } });
  if (reply.result?.isError) throw new Error(`${name}: ${reply.result.content?.[0]?.text}`);
  return JSON.parse(reply.result.content[0].text);
}

async function waitReady(url) {
  for (let i = 0; i < 100; i++) {
    try { await mcp(url, 'platform_overview', {}); return; } catch {}
    await new Promise(resolveDelay => setTimeout(resolveDelay, 100));
  }
  throw new Error('isolated Bridge MCP did not start');
}

function stopWatchdog(parentPid) {
  if (!parentPid) return;
  const query = `Get-CimInstance Win32_Process -Filter "name='bridge-app.exe'" | ` +
    `Where-Object { $_.CommandLine -match '--watchdog ${parentPid}$' } | ` +
    `Select-Object -ExpandProperty ProcessId`;
  const output = execFileSync('pwsh', ['-NoProfile', '-Command', query], { encoding: 'utf8' });
  for (const pid of output.trim().split(/\s+/).filter(Boolean)) process.kill(Number(pid));
}

try {
  for (const [mac, targetName, label] of [
    ['02:00:00:00:00:A1', 'codex-status-154g', 'a'],
    ['02:00:00:00:00:B2', 'zectrix-note4-400x300', 'b'],
  ]) {
    const data = join(root, label);
    mkdirSync(data);
    const port = await freePort();
    const started = start(simExe, ['--listen', `127.0.0.1:${port}`, '--mac', mac,
      '--target', targetName, '--data-dir', data, '--seed', '1',
      '--epoch-ms', '1790553600000'], {
      CODEX_STATUS_SIM_ENDPOINT_TOKEN: endpoint,
      CODEX_STATUS_SIM_DEVICE_TOKEN: deviceToken,
      CODEX_STATUS_SIM_CONTROL_TOKEN: control,
    }, label, true);
    const ready = await started.ready;
    fake.push({ mac: mac.replaceAll(':', ''), mac_colon: mac, target: targetName,
      port, data, url: ready.http, token: control });
  }
  const httpPort = await freePort();
  const mcpPort = await freePort();
  const mcpUrl = `http://127.0.0.1:${mcpPort}`;
  const endpoints = Object.fromEntries(fake.map(item => [item.mac, item.url]));
  const bridgeEnv = {
    CODEX_STATUS_INSTANCE: instance, CODEX_STATUS_PORT: String(httpPort),
    CODEX_STATUS_MCP_PORT: String(mcpPort), CODEX_STATUS_TOKEN: endpoint,
    CODEX_STATUS_BRIDGE_SIM_CONTROL_TOKEN: control,
    CODEX_STATUS_SIM_COOPERATIVE: '1',
    CODEX_STATUS_SIM_BLE_ENDPOINTS: JSON.stringify(endpoints),
    COMPUTERNAME: host,
  };
  let bridge = start(bridgeExe, [], bridgeEnv, 'bridge');
  await waitReady(mcpUrl);
  for (const device of fake) {
    const state = await call(device.url, control, '/sim/state');
    await call(device.url, control, '/sim/time', { op: 'rate', rate_ppm: 0 });
    await call(mcpUrl, control, '/sim/clock', { mac: device.mac, op: 'set',
      monotonic_ms: state.clock.monotonic_ms, wall_ms: state.clock.wall_ms, rate_ppm: 0 });
    await mcp(mcpUrl, 'platform_device_register', { mac: device.mac,
      endpoint: device.url.replace('http://', ''), name: `Fake ${device.mac.slice(-2)}` });
    const claimed = await call(device.url, deviceToken,
      `/claim?id=${bridgeId}&lease=3600`, {});
    if (claimed.owner?.id !== bridgeId) throw new Error(`claim failed for ${device.mac}`);
    const status = await call(device.url, endpoint, '/api/status');
    const configured = await call(device.url, endpoint, '/sim/ble/command', {
      op: 'sync_config', request_id: `config-${device.mac.slice(-2)}`,
      token: endpoint, device_mac: status.device_mac, session_nonce: status.session_nonce,
      bridge_id: bridgeId, enabled: true });
    if (configured.result !== 'applied') throw new Error(`sync_config: ${JSON.stringify(configured)}`);
    // The fixture's operation token is known here; preseed only this isolated
    // instance so its first HTTP cycle can establish the Bridge-owned serial.
    writeFileSync(join(target, 'instances', instance, 'data',
      `device-token-${device.mac}.json`), JSON.stringify({
        device_mac: device.mac, token: deviceToken, updated_at: 0 }));
    console.log(`bootstrap ${device.mac}: refresh`);
    const refreshed = await mcp(mcpUrl, 'platform_status_refresh', { mac: device.mac });
    console.log(`bootstrap ${device.mac}: HTTP cycle`);
    const initial = await call(mcpUrl, control, '/sim/run', { mac: device.mac, kind: 'http' }, 90_000);
    const observed = await call(device.url, control, '/sim/state');
    console.log(JSON.stringify({ bootstrap_mac: device.mac, refresh: refreshed.result,
      outcome: initial.outcome,
      power: observed.power.mode, sync_enabled: observed.sync?.enabled,
      sync_rounds: observed.sync?.rounds,
      completed_serial: observed.sync?.last_completed?.client_serial }));
    if (observed.sync?.rounds !== 0 || !observed.sync?.last_completed?.batch_id) {
      throw new Error(`Bridge baseline sync incomplete for ${device.mac}`);
    }
  }
  if (scenario === 'restart') {
    stopWatchdog(bridge.child.pid);
    await new Promise(resolveDelay => setTimeout(resolveDelay, 100));
    const exited = new Promise(resolveExit => bridge.child.once('exit', resolveExit));
    bridge.child.kill();
    await exited;
    bridge = start(bridgeExe, [], bridgeEnv, 'bridge-restarted');
    await waitReady(mcpUrl);
    for (const device of fake) {
      const state = await call(device.url, control, '/sim/state');
      if (state.sync?.last_completed?.client_serial !== '1') {
        throw new Error(`device baseline lost on bridge restart: ${device.mac}`);
      }
      await mcp(mcpUrl, 'platform_status_refresh', { mac: device.mac });
    }
  }
  if (scenario === 'device-restart') {
    const exited = new Promise(resolveExit => children[0].once('exit', resolveExit));
    children[0].kill();
    await exited;
    const device = fake[0];
    const resumed = start(simExe, ['--listen', `127.0.0.1:${device.port}`,
      '--mac', device.mac_colon, '--target', device.target,
      '--data-dir', device.data, '--seed', '1', '--epoch-ms', '1790553600000'], {
      CODEX_STATUS_SIM_ENDPOINT_TOKEN: endpoint,
      CODEX_STATUS_SIM_DEVICE_TOKEN: deviceToken,
      CODEX_STATUS_SIM_CONTROL_TOKEN: control,
    }, 'a-restarted', true);
    const ready = await resumed.ready;
    if (ready.http !== device.url) throw new Error('device restart changed endpoint');
    const state = await call(device.url, control, '/sim/state');
    if (state.sync?.last_completed?.client_serial !== '1' ||
        state.sync?.enabled !== true) {
      throw new Error('Fake ROM persisted sync state was not restored');
    }
  }
  const config = { until_ms: duration, max_events: 20_000,
    bridge: { url: mcpUrl, token: control }, devices: fake,
    actions: scenario === 'light' ? [{ at_ms: 600_000, mac: fake[0].mac, kind: 'light' }] : [],
    trace: join(root, 'trace.ndjson') };
  writeFileSync(join(root, 'runner.json'), JSON.stringify(config, null, 2));
  const runner = start(process.execPath, [join(repo, 'tools/fake-rom-runner.mjs'),
    join(root, 'runner.json')], {}, 'runner');
  const code = await new Promise(resolveExit => runner.child.once('exit', resolveExit));
  if (code !== 0) throw new Error(`runner exited ${code}; see ${root}/runner.err`);
  const trace = readFileSync(config.trace, 'utf8').trim().split('\n').map(JSON.parse);
  const maxima = fake.map((_, index) => Math.max(...trace.map(row => row.devices[index].pre_sync_rounds ?? 0)));
  const completed = fake.map((_, index) => new Set(trace.map(row => row.devices[index].sync_serial).filter(Boolean)).size);
  const archives = fake.map(device => {
    const dir = join(target, 'instances', instance, 'data', 'platform', 'diagnostics', device.mac);
    const checkpoint = JSON.parse(readFileSync(join(dir, 'checkpoint.json'), 'utf8'));
    const files = readdirSync(dir).filter(file => /^\d+-.*\.json$/.test(file));
    const entries = files.map(file => {
      const bytes = readFileSync(join(dir, file));
      const body = JSON.parse(bytes);
      if (body.device_mac?.replaceAll(':', '').toUpperCase() !== device.mac ||
          body.bridge_id !== bridgeId ||
          `${body.batch_id}.json` !== file) throw new Error(`archive identity mismatch: ${file}`);
      return { id: body.batch_id, sha256: createHash('sha256').update(bytes).digest('hex'),
        bytes: bytes.length, serial: Number(body.client_serial), gaps: body.diag?.gaps?.length,
        reasons: body.reasons };
    });
    const latest = entries.find(entry => entry.id === checkpoint.batch_id);
    if (checkpoint.phase !== 'complete' || !latest ||
        checkpoint.sha256 !== latest.sha256 || checkpoint.bytes !== latest.bytes ||
        checkpoint.client_serial !== latest.serial) throw new Error(`checkpoint mismatch: ${device.mac}`);
    return { mac: device.mac, count: entries.length, serial: checkpoint.client_serial,
      latest_sha256: latest.sha256, gaps: entries.reduce((sum, entry) => sum + entry.gaps, 0),
      reasons: [...new Set(entries.flatMap(entry => entry.reasons ?? []))] };
  });
  if (scenario === 'periodic' && duration >= 1_200_000 &&
      (maxima.some(rounds => rounds < 15) ||
      archives.some(archive => archive.count < 2))) throw new Error('periodic diagnostic sync was not observed');
  if (scenario === 'light' && (!archives[0].reasons.includes('light_enter') ||
      !archives[0].reasons.includes('light_exit') ||
      !trace.some(row => row.devices[0].mode === 'light' && row.t >= 600_000) ||
      trace.at(-1).devices[0].mode !== 'deep')) {
    throw new Error('formal light entry/exit was not observed');
  }
  if (scenario === 'restart' && archives.some(archive => archive.serial < 2)) {
    throw new Error('Bridge restart did not continue diagnostic serials');
  }
  if (scenario === 'device-restart' && archives[0].serial < 2) {
    throw new Error('Fake ROM restart did not continue diagnostic serials');
  }
  writeFileSync(join(root, 'summary.json'), JSON.stringify({ duration, events: trace.length,
    scenario, max_rounds: maxima, completed_serials: completed, archives, instance }, null, 2));
  console.log(JSON.stringify({ root, duration, events: trace.length,
    max_rounds: maxima, completed_serials: completed, archives }));
} finally {
  const bridge = children.filter(child => child.spawnfile === bridgeExe).at(-1);
  try { stopWatchdog(bridge?.pid); } catch (error) { console.error(`watchdog cleanup: ${error}`); }
  for (const child of children.reverse()) if (child.exitCode === null) child.kill();
}
