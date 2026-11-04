// Production page functions -> test IPC transport -> real Rust app -> C++ HTTP.
// No Tauri window, production data directory, or physical device is used.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import vm from 'node:vm';
const endpoint=new URL(process.env.AP01_UI_RPC);assert.equal(endpoint.hostname,'127.0.0.1');
const html=readFileSync(new URL('../bridge/crates/app/ui/index.html',import.meta.url),'utf8');
const script=html.match(/<script>([\s\S]*?)<\/script>/)[1];
const source=script.slice(0,script.lastIndexOf('\n  renderFamilyMenu();'));
const elements=new Map();
const element=id=>{
  if(!elements.has(id))elements.set(id,{value:'',textContent:'',innerHTML:'',hidden:false,disabled:false,checked:false,files:[],
    classList:{add(){},remove(){},contains(){return false;}},focus(){},scrollIntoView(){},setAttribute(){},getAttribute(){return '';}});
  return elements.get(id);
};
const calls=[];let latest;
const invoke=async(name,args)=>{
  calls.push(args.body.op);
  const response=await fetch(endpoint,{method:'POST',headers:{'content-type':'application/json','x-test-token':process.env.AP01_UI_TOKEN},body:JSON.stringify({name,args}),signal:AbortSignal.timeout(15000)});
  assert(response.ok);const value=await response.json();latest=value;return value;
};
const context=vm.createContext({window:{__TAURI__:{core:{invoke}},confirm:()=>true},
  document:{getElementById:element,querySelectorAll:()=>[],querySelector:()=>({classList:{contains:()=>true}}),addEventListener(){}},
  localStorage:{getItem:()=>null,setItem(){},removeItem(){}},setInterval(){},clearInterval(){},setTimeout,structuredClone,btoa});
vm.runInContext(source,context);const run=code=>vm.runInContext(code,context);
run("selectedDeviceMac='010203040506';deviceSelectionGeneration++;clearDeviceView()");element('app-update-card').hidden=false;
const file=path=>{const bytes=readFileSync(path);return {size:bytes.length,arrayBuffer:async()=>bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength)};};
element('app-update-manifest').files=[file(process.env.AP01_MANIFEST)];
element('app-update-packages').files=[process.env.AP01_PACKAGE,process.env.AP01_PEER].filter(Boolean).map(file);
element('app-update-code').value='01234567';await run('appUpdatePrepare()');
const operation=run('appUpdate.operation');assert.match(operation||'',/^[0-9a-f]{32}$/,'prepare: '+element('app-update-result').textContent);
assert(!calls.includes('run'));assert.equal(element('app-update-run').disabled,false);
await run("appUpdateAction('run')");assert.equal(run('appUpdate.active'),true,JSON.stringify(latest));assert.equal(element('app-update-code').value,'');
// Reopening the page loses transient inputs but must recover the same worker.
run('deviceSelectionGeneration++;clearDeviceView()');element('app-update-card').hidden=false;
element('app-update-manifest').files=[];element('app-update-packages').files=[];
await run('appUpdateRestore()');assert.equal(run('appUpdate.operation'),operation);
const deadline=Date.now()+130000;
while(run('appUpdate.active')){
  assert(Date.now()<deadline,'UI worker deadline');await new Promise(resolve=>setTimeout(resolve,30));await run('appUpdatePoll()');
}
assert.equal(latest.status?.result,process.env.AP01_EXPECT_PHASE==='succeeded'?'confirmed':'terminal',JSON.stringify(latest));
assert.equal(latest.status.reply.phase,process.env.AP01_EXPECT_PHASE);
const label=element('app-update-result').textContent;
assert(label.includes(process.env.AP01_EXPECT_PHASE==='succeeded'?'\u66f4\u65b0\u6210\u529f':'\u6574\u4f53\u56de\u9000'));
assert.equal(calls.filter(op=>op==='run').length,1);
element('app-update-code').value='01234567';await run("appUpdateAction('query')");
assert.equal(latest.reply?.phase,process.env.AP01_EXPECT_PHASE,JSON.stringify(latest));
writeFileSync(process.env.AP01_RESULT,JSON.stringify(latest.reply,null,2)+'\n');
await run("appUpdateAction('archive')");assert.equal(run('appUpdate.operation'),null,element('app-update-result').textContent);
await run('appUpdatePoll()');assert.equal(run('appUpdate.operation'),null,'archived operation resurrected');
console.log('Production UI -> Rust -> C++: files, prepare, worker, page recovery, result and archive passed');
