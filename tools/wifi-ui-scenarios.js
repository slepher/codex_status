// Synthetic, non-secret UI scenarios; loaded only by the native rendering test.
window.__wifiUiScenario = async function(scenario) {
  const check = (ok, message) => { if (!ok) throw new Error(message); };
  const wait = () => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  window.__wifiTestState=null;window.confirm=()=>true;
  const mac = '0200000000A1';
  const now=Math.floor(Date.now()/1000);
  document.querySelectorAll('.panel').forEach(p => p.classList.toggle('on', p.id === 'tab-device'));
  selectedDeviceMac = mac; deviceSelectionGeneration++;
  clearDeviceView();
  const network = {network_id:'home', label:'书桌网络', host_ssid:'Home-5G', ssid:'Home-2G',
    password:'fixture-pass',credential:'keep', origin:'local', version:2, deleted:false};
  const long = 'Long-Network-Name-'.repeat(6);
  const own = {bond:'own',writable:true,network};
  const other = {bond:'other',writable:false,network:{...network,network_id:'travel',label:'其他绑定的网络',host_ssid:long,ssid:long}};
  const legacy = {bond:'legacy',writable:false,network:{...network,network_id:'legacy-0',label:'USB 保存的旧网络'}};
  wifiState = {local_revision:3,targets:scenario === 'empty' ? [] : [network],
    saved_at:100, jobs:[], snapshot:scenario === 'empty' ? null : {
      complete:true,revision:7,snapshot_id:'fixture-7',observed_at:100,
      active:{network_id:'home',version:1},capacity:{total:12,per_bond:4},networks:[own,other,legacy]}};
  wifiScheme = wifiState.snapshot;
  wifiDraft = structuredClone(wifiState.targets); wifiDirty = false;
  wifiDifferences = scenario === 'empty' ? [] : [{
    network_id:'home',state:'bridge_changed',writable:true,bridge:network,device:[own],
    fields:[{field:'ssid',bridge:'Home-2G',device:'Old-2G',conflict:false}]}];
  wifiSelections.clear(); wifiPreview = null;
  $('wifi-target').textContent = 'Note4 · ' + mac;
  $('wifi-auto').textContent = scenario==='empty'?'活动网络未知 · 自动选网使用已确认映射':'活动网络 Home-2G · 自动选网使用已确认映射';
  $('wifi-times').textContent = scenario==='empty'?'Bridge 尚无目标 · 尚未读取完整设备方案':'Bridge 已保存 · 设备完整快照 r7';
  wifiShowHost({state:scenario === 'empty' ? 'permission_required' : 'connected',networks:[{ssid:'Home-5G'}]});
  wifiRenderCurrent({automatic_state:scenario==='empty'?'permission_required':'waiting_for_window',matched_target:scenario==='empty'?null:network});
  wifiShowPermissions({platform:'macos',location:scenario==='permission-request'?'not_determined':
    scenario==='permission-settings'?'disabled':'allowed',bluetooth:scenario==='permission-request'?'not_determined':
    scenario==='permission-settings'?'denied':'allowed'});
  if(scenario.startsWith('permission-')) {
    check(!$('wifi-permissions').hidden,'Missing actionable permission setup');
    wifiPermissionGuideShown=false;
    const guideCalls=window.__uiCalls.length;
    wifiGuidePermissionsOnce();
    check($('tab-device').classList.contains('on'),'Permission guide did not select device page');
    $('tab-device').classList.remove('on');
    wifiGuidePermissionsOnce();
    check(!$('tab-device').classList.contains('on'),'Permission guide repeated within the same app session');
    $('tab-device').classList.add('on');
    check(window.__uiCalls.length===guideCalls,'Permission guide unexpectedly requested permission');
    check($('wifi-location-note').textContent.length>10,'Missing location instructions');
    check($('wifi-bluetooth-note').textContent.length>10,'Missing Bluetooth instructions');
    check($('wifi-location-action').textContent.includes(scenario==='permission-request'?'授权':'设置'),'Incorrect location action');
  } else check($('wifi-permissions').hidden,'Granted permissions should hide setup');
  if (scenario === 'compare') $('wifi-direction').value='import';
  wifiRenderLocal(); wifiRenderScheme(); wifiRenderComparison(); wifiRenderJobs();
  check(!document.querySelector('[onclick="wifiRefreshImport()"]'),'Cached import must not expose a combined read action');

  check(document.querySelectorAll('.wifi-tabs [role="tab"]').length === 3,'Missing three Wi-Fi tabs');
  const callsBefore = window.__uiCalls.length;
  $('wifi-id').value = 'draft-retained';
  for (const tab of ['bridge','device','compare','bridge']) {
    $('wifi-tab-'+tab).click();
    await wait();
    check(document.querySelectorAll('.wifi-panel:not([hidden])').length === 1,'Multiple Wi-Fi panels visible');
    check(!$('wifi-panel-'+tab).hidden,'Selected panel is hidden');
    check($('wifi-tab-'+tab).getAttribute('aria-selected') === 'true','Missing selected tab semantics');
    check($('wifi-id').value === 'draft-retained','Tab switch discarded editor input');
  }
  $('wifi-tab-bridge').dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));
  check(!$('wifi-panel-device').hidden,'ArrowRight did not switch tab');
  $('wifi-tab-device').dispatchEvent(new KeyboardEvent('keydown',{key:'End',bubbles:true}));
  check(!$('wifi-panel-compare').hidden,'End did not select final tab');
  $('wifi-tab-compare').dispatchEvent(new KeyboardEvent('keydown',{key:'Home',bubbles:true}));
  check(!$('wifi-panel-bridge').hidden,'Home did not select first tab');
  check(window.__uiCalls.length === callsBefore,'Tab switching contacted backend');

  $('wifi-editor').open=true;await wait();
  $('wifi-password').value='fixture-pass';
  check($('wifi-password').type==='password','Password must default to hidden');
  $('wifi-password-toggle').click();check($('wifi-password').type==='text','Eye failed to reveal password');
  check($('wifi-password-toggle').getAttribute('aria-label')==='隐藏密码','Eye missing accessible hide action');
  $('wifi-password-toggle').click();check($('wifi-password').type==='password','Eye failed to hide password');
  const fields = [...document.querySelectorAll('.wifi-field')];
  check(fields.length === 6,'Editor needs six separate field groups');
  for (const group of fields) {
    const label = group.querySelector('label').getBoundingClientRect();
    const input = group.querySelector('input,select').getBoundingClientRect();
    check(input.top >= label.bottom - 1,'Input shares an inline row with its label');
    check(input.width > 100,'Input collapsed');
  }
  if (innerWidth <= 640) {
    const a=fields[0].getBoundingClientRect(), b=fields[1].getBoundingClientRect();
    check(b.top >= a.bottom,'Narrow window needs a single-column editor');
  }
  if (innerWidth > 640) {
    const a=fields[0].getBoundingClientRect(),b=fields[1].getBoundingClientRect();
    check(b.left>=a.right && Math.abs(b.top-a.top)<1,'Wide window needs a two-column editor');
  }
  check(!$('wifi-compare').textContent.includes('字段选择'),'Field-by-field selection returned');
  if (scenario !== 'empty') {
    wifiEdit(0);
    $('wifi-label').value = '未提交表单';
    wifiChoose(0,true); wifiPreviewSync();
    const selected = JSON.stringify([...wifiSelections]);
    const preview = JSON.stringify(wifiPreview);
    $('wifi-tab-device').click(); $('wifi-tab-compare').click(); $('wifi-tab-bridge').click();
    check($('wifi-label').value === '未提交表单','Tab switch lost edited text');
    check(JSON.stringify([...wifiSelections]) === selected,'Tab switch lost field selections');
    check(JSON.stringify(wifiPreview) === preview,'Tab switch invalidated preview');
  }
  if (scenario !== 'bridge') $('wifi-editor').open=false;
  if (scenario === 'device') {
    $('wifi-tab-device').click();
    const text=$('wifi-networks').textContent;
    check(text.includes('本机绑定') && text.includes('其他绑定') && text.includes('USB / AP'),'Missing bond/legacy group');
  } else if (scenario === 'compare') {
    $('wifi-tab-compare').click();
    check(!$('wifi-sync-submit').disabled,'Preview should enable scoped sync');
  } else if (scenario === 'waiting' || scenario === 'error') {
    wifiState.jobs=[{job_id:'fixture-job',device_mac:mac,kind:'read',state:scenario === 'waiting' ? 'waiting_rendezvous' : 'failed',
      attempt_count:1,last_stage:'connecting',error_code:scenario === 'error' ? 'unsupported' : '',
      expires_at:now+86400,last_attempt_at:now-60}];
    wifiRenderJobs();
    wifiShowResult({job_id:'fixture-job',state:wifiState.jobs[0].state,error_code:wifiState.jobs[0].error_code});
    check($(scenario==='waiting'?'wifi-current-jobs':'wifi-jobs').textContent.includes(scenario === 'waiting' ? '等待下次真实会合' : '不支持'),'Missing task/error state');
    check(!$('wifi-job-history').open,'Execution history must default to collapsed');
  }
  if(scenario==='rename'||scenario==='delete') {
    window.__wifiTestState=structuredClone(wifiState);
    $('wifi-tab-bridge').click();
    $('wifi-local').querySelector('button').click();
    check($('wifi-id').readOnly,'Existing row identifier must be read-only');
    if(scenario==='rename'){$('wifi-label').value='重新命名的网络';$('wifi-ssid').value='Renamed-2G';}
    else $('wifi-local').querySelectorAll('button')[2].click();
    const save=document.querySelector('[onclick="wifiSaveLocal()"]');
    check(save.getClientRects().length>0,'Save must remain visible for deletion');
    save.click();await wait();
    const targets=window.__wifiTestState.targets;
    check(targets.length===1&&targets[0].network_id==='home','Edit inserted an extra network');
    check(targets[0].version===2&&targets[0].password==='fixture-pass','Edit lost version/password');
    if(scenario==='rename')check(targets[0].label==='重新命名的网络'&&targets[0].ssid==='Renamed-2G','Rename lost changes');
    else {check(targets[0].deleted,'Editor resurrected deleted row');check($('wifi-local').textContent.includes('撤销删除'),'Deletion needs an undo action');}
    check(!window.__uiCalls.slice(callsBefore).some(c=>c.name==='platform_wifi'&&['get','sync','trial'].includes(c.args?.op)),'Local edit contacted device');
  }
  if(scenario.startsWith('current-')) {
    const kind=scenario.slice(8),state={missing:'no_mapping',connecting:'connecting',error:'connection_failed',ready:'connected',unreachable:'bridge_unreachable'}[kind];
    wifiRenderCurrent({automatic_state:state,matched_target:kind==='missing'?null:network,
      connection:{state,error:kind==='error'?'authentication_failed':kind==='unreachable'?'http_unavailable':null,http_ready:kind==='ready'}});
    check($('wifi-auto').textContent.includes({missing:'没有该网络',connecting:'正在连接',error:'认证失败',ready:'交接已确认',unreachable:'无法访问设备 HTTP'}[kind]),'Incorrect current-network guidance');
    if(kind==='missing') {
      wifiEditCurrent();
      check($('wifi-host-ssid').value==='Home-5G','Current host was not prefilled');
      $('wifi-editor').open=false;
    }
  }
  if(scenario==='sync-guard') {
    wifiScheme.networks=wifiScheme.networks.filter(r=>r.writable);
    wifiShowTab('compare');$('wifi-direction').value='sync';wifiRenderComparison();
    const saved=structuredClone(wifiState.targets);wifiState.targets[0].deleted=true;
    wifiDifferences[0].bridge=wifiState.targets[0];wifiChoose(0,true);wifiPreviewSync();
    check($('wifi-sync-submit').disabled&&$('wifi-preview').textContent.includes('保留或加入当前网络'),'Unsafe post-sync state was allowed');
    wifiState.targets=saved;wifiDifferences[0].bridge=saved[0];wifiPreviewSync();
    check(!$('wifi-sync-submit').disabled,'Retained current mapping was rejected');
    wifiState.targets[0].deleted=true;wifiDifferences[0].bridge=wifiState.targets[0];wifiRenderComparison();wifiPreviewSync();$('wifi-preview').scrollIntoView({block:'center'});
  }
  await wait();
  const card = $('wifi-card'), bounds=card.getBoundingClientRect();
  check(card.scrollWidth <= card.clientWidth+1,'Wi-Fi card overflows horizontally');
  for (const node of card.querySelectorAll('input:not([type=checkbox]),select,button,.pmeta')) {
    if (!node.getClientRects().length) continue;
    const r=node.getBoundingClientRect();
    check(r.left >= bounds.left-1 && r.right <= bounds.right+1,'Control exceeds Wi-Fi card: '+(node.id||node.textContent));
  }
  check(window.__uiErrors.length === 0,'JavaScript errors: '+window.__uiErrors.join('; '));
  card.scrollIntoView({block:'start'});
  await wait();
  return {scenario,width:innerWidth,visiblePanel:document.querySelector('.wifi-panel:not([hidden])').id,
    cardWidth:Math.round(bounds.width),backendCallsDuringTabSwitch:window.__uiCalls.length-callsBefore};
};
