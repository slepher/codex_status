import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const five = 'buckets[codex].5h.remaining';
const week = 'buckets[codex].weekly.remaining';
const month = 'buckets[codex].monthly.remaining';
const condition = exists => ({bind: five, exists});
const elements = [];
const text = (value, x, y, font, color='white', extras={}) => elements.push({type:'text', text:value, x,y,font,color,bg:'black',...extras});
const bound = (bind,x,y,font,color='white',extras={}) => elements.push({type:'text',bind,x,y,font,color,bg:'black',...extras});
const line = (y,color='track') => elements.push({type:'line',x1:16,y1:y,x2:303,y2:y,color});
const ring = (x,y,radius,when,bind=week) => elements.push({type:'ring',x,y,radius,thickness:4,bind,fg:'cyan',bg:'track',when});
elements.push({type:'rect',rect:[0,0,320,240],fill:true,color:'black'});
bound('account.plan',16,10,'nt16','white',{region:[16,10,52,26]});
bound('bridge.label',76,10,'nt16','muted',{region:[76,10,163,26],when:{bind:'bridge.label',exists:true}});
bound('device.now',244,10,'nt16','white',{region:[244,10,60,26],align:'right'}); line(39);
// Independent layouts, selected by source existence, not a device-side Codex rule.
// Recover the original weekly-only layout, then overlay monthly and paid layouts.
// Clear only the quota area so a single-condition when cannot leave stale labels.
for (const layout of ['weekly','monthly','dual']) {
  const dual=layout==='dual', long=layout==='monthly'?month:week;
  const when=dual?condition(true):{bind:long,exists:true}, cx=dual?78:160;
  if (layout!=='weekly') elements.push({type:'rect',rect:[0,42,320,162],fill:true,color:'black',when});
  ring(cx,124,77,when,long);
  text(layout==='monthly'?'月额度剩余':'周额度剩余',cx-60,64,'ntthin18','cyan',{when,region:[cx-60,64,120,26],align:'center'});
  bound(long,cx-60,90,'ntreg64','white',{region:[cx-60,90,120,76],align:'center',when});
  text('%',cx-10,143,'nt16','white',{when,region:[cx-10,143,20,22],align:'center'});
  if (dual) {
    text('7 天窗口',cx-58,166,'ntthin18','muted',{when,region:[cx-58,166,116,24],align:'center'});
    elements.push({type:'line',x1:159,y1:55,x2:159,y2:196,color:'track',when});
    text('5 小时剩余',166,57,'ntthin18','orange',{when,region:[166,57,138,26]});
    bound(five,166,80,'nt30','white',{when,region:[166,80,110,68]});
    text('%',203,116,'nt16','white',{when,digit_bind:five,digit_x:[203,237,271]});
    bound('buckets[codex].5h.resetsAt',166,128,'ntthin18','muted',{time_format:'hhmm',suffix:' 重置',when,region:[166,128,138,26]});
    text('周额度重置',166,154,'ntthin18','cyan',{when,region:[166,154,138,24]});
    bound('buckets[codex].weekly.resetsAt',166,178,'nt16','white',{when,region:[166,178,138,24]});
  } else {
    bound(long.replace('.remaining','.resetsAt'),cx-58,154,'nt16','muted',{when,region:[cx-58,154,116,24],align:'center'});
  }
}
line(204);
bound('device.sync_hhmm',16,214,'nt16','muted',{prefix:'SYNC ',when:{bind:'device.offline_mins',exists:false},region:[16,214,180,26]});
bound('device.offline_mins',16,214,'nt16','orange',{prefix:'OFF ',suffix:'m',when:{bind:'device.offline_mins',exists:true},region:[16,214,180,26]});
bound('resetCredits.availableCount',240,214,'nt16','orange',{prefix:'RC ',when:{bind:'resetCredits.availableCount',exists:true},region:[240,214,64,26],align:'right'});
const template={schema:1,id:'codex-status-a',version:9,min_fw:'1.0.2_0041',render_target:'lcd-jd9853-320x240-palette4',canvas:{w:320,h:240},elements};
fs.writeFileSync(path.join(root,'tools/test-bridge/templates/codex-status-ap01.json'),JSON.stringify(template,null,2)+'\n');
// Only the characters required by this template; font assets remain Bundle data.
const characters=[...new Set(elements.flatMap(e=>[e.text??'',e.prefix??'',e.suffix??'']).join(''))].sort().join('');
fs.writeFileSync(path.join(root,'project-workflow/ap01-template-integration/font-characters.txt'),characters+'\n');
console.log(`${elements.length} ops; ${characters.length} explicit font characters`);
