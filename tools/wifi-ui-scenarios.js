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
  wifiLibrary={revision:0,families:[],devices:{},wired_family:null};wifiManualSelection=false;$('wifi-family-editor').open=false;wifiFamilyRender();
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
  wifiLibrary={revision:3,families:scenario==='empty'?[]:[{id:'home',name:'Home-5G',ssid_2g:'Home-2G',ssid_5g:'Home-5G',password_2g:'fixture-pass',password_5g:'fixture-pass'}],devices:{[mac]:scenario==='empty'?[]:['home']},wired_family:null};wifiFamilyRender();
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
  if (['family-menu-refresh','ap01-family-menu','154-family-menu'].includes(scenario)) {
    document.querySelectorAll('.panel').forEach(p => p.classList.toggle('on', p.id === 'tab-templates'));
    const target=scenario==='ap01-family-menu'?RENDER_TARGET_AP01:scenario==='154-family-menu'?RENDER_TARGET_154G:RENDER_TARGET_NOTE4;
    const templateId=target===RENDER_TARGET_154G?'quad':'codex-status-a';
    const profileName=scenario==='ap01-family-menu'?'AP01 Codex 额度':scenario==='154-family-menu'?'1.54 Codex 额度':'Note4 Codex 额度';
    selectedRenderTarget=target;selectedFamilyProfileId=null;
    renderFamilyMenu();
    document.querySelectorAll('.tabs button').forEach(b=>b.classList.toggle('on',b.dataset.tab==='templates'));
    $('sub').textContent='原生 WKWebView · 配置菜单回归（合成 IPC）';
    platformTemplates=[{template_id:templateId,render_target:target}];
    let profiles=[];
    window.__wifiRoundtripInvoke=async(name,args)=>{
      if(name==='platform_family_profiles')return {profiles};
      return {templates:platformTemplates};
    };
    try {
      await refreshProfiles();
      check($('menu-profile').textContent.includes('此族暂无 Profile'),'expected initial empty cache');
      profiles=[{id:'codex-status',name:profileName,render_target:target,
        template_ids:[templateId],sync_enabled:true}];
      closeMenus();
      await toggleMenu('menu-profile',{stopPropagation(){}});
      await wait();
      check($('menu-profile').textContent.includes(profileName),'menu kept stale empty cache');
      check($('profile-rows').textContent.includes(templateId),'template row missing');
      return {scenario,width:innerWidth};
    } finally {window.__wifiRoundtripInvoke=null;}
  }
  if (scenario === 'ble-recovery') {
    const fixture = {device:{device_mac:'7C4FADB93408',identity:{name:'Note4'},
      contact:{label:'蓝牙异常，正在自动恢复',ble_recovery:{state:'recovering',attempt:2},wifi_sampled_at:Math.floor(Date.now()/1000)-60},
      firmware:{current:{value:'0.18.104-note4'},latest:{},label:'运行版本高于登记版本'},
      display:{installed:{value:1},active_template:{value:true},data_applied:{value:true},screen_state:{value:'displayed'},label:'屏幕已显示'},
      battery:{value:73},delivery:{enabled:true,required_fields:7},occupancy:{label:'上次观察为已占用'}}};
    selectedDeviceMac='7C4FADB93408';deviceSelectionGeneration++;
    window.__wifiRoundtripInvoke=async(name)=>{
      check(name==='platform_device_view','Recovery view must stay passive');return fixture;
    };
    try { await refreshDevice(); } finally {window.__wifiRoundtripInvoke=null;}
    check($('device-state').textContent.includes('正在自动恢复'),'Missing recovery status');
    check($('dev-sample-note').textContent.includes('第 2 次'),'Missing recovery attempt');
    $('device-state').closest('.card').scrollIntoView({block:'start'});
    await wait();await wait();
    return {scenario,width:innerWidth};
  }
  if (scenario === 'compare') $('wifi-direction').value='import';
  wifiRenderScheme(); wifiRenderComparison(); wifiRenderJobs();
  check(!document.querySelector('[onclick="wifiRefreshImport()"]'),'Cached import must not expose a combined read action');

  check(document.querySelectorAll('.wifi-tabs [role="tab"]').length === 3,'Missing three Wi-Fi tabs');
  const callsBefore = window.__uiCalls.length;
  $('wifi-family-name').value = 'draft-retained';
  for (const tab of ['bridge','device','compare','bridge']) {
    $('wifi-tab-'+tab).click();
    await wait();
    check(document.querySelectorAll('.wifi-panel:not([hidden])').length === 1,'Multiple Wi-Fi panels visible');
    check(!$('wifi-panel-'+tab).hidden,'Selected panel is hidden');
    check($('wifi-tab-'+tab).getAttribute('aria-selected') === 'true','Missing selected tab semantics');
    check($('wifi-family-name').value === 'draft-retained','Tab switch discarded editor input');
  }
  $('wifi-tab-bridge').dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));
  check(!$('wifi-panel-device').hidden,'ArrowRight did not switch tab');
  $('wifi-tab-device').dispatchEvent(new KeyboardEvent('keydown',{key:'End',bubbles:true}));
  check(!$('wifi-panel-compare').hidden,'End did not select final tab');
  $('wifi-tab-compare').dispatchEvent(new KeyboardEvent('keydown',{key:'Home',bubbles:true}));
  check(!$('wifi-panel-bridge').hidden,'Home did not select first tab');
  check(window.__uiCalls.length === callsBefore,'Tab switching contacted backend');

  $('wifi-family-editor').open=true;await wait();
  $('wifi-family-pass2').value='fixture-pass';
  check($('wifi-family-pass2').type==='password','Password must default to hidden');
  $('wifi-family-toggle2').click();check($('wifi-family-pass2').type==='text','Eye failed to reveal password');
  check($('wifi-family-toggle2').getAttribute('aria-pressed')==='true','Eye missing accessible hide action');
  $('wifi-family-toggle2').click();check($('wifi-family-pass2').type==='password','Eye failed to hide password');
  const fields = [...document.querySelectorAll('#wifi-family-editor .wifi-field')];
  check(fields.length === 5,'Editor needs five separate family field groups');
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
    const a=fields[1].getBoundingClientRect(),b=fields[2].getBoundingClientRect();
    check(b.left>=a.right && Math.abs(b.top-a.top)<1,'Wide window needs a two-column editor');
  }
  check(!$('wifi-compare').textContent.includes('字段选择'),'Field-by-field selection returned');
  if (scenario !== 'empty') {
    wifiFamilyEdit(0);
    $('wifi-family-name').value = '未提交表单';
    wifiChoose(0,true); wifiPreviewSync();
    const selected = JSON.stringify([...wifiSelections]);
    const preview = JSON.stringify(wifiPreview);
    $('wifi-tab-device').click(); $('wifi-tab-compare').click(); $('wifi-tab-bridge').click();
    check($('wifi-family-name').value === '未提交表单','Tab switch lost edited text');
    check(JSON.stringify([...wifiSelections]) === selected,'Tab switch lost field selections');
    check(JSON.stringify(wifiPreview) === preview,'Tab switch invalidated preview');
  }
  if (scenario !== 'bridge') $('wifi-family-editor').open=false;
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
  } else if (scenario.startsWith('endpoint-')) {
    const unknown=scenario==='endpoint-unknown';
    wifiShowResult({outcome:unknown?'unknown':'rejected',stage:unknown?'ack':'persist',
      error:unknown?'window_budget_exhausted':'storage',
      action:unknown?'read_registration_before_retry':'offline_migration_or_storage_repair'});
    check($('wifi-result').textContent.includes(unknown?'设备登记结果未知':'设备登记失败'),'Missing endpoint outcome');
    check($('wifi-result').textContent.includes(unknown?'请勿重复提交':'离线迁移'),'Missing endpoint action');
  } else if (scenario.startsWith('failure-')) {
    const errors={sent:['not_sent','未发送'],unknown:['result_unknown','勿重复提交'],network:['network_not_found','未找到网络'],budget:['window_budget_exhausted','预算已耗尽']};
    const [code,label]=errors[scenario.slice(8)];
    wifiState.jobs=[{job_id:'failure-proof',device_mac:mac,kind:'sync',state:code==='result_unknown'?'awaiting_confirmation':'waiting_rendezvous',
      sent:code==='result_unknown',may_have_committed:code==='result_unknown',error_code:code,created_at:now,attempt_count:1}];
    wifiRenderJobs();wifiShowTab('device');
    check($('wifi-current-jobs').textContent.includes(label),'Missing classified failure '+code);
  } else if (scenario === 'job-blocked') {
    wifiState.jobs=[{job_id:'blocked',device_mac:mac,attempt_count:3,last_stage:'query_result',kind:'activate',state:'needs_attention',error_code:'transaction_identity_unknown',created_at:now,sent:true,may_have_committed:true},
      {job_id:'cancelled',device_mac:mac,attempt_count:2,last_stage:'query_result',kind:'sync',state:'cancelled',created_at:now,sent:true,may_have_committed:true}];
    wifiRenderJobs();wifiShowTab('device');$('wifi-job-history').open=true;
    check($('wifi-current-jobs').textContent.includes('已停止自动重试'),'Blocked task still looks runnable');
    check($('wifi-current-jobs').textContent.includes('缺少原事务身份'),'Missing actionable root cause');
    check(!$('wifi-current-jobs').textContent.includes('同步到设备'),'Cancelled task revived');
    check($('wifi-jobs').textContent.includes('原操作结果未知'),'Cancellation must preserve unknown result');
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
    wifiSetResult('Bridge 配置已保存。');await wifiRefreshView();
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
    check(taskText.includes('fixture-job'),'Task ID missing from current/history row');
    check(!$('wifi-job-history').open,'Execution history must default to collapsed');
    if(scenario==='error')$('wifi-job-history').open=true;
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
  if(scenario.startsWith('endpoint-'))$('wifi-result').scrollIntoView({block:'center'});
  if(scenario.startsWith('current-')) {
    const kind=scenario.slice(8),state={missing:'no_mapping',connecting:'connecting',error:'connection_failed',ready:'connected',unreachable:'bridge_unreachable'}[kind];
    wifiRenderCurrent({automatic_state:state,matched_target:kind==='missing'?null:network,
      connection:{state,error:kind==='error'?'authentication_failed':kind==='unreachable'?'http_unavailable':null,http_ready:kind==='ready'}});
    check($('wifi-auto').textContent.includes({missing:'选择对应族',connecting:'正在连接',error:'认证失败',ready:'Bridge HTTP 已确认',unreachable:'无法访问设备 HTTP'}[kind]),'Incorrect current-network guidance');
    if(kind==='missing') {
      wifiEditCurrent();
      check($('wifi-family-name').value==='Home-5G','Current host was not prefilled');
      $('wifi-family-editor').open=false;
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
  if(scenario.startsWith('wifi-family-')) {
    wifiLibrary={revision:3,families:[{id:'home',name:'Home-5G',ssid_2g:'Home-2G',ssid_5g:'Home-5G',password_2g:'fixture-pass',password_5g:'fixture-pass'},
      {id:'office',name:'Office',ssid_2g:'Office',ssid_5g:'Office',password_2g:'',password_5g:''}],devices:{[mac]:['home','office']},wired_family:'office'};
    wifiManualSelection=scenario==='wifi-family-ap01';wifiFamilyRender();wifiShowTab('bridge');
    wifiShowHost({state:scenario==='wifi-family-wired'?'no_wifi':'connected',family_name:scenario==='wifi-family-wired'?'Office':'Home-5G',networks:scenario==='wifi-family-wired'?[]:[{ssid:'Home-2G'}]});
    wifiRenderCurrent({automatic_state:wifiManualSelection?'manual_selection':'waiting_for_window',matched_target:network});
    wifiRenderScheme();
    check($('wifi-family-list').querySelectorAll('input:checked').length===2,'Family selections missing');
    check($('wifi-wired-family').value==='office','Wired family missing');
    const familyBounds=$('wifi-family-library').getBoundingClientRect();
    for(const control of $('wifi-family-library').querySelectorAll('input,select,button')){if(!control.getClientRects().length)continue;const r=control.getBoundingClientRect();check(r.left>=familyBounds.left-1&&r.right<=familyBounds.right+1,'Family control exceeds library: '+control.id);}
    if(wifiManualSelection) {
      check(!$('wifi-ap01-auth').hidden&&$('wifi-note-recovery').hidden&&$('wifi-current-action').hidden,'AP01 automatic network controls exposed');
      check(!$('wifi-networks').innerHTML.includes('wifiSelect('),'AP01 must select network on device');
    } else {
      wifiFamilyEdit(0);check($('wifi-family-name').value==='Home-5G','Family editor missing');
      check($('wifi-family-pass2').type==='password','Family password exposed');
    }
  }
  if(['rename','delete','save-error','imported-edit','roundtrip-own','roundtrip-other'].includes(scenario)) {
    const originalInvoke=window.__wifiRoundtripInvoke,stored=structuredClone(wifiState);
    let library=structuredClone(wifiLibrary),readCalls=0,syncCalls=0;
    const foreign=scenario==='roundtrip-other';
    if(foreign)stored.snapshot.networks[0]={...stored.snapshot.networks[0],bond:'other',writable:false};
    const differences=()=>[{network_id:'home',state:'bridge_changed',bridge:stored.targets[0],device:stored.snapshot.networks.filter(r=>r.network.network_id==='home')}];
    window.__wifiRoundtripInvoke=async(name,args)=>{
      window.__uiCalls.push({name,args});
      if(name!=='platform_wifi')throw Error('Unexpected IPC '+name);
      if(args.op==='view')return {wifi:stored,library,differences:differences(),host:{state:'connected',family_name:'Home-5G',networks:[{ssid:'Home-5G'}]}};
      if(args.op==='library_save'){
        if(scenario==='save-error')throw 'wifi_library_revision_conflict';
        library={...library,families:structuredClone(args.body.families),revision:library.revision+1};
        if(library.families[0])stored.targets=[{...stored.targets[0],label:library.families[0].name,password:library.families[0].password_2g}];
        stored.local_revision++;return library;
      }
      if(args.op==='library_import'){check(args.body.snapshot_id===stored.snapshot.snapshot_id,'Import lost snapshot fence');readCalls++;return {};}
      if(args.op==='sync'){
        check(args.body.selected.length===1&&args.body.selected[0]==='home','Sync lost selection');
        check(stored.targets[0].label==='重整后的共享族','Sync lost shared edit');
        const row={bond:'own',writable:true,network:{...stored.targets[0],version:8}};
        if(foreign)stored.snapshot.networks.push(row);else stored.snapshot.networks[0]=row;
        syncCalls++;return {job_id:'shared-roundtrip',state:'completed'};
      }
      throw Error('Unexpected operation '+args.op);
    };
    try {
      wifiShowTab('bridge');wifiFamilyEdit(0);
      $('wifi-family-name').value='重整后的共享族';
      document.querySelector('#wifi-family-editor button[type=submit]').click();await wait();
      if(scenario==='save-error') {
        check($('wifi-family-editor').open&&$('wifi-family-name').value==='重整后的共享族','Conflict discarded editor draft');
        check($('wifi-family-result').textContent.includes('已变化'),'Conflict feedback missing');
      } else {
        check(library.families[0].id==='home'&&library.families[0].name==='重整后的共享族','Shared edit changed identity or lost name');
        check(!$('wifi-family-editor').open,'Successful save must close editor');
      }
      if(scenario==='delete') {
        await wifiFamilyDelete(0);check(library.families.length===1,'Subscribed family was deleted');
        library.devices[mac]=[];wifiLibrary=library;await wifiFamilyDelete(0);check(library.families.length===0,'Unsubscribed family could not be deleted');
      }
      if(['imported-edit','roundtrip-own','roundtrip-other'].includes(scenario)) {
        wifiShowTab('compare');$('wifi-direction').value='import';wifiRenderComparison();
        check(!$('wifi-family-import').hidden,'Import destination missing from compare panel');
        wifiSelections.clear();wifiSelections.set('home',true);$('wifi-import-family').value='home';
        document.querySelector('[onclick="wifiFamilyImport()"]').click();await wait();
        check(readCalls===1,'Cached family import not dispatched');
        $('wifi-direction').value='sync';wifiSelections.clear();wifiRenderComparison();check($('wifi-family-import').hidden,'Import control visible in sync direction');
        if(scenario.startsWith('roundtrip-')){
          const box=$('wifi-compare').querySelector('input[type=checkbox]');check(!box.disabled,'Shared target cannot sync');box.click();
          document.querySelector('[onclick="wifiPreviewSync()"]').click();check(!$('wifi-sync-submit').disabled,'Shared sync preview blocked');
          $('wifi-sync-submit').click();await wait();check(syncCalls===1,'Shared roundtrip did not sync');
          check(stored.snapshot.networks.some(r=>r.writable&&r.network.label==='重整后的共享族'),'Roundtrip did not update current binding');
          if(foreign)check(stored.snapshot.networks[0].network.label===network.label,'Roundtrip changed foreign source');
        }
      }
    } finally {window.__wifiRoundtripInvoke=originalInvoke;}
  }
  const card = $('wifi-card'), bounds=card.getBoundingClientRect();
  check(card.scrollWidth <= card.clientWidth+1,'Wi-Fi card overflows horizontally');
  for (const node of card.querySelectorAll('input:not([type=checkbox]),select,button,.pmeta')) {
    if (!node.getClientRects().length) continue;
    const r=node.getBoundingClientRect();
    check(r.left >= bounds.left-1 && r.right <= bounds.right+1,'Control exceeds Wi-Fi card: '+(node.id||node.textContent));
  }
  check(window.__uiErrors.length === 0,'JavaScript errors: '+window.__uiErrors.join('; '));
  if(scenario.startsWith('endpoint-'))$('wifi-result').scrollIntoView({block:'center'});
  else if(scenario==='save-error')$('wifi-family-result').scrollIntoView({block:'center'});
  else if(scenario==='waiting'||scenario==='error'||scenario==='stale-cache'||scenario.startsWith('job-feedback')||scenario==='job-blocked'||scenario.startsWith('failure-'))$('wifi-current-jobs').closest('.wifi-tasks').scrollIntoView({block:'start'});
  else (scenario==='delivery-alignment'?$('pt-sync').closest('.card'):card).scrollIntoView({block:'start'});
  await wait();
  return {scenario,width:innerWidth,visiblePanel:document.querySelector('.wifi-panel:not([hidden])').id,
    cardWidth:Math.round(bounds.width),backendCallsDuringTabSwitch:window.__uiCalls.length-callsBefore};
};
