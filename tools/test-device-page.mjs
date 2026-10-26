import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const html = readFileSync(new URL('../bridge/crates/app/ui/index.html', import.meta.url), 'utf8');
assert(!html.includes('id="ring-week"'));
assert(!html.includes('加入 / 更新草稿'));
assert(!html.includes('id="wifi-editor"'));
assert(!html.includes('id="wifi-local"'));
assert(!html.includes('id="wifi-family-selection"'));
assert(html.includes('onsubmit="event.preventDefault();wifiFamilySave()"'));
assert(html.indexOf('id="wifi-panel-bridge"')<html.indexOf('id="wifi-family-library"'));
assert(html.indexOf('id="wifi-panel-compare"')<html.indexOf('id="wifi-family-import"'));
assert(!html.includes('id="pt-profile"'));
assert(!html.includes('id="pt-fonts"'));
assert(!html.includes('屏幕内容与发布'));
const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
assert(script);
assert(script.includes('setInterval(() => { refresh(); refreshVisibleDevice(); }, 3000)'));
for (const [, id] of script.matchAll(/\$\('([^']+)'\)/g))
  assert(html.includes('id="' + id + '"'), 'missing DOM id ' + id);
const source = script.slice(0, script.lastIndexOf('\n  renderFamilyMenu();'));
const elements = new Map();
const element = id => {
  if (!elements.has(id)) elements.set(id, {
    textContent: '', innerHTML: '', value: '', checked: false, disabled: false, hidden: false,
    classList: { add() {}, remove() {}, contains() { return false; } },
    focus() {}, scrollIntoView() {}, attributes: {}, setAttribute(name,value) { this.attributes[name]=value; }, getAttribute(name) { return this.attributes[name] || ''; },
  });
  return elements.get(id);
};
const calls = [];
const handlers = {};
let deviceTabOn = false;
const context = vm.createContext({
  window: { confirm: () => true, __TAURI__: { core: { invoke: (name, args) => {
    calls.push({ name, args });
    return Promise.resolve(handlers[name]?.(args) ?? {});
  } } } },
  document: { getElementById: element, querySelectorAll: () => [],
    querySelector: () => ({ classList: { contains: () => deviceTabOn } }), addEventListener() {} },
  localStorage: { getItem: () => null, setItem() {}, removeItem() {} },
  setInterval() {}, clearInterval() {}, setTimeout(callback) {callback();}, structuredClone,
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
assert.match(element('dev-data-needs').textContent, /等待设备报告/);
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
sameWifi.device.delivery.required_fields = 0;
await run('refreshDevice()');
assert.equal(element('dev-contact-row').hidden, true);
assert.match(element('dev-data-needs').textContent, /0 个数据字段/);
sameWifi.device.contact.ble_recovery = { state: 'recovering', attempt: 2, retry_at: 130 };
sameWifi.device.contact.label = '蓝牙异常，正在自动恢复';
await run('refreshDevice()');
assert.match(element('device-state').textContent, /蓝牙异常/);
assert.match(element('dev-sample-note').textContent, /第 2 次.*无需按 BOOT/);
sameWifi.device.contact.ble_recovery.state = 'recovered';
await run('refreshDevice()');
assert.doesNotMatch(element('dev-sample-note').textContent, /蓝牙操作未确认/);
deviceTabOn = true;
handlers.platform_device_view = () => view(macA, 'current');
await run('refreshVisibleDevice()');
assert.equal(element('dev-verdict').textContent, '已是最新版');
deviceTabOn = false;
const visibleCalls = calls.length;
run('refreshVisibleDevice()');
assert.equal(calls.length, visibleCalls);
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
handlers.platform_device_view = ({ mac }) => mac ? view(mac, 'current') : { devices: [
  { device_mac: macA, name: 'A' }, { device_mac: macB, name: 'B' },
] };
run(`platformDevices = [{device_mac:'${macA}'}, {device_mac:'${macB}'}]`);
handlers.platform_data_sync_save = () => ({ device_mac: macA, sync_enabled: true });
await run('saveDeviceDataSync()');
assert(calls.some(c => c.name === 'platform_data_sync_save' && c.args.mac === macA && c.args.enabled));
assert.match(element('pt-device-state').textContent, /数据投递许可已保存：允许/);
assert.doesNotMatch(element('pt-device-state').textContent, /模板|发布/);
assert(!calls.some(c => c.name === 'platform_publish'));
handlers.platform_status_refresh = () => ({ result: 'ok', status: { device_mac: macB } });
await run('recoverPlatform()');
assert(!calls.some(c => c.name === 'platform_recovery'));
const network={network_id:'home',label:'Desk',host_ssid:'Host 5G',ssid:'<script>alert(1)</script>',credential:'keep',origin:'local',version:1,deleted:false};
const wifi={local_revision:1,saved_at:10,targets:[network],snapshot:{schema:2,complete:true,revision:1,snapshot_id:'full-1',observed_at:10,active:{network_id:'home',version:1},networks:[
  {bond:'own',writable:true,network}, {bond:'other',writable:false,network:{...network,network_id:'other'}},
  {bond:'legacy',writable:false,network:{...network,network_id:'legacy-0'}}]},jobs:[]};
const difference={network_id:'home',state:'bridge_changed',writable:true,bridge:network,device:[wifi.snapshot.networks[0]],fields:[
  {field:'ssid',bridge:network.ssid,device:'Old',conflict:false},{field:'host_ssid',bridge:network.host_ssid,device:network.host_ssid,conflict:false}]};
handlers.platform_wifi = ({op,body}) => op==='view'?{wifi,differences:[difference],host:{state:'permission_required'}}:
  op==='get'?{job_id:'read-1',device_mac:macA,state:'queued',last_stage:'accepted'}:
  op==='save'?{...wifi,local_revision:2,targets:body.targets}:op==='sync'?{job_id:'sync-1',device_mac:macA,state:'waiting_rendezvous'}:
  op==='trial'?{stage:'trial_complete',persisted:false}:{};
select(macA);await run('wifiRefreshView()');
assert.match(element('wifi-networks').innerHTML,/设备配置（需占用）/);assert.match(element('wifi-networks').innerHTML,/USB \/ AP legacy/);
assert(!element('wifi-networks').innerHTML.includes('<script>'));
const beforeRead=calls.length;await run('wifiReadScheme()');
assert(calls.slice(beforeRead).some(c=>c.name==='platform_wifi'&&c.args.op==='get'));
assert.match(element('wifi-result').textContent,/已排队/);
// Match the user's queue acknowledgement -> passive completion refresh sequence.
wifi.jobs=[{job_id:'read-1',device_mac:macA,kind:'read',state:'completed',last_stage:'complete_snapshot',attempt_count:1}];
await run('wifiRefreshView()');
assert.match(element('wifi-result').textContent,/完成/);
assert.doesNotMatch(element('wifi-result').textContent,/已排队|accepted/);
assert.match(element('wifi-current-jobs').innerHTML,/暂无进行中的任务/);
assert.match(element('wifi-jobs').innerHTML,/完成/);
// An acknowledgement arriving after completion must immediately reconcile from view.
await run('wifiReadScheme()');
assert.match(element('wifi-result').textContent,/完成.*设备方案已读取/);
wifi.jobs[0]={...wifi.jobs[0],state:'failed',error_code:'unsupported'};
await run('wifiRefreshView()');
assert.match(element('wifi-result').textContent,/已终止.*不支持/);
run("wifiSetResult('Bridge 配置已保存。')");
await run('wifiRefreshView()');
assert.equal(element('wifi-result').textContent,'Bridge 配置已保存。');
wifi.jobs=[];
run("wifiHostNetwork={state:'connected',networks:[{ssid:'Host 5G'}]}");
element('wifi-direction').value='sync';run("wifiChoose(0,true);wifiPreviewSync()");
assert.match(element('wifi-preview').textContent,/整条覆盖/);assert.equal(element('wifi-sync-submit').disabled,false);
await run('wifiCommitPreview()');
const sync=calls.findLast(c=>c.name==='platform_wifi'&&c.args.op==='sync');assert(!('fields' in sync.args.body));assert.deepEqual(Array.from(sync.args.body.selected),['home']);
assert.match(element('wifi-result').textContent,/等待设备可用通道/);
assert(!element('wifi-compare').innerHTML.includes('字段选择'));assert(!element('wifi-compare').innerHTML.includes('密码不回读'));
run("wifiLibrary.families=[{id:'home',name:'Home',ssid_2g:'Device'}]");element('wifi-direction').value='import';run("wifiChoose(0,true);wifiPreviewSync()");
const importStart=calls.length;await run('wifiCommitPreview()');
const imported=calls.slice(importStart).find(c=>c.name==='platform_wifi'&&c.args.op==='import');
assert(!('fields' in imported.args.body));assert.deepEqual(Array.from(imported.args.body.selected),['home']);assert.equal(imported.args.body.snapshot_id,'full-1');
assert.equal(imported.args.body.local_revision,run('wifiState.local_revision'));assert(!('credentials' in imported.args.body));
assert(!calls.slice(importStart).some(c=>c.name==='platform_wifi'&&['get','read_import','authorize'].includes(c.args.op)));
assert.match(element('wifi-result').textContent,/选中的设备快照方案整条保存到 Bridge/);
assert(!source.includes('wifiRefreshImport()'));element('wifi-direction').value='sync';
run(`wifiDifferences=[{network_id:'readonly',state:'device_only',writable:false,bridge:null,device:[{bond:'other',writable:false,network:{ssid:'Other'}}],fields:[{field:'ssid',bridge:null,device:'Other'}]},
 {network_id:'delete',state:'explicit_delete',writable:true,bridge:{deleted:true,credential:'keep'},device:[],fields:[]},
 {network_id:'unknown',state:'no_baseline',writable:true,bridge:{credential:'keep'},device:[],fields:[]}];wifiSelectDifferences()`);
assert.equal(run('wifiSelections.size'),0);run("wifiChoose(0,true);wifiPreviewSync()");
assert.match(element('wifi-preview').textContent,/缺少已保存配置/);
run("wifiState.jobs=[{job_id:'uncertain',kind:'sync',state:'awaiting_confirmation',device_mac:'"+macA+"',attempt_count:1,last_stage:'request_sent',may_have_committed:true,created_at:1791121712,expires_at:86400}];wifiRenderJobs()");
assert.match(element('wifi-current-jobs').innerHTML,/创建时间/);assert(element('wifi-current-jobs').innerHTML.includes(run('wifiCreatedTime(1791121712)')));
assert.match(element('wifi-current-jobs').innerHTML,/先查询原事务/);assert.match(element('wifi-current-jobs').innerHTML,/取消不回滚/);
run("wifiState.jobs.push({job_id:'done',kind:'read',state:'completed',device_mac:'"+macA+"',attempt_count:1});wifiRenderJobs()");
assert.match(element('wifi-jobs').innerHTML,/创建时间 时间未记录/);assert.match(element('wifi-jobs').innerHTML,/完成/);assert(!element('wifi-current-jobs').innerHTML.includes('done'));assert.equal(element('wifi-job-history').open,false);
let finishWifi;handlers.platform_wifi=({op})=>op==='get'?new Promise(resolve=>{finishWifi=resolve}):{wifi,differences:[difference]};
const oldRead=run('wifiReadScheme()');select(macB);finishWifi({job_id:'old',device_mac:macA,state:'completed'});await oldRead;
assert.equal(element('wifi-result').textContent,'操作结果将在此显示');
assert(!calls.some(c=>c.name==='platform_publish'));
const tabCalls=calls.length;
element('wifi-family-name').value='unsaved-editor';
run("wifiSelections.set('home',true);wifiPreview={scope:'keep'}");
for(const tab of ['device','compare','bridge']) {
 run("wifiShowTab('"+tab+"')");
 for(const other of ['bridge','device','compare'])
  assert.equal(element('wifi-panel-'+other).hidden,other!==tab);
 assert.equal(element('wifi-tab-'+tab).getAttribute('aria-selected'),'true');
}
assert.equal(element('wifi-family-name').value,'unsaved-editor');
assert.equal(run("wifiSelections.get('home')"),true);
assert.equal(run('wifiPreview.scope'),'keep');
assert.equal(calls.length,tabCalls);
// One combined request, sequential system decisions, and one startup attempt.
assert(!html.includes('id="wifi-location-action"'));
assert(!html.includes('id="wifi-bluetooth-action"'));
run("wifiShowPermissions({platform:'macos',location:'not_determined',bluetooth:'denied'})");
assert.equal(element('wifi-permission-action').textContent,'授权定位与蓝牙');
assert.match(element('wifi-bluetooth-note').textContent,/隐私与安全性 → 蓝牙/);
let permissionState={platform:'macos',location:'not_determined',bluetooth:'not_determined'};
let authorizationPoll=0,requested=false;
handlers.platform_wifi=({op})=>{
  if(op==='permission_all'){requested=true;return permissionState;}
  if(op==='permissions') {
    if(requested){authorizationPoll++;permissionState={platform:'macos',location:'allowed',bluetooth:authorizationPoll===1?'not_determined':'allowed'};}
    return permissionState;
  }
  if(op==='host')return {state:'connected',networks:[{ssid:'Host-5G'}]};
  if(op==='settings_permissions')return {opened:true};
  throw new Error('Unexpected permission operation '+op);
};
const requestStart=calls.length;
run('wifiPermissionStartupChecked=false');
await run('wifiInitPermissions()');
assert.equal(authorizationPoll,2);
assert.equal(calls.slice(requestStart).filter(c=>c.args.op==='permission_all').length,1);
assert.equal(element('wifi-permissions').hidden,true);
assert.equal(element('wifi-permission-result').textContent,'两项权限已授权。');
assert.equal(element('wifi-host').textContent,'Host-5G');
const secondStart=calls.length;await run('wifiInitPermissions()');
assert.equal(calls.length,secondStart);
for(const state of [{platform:'macos',location:'allowed',bluetooth:'allowed'},
 {platform:'macos',location:'denied',bluetooth:'restricted'}, {platform:'other'}]) {
 permissionState=state;requested=false;run('wifiPermissionStartupChecked=false');
 const start=calls.length;await run('wifiInitPermissions()');
 assert(!calls.slice(start).some(c=>/^(permission_|settings_)/.test(c.args.op)));
}
permissionState={platform:'macos',location:'disabled',bluetooth:'denied'};
run('wifiShowPermissions('+JSON.stringify(permissionState)+')');
assert.equal(element('wifi-permission-action').textContent,'打开权限设置');
assert.match(element('wifi-location-note').textContent,/开启定位服务/);
const settingsStart=calls.length;await run('wifiPermissionAction()');
assert.equal(calls.slice(settingsStart).filter(c=>c.args.op==='settings_permissions').length,1);
assert(!calls.slice(settingsStart).some(c=>/^permission_/.test(c.args.op)));
// Slow decisions stay pending; native callbacks own the second system request.
permissionState={platform:'macos',location:'not_determined',bluetooth:'allowed'};
handlers.platform_wifi=({op})=>op==='host'?{state:'permission_required',networks:[]}:permissionState;
const pendingStart=calls.length;await run('wifiPermissionAction()');
assert.equal(calls.slice(pendingStart).filter(c=>c.args.op==='permissions').length,13);
assert.equal(calls.slice(pendingStart).filter(c=>c.args.op==='permission_all').length,1);
assert.match(element('wifi-permission-result').textContent,/等待系统授权/);
assert.equal(element('wifi-permissions').hidden,false);
assert.equal(element('wifi-permission-action').disabled,false);
// Duplicate clicks cannot issue another native request while the first is active.
let finishPermissionCheck;
handlers.platform_wifi=({op})=>op==='permissions'?new Promise(resolve=>{finishPermissionCheck=resolve}):permissionState;
const busyStart=calls.length,activeRequest=run('wifiPermissionAction()');
await run('wifiPermissionAction()');assert.equal(calls.length,busyStart+1);
permissionState={platform:'macos',location:'allowed',bluetooth:'allowed'};
finishPermissionCheck(permissionState);await activeRequest;
assert.equal(element('wifi-permissions').hidden,true);
const startup=script.slice(script.lastIndexOf('\n  renderFamilyMenu();'));
assert(startup.includes('wifiInitPermissions()'));
run("wifiPermissionGuideShown=false;wifiShowPermissions({platform:'macos',location:'not_determined',bluetooth:'allowed'});wifiGuidePermissionsOnce()");
assert.equal(run('wifiPermissionGuideShown'),true);
const guidedCalls=calls.length;run('wifiGuidePermissionsOnce()');assert.equal(calls.length,guidedCalls);

console.log('permission UI: combined first-start request, bounded follow-up, denied/settings, duplicate guard, real-grant visibility OK');
console.log('device page: passive cache, full bond/legacy groups, offline drafts, queued reads, whole-network preview, stale-MAC isolation and secret cleanup OK');

// Manual sync guard applies to merged device rows, not local editing/import.
run("wifiState.targets=[{network_id:'old',host_ssid:'Host 5G',ssid:'Old',origin:'local',deleted:true},{network_id:'new',host_ssid:'Host 5G',ssid:'New',origin:'local',deleted:false}];wifiScheme={complete:true,snapshot_id:'guard',networks:[{writable:true,network:wifiState.targets[0]}]};wifiHostNetwork={state:'connected',networks:[{ssid:'Host 5G'}]}");
assert.match(run("wifiManualNetworkGuard(['old'])"),/保留或加入当前网络/);
assert.equal(run("wifiManualNetworkGuard(['old','new'])"),null);
assert.equal(run("wifiManualNetworkGuard(['new'])"),null);
run("wifiHostNetwork={state:'permission_required'}");assert.match(run("wifiManualNetworkGuard(['new'])"),/授权/);
console.log('Manual Wi-Fi post-state guard passed');

// Source ownership does not determine whether a saved Bridge target can be sent.
run("wifiScheme={complete:true};wifiDifferences=[{network_id:'home',writable:false,bridge:{origin:'local',deleted:false},device:[{writable:false,network:{label:'Old source'}}]}]");
assert.equal(run('wifiCanSelect(wifiDifferences[0],false)'),true);
run('wifiDifferences[0].bridge.deleted=true');assert.equal(run('wifiCanSelect(wifiDifferences[0],false)'),false);
run("wifiDifferences[0].bridge.deleted=false;wifiDifferences[0].device.push({writable:true,network:{label:'Current target'}})");
assert.equal(run('wifiCanSelect(wifiDifferences[0],false)'),true);
assert.equal(run('wifiCanSelect(wifiDifferences[0],true)'),true);
assert.equal(run('wifiImportRow(wifiDifferences[0]).network.label'),'Current target');
console.log('Roundtrip selection: source ownership independent, current target preferred, absent-target deletion blocked OK');

run(`wifiRenderOnline({observed_at:Date.now()/1000,presence:{mode:'continuous',remaining_ms:null,reasons:['usb']},transports:{http:{ready:true},ble:{ready:false}},wifi_control:{write_allowed:true}})`);
assert.match(element('wifi-online').textContent,/USB 常驻在线.*HTTP 就绪.*BLE 关闭/);
run(`wifiRenderOnline({observed_at:Date.now()/1000,presence:{mode:'window',remaining_ms:120000,reasons:['boot']},transports:{http:{ready:false},ble:{ready:true}}})`);
assert.match(element('wifi-online').textContent,/BOOT 在线.*剩余 (119|120) 秒/);
run(`wifiRenderOnline({observed_at:Date.now()/1000-60,presence:{mode:'continuous',remaining_ms:null,reasons:['usb']}})`);
assert.match(element('wifi-online').textContent,/在线状态待确认/);

for(const [code,text] of [['not_sent','未发送'],['confirmation_budget_exhausted','执行已结束；设备结果仍未知'],['result_unknown','勿重复提交'],['network_not_found','未找到网络'],['window_budget_exhausted','预算已耗尽'],['http_proof_budget_exhausted','未能确认 HTTP']])
  assert(run(`wifiError('${code}')`).includes(text));
assert.match(run("wifiError('revision_conflict')"),/缓存已过期.*重新读取设备/);

run("wifiState={local_revision:7,targets:[{network_id:'home',host_ssid:'Host 5G',ssid:'Device',deleted:false}],jobs:[]};wifiHostNetwork={state:'connected',networks:[{ssid:'Host 5G'}]}");
run("wifiRenderCurrent({matched_target:wifiState.targets[0],automatic_state:'retry_exhausted',connection:{attempts:3,error:'connect_timeout'}})");
assert.match(element('wifi-auto').textContent,/已尝试 3 次.*已停止.*连接超时/);
assert.equal(element('wifi-current-retry').hidden,false);
run("wifiState.jobs=[{kind:'activate',state:'awaiting_confirmation',sent:true}];wifiRenderCurrent({matched_target:wifiState.targets[0],automatic_state:'retry_exhausted',connection:{attempts:3}})");
assert.equal(element('wifi-current-retry').hidden,true);
assert.match(element('wifi-auto').textContent,/确认上次操作.*勿重复/);
run('wifiState.jobs=[]');
const retryStart=calls.length;
handlers.platform_wifi=()=>({state:'waiting_rendezvous'});
await run('wifiRetryCurrent()');
const retry=calls.slice(retryStart).find(c=>c.args.op==='use_current');
assert(retry);assert.deepEqual(JSON.parse(JSON.stringify(retry.args.body)),{network_id:'home',local_revision:7});
assert.equal(element('wifi-current-retry').disabled,false);
for(const state of ['connecting','awaiting_confirmation','connected','connection_failed']){
  run(`wifiRenderCurrent({matched_target:wifiState.targets[0],automatic_state:'${state}',connection:{error:'authentication_failed'}})`);
  assert.equal(element('wifi-current-retry').hidden,true);
}
handlers.platform_wifi=()=>{throw 'local_revision_conflict'};
run("wifiState.targets=[{network_id:'home',host_ssid:'Host 5G',ssid:'Device',deleted:false}];wifiHostNetwork={state:'connected',networks:[{ssid:'Host 5G'}]}");
await run('wifiRetryCurrent()');
assert.equal(element('wifi-current-retry').disabled,false);
assert.match(element('wifi-result').textContent,/修订|冲突|重新/);
console.log('Wi-Fi exhausted budget: explicit message/retry, unknown/auth states protected, failure visible OK');

run(`wifiShowResult({outcome:'rejected',stage:'persist',error:'storage',action:'offline_migration_or_storage_repair'})`);
assert.match(element('wifi-result').textContent,/设备登记失败.*保存设备登记.*离线迁移/);
run(`wifiShowResult({outcome:'unknown',stage:'ack',error:'window_budget_exhausted',action:'read_registration_before_retry'})`);
assert.match(element('wifi-result').textContent,/设备登记结果未知.*确认登记结果.*请勿重复提交/);

run(`platformDevices=[{device_mac:'0200000000A1',name:'screen',model:'Note4',status:'最近已认证',attention:['firmware_upgrade']}];renderRegisteredDevices('0200000000A1')`);
assert.match(element('registered-devices').innerHTML,/有可用固件升级/);
assert.doesNotMatch(element('registered-devices').innerHTML,/待处理 1 项/);
run(`wifiState.jobs=[{job_id:'visible-job-id',device_mac:'0200000000A1',kind:'read',state:'waiting_rendezvous',created_at:1}];wifiRenderJobs()`);
assert.match(element('wifi-current-jobs').innerHTML,/visible-job-id/);
assert(html.includes('操作结果将在此显示'));

// Local second ticks reuse one sample, never invoke IPC, and cannot leak another device's state.
assert(script.includes('setInterval(wifiTickOnline, 1000)'));
deviceTabOn=true;
run(`globalThis.countdownNow=2000000;Date.now=()=>countdownNow;wifiRenderOnline({observed_at:2000,presence:{mode:'window',remaining_ms:112000,reasons:['bridge']}})`);
assert.match(element('wifi-online').textContent,/剩余 112 秒/);
const countdownCalls=calls.length;
run('countdownNow+=1000;wifiTickOnline()');
assert.match(element('wifi-online').textContent,/剩余 111 秒/);
run('countdownNow+=5000;wifiTickOnline()');
assert.match(element('wifi-online').textContent,/剩余 106 秒/);
assert.equal(calls.length,countdownCalls);
run('countdownNow+=25000;wifiTickOnline()');
assert.match(element('wifi-online').textContent,/在线状态待确认/);
run('clearDeviceView();wifiTickOnline()');
assert.equal(element('wifi-online').textContent,'等待认证在线状态');
run(`wifiRenderOnline({observed_at:countdownNow/1000-5,presence:{mode:'window',remaining_ms:1000,reasons:['bridge']}});wifiTickOnline()`);
assert.match(element('wifi-online').textContent,/窗口已结束 \/ 预计休眠/);
assert.doesNotMatch(element('wifi-online').textContent,/剩余 -/);
run('document.hidden=true;wifiRenderOnline(null);wifiOnlineStatus={observed_at:countdownNow/1000,presence:{mode:"continuous",reasons:["usb"]}};wifiTickOnline()');
assert.equal(element('wifi-online').textContent,'等待认证在线状态');
run('document.hidden=false;wifiTickOnline()');
assert.match(element('wifi-online').textContent,/USB 常驻在线/);
console.log('Local countdown: second ticks, elapsed-time correction, stale/zero/reset/hidden guards, zero IPC OK');

assert(html.includes('添加 AP01 · 局域网'));
assert(script.includes("invoke('platform_ap01'"));
assert.equal(run("ap01Message({result:'pending',display_code:'0042',reply:{state:'awaiting_confirmation'}})"),'awaiting_confirmation');
assert.match(element('ap01-result').textContent,/0042.*旋钮/);
assert.equal(run("ap01Message({result:'rejected',reply:{state:'binding_storage',error:'binding_storage'}})"),'rejected');
assert.equal(run("ap01Message({result:'unknown',action:'查询原配对事务'})"),'unknown');
assert.match(element('ap01-result').textContent,/结果未知.*查询原配对事务/);
assert.equal(run("ap01Message({result:'paired'})"),'paired');
assert.match(element('ap01-result').textContent,/配置模板并发布/);
assert(script.includes("$('device-power').hidden=d.power_mode==='always_on'"));
console.log('AP01 onboarding: fixed four-digit code, unknown-query guidance, terminal storage rejection, separate publication and always-on UI OK');

// A family draft saved through MCP must become visible when opening the menu,
// even if the panel missed an event while it was already open.
let savedFamilyProfiles = [];
handlers.platform_family_profiles = ({ renderTarget }) => {
  assert.equal(renderTarget, 'epd-ssd2683-400x300-1bpp');
  return { profiles: savedFamilyProfiles };
};
run("selectedRenderTarget = RENDER_TARGET_NOTE4; selectedFamilyProfileId = null");
await run('refreshProfiles()');
assert.match(element('menu-profile').innerHTML, /此族暂无 Profile/);
savedFamilyProfiles = [{id:'codex-status',name:'Note4 Codex 额度',
  render_target:'epd-ssd2683-400x300-1bpp',template_ids:[]}];
await run("toggleMenu('menu-profile', { stopPropagation() {} })");
assert.match(element('menu-profile').innerHTML, /Note4 Codex 额度/);
assert.equal(run('selectedFamilyProfileId'), 'codex-status');
console.log('Family menu reloads persisted drafts instead of keeping an empty panel cache');

handlers.platform_family_profiles = () => ({ profiles: [] });
await run("selectFamily('lcd-jd9853-320x240-palette4')");
assert.equal(run('selectedRenderTarget'), 'lcd-jd9853-320x240-palette4');
assert.match(element('menu-family').innerHTML, /AP01/);
console.log('AP01 template family is selectable in the same configuration menu');

// Shared Wi-Fi library: per-device selection, family keys on the existing wire,
// explicit credential treatment, cached reverse import and AP01 manual controls.
select(macA);
let sharedWifiLibrary={revision:9,families:[{id:'home',name:'Home-5',ssid_2g:'Home-2',ssid_5g:'Home-5',password_2g:'fixture-password',password_5g:'fixture-password'}],devices:{[macA]:['home'],[macB]:[]},wired_family:'home'};
let manualWifi=false;
const familyTarget={network_id:'home',host_ssid:'Home-5',ssid:'Home-2',credential:'keep',password:'fixture-password',origin:'local',version:2};
handlers.platform_wifi=({op,body})=>{
  if(op==='view')return {library:sharedWifiLibrary,manual_selection:manualWifi,host:{state:'connected',family_name:'Home-5',networks:[{ssid:'Home-2'}]},
    wifi:{local_revision:4,targets:[familyTarget],jobs:[],snapshot:{schema:2,complete:true,snapshot_id:'family-snapshot',revision:2,capacity:{total:12},networks:[{bond:'own',writable:true,network:familyTarget}]}},differences:[],automatic_state:manualWifi?'manual_selection':'connected',matched_target:familyTarget};
  if(op==='library_save'){sharedWifiLibrary={...sharedWifiLibrary,families:body.families,revision:sharedWifiLibrary.revision+1};return sharedWifiLibrary;}
  return {state:'completed'};
};
await run('wifiRefreshView()');
assert.equal(run('wifiNetworkKey()'),'Home-5');
assert.match(element('wifi-host').textContent,/Home-2.*Home-5/);
assert.match(element('wifi-family-list').innerHTML,/home/);
run('wifiFamilyEdit(0)');assert.equal(element('wifi-family-pass2').value,'fixture-password');
run('wifiFamilyTogglePassword(2)');assert.equal(element('wifi-family-pass2').type,'text');
element('wifi-family-name').value='My home';
await run('wifiFamilySave()');
const familySave=calls.filter(c=>c.name==='platform_wifi'&&c.args.op==='library_save').at(-1);
assert.equal(familySave.args.body.library_revision,9);assert.equal(familySave.args.body.families[0].id,'home');
assert.equal(familySave.args.body.families[0].password_2g,'fixture-password');
assert.equal(element('wifi-family-pass2').value,'');assert.equal(element('wifi-family-pass2').type,'password');
context.document.querySelectorAll=selector=>selector==='[data-wifi-family]:checked'?[{dataset:{wifiFamily:'home'}}]:[];
await run('wifiFamilySelect()');
const familySelection=calls.filter(c=>c.name==='platform_wifi'&&c.args.op==='library_select').at(-1);
assert.deepEqual(Array.from(familySelection.args.body.selected),['home']);assert.equal(familySelection.args.mac,macA);
context.document.querySelectorAll=()=>[];
run("wifiSelections.clear();wifiSelections.set('home',true)");element('wifi-import-family').value='home';
const familyImportStart=calls.length;await run('wifiFamilyImport()');
const familyImport=calls.slice(familyImportStart).find(c=>c.args?.op==='library_import');
assert.equal(familyImport.args.body.snapshot_id,'family-snapshot');assert.equal(familyImport.args.body.network_id,'home');
assert(!calls.slice(familyImportStart).some(c=>c.args?.op==='get'));
// Background refresh preserves a wired choice and unfinished subscription choices.
element('wifi-wired-family').value='';run('wifiWiredDirty=true;wifiFamilySelectionDirty=true');
context.document.querySelectorAll=selector=>selector==='[data-wifi-family]:checked'?[{dataset:{wifiFamily:'home'}}]:[];await run('wifiRefreshView()');
assert.equal(element('wifi-wired-family').value,'');assert.match(element('wifi-family-list').innerHTML,/checked/);context.document.querySelectorAll=()=>[];
manualWifi=true;run('wifiFamilySelectionDirty=false;wifiWiredDirty=false');await run('wifiRefreshView()');
assert.equal(element('wifi-ap01-auth').hidden,false);assert.equal(element('wifi-current-action').hidden,true);
assert.equal(element('wifi-note-recovery').hidden,true);assert(!element('wifi-networks').innerHTML.includes('wifiSelect('));
element('wifi-ap01-code').value='01234567';await run('wifiReadScheme()');assert.equal(element('wifi-ap01-code').value,'');
const ap01Read=calls.filter(c=>c.name==='platform_wifi'&&c.args.op==='get').at(-1);assert.equal(ap01Read.args.body.device_code,'');
assert.equal(run("wifiManualNetworkGuard(['home'])"),null);
select(macB);assert.equal(element('wifi-ap01-code').value,'');assert.equal(run('wifiFamilySelectionDirty'),false);
console.log('Wi-Fi families: shared edits, independent selections, family-name wire key, wired draft, cached import, password cleanup and AP01 manual selection OK');

// Shared editor is the only edit path, including current host shortcuts.
manualWifi=false;select(macA);await run('wifiRefreshView()');
run('wifiEditCurrent()');assert.equal(run('wifiFamilyEditing'),'home');
assert.equal(element('wifi-family-editor').open,true);
const editCalls=calls.length;
element('wifi-family-name').value='draft retained';await run('wifiRefreshView()');
assert.equal(element('wifi-family-name').value,'draft retained');
assert(!calls.slice(editCalls).some(c=>['save','sync','get'].includes(c.args?.op)));
handlers.platform_wifi=()=>{throw 'wifi_library_revision_conflict'};
await run('wifiFamilySave()');
assert.equal(element('wifi-family-name').value,'draft retained');
assert.equal(element('wifi-family-editor').open,true);
assert.match(element('wifi-family-result').textContent,/已变化/);
run('wifiFamilyCancel()');assert.equal(element('wifi-family-editor').open,false);
assert.equal(element('wifi-family-pass2').value,'');
element('wifi-direction').value='import';run('wifiRenderComparison()');assert.equal(element('wifi-family-import').hidden,false);
element('wifi-direction').value='sync';run('wifiRenderComparison()');assert.equal(element('wifi-family-import').hidden,true);
// Recovery uses a saved subscription; unknown credentials must not become empty passwords.
run("wifiLibrary.devices[selectedTarget()]=['home'];wifiState.targets=[{network_id:'home',ssid:'Home-2',host_ssid:'Home-5',password:null}]");
element('wifi-recovery-family').value='home';const recoveryCalls=calls.length;
await run('wifiTrial()');assert.equal(calls.length,recoveryCalls);assert.match(element('wifi-result').textContent,/补充/);
run("wifiState.targets[0].password='fixture-recovery'");
handlers.platform_wifi=({op})=>op==='trial'?{stage:'trial_complete'}:{wifi:run('wifiState'),library:run('wifiLibrary')};
await run('wifiTrial()');assert(calls.slice(recoveryCalls).some(c=>c.args?.op==='trial'));
assert.match(element('wifi-result').textContent,/恢复原链路/);
console.log('Unified Wi-Fi workspace: one editor, current-family shortcut, draft/conflict retention, direction import and bounded recovery OK');

element('wifi-import-family').value='';const noDestinationCalls=calls.length;
run("wifiSelections.clear();wifiSelections.set('unknown',true);wifiScheme={complete:true}");
await run('wifiFamilyImport()');assert.equal(calls.length,noDestinationCalls);assert.match(element('wifi-result').textContent,/明确选择/);
run("wifiDifferences=[{network_id:'unknown',device:[{writable:true,network:{}}]}]");
element('wifi-direction').value='import';run('wifiPreviewSync()');assert.equal(element('wifi-sync-submit').disabled,true);assert.match(element('wifi-preview').textContent,/选择目标族/);
