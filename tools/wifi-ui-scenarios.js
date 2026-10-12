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
  wifiRenderOnline({observed_at:Date.now()/1000,presence:{mode:'continuous',remaining_ms:null,reasons:['usb']},transports:{http:{ready:true},ble:{ready:false}},wifi_control:{write_allowed:true}});
  check($('wifi-online').textContent.includes('USB 常驻在线') && $('wifi-online').textContent.includes('BLE 关闭'),'Missing independent transport availability');
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
    check($('wifi-permission-action').textContent.includes(scenario==='permission-request'?'授权':'设置'),'Incorrect location action');
    check(!document.getElementById('wifi-location-action')&&!document.getElementById('wifi-bluetooth-action'),'Separate permission buttons returned');
    let permission={platform:'macos',location:scenario==='permission-request'?'not_determined':'disabled',bluetooth:scenario==='permission-request'?'not_determined':'denied'};
    const permissionCalls=[];
    window.__wifiRoundtripInvoke=async(name,args)=>{
      check(name==='platform_wifi','Unexpected permission IPC');
      permissionCalls.push(args.op);
      if(args.op==='permissions')return permission;
      if(args.op==='permission_all'){permission={platform:'macos',location:'allowed',bluetooth:'allowed'};return permission;}
      if(args.op==='settings_permissions')return {opened:true};
      if(args.op==='host')return {state:'connected',networks:[{ssid:'Home-5G'}]};
      throw Error('Unexpected permission operation '+args.op);
    };
    try {
      wifiPermissionStartupChecked=false;
      await wifiInitPermissions();await wifiInitPermissions();
      if(scenario==='permission-request') {
        check(permissionCalls.filter(op=>op==='permission_all').length===1,'Startup must request once');
        check($('wifi-permissions').hidden,'Successful combined grant must hide setup');
      } else {
        check(!permissionCalls.some(op=>op.startsWith('permission_')||op.startsWith('settings_')),'Startup repeated a denied prompt');
        $('wifi-permission-action').click();await wait();
        check(permissionCalls.filter(op=>op==='settings_permissions').length===1,'Combined settings button failed');
      }
    } finally {window.__wifiRoundtripInvoke=null;}
    wifiShowPermissions({platform:'macos',location:scenario==='permission-request'?'not_determined':'disabled',bluetooth:scenario==='permission-request'?'not_determined':'denied'});
    if(scenario==='permission-request')$('wifi-permission-result').textContent='';

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
    check(text.includes('设备配置') && text.includes('设备配置（需占用）') && text.includes('USB / AP'),'Missing device/legacy group');
  } else if (scenario === 'compare') {
    $('wifi-tab-compare').click();
    check(!$('wifi-sync-submit').disabled,'Preview should enable scoped sync');
  } else if (scenario === 'stale-cache') {
    wifiState.jobs=[{job_id:'fresh-stale-job',device_mac:mac,kind:'sync',state:'conflict',attempt_count:0,
      last_stage:'accepted',error_code:'revision_conflict',created_at:now,may_have_committed:false}];
    wifiRenderJobs();wifiShowResult(wifiState.jobs[0]);$('wifi-job-history').open=true;
    check($('wifi-current-jobs').textContent.includes('需处理冲突'),'Missing terminal conflict');
    check($('wifi-current-jobs').textContent.includes('缓存已过期') && $('wifi-current-jobs').textContent.includes('重新读取设备'),'Missing actionable stale-cache feedback');
    check(!$('wifi-current-jobs').textContent.includes('待确认'),'Rejected new job must not remain pending');
    check($('wifi-result').textContent.includes('Bridge 配置已保留'),'Local edits must stay preserved');
  } else if (scenario.startsWith('failure-')) {
    const errors={sent:['not_sent','未发送'],unknown:['result_unknown','勿重复提交'],network:['network_not_found','未找到网络'],budget:['window_budget_exhausted','预算已耗尽']};
    const [code,label]=errors[scenario.slice(8)];
    wifiState.jobs=[{job_id:'failure-proof',device_mac:mac,kind:'sync',state:code==='result_unknown'?'awaiting_confirmation':'waiting_rendezvous',
      sent:code==='result_unknown',may_have_committed:code==='result_unknown',error_code:code,created_at:now,attempt_count:1}];
    wifiRenderJobs();wifiShowTab('device');
    check($('wifi-current-jobs').textContent.includes(label),'Missing classified failure '+code);
  } else if (scenario === 'job-feedback' || scenario === 'job-feedback-error') {
    const state=structuredClone(wifiState);
    const job={job_id:'read-feedback',device_mac:mac,kind:'read',state:'queued',last_stage:'accepted',created_at:now,attempt_count:0};
    state.jobs=[job];
    window.__wifiRoundtripInvoke=async(name,args)=>{
      window.__uiCalls.push({name,args});check(name==='platform_wifi','Unexpected job-feedback IPC');
      if(args.op==='get')return structuredClone(job);
      check(args.op==='view','Unexpected job-feedback operation');
      return {wifi:structuredClone(state),differences:[],host:{state:'connected',networks:[{ssid:'Home-5G'}]}};
    };
    wifiShowTab('device');
    $('wifi-panel-device').querySelector('[onclick="wifiReadScheme()"]').click();
    await wait();
    check($('wifi-result').textContent.includes('已排队'),'Missing initial queue acknowledgement');
    job.state='reading';job.last_stage='snapshot';job.attempt_count=1;
    await wifiRefreshView();
    check($('wifi-result').textContent.includes('正在读取'),'Task progress did not update feedback');
    job.state=scenario==='job-feedback'?'completed':'failed';
    job.last_stage='complete_snapshot';job.error_code=scenario==='job-feedback'?null:'unsupported';
    await wifiRefreshView();
    check(!$('wifi-result').textContent.includes('已排队')&&!$('wifi-result').textContent.includes('accepted'),'Stale queue acknowledgement survived terminal refresh');
    check($('wifi-result').textContent.includes(scenario==='job-feedback'?'设备方案已读取':'不支持'),'Missing final success/failure feedback');
    check($('wifi-current-jobs').textContent==='暂无进行中的任务','Terminal task still shown as current');
    check(!$('wifi-job-history').open,'Completion unexpectedly expanded history');
    wifiLocalResult('Bridge 配置已保存。');await wifiRefreshView();
    check($('wifi-result').textContent==='Bridge 配置已保存。','Old task overwrote a newer local operation');
    wifiShowResult(job);window.__wifiRoundtripInvoke=null;
  } else if (scenario === 'waiting' || scenario === 'error') {
    wifiState.jobs=[{job_id:'fixture-job',device_mac:mac,kind:'read',state:scenario === 'waiting' ? 'waiting_rendezvous' : 'failed',
      attempt_count:1,last_stage:'connecting',error_code:scenario === 'error' ? 'unsupported' : '',
      created_at:1791121712,expires_at:now+86400,last_attempt_at:now-60}];
    wifiRenderJobs();
    wifiShowResult({job_id:'fixture-job',state:wifiState.jobs[0].state,error_code:wifiState.jobs[0].error_code});
    check($(scenario==='waiting'?'wifi-current-jobs':'wifi-jobs').textContent.includes(scenario === 'waiting' ? '等待设备可用通道' : '不支持'),'Missing task/error state');
    const taskText=$(scenario==='waiting'?'wifi-current-jobs':'wifi-jobs').textContent;
    check(taskText.includes('创建时间 '+wifiCreatedTime(1791121712)),'Task creation timestamp missing');
    check(!$('wifi-job-history').open,'Execution history must default to collapsed');
    if(scenario==='error')$('wifi-job-history').open=true;
  }
  if(scenario==='rename'||scenario==='delete') {
    window.__wifiTestState=structuredClone(wifiState);
    $('wifi-tab-bridge').click();
    $('wifi-local').querySelector('button').click();
    check($('wifi-id').readOnly,'Existing row identifier must be read-only');
    if(scenario==='rename'){$('wifi-label').value='重新命名的网络';$('wifi-ssid').value='Renamed-2G';}
    else $('wifi-local').querySelectorAll('button')[2].click();
    const save=$('wifi-submit');
    check(save.getClientRects().length>0,'Save must remain visible for deletion');
    save.click();await wait();
    const targets=window.__wifiTestState.targets;
    check(targets.length===1&&targets[0].network_id==='home','Edit inserted an extra network');
    check(targets[0].version===2&&targets[0].password==='fixture-pass','Edit lost version/password');
    if(scenario==='rename')check(targets[0].label==='重新命名的网络'&&targets[0].ssid==='Renamed-2G','Rename lost changes');
    else {check(targets[0].deleted,'Editor resurrected deleted row');check($('wifi-local').textContent.includes('撤销删除'),'Deletion needs an undo action');}
    check(!window.__uiCalls.slice(callsBefore).some(c=>c.name==='platform_wifi'&&['get','sync','trial'].includes(c.args?.op)),'Local edit contacted device');
  }
  if(scenario==='roundtrip-own'||scenario==='roundtrip-other') {
    const foreign=scenario==='roundtrip-other';
    const remote={bond:foreign?'other':'own',writable:!foreign,network:{...structuredClone(network),password:foreign?undefined:network.password}};
    const stored={local_revision:0,saved_at:100,targets:[],jobs:[],snapshot:{...structuredClone(wifiScheme),networks:[remote]}};
    const differences=()=>[{network_id:'home',state:stored.targets.length?'bridge_changed':'device_only',writable:!foreign,
      bridge:stored.targets[0]||null,device:stored.snapshot.networks.filter(r=>r.network.network_id==='home'),fields:[]}];
    window.__wifiRoundtripInvoke=async(name,args)=>{
      window.__uiCalls.push({name,args});check(name==='platform_wifi','Unexpected roundtrip IPC');
      if(args.op==='view')return {wifi:structuredClone(stored),differences:differences(),host:{state:'connected',networks:[{ssid:network.host_ssid}]}};
      if(args.op==='import'){stored.targets=[{...structuredClone(remote.network),origin:'local',credential:'device'}];stored.local_revision++;return structuredClone(stored);}
      if(args.op==='save'){stored.targets=structuredClone(args.body.targets);stored.local_revision++;return structuredClone(stored);}
      if(args.op==='sync'){
        check(args.body.selected.length===1&&args.body.selected[0]==='home','Roundtrip selection lost identity');
        check(stored.targets[0].label==='回传修改'&&stored.targets[0].password==='fixture-updated-pass','Sync did not use edited saved target');
        const row={bond:'own',writable:true,network:{...structuredClone(stored.targets[0]),version:8}};
        if(foreign)stored.snapshot.networks.push(row);else stored.snapshot.networks[0]=row;
        return {job_id:'roundtrip-job',state:'completed'};
      }
      throw Error('Unexpected roundtrip operation '+args.op);
    };
    wifiDirty=false;wifiSelections.clear();wifiPreview=null;await wifiRefreshView();$('wifi-tab-compare').click();$('wifi-direction').value='import';wifiRenderComparison();
    let box=$('wifi-compare').querySelector('input[type=checkbox]');check(!box.disabled,'Imported source must be selectable');box.click();
    document.querySelector('[onclick="wifiPreviewSync()"]') .click();$('wifi-sync-submit').click();await wait();
    check(stored.targets.length===1,'Device→Bridge click failed');
    $('wifi-tab-bridge').click();$('wifi-local').querySelector('button').click();
    $('wifi-label').value='回传修改';$('wifi-password').value='fixture-updated-pass';$('wifi-submit').click();await wait();
    check(stored.targets.length===1&&stored.targets[0].network_id==='home','Editing changed imported identity');
    $('wifi-tab-compare').click();$('wifi-direction').value='sync';wifiSelections.clear();wifiRenderComparison();
    box=$('wifi-compare').querySelector('input[type=checkbox]');check(!box.disabled,'Saved imported target cannot be selected for Bridge→device');
    box.click();check(wifiSelections.has('home'),'Real checkbox click failed to select the target');
    document.querySelector('[onclick="wifiPreviewSync()"]') .click();check(!$('wifi-sync-submit').disabled,'Roundtrip preview blocked');
    $('wifi-sync-submit').click();await wait();
    check(stored.snapshot.networks.some(r=>r.writable&&r.network.label==='回传修改'),'Roundtrip failed to save into current binding');
    if(foreign)check(stored.snapshot.networks[0].bond==='other'&&stored.snapshot.networks[0].network.label===network.label,'Roundtrip changed foreign source');
    box=$('wifi-compare').querySelector('input[type=checkbox]');check(!box.disabled,'Own+foreign same ID blocked subsequent updates');
    $('wifi-direction').value='import';wifiSelections.clear();wifiRenderComparison();
    box=$('wifi-compare').querySelector('input[type=checkbox]');check(!box.disabled,'Source row blocked importing the current target after sync');
    box.click();document.querySelector('[onclick="wifiPreviewSync()"]').click();check($('wifi-preview').textContent.includes('回传修改'),'Reimport preview used old source row');
    $('wifi-direction').value='sync';$('wifi-direction').dispatchEvent(new Event('change',{bubbles:true}));
    $('wifi-compare').querySelector('input[type=checkbox]').click();document.querySelector('[onclick="wifiPreviewSync()"]').click();
    if(foreign)check(!$('wifi-compare').textContent.includes('其他绑定'),'Comparison still displays the obsolete source next to the current target');
    window.__wifiRoundtripInvoke=null;
  }
  if(scenario==='delivery-alignment') {
    const input=$('pt-sync'),label=input.closest('label');
    const text=[...label.childNodes].find(n=>n.nodeType===Node.TEXT_NODE&&n.textContent.trim())||label.querySelector('span');
    const range=document.createRange();range.selectNodeContents(text);
    const a=input.getBoundingClientRect(),b=range.getBoundingClientRect();
    check(Math.abs((a.top+a.bottom-b.top-b.bottom)/2)<=2,'Delivery checkbox and label text are not vertically aligned');
    check(a.height<=20,'Text-field minimum height inflated native checkbox');
    label.closest('.card').scrollIntoView({block:'start'});await wait();
  }
  if(scenario==='imported-edit'||scenario==='save-error') {
    const original={...structuredClone(network),origin:'other-bond',credential:'device',password:undefined};
    wifiState.targets=[original];wifiDraft=structuredClone(wifiState.targets);wifiDirty=false;
    window.__wifiTestState=structuredClone(wifiState);wifiRenderLocal();
    $('wifi-tab-bridge').click();
    const edit=$('wifi-local').querySelector('button');
    check(edit.textContent==='编辑','Imported Bridge target must offer direct editing');
    const saveCalls=window.__uiCalls.filter(c=>c.name==='platform_wifi'&&c.args?.op==='save').length;
    edit.click();await wait();
    $('wifi-label').value='导入后修改的网络';$('wifi-password').value='fixture-new-pass';
    check($('wifi-submit').textContent==='保存到 Bridge','Editor must provide one formal save action');
    check(!document.querySelector('[onclick="wifiSaveLocal()"]'),'Outer save action must be removed');
    check(window.__uiCalls.filter(c=>c.name==='platform_wifi'&&c.args?.op==='save').length===saveCalls,'Editing must not auto-save');
    const save=$('wifi-submit');
    window.__wifiSaveError=scenario==='save-error'?'fixture_save_failed':null;
    save.click();await wait();window.__wifiSaveError=null;
    const targets=window.__wifiTestState.targets;
    if(scenario==='save-error') {
      check(targets.length===1&&wifiDirty,'Failed save must retain edits without changing stored targets');
      check($('wifi-save-result').textContent.includes('fixture_save_failed'),'Save error must be visible beside save button');
    } else {
      check(targets.length===1&&targets[0].origin==='local'&&targets[0].network_id===original.network_id,'Direct editing must retain row identity');
      check(targets[0].label==='导入后修改的网络'&&targets[0].password==='fixture-new-pass','Saved imported target lost edits');
      check($('wifi-save-result').textContent.includes('已保存'),'Save feedback missing beside button');
    }
    check(!window.__uiCalls.slice(callsBefore).some(c=>c.name==='platform_wifi'&&['get','sync','trial','current'].includes(c.args?.op)),'Local imported edit contacted device');
    $('wifi-editor').scrollIntoView({block:'start'});await wait();
  }
  if(scenario.startsWith('current-')) {
    const kind=scenario.slice(8),state={missing:'no_mapping',connecting:'connecting',error:'connection_failed',ready:'connected',unreachable:'bridge_unreachable'}[kind];
    wifiRenderCurrent({automatic_state:state,matched_target:kind==='missing'?null:network,
      connection:{state,error:kind==='error'?'authentication_failed':kind==='unreachable'?'http_unavailable':null,http_ready:kind==='ready'}});
    check($('wifi-auto').textContent.includes({missing:'没有该网络',connecting:'正在连接',error:'认证失败',ready:'Bridge HTTP 已确认',unreachable:'无法访问设备 HTTP'}[kind]),'Incorrect current-network guidance');
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
  if(scenario==='online-usb'||scenario==='online-boot') {
    wifiRenderOnline({observed_at:Date.now()/1000,presence:scenario==='online-usb'?{mode:'continuous',remaining_ms:null,reasons:['usb']}:{mode:'window',remaining_ms:120000,reasons:['boot']},transports:{http:{ready:true},ble:{ready:scenario==='online-boot'}},wifi_control:{write_allowed:true}});
    check($('wifi-online').textContent.includes(scenario==='online-usb'?'USB 常驻在线':'BOOT 在线'),'Missing presence reason');
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
  if(scenario==='imported-edit'||scenario==='save-error')$('wifi-save-result').scrollIntoView({block:'center'});
  else if(scenario==='waiting'||scenario==='error'||scenario==='stale-cache'||scenario.startsWith('job-feedback')||scenario.startsWith('failure-'))$('wifi-current-jobs').closest('.wifi-tasks').scrollIntoView({block:'start'});
  else (scenario==='delivery-alignment'?$('pt-sync').closest('.card'):card).scrollIntoView({block:'start'});
  await wait();
  return {scenario,width:innerWidth,visiblePanel:document.querySelector('.wifi-panel:not([hidden])').id,
    cardWidth:Math.round(bounds.width),backendCallsDuringTabSwitch:window.__uiCalls.length-callsBefore};
};
