import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const html = readFileSync(new URL('../bridge/crates/app/ui/index.html', import.meta.url), 'utf8');
assert(!html.includes('id="ring-week"'));
assert(!html.includes('加入 / 更新草稿'));
assert(!html.includes('onclick="wifiSaveLocal()"'));
assert(html.includes('onsubmit="event.preventDefault();wifiSaveLocal()"'));
assert.match(html,/<button id="wifi-submit"[^>]+>保存到 Bridge<\/button>/);
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
assert.match(element('wifi-networks').innerHTML,/其他绑定/);assert.match(element('wifi-networks').innerHTML,/USB \/ AP legacy/);
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
run("wifiLocalResult('Bridge 配置已保存。')");
await run('wifiRefreshView()');
assert.equal(element('wifi-result').textContent,'Bridge 配置已保存。');
wifi.jobs=[];
run('wifiDraft[0].password="fixture-pass";wifiEdit(0)');assert.equal(element('wifi-password').value,'fixture-pass');
assert.equal(element('wifi-password').type,'password');run('wifiTogglePassword()');assert.equal(element('wifi-password').type,'text');
assert.equal(element('wifi-password-toggle').getAttribute('aria-label'),'隐藏密码');
run('wifiTogglePassword()');assert.equal(element('wifi-password').type,'password');
run('wifiTogglePassword();wifiEdit(0)');assert.equal(element('wifi-password').type,'password');
element('wifi-ssid').value='Local edit';run('wifiSubmit()');
const beforeRefresh=run('wifiDraft[0].ssid');await run('wifiRefreshView()');assert.equal(run('wifiDraft[0].ssid'),beforeRefresh);
await run('wifiSaveLocal()');assert(calls.some(c=>c.name==='platform_wifi'&&c.args.op==='save'));
assert.equal(calls.findLast(c=>c.name==='platform_wifi'&&c.args.op==='save').args.body.targets[0].password,'fixture-pass');
assert(!calls.some(c=>c.name==='platform_wifi'&&c.args.op==='setup'));
run("wifiHostNetwork={state:'connected',networks:[{ssid:'Host 5G'}]}");
element('wifi-direction').value='sync';run("wifiChoose(0,true);wifiPreviewSync()");
assert.match(element('wifi-preview').textContent,/整条覆盖/);assert.equal(element('wifi-sync-submit').disabled,false);
await run('wifiCommitPreview()');
const sync=calls.findLast(c=>c.name==='platform_wifi'&&c.args.op==='sync');assert(!('fields' in sync.args.body));assert.deepEqual(Array.from(sync.args.body.selected),['home']);
assert.match(element('wifi-result').textContent,/等待设备可用通道/);
assert(!element('wifi-compare').innerHTML.includes('字段选择'));assert(!element('wifi-compare').innerHTML.includes('密码不回读'));
element('wifi-direction').value='import';run("wifiChoose(0,true);wifiPreviewSync()");
const importStart=calls.length;await run('wifiCommitPreview()');
const imported=calls.slice(importStart).find(c=>c.name==='platform_wifi'&&c.args.op==='import');
assert(!('fields' in imported.args.body));assert.deepEqual(Array.from(imported.args.body.selected),['home']);assert.equal(imported.args.body.snapshot_id,'full-1');
assert.equal(imported.args.body.local_revision,run('wifiState.local_revision'));assert(!('credentials' in imported.args.body));
assert(!calls.slice(importStart).some(c=>c.name==='platform_wifi'&&['get','read_import','authorize'].includes(c.args.op)));
assert.match(element('wifi-result').textContent,/选中的设备快照方案整条保存到 Bridge/);
assert(!source.includes('wifiRefreshImport()'));element('wifi-direction').value='sync';
element('wifi-password').value=Array.from({length:12},(_,i)=>String.fromCharCode(97+i)).join('');
await run('wifiTrial()');assert.equal(element('wifi-password').value,'');assert.match(element('wifi-result').textContent,/恢复原链路/);
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
assert.equal(element('wifi-result').textContent,'等待操作');assert.equal(element('wifi-password').value,'');
assert(!calls.some(c=>c.name==='platform_publish'));
const tabCalls=calls.length;
element('wifi-id').value='unsaved-editor';
run("wifiSelections.set('home',true);wifiPreview={scope:'keep'}");
for(const tab of ['device','compare','bridge']) {
 run("wifiShowTab('"+tab+"')");
 for(const other of ['bridge','device','compare'])
  assert.equal(element('wifi-panel-'+other).hidden,other!==tab);
 assert.equal(element('wifi-tab-'+tab).getAttribute('aria-selected'),'true');
}
assert.equal(element('wifi-id').value,'unsaved-editor');
assert.equal(run("wifiSelections.get('home')"),true);
assert.equal(run('wifiPreview.scope'),'keep');
assert.equal(calls.length,tabCalls);
// Permission checks are passive; prompts/settings only follow explicit actions.
run("wifiShowPermissions({platform:'macos',location:'not_determined',bluetooth:'denied'})");
assert.equal(element('wifi-permissions').hidden,false);
assert.equal(element('wifi-location-action').textContent,'授权定位');
assert.equal(element('wifi-bluetooth-action').textContent,'打开蓝牙设置');
assert.match(element('wifi-bluetooth-note').textContent,/隐私与安全性 → 蓝牙/);
let authorizationPoll=0;
handlers.platform_wifi=({op})=>{
  if(op==='permission_location')return {platform:'macos',location:'not_determined',bluetooth:'denied'};
  if(op==='permissions') {authorizationPoll++;return {platform:'macos',location:'allowed',bluetooth:'denied'};}
  if(op==='settings_bluetooth')return {opened:true};
  if(op==='host')return {state:'connected',networks:[{ssid:'Host-5G'}]};
  throw new Error('Unexpected permission operation '+op);
};
const requestStart=calls.length;
await run("wifiPermissionAction('location')");
assert.equal(authorizationPoll,1);
assert.equal(element('wifi-permission-location').hidden,true);
assert.equal(element('wifi-permission-bluetooth').hidden,false);
assert.equal(element('wifi-host').textContent,'Host-5G');
assert.equal(calls.slice(requestStart).filter(c=>c.args.op==='permission_location').length,1);
assert.equal(element('wifi-permission-result').textContent,'已授权。');
const settingsStart=calls.length;
await run("wifiPermissionAction('bluetooth')");
assert(calls.slice(settingsStart).some(c=>c.args.op==='settings_bluetooth'));
assert(!calls.slice(settingsStart).some(c=>c.args.op==='permission_bluetooth'));
run("wifiShowPermissions({platform:'macos',location:'disabled',bluetooth:'restricted'})");
assert.equal(element('wifi-location-action').textContent,'打开定位设置');
assert.match(element('wifi-location-note').textContent,/开启定位服务/);
assert.match(element('wifi-bluetooth-note').textContent,/管理员/);
run("wifiShowPermissions({platform:'macos',location:'allowed',bluetooth:'allowed'})");
assert.equal(element('wifi-permissions').hidden,true);
run("wifiShowPermissions({platform:'other'})");assert.equal(element('wifi-permissions').hidden,true);
run("wifiShowPermissions({platform:'macos',location:'not_determined',bluetooth:'allowed'})");
handlers.platform_wifi=({op})=>op==='host'?{state:'permission_required',networks:[]}:
  {platform:'macos',location:'not_determined',bluetooth:'allowed'};
const pendingStart=calls.length;
await run("wifiPermissionAction('location')");
assert.equal(calls.slice(pendingStart).filter(c=>c.args.op==='permissions').length,12);
assert.equal(calls.slice(pendingStart).filter(c=>c.args.op==='permission_location').length,1);
assert.match(element('wifi-permission-result').textContent,/等待系统授权/);
assert.equal(element('wifi-permissions').hidden,false); // Never fake a grant after timeout.
assert.equal(element('wifi-location-action').disabled,false);
const startup=script.slice(script.lastIndexOf('\n  renderFamilyMenu();'));
assert(startup.includes('wifiCheckPermissions(true)'));
assert(!/wifiPermissionAction|host_permission|permission_location|permission_bluetooth/.test(startup));
run("wifiPermissionGuideShown=false;wifiShowPermissions({platform:'macos',location:'not_determined',bluetooth:'allowed'});wifiGuidePermissionsOnce()");
assert.equal(run('wifiPermissionGuideShown'),true);
const guidedCalls=calls.length;
run('wifiGuidePermissionsOnce()');assert.equal(calls.length,guidedCalls);

// Exercise the real edit/delete/save chain through production HTML and IPC.
let persistedWifi;
handlers.platform_wifi=({op,body})=>{
  if(op==='view')return {wifi:persistedWifi,differences:[],host:{state:'permission_required'}};
  if(op==='save'){persistedWifi={...persistedWifi,local_revision:persistedWifi.local_revision+1,targets:structuredClone(body.targets)};return persistedWifi;}
  throw new Error('Unexpected edit operation '+op);
};
const editFailures=[];
for(const scenario of ['delete-open-editor','rename-original']) {
  persistedWifi={...structuredClone(wifi),targets:[{...network,ssid:'Device-2G',password:'fixture-pass',version:7}]};
  run('wifiDrafts.clear()');select(macA);await run('wifiRefreshView()');run('wifiEdit(0)');
  if(scenario==='delete-open-editor')run('wifiDelete(0)');
  else {element('wifi-id').value='renamed-id';element('wifi-label').value='Renamed network';element('wifi-ssid').value='Renamed-2G';}
  await run('wifiSaveLocal()');
  try {
    assert.equal(persistedWifi.targets.length,1,'Editing must update the original row, not insert');
    assert.equal(persistedWifi.targets[0].network_id,'home','Stable identity must be preserved');
    assert.equal(persistedWifi.targets[0].password,'fixture-pass');
    if(scenario==='delete-open-editor')assert.equal(persistedWifi.targets[0].deleted,true,'Open editor must not resurrect a deletion');
    else assert.equal(persistedWifi.targets[0].label,'Renamed network');
    assert.equal(persistedWifi.targets[0].version,7,'Editing must preserve the observed version');
  }catch(e){editFailures.push(scenario+': '+e.message);}
}
assert.deepEqual(editFailures,[],'Wi-Fi edit regressions');
// Undo, cancelled confirmation and duplicate identifiers must preserve the row.
assert.equal(element('wifi-id').readOnly,true);
run('wifiDelete(0)');assert.equal(run('wifiDraft[0].deleted'),true);
run('wifiDelete(0)');await run('wifiSaveLocal()');assert.equal(persistedWifi.targets[0].deleted,false);
context.window.confirm=()=>false;run('wifiDelete(0)');assert.equal(run('wifiDraft[0].deleted'),false);context.window.confirm=()=>true;
run('wifiCancelEdit();wifiNew()');assert.equal(element('wifi-id').readOnly,false);
element('wifi-id').value='home';element('wifi-host-ssid').value='Host 5G';element('wifi-ssid').value='Device';
assert.equal(run('wifiSubmit()'),false);assert.match(element('wifi-result').textContent,/标识已存在/);
assert.equal(run('wifiDraft.length'),1);
// The user's real cache shape: imported from a different binding, hence read-only.
persistedWifi={...structuredClone(wifi),targets:[{...network,origin:'other-bond',credential:'device',password:undefined,version:7}]};
run('wifiDrafts.clear()');select(macA);await run('wifiRefreshView()');
run('wifiEdit(0)');
element('wifi-label').value='Edited imported network';element('wifi-password').value='fixture-new-pass';
await run('wifiSaveLocal()');
assert.equal(persistedWifi.targets.length,1);
assert.equal(persistedWifi.targets[0].network_id,network.network_id);
assert.equal(persistedWifi.targets[0].origin,'local');
assert.equal(persistedWifi.targets[0].version,7);
assert.equal(persistedWifi.targets[0].label,'Edited imported network');
assert.equal(persistedWifi.targets[0].password,'fixture-new-pass');
assert.match(element('wifi-save-result').textContent,/已保存/);
console.log('Imported Bridge target: direct edit saved under original identity, nearby feedback OK');

console.log('Wi-Fi edits: stable identity, rename, delete/save, undo, cancelled delete, duplicate protection and version/password retention OK');

console.log('permission UI: passive startup, bounded request follow-up, denied/settings, global disable, real-grant visibility OK');
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

assert.match(run("wifiError('revision_conflict')"),/缓存已过期.*重新读取设备/);
