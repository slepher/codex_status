import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const html = readFileSync(new URL('../bridge/crates/app/ui/index.html', import.meta.url), 'utf8');
assert(!html.includes('id="ring-week"'));
assert(!html.includes('id="pt-profile"'));
assert(!html.includes('id="pt-fonts"'));
assert(!html.includes('屏幕内容与发布'));
const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
assert(script);
for (const [, id] of script.matchAll(/\$\('([^']+)'\)/g))
  assert(html.includes('id="' + id + '"'), 'missing DOM id ' + id);
const source = script.slice(0, script.lastIndexOf('\n  renderFamilyMenu();'));
const elements = new Map();
const element = id => {
  if (!elements.has(id)) elements.set(id, {
    textContent: '', innerHTML: '', value: '', checked: false, disabled: false, hidden: false,
    classList: { add() {}, remove() {}, contains() { return false; } },
    focus() {}, getAttribute() { return ''; },
  });
  return elements.get(id);
};
const calls = [];
const handlers = {};
const context = vm.createContext({
  window: { __TAURI__: { core: { invoke: (name, args) => {
    calls.push({ name, args });
    return Promise.resolve(handlers[name]?.(args) ?? {});
  } } } },
  document: { getElementById: element, querySelectorAll: () => [], addEventListener() {} },
  localStorage: { getItem: () => null, setItem() {}, removeItem() {} },
  setInterval() {}, structuredClone,
});
vm.runInContext(source, context);
const run = code => vm.runInContext(code, context);
const macA = '0200000000A1', macB = '0200000000B2';
const select = mac => run("selectedDeviceMac = '" + mac + "'; deviceSelectionGeneration++; clearDeviceView()");
const view = (mac, verdict, screen = '屏幕已显示') => ({
  device: {
    device_mac: mac, identity: { name: mac, model: '书桌屏 200×200' },
    contact: { state: 'expected_sleep', label: '预计休眠', last_authenticated_at: 20,
      transport: 'ble', wifi_sampled_at: 10 },
    firmware: { current: { value: '1.0' }, latest: { value: '2.0', published_at: 5 },
      verdict, label: verdict === 'upgrade_available' ? '可升级' : '已是最新版' },
    upgrade_failure: verdict === 'upgrade_available' ? { reason: '上传失败', at: 10 } : null,
    display: { installed: { value: 0 }, active_template: { value: true },
      data_applied: { value: false }, screen_state: { value: 'displayed' }, label: screen },
    battery: { value: 0 }, delivery: { enabled: false },
    occupancy: { label: '尚未读取占用状态', observed_at: null },
  },
});
const pending = [];
handlers.platform_device_view = ({ mac }) => mac
  ? new Promise(resolve => pending.push({ mac, resolve }))
  : { devices: [] };
handlers.platform_devices = () => ({ devices: [
  { device_mac: macA, name: 'A' }, { device_mac: macB, name: 'B' },
] });
select(macA);
const oldA = run('refreshDevice()');
select(macB);
const b = run('refreshDevice()');
pending[1].resolve(view(macB, 'upgrade_available'));
await b;
assert.equal(element('dev-fw').textContent.startsWith('1.0'), true);
assert.equal(element('dev-verdict').textContent, '可升级');
assert.equal(element('dev-upgrade-failure-row').hidden, false);
assert.match(element('dev-installed').textContent, /未安装内容/);
assert.match(element('dev-applied').textContent, /尚无数据应用/);
assert.match(element('dev-batt').textContent, /0%/);
assert.match(element('device-state').textContent, /预计休眠/);
assert.match(element('dev-sample-note').textContent, /Wi-Fi/);
assert.doesNotMatch(element('dev-fw').textContent, /观察于|采样于/);
assert.doesNotMatch(element('dev-batt').textContent, /观察于|采样于/);
assert.doesNotMatch(element('dev-display').textContent, /观察于|采样于/);
assert.match(element('dev-contact').textContent, /BLE/);
assert.match(run(`relTime(${Math.floor(Date.now() / 1000) + 3600})`), /未来时间/);
select(macA);
const newA = run('refreshDevice()');
pending[2].resolve(view(macA, 'current'));
await newA;
pending[0].resolve(view(macA, 'upgrade_available'));
await oldA;
handlers.platform_device_view = ({ mac }) => mac ? view(mac, 'current') : { devices: [
  { device_mac: macA, name: 'A', model: 'screen', status: '预计休眠', attention: [] },
  { device_mac: macB, name: 'B', model: 'screen', status: '预计休眠', attention: [] },
] };
assert.equal(element('dev-verdict').textContent, '已是最新版');
assert.equal(element('dev-upgrade-failure-row').hidden, true);
const sameWifi = view(macA, 'current');
sameWifi.device.contact = { ...sameWifi.device.contact, transport: 'http', last_authenticated_at: 10 };
handlers.platform_device_view = () => sameWifi;
await run('refreshDevice()');
assert.equal(element('dev-contact-row').hidden, true);
handlers.platform_device_detail = ({ mac, section }) => ({
  device_mac: mac, section, data: { target: 'codex-status-154g', slot: { value: 'ota_0', observed_at: 10 } },
});
const beforeDetail = calls.filter(c => c.name === 'platform_device_detail').length;
await run("loadDeviceDetail('firmware')");
assert.equal(calls.filter(c => c.name === 'platform_device_detail').length, beforeDetail + 1);
assert.match(element('detail-firmware').innerHTML, /ota_0/);
handlers.platform_device_detail = ({ mac, section }) => ({ device_mac: mac, section, data: {
  plan: { plan: { plan_id: 2, mode: 'light' }, prepared_at: 10, accepted_plan_id: 2,
    accepted_remaining_s: 100, accepted_at: 11, observed: { provisional: false },
    observed_at: 12, observed_transport: 'ble' },
  device_power: { value: { battery: 0 } },
  pm_stats: { fetched_at: 13, last_attempt_at: 14, last_error: 'timeout', text: '' },
} });
await run('refreshPlatformPower()');
assert.match(element('pt-plan').textContent, /Bridge 生成于/);
assert.match(element('pt-plan-remaining').textContent, /ACK 于/);
assert.match(element('pm-state').textContent, /PM 采样/);
assert.match(element('pm-state').textContent, /最近尝试失败于/);
assert(!calls.some(c => c.name === 'platform_power'));
handlers.platform_device_detail = ({ mac, section }) => ({ device_mac: mac, section, data: {
  archive: { last_full_success: 'batch-1', phase: 'complete', completed_at: null, gap_count: 0 },
} });
await run("loadDeviceDetail('sync_diagnostics')");
assert.match(element('detail-sync_diagnostics').innerHTML, /时间未记录/);
assert(!calls.some(c => c.name === 'platform_status_refresh'));
assert(!calls.some(c => c.name === 'get_pmstats'));
element('pt-sync').checked = true;
handlers.platform_data_sync_save = () => ({ device_mac: macA, sync_enabled: true });
await run('saveDeviceDataSync()');
assert(calls.some(c => c.name === 'platform_data_sync_save' && c.args.mac === macA && c.args.enabled));
assert(!calls.some(c => c.name === 'platform_publish'));
handlers.platform_status_refresh = () => ({ result: 'ok', status: { device_mac: macB } });
await run('recoverPlatform()');
assert(!calls.some(c => c.name === 'platform_recovery'));
console.log('device page: shared view, MAC isolation, stale response, zero/empty values, passive detail, permission and recovery guard OK');
