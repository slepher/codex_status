import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const html = readFileSync(new URL('../bridge/crates/app/ui/index.html', import.meta.url), 'utf8');
assert(!html.includes('id="ring-week"'));
assert(!html.includes('id="btn-pause"'));
const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
assert(script);
for (const [, id] of script.matchAll(/\$\('([^']+)'\)/g)) {
  assert(html.includes(`id="${id}"`), `missing DOM id ${id}`);
}
const source = script.slice(0, script.lastIndexOf('\n  renderFamilyMenu();'));
const elements = new Map();
const element = id => {
  if (!elements.has(id)) elements.set(id, {
    textContent: '', innerHTML: '', value: '', checked: false, disabled: false,
    classList: { add() {}, remove() {}, contains() { return false; } },
    focus() {}, getAttribute() { return ''; },
  });
  return elements.get(id);
};
const calls = [];
let handlers = {};
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
const wait = () => new Promise(resolve => setImmediate(resolve));
const macA = '0200000000A1', macB = '0200000000B2';
const select = mac => run(`selectedDeviceMac = '${mac}'; deviceSelectionGeneration++; clearDeviceView()`);
run(`platformDevices = [{device_mac:'${macA}'},{device_mac:'${macB}'}]`);
assert.equal(run(`resolveSelectedDevice('${macA}')`), null);
const status = (mac, fw, battery = 0) => ({
  device: { mac, name: fw, owner_known: false }, reachability: 'expected_sleep',
  last_authenticated_contact_at: 10,
  last_success: { observed_at: 10, body: {
    fw, running_slot: 'ota_0', reset_reason: 0, power: { battery },
    radio: { wifi_connected: false, ble_connected: false, rssi: 0 },
    display: { epd_writes: 0, epd_busy_fails: 0 }, template_ids: [],
    heap_free: 0, heap_min: 0, sync_v1: 1,
    sync: { enabled: true, rounds: 0, due: false, phase: 'DEEP',
      pending_batch: { batch_id: 'batch-B', acked_offset: 0, bytes: 100 },
      last_completed: null, reasons: [] },
    groups: Object.fromEntries(['firmware', 'power', 'radio', 'display', 'runtime'].map(group =>
      [group, { received_at: 10, transport: 'http' }])),
  } },
});
const pending = [];
handlers.get_device_status = ({ mac }) => new Promise(resolve => pending.push({ mac, resolve }));

select(macA);
const oldA = run('refreshDevice()');
select(macB);
const b = run('refreshDevice()');
pending[1].resolve(status(macB, 'B'));
await b;
assert.match(element('dev-fw').textContent, /^B/);
assert.match(element('dev-templates').textContent, /未安装模板/);
assert.match(element('dev-heap').textContent, /^0 \/ 0 B/);
assert.match(element('dev-wifi').textContent, /采样时未连接/);
assert.match(element('dev-owner').textContent, /尚未读取/);
assert.match(element('dev-sync-state').textContent, /0\/15 轮/);
assert.match(element('dev-sync-batch').textContent, /ACK 0\/100 B/);

select(macA);
const newA = run('refreshDevice()');
pending[2].resolve(status(macA, 'A-new'));
await newA;
pending[0].resolve(status(macA, 'A-old'));
await oldA;
assert.match(element('dev-fw').textContent, /^A-new/);

const beforePm = calls.filter(call => call.name === 'get_pmstats').length;
run('showPmStatsCache()');
assert.equal(calls.filter(call => call.name === 'get_pmstats').length, beforePm);
select(macB);
assert.equal(element('pm-raw').value, '');
assert.equal(element('dev-fw').textContent, '尚未读取');
run(`pmStatsCache.set('${macB}', {device_mac:'${macB}', fetched_at:10,
  online:false, last_error:'timeout', text:'Mode stats:\\nLock stats:', ip:'192.0.2.2'}); showPmStatsCache()`);
assert.equal(element('pm-raw').value, 'Mode stats:\nLock stats:');
assert.match(element('pm-state').textContent, /保留上次成功采样.*timeout/);

handlers.platform_devices = () => ({ devices: [{ device_mac: macB,
  capabilities: { render_target: 'epd-ssd1681-200x200-1bpp' },
  profile: { device_mac: macB, template_ids: ['saved'], sync_enabled: false },
}], selected_device_mac: macB });
handlers.platform_font_list = () => ({ fonts: [] });
handlers.platform_template_get = () => ({ source: {} });
run('platformDraftDirty = false');
await run('refreshPlatformDevice()');
run("platformProfile.template_ids = ['draft']; markPlatformDraft()");
await run('refreshPlatformDevice()');
assert.equal(run('platformProfile.template_ids[0]'), 'draft');

handlers.platform_status_refresh = () => ({ result: 'ok', status: { device_mac: macA, template_ids: ['wrong'] } });
await run('recoverPlatform()');
assert(!calls.some(call => call.name === 'platform_recovery'));
handlers.platform_power = () => ({ coordinator: { plan: {
  last_sent: { plan_id: 9, mode: 'sleep', rendezvous_period_s: 60 },
  last_accepted_id: 0, last_accepted_remaining_s: 0,
} } });
await run('refreshPlatformPower()');
assert.match(element('pt-plan-remaining').textContent, /未见 ACK \/ 未报告/);

select(macA);
run('modalRenameDevice()');
element('modal-device-name').value = 'A renamed';
select(macB);
handlers.rename_device = () => ({ mac: macA, name: 'A renamed' });
await run('submitRenameDevice()');
assert.equal(calls.at(-1).name, 'rename_device');
assert.equal(calls.at(-1).args.mac, macA);

select(macA);
handlers.platform_publish_preview = () => ({ target_id: 'frozen-A' });
await run('publishPlatform()');
select(macB);
await run('confirmPlatformPublish()');
assert(!calls.some(call => call.name === 'platform_publish'));
await wait();
console.log('device page: MAC isolation, stale response, zero/false/empty values, passive PM, draft, frozen actions, recovery and Plan ACK guard OK');
