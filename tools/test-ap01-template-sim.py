"""Real X11 navigation and shared-render pixel checks in an isolated AP01 SIM.
No physical device/Bridge request is issued. Only processes created here stop.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import sys
import time
from PIL import Image, ImageChops

repo = Path(__file__).resolve().parents[1]
root = Path(sys.argv[1]).resolve()
native = Path(sys.argv[2]).resolve()
assert root.name.startswith('nuttx-ap01-template-')
evidence = repo / 'artifacts/ap01-template-integration'
assert evidence.is_dir()
spec = importlib.util.spec_from_file_location('sim_control', native / 'sim-control.py')
control = importlib.util.module_from_spec(spec)
spec.loader.exec_module(control)
report = {'passed':False,'scope':'isolated 0041 NuttX rehost; shared compiled-template frame fixtures; no device transport', 'checks':[], 'screenshots':[]}
report['elf_sha256'] = hashlib.sha256((root/'nuttx/nuttx').read_bytes()).hexdigest()
sim = xserver = log = xlog = None
wid = None
env = None

def snapshot():
    try: return json.loads((root/'sim-response.json').read_text())
    except (FileNotFoundError, json.JSONDecodeError): return {}

def wait(predicate, label, timeout=8):
    deadline = time.monotonic()+timeout
    while time.monotonic()<deadline:
        state = snapshot()
        if predicate(state):
            report['checks'].append(label)
            return state
        if sim and sim.poll() is not None: raise RuntimeError(f'SIM exited: {sim.returncode}')
        time.sleep(.08)
    raise AssertionError(f'{label}: {snapshot()}')

def key(name, count=1):
    for _ in range(count):
        subprocess.run(['xdotool','keydown','--window',wid,name],env=env,check=True,timeout=5,capture_output=True)
        time.sleep(.09)
        subprocess.run(['xdotool','keyup','--window',wid,name],env=env,check=True,timeout=5,capture_output=True)
        time.sleep(.22)

def command(name,*values): return control.request(name,values,root=root)

def go(page):
    for _ in range(16):
        if snapshot().get('page') == page: return
        key('Right')
    raise AssertionError(f'Cannot reach {page}: {snapshot()}')

def capture(name, reference=None):
    time.sleep(.25)
    path=evidence/f'sim-{name}.png'
    subprocess.run(['import','-window',wid,str(path)],env=env,check=True,timeout=5)
    image=Image.open(path).convert('RGB')
    assert image.size==(320,240)
    if reference:
        wanted=Image.open(evidence/f'{reference}.png').convert('RGB')
        delta=ImageChops.difference(image,wanted)
        # The X11 fb uses the same RGB565-expanded palette as the host PNG.
        error=sum(sum(pixel) for pixel in delta.getdata())
        assert error==0, f'{name}: shared-render/native pixels differ by {error}'
        report['checks'].append(f'{name}: shared renderer and native LVGL pixels identical')
    report['screenshots'].append({'path':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest()})

try:
    for name in ('weekly-only','both-windows'):
        shutil.copyfile(evidence/f'{name}.palette4',root/f'{name}.palette4')
    (root/'template-scene.txt').write_text('0\n')
    (root/'template-count.txt').write_text('1\n')
    for name in ('sim-request.txt','sim-response.json','template-activation-fail.txt'):
        (root/name).unlink(missing_ok=True)
    xlog=(evidence/'sim-xvfb.log').open('w')
    xserver=subprocess.Popen(['Xvfb','-displayfd','1','-screen','0','640x480x24','-nolisten','unix','-nolisten','tcp'],stdout=subprocess.PIPE,stderr=xlog)
    assert select.select([xserver.stdout],[],[],20)[0], 'Xvfb startup timeout; inspect sim-xvfb.log'
    display=xserver.stdout.readline().decode().strip()
    assert display.isdecimal()
    env=dict(os.environ,DISPLAY=f':{display}')
    log=(evidence/'sim-integration.log').open('w')
    sim=subprocess.Popen([str(root/'nuttx/nuttx')],cwd=root/'nuttx',env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT)
    wait(lambda s: bool(s.get('session')) and not s.get('ui',{}).get('boot'), 'boot and local service ready',15)
    windows=subprocess.check_output(['xdotool','search','--name','^NuttX$'],env=env,text=True,timeout=5).splitlines()
    assert len(windows)==1
    wid=windows[0]
    if not snapshot()['settings']['guide']:
        if not snapshot()['ui']['guide_active']: key('Return')
        key('Return',3)
        wait(lambda s:s['ui']['guide_stage']==0x28,'native onboarding rotary task')
        key('Right')
        key('Return',3)
        wait(lambda s:s['ui']['guide_stage']==0x2c,'native onboarding complete stage')
        key('Return')
        wait(lambda s:s['settings']['guide'] and not s['ui']['guide_active'],'native onboarding saved')
    command('wifi',1)
    command('fixture',1)
    go('Weather'); capture('weather')
    key('Right'); wait(lambda s:s.get('page')=='Codex','Weather → template')
    capture('weekly-only','weekly-only')
    key('Right'); wait(lambda s:s.get('page')=='Pet','template → Pet'); capture('pet')
    key('Left'); wait(lambda s:s.get('page')=='Codex','Pet → template')
    key('Left'); wait(lambda s:s.get('page')=='Weather','template → Weather')
    (root/'template-scene.txt').write_text('1\n'); time.sleep(.4)
    assert snapshot()['page']=='Weather'
    report['checks'].append('background data update does not steal native page focus')
    key('Right'); wait(lambda s:s.get('page')=='Codex','return shows latest template data')
    capture('both-windows','both-windows')
    (root/'template-scene.txt').write_text('0\n'); time.sleep(.4)
    wait(lambda s:s.get('page')=='Codex','foreground reflow retains page focus')
    capture('reflow','weekly-only')
    command('bank',0,1)
    wait(lambda s:bool(s['ui']['charge_popup']),'native charging popup overlays template')
    capture('charging-popup')
    key('Right'); assert snapshot()['page']=='Codex'
    report['checks'].append('charging popup consumes rotary input')
    wait(lambda s:not s['ui']['charge_popup'],'native popup timeout restores template',6)
    capture('popup-restored','weekly-only')
    command('saver',2)
    wait(lambda s:bool(s['ui']['saver']),'native screensaver overlays template')
    capture('screensaver')
    key('Right')
    wait(lambda s:not s['ui']['saver'] and s['page']=='Codex','first rotary wake restores template and is consumed')
    # All eight installed entries are traversed; rejected activation holds focus.
    (root/'template-count.txt').write_text('8\n')
    (root/'template-activation-fail.txt').write_text('1\n')
    key('Right'); assert snapshot()['page']=='Codex'
    (root/'template-activation-fail.txt').unlink()
    key('Right',7); assert snapshot()['page']=='Codex'
    key('Right'); wait(lambda s:s['page']=='Pet','all 8 templates precede native Pet')
    key('Left',8); assert snapshot()['page']=='Codex'
    key('Left'); wait(lambda s:s['page']=='Weather','reverse traversal visits all 8 templates')
    (root/'template-count.txt').write_text('1\n')
    # A local Profile refresh has reset the fixture's active index to zero.
    # The production callback receives the real active context from its service.
    key('Right'); wait(lambda s:s['page']=='Codex','re-enter single-template Profile')
    home='Power' if snapshot()['charger'] else 'Time'
    subprocess.run(['xdotool','keydown','--window',wid,'Return'],env=env,check=True,timeout=5,capture_output=True)
    time.sleep(3.3)
    subprocess.run(['xdotool','keyup','--window',wid,'Return'],env=env,check=True,timeout=5,capture_output=True)
    wait(lambda s:s['page']==home,'native three-second home clears template focus and follows charger state')
    capture('native-home')
    report['passed']=True
except Exception as error:
    report['error']=repr(error)
    raise
finally:
    if sim and sim.poll() is None: sim.terminate(); sim.wait(timeout=5)
    if xserver and xserver.poll() is None: xserver.terminate(); xserver.wait(timeout=5)
    if log: log.close()
    if xlog: xlog.close()
    (evidence/'sim-integration-report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print(f"AP01 SIM: {len(report['checks'])} checks, {len(report['screenshots'])} screenshots")
