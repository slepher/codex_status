import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const five = 'buckets[codex].5h.remaining';
const week = 'buckets[codex].weekly.remaining';
const condition = exists => ({bind: five, exists});
const elements = [];
const text = (value, x, y, font, color='white', extras={}) => elements.push({type:'text', text:value, x,y,font,color,bg:'black',...extras});
const bound = (bind,x,y,font,color='white',extras={}) => elements.push({type:'text',bind,x,y,font,color,bg:'black',...extras});
const line = (y,color='track') => elements.push({type:'line',x1:16,y1:y,x2:303,y2:y,color});
const ring = (x,y,radius,when) => elements.push({type:'ring',x,y,radius,thickness:4,bind:week,fg:'cyan',bg:'track',when});
elements.push({type:'rect',rect:[0,0,320,240],fill:true,color:'black'});
text('CODEX STATUS',16,11,'ntthin18'); text('AP01',274,13,'ntthin18','muted'); line(35);
// Independent layouts, selected by source existence, not a device-side Codex rule.
for (const dual of [false,true]) {
  const when=condition(dual), cx=dual?73:160;
  ring(cx,104,dual?54:61,when);
  text('周额度剩余',cx-30,dual?65:62,'ntthin18','cyan',{when});
  bound(week,cx-49,78,'ntreg64','white',{region:[cx-49,78,98,64],align:'center',when});
  text('%',cx+35,109,'ntthin18','white',{when,digit_bind:week,digit_x:[cx+21,cx+35,cx+48]});
  if (dual) {
    text('7 天窗口',47,128,'ntthin18','muted',{when});
    elements.push({type:'line',x1:145,y1:53,x2:145,y2:164,color:'track',when});
    text('5 小时剩余',160,52,'ntthin18','orange',{when});
    bound(five,157,74,'nt30','white',{suffix:'%',when});
    bound('buckets[codex].5h.resetsAt',160,108,'ntthin18','muted',{time_format:'hhmm',suffix:' 重置',when});
    text('周额度重置',160,130,'ntthin18','cyan',{when});
    bound('buckets[codex].weekly.resetsAt',153,150,'ntthin18','white',{when,region:[153,149,151,18],align:'center'});
  } else {
    bound('buckets[codex].weekly.resetsAt',106,128,'ntthin18','muted',{when,region:[106,126,108,18],align:'center'});
  }
}
line(176);
bound('account.plan',16,184,'ntthin18');
bound('bridge.label',65,186,'ntthin18','muted',{region:[65,184,174,20],when:{bind:'bridge.label',exists:true}});
bound('resetCredits.availableCount',257,186,'ntthin18','orange',{prefix:'RC ',when:{bind:'resetCredits.availableCount',exists:true},region:[249,184,54,20],align:'right'});
line(211);
bound('device.sync_hhmm',16,220,'ntthin18','muted',{prefix:'SYNC ',when:{bind:'device.offline_mins',exists:false}});
bound('device.offline_mins',16,220,'ntthin18','orange',{prefix:'OFF ',suffix:'m',when:{bind:'device.offline_mins',exists:true}});
text('额度看板',260,219,'ntthin18','muted');
const template={schema:1,id:'codex-status-a',version:1,render_target:'lcd-jd9853-320x240-palette4',canvas:{w:320,h:240},elements};
fs.writeFileSync(path.join(root,'tools/test-bridge/templates/codex-status-ap01.json'),JSON.stringify(template,null,2)+'\n');
// Only the characters required by this template; font assets remain Bundle data.
const characters=[...new Set(elements.flatMap(e=>[e.text??'',e.prefix??'',e.suffix??'']).join(''))].sort().join('');
fs.writeFileSync(path.join(root,'project-workflow/ap01-template-integration/font-characters.txt'),characters+'\n');
console.log(`${elements.length} ops; ${characters.length} explicit font characters`);
