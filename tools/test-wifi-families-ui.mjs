// Windows visual fallback using the installed Edge and the same production HTML/scenarios.
// Synthetic IPC only. macOS WKWebView remains a separate required acceptance check.
import {readFileSync,writeFileSync,existsSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {spawn} from 'node:child_process';
import assert from 'node:assert/strict';

const root=resolve(import.meta.dirname,'..'),output=resolve(root,'artifacts/wifi-families');
assert(existsSync(output),'Pre-create the evidence directory with project-required elevation.');
const edge='C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe';
assert(existsSync(edge),'Installed Edge required');
let html=readFileSync(resolve(root,'bridge/crates/app/ui/index.html'),'utf8');
const script=html.match(/<script>([\s\S]*?)<\/script>/)[1];
const bootstrap="window.__uiCalls=[];window.__uiErrors=[];window.addEventListener('error',e=>window.__uiErrors.push(e.message));window.__TAURI__={core:{invoke:async(name,args)=>{window.__uiCalls.push({name,args});return {devices:[],profiles:[],templates:[],sources:[],jobs:[],plans:[],contexts:[]};}}};";
const production=script.slice(0,script.lastIndexOf('\n  renderFamilyMenu();'));
html=html.replace(script,bootstrap+production+'\n'+readFileSync(resolve(root,'tools/wifi-ui-scenarios.js'),'utf8'));
const fixture=resolve(output,'fixture.html');writeFileSync(fixture,html);
const browser=spawn(edge,['--headless=new','--disable-gpu','--no-first-run','--remote-debugging-port=0',`--user-data-dir=${resolve(output,'edge-profile')}`,'about:blank'],{windowsHide:true,stdio:['ignore','ignore','pipe']});
let socket;
try {
  const endpoint=await new Promise((done,fail)=>{
    const timer=setTimeout(()=>fail(new Error('Edge startup timeout')),15000);
    browser.stderr.on('data',chunk=>{const match=String(chunk).match(/DevTools listening on (ws:\/\/\S+)/);if(match){clearTimeout(timer);done(match[1]);}});
    browser.once('error',fail);browser.once('exit',code=>fail(new Error('Edge exited '+code)));
  });
  socket=new WebSocket(endpoint);await new Promise((done,fail)=>{socket.addEventListener('open',done,{once:true});socket.addEventListener('error',fail,{once:true});});
  let next=0;const pending=new Map();
  socket.addEventListener('message',({data})=>{const r=JSON.parse(data);if(!r.id)return;const task=pending.get(r.id);if(!task)return;pending.delete(r.id);clearTimeout(task.timer);r.error?task.fail(new Error(JSON.stringify(r.error))):task.done(r.result);});
  const cdp=(method,params={},sessionId)=>new Promise((done,fail)=>{const id=++next;const timer=setTimeout(()=>{pending.delete(id);fail(new Error('CDP timeout '+method));},15000);pending.set(id,{done,fail,timer});socket.send(JSON.stringify({id,method,params,...(sessionId?{sessionId}:{})}));});
  const {targetId}=await cdp('Target.createTarget',{url:'about:blank'});
  const {sessionId}=await cdp('Target.attachToTarget',{targetId,flatten:true});
  const call=(method,params)=>cdp(method,params,sessionId);
  const evaluate=async expression=>{
    const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
    if(r.exceptionDetails)throw new Error(r.exceptionDetails.exception?.description||r.exceptionDetails.text);
    return r.result.value;
  };
  await call('Page.enable');const results=[];
  for(const width of [560,1000]) {
    await call('Emulation.setDeviceMetricsOverride',{width,height:800,deviceScaleFactor:1,mobile:false});
    for(const scenario of ['bridge','device','compare','wifi-family-edit','wifi-family-wired','wifi-family-ap01']) {
      await call('Page.navigate',{url:pathToFileURL(fixture).href});
      for(let n=0;n<50;n++){if(await evaluate('typeof window.__wifiUiScenario==="function"'))break;await new Promise(r=>setTimeout(r,100));}
      const result=await evaluate(`window.__wifiUiScenario(${JSON.stringify(scenario)})`);
      const clip=await evaluate(`(()=>{const e=document.getElementById(${JSON.stringify(scenario==='wifi-family-ap01'?'wifi-ap01-auth':scenario.startsWith('wifi-family-')?'wifi-family-library':'wifi-card')});e.scrollIntoView();const r=e.getBoundingClientRect();return {x:Math.max(0,r.x+scrollX-14),y:Math.max(0,r.y+scrollY-14),width:r.width+28,height:r.height+28,scale:1};})()`);
      const screenshot=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:true,clip});
      writeFileSync(resolve(output,`${width}-${scenario}.png`),Buffer.from(screenshot.data,'base64'));
      results.push({...result,clip});console.log('PASS',width,scenario);
    }
  }
  writeFileSync(resolve(output,'edge-results.json'),JSON.stringify({engine:'Edge synthetic IPC; not WKWebView/Rust/RF proof',results},null,2));
  await cdp('Browser.close');
} finally {socket?.close();browser.kill();}
