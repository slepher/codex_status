"""Linux/X11 side of the isolated live AP01 integration harness."""
from pathlib import Path
import hashlib, importlib.util, json, os, re, select, subprocess, sys, time
from PIL import Image, ImageChops

root, native, evidence = map(lambda value: Path(value).resolve(), sys.argv[1:])
assert root.name.startswith('nuttx-ap01-template-') and evidence.is_dir()
repo = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('control', native/'sim-control.py')
control = importlib.util.module_from_spec(spec); spec.loader.exec_module(control)
report = {'passed': False, 'scope': 'real Bridge client + shared device cores + isolated NuttX UI; no physical ROM', 'checks': [], 'screenshots': []}
sim = xvfb = log = xlog = None
env = None; window = None

def snapshot():
    try: return json.loads((root/'sim-response.json').read_text())
    except (OSError, json.JSONDecodeError): return {}

def stage():
    try: return json.loads((evidence/'client-stage.json').read_text())
    except (OSError, json.JSONDecodeError): return {}

def wait(predicate, message, seconds=30):
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline:
        if (evidence/"stop-request.txt").exists(): raise RuntimeError("test cancelled by owner")
        if predicate(): return
        time.sleep(.05)
    raise AssertionError(f'{message}; stage={stage()}, ui={snapshot()}')

def key(name, count=1):
    for _ in range(count):
        subprocess.run(['xdotool','keydown','--window',window,name],env=env,check=True,capture_output=True,timeout=5)
        time.sleep(.12)
        subprocess.run(['xdotool','keyup','--window',window,name],env=env,check=True,capture_output=True,timeout=5)
        time.sleep(.25)

def capture(name, compare=False):
    time.sleep(.3)
    output=evidence/f'{name}.png'
    subprocess.run(['import','-window',window,str(output)],env=env,check=True,timeout=5)
    actual=Image.open(output).convert('RGB'); assert actual.size==(320,240)
    if compare:
        source=(repo/'src/ap01_template_display.h').read_text()
        colors=re.search(r'palette\[\] = \{(.*?)\};',source,re.S).group(1)
        palette=[]
        for color in re.findall(r'0x[0-9a-f]+',colors):
            color=int(color,16);r=color>>11;g=(color>>5)&63;b=color&31
            palette.append(((r<<3)|(r>>2),(g<<2)|(g>>4),(b<<3)|(b>>2)))
        raw=(root/'live-frame.palette4').read_bytes(); assert len(raw)==38400
        indexes=[value for byte in raw for value in (byte>>4,byte&15)]
        reference=Image.new('RGB',(320,240));reference.putdata([palette[index] for index in indexes])
        delta=ImageChops.difference(actual,reference)
        assert delta.getbbox() is None, f'{name}: native pixels differ from live C++ output'
        if name in ('weekly','restored'):
            assert all(indexes[y*320+x]==0 for y in range(52,100) for x in range(260,304)), '5h group was not hidden'
        if name=='no-rc':
            assert all(indexes[y*320+x]==0 for y in range(184,205) for x in range(249,304)), 'missing RC was not hidden'
        else:
            assert any(indexes[y*320+x]==3 for y in range(184,205) for x in range(249,304)), 'RC 2 was lost'
        report['checks'].append(f'{name}: live frame matches NuttX pixels; conditional layout checked')
    report['screenshots'].append({'file':str(output),'sha256':hashlib.sha256(output.read_bytes()).hexdigest()})

def go(page):
    for _ in range(18):
        if snapshot().get('page')==page:return
        key('Right')
    raise AssertionError(f'Cannot reach {page}: {snapshot()}')

try:
    for name in ('sim-request.txt','sim-response.json'):(root/name).unlink(missing_ok=True)
    xlog=(evidence/'xvfb.log').open('w')
    xvfb=subprocess.Popen(['Xvfb','-displayfd','1','-screen','0','640x480x24','-nolisten','unix','-nolisten','tcp'],stdout=subprocess.PIPE,stderr=xlog)
    assert select.select([xvfb.stdout],[],[],20)[0], 'Xvfb did not become ready'
    display=xvfb.stdout.readline().decode().strip(); assert display.isdecimal()
    env=dict(os.environ,DISPLAY=f':{display}')
    log=(evidence/'native-ui.log').open('w')
    sim=subprocess.Popen([str(root/'nuttx/nuttx')],cwd=root/'nuttx',env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT)
    wait(lambda: bool(snapshot().get('session')) and not snapshot().get('ui',{}).get('boot'),'native UI ready',20)
    windows=subprocess.check_output(['xdotool','search','--name','^NuttX$'],env=env,text=True).splitlines();assert len(windows)==1
    window=windows[0]
    if not snapshot()['settings']['guide']:
        if not snapshot()['ui']['guide_active']:key('Return')
        key('Return',3);key('Right');key('Return',3);key('Return')
        wait(lambda:snapshot()['settings']['guide'] and not snapshot()['ui']['guide_active'],'finish onboarding')
    control.request('wifi',[1],root=root);control.request('fixture',[1],root=root)
    go('Weather');capture('weather')
    # This press predates the popup and must never confirm the new request.
    subprocess.run(['xdotool','keydown','--window',window,'Return'],env=env,check=True,capture_output=True,timeout=5)
    (evidence/'ui-ready.txt').write_text('ready')
    wait(lambda:stage().get('stage')=='pending-display','pairing queued while button held',30)
    subprocess.run(['xdotool','keyup','--window',window,'Return'],env=env,check=True,capture_output=True,timeout=5)
    (evidence/'client-continue.txt').write_text('pending-display')
    wait(lambda:stage().get('stage')=='awaiting_confirmation','pairing popup displayed',15)
    capture('pairing')
    time.sleep(.25);(evidence/'client-continue.txt').write_text('stale-release')
    wait(lambda:stage().get('stage')=='fresh-confirmation','old button release rejected')
    report['checks'].append('a button held before the popup cannot approve the binding')
    key('Return')
    for name in ('weekly','both','no-rc','zero'):
        wait(lambda:stage().get('stage')==name,f'Bridge delivered {name}',45)
        if name=='weekly':
            assert snapshot().get('page')=='Weather','background installation stole focus'
            report['checks'].append('binding and background Bundle/Data do not steal Weather focus')
            if snapshot().get('mode')!=0:key('Escape')
            key('Right');wait(lambda:snapshot().get('page')=='Codex','native Weather → installed template')
        capture(name,True)
        (evidence/'client-continue.txt').write_text(name)
    wait(lambda:stage().get('stage')=='rotate-next','request next installed profile item')
    key('Right')
    wait(lambda:(root/'live-ui.txt').read_text().splitlines()[-1]=='1','atomic local profile activation')
    (evidence/'client-continue.txt').write_text('rotate-next')
    wait(lambda:stage().get('stage')=='second','Bridge supplied new active context')
    capture('second',True)
    (evidence/'client-continue.txt').write_text('second')
    report['checks'].append('real knob activates the second installed template with a distinct context')
    wait(lambda:stage().get('stage')=='restart','request process reboot')
    wait(lambda:stage().get('stage')=='restored','binding/Bundle survive reboot',45)
    time.sleep(.4);capture('restored',True)
    (evidence/'client-continue.txt').write_text('restored')
    wait(lambda:stage().get('stage')=='complete','Bridge client integration complete')
    key('Right');wait(lambda:snapshot().get('page')=='Pet','template → native Pet')
    key('Left');wait(lambda:snapshot().get('page')=='Codex','Pet → template')
    report['checks'].append('installed template retains native bidirectional Weather/Pet navigation')
    report['client_result']=stage()['value'];report['passed']=True
finally:
    report['elf_sha256']=hashlib.sha256((root/'nuttx/nuttx').read_bytes()).hexdigest()
    report['elf_bytes']=(root/'nuttx/nuttx').stat().st_size
    (evidence/'live-report.json').write_text(json.dumps(report,indent=2))
    for process in (sim,xvfb):
        if process and process.poll() is None:
            process.terminate()
            try:process.wait(timeout=5)
            except subprocess.TimeoutExpired:process.kill();process.wait(timeout=5)
    for stream in (log,xlog):
        if stream:stream.close()
print(f'Live AP01 UI: {len(report["checks"])} checks, {len(report["screenshots"])} screenshots')
