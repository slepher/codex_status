"""Windows Edge screenshots of the production HTML with synthetic IPC only.
Run elevated: the isolated browser creates private profile directories.
This does not replace macOS WKWebView or real Rust/device acceptance.
"""
from pathlib import Path
import hashlib, json, os, subprocess
repo=Path(__file__).resolve().parents[1]
out=repo/'artifacts/ap01-platform-integration/bridge'
assert out.is_dir()
edge=Path(os.environ['ProgramFiles(x86)'])/'Microsoft/Edge/Application/msedge.exe'
html=(repo/'bridge/crates/app/ui/index.html').read_text(encoding='utf-8')
bootstrap="""<script>
window.setInterval=()=>0;
window.__TAURI__={core:{invoke:async(name,args)=>{
 if(name==='platform_ap01')return {result:'complete',candidates:[
  {device_mac:'02000000A001',name:'AP01 · 书桌'},
  {device_mac:'02000000A002',name:'Long-name-'.repeat(9)}]};
 return {devices:[],profiles:[],templates:[],sources:[],jobs:[]};
}}};
</script>"""
html=html.replace('<script>',bootstrap+'<script>',1)
start=html.rindex('\n  renderFamilyMenu();')
end=html.index('</script>',start)
scenario="""
document.querySelectorAll('.panel').forEach(p=>p.classList.toggle('on',p.id==='tab-device'));
document.querySelectorAll('.tabs button').forEach(b=>b.classList.toggle('on',b.dataset.tab==='device'));
document.getElementById('registered-devices').textContent='选择发现的 AP01 完成设备端确认';
ap01Search().then(()=>{
 ap01Message({result:'pending',display_code:'0042',reply:{state:'awaiting_confirmation'}});
 const width=document.documentElement.clientWidth;
 const bad=[...document.querySelectorAll('#ap01-add button,#ap01-add span')].filter(node=>{
  const r=node.getBoundingClientRect();return r.width>0&&(r.left<0||r.right>width+1);
 });
 document.body.dataset.ap01Check=bad.length===0&&document.querySelectorAll('#ap01-candidates button').length===6?'passed':'failed';
});
"""
html=html[:start]+scenario+html[end:]
fixture=out/'ap01-onboarding-fixture.html';fixture.write_text(html,encoding='utf-8')
report={'scope':'production HTML + synthetic IPC, Edge; not native WKWebView or Rust transport','views':[]}
for width in (560,1000):
 screenshot=out/f'ap01-onboarding-{width}.png'
 command=[str(edge),'--headless','--disable-gpu','--disable-background-networking','--no-first-run',
  '--no-default-browser-check','--disable-component-update',f'--user-data-dir={out/f"edge-profile-{width}"}',
  f'--window-size={width},800','--virtual-time-budget=1500',f'--screenshot={screenshot}','--dump-dom',fixture.as_uri()]
 completed=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=35,
                          creationflags=subprocess.CREATE_NO_WINDOW)
 dom=completed.stdout.decode('utf-8',errors='replace')
 assert completed.returncode==0 and 'data-ap01-check="passed"' in dom and screenshot.is_file(), completed.stderr.decode(errors='replace')[-1500:]
 report['views'].append({'width':width,'file':str(screenshot),'sha256':hashlib.sha256(screenshot.read_bytes()).hexdigest()})
(out/'ui-report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'passed':True,'views':len(report['views']),'report':str(out/'ui-report.json')}))
