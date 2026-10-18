"""Windows orchestrator. Only launches/stops its own loopback simulator and UI.
Run elevated per AGENTS: it creates a private evidence run directory. No physical
device, production Bridge, Wi-Fi credentials or OTA upload is used.
"""
from pathlib import Path
import hashlib, json, os, socket, subprocess, sys, time
from datetime import datetime

repo=Path(__file__).resolve().parents[1]
assert os.name=='nt'
root='/home/slepher/nuttx-ap01-template-20261018'
evidence=repo/'artifacts/ap01-platform-integration'/datetime.now().strftime('run-%Y%m%d-%H%M%S')
evidence.mkdir();(evidence/'device').mkdir()
native=repo.parent/'cuktech-ap01-firmware/tooling/scripts'
binary=repo/'bridge/target-note4-deploy/debug/device-sim.exe'
tests=list((repo/'bridge/target-note4-deploy/debug/deps').glob('ap01_live-*.exe'))
assert binary.is_file() and tests
client_binary=max(tests,key=lambda path:path.stat().st_mtime)
for port in (19760,19761):
    with socket.socket() as sock: assert sock.connect_ex(('127.0.0.1',port))!=0,f'test port {port} occupied; not stopping its process'
with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as sock:
    sock.bind(('127.0.0.1',18767)) # Refuse to use another process's discovery listener.
flags=subprocess.CREATE_NO_WINDOW
environment=dict(os.environ,CODEX_STATUS_AP01_UI_ROOT=rf'\\wsl.localhost\Ubuntu-24.04{root.replace("/", chr(92))}',CODEX_STATUS_AP01_UDP_BIND='127.0.0.1:18767')
environment['CODEX_STATUS_AP01_MAINTENANCE_BIND']='127.0.0.1:19760'
environment['CODEX_STATUS_AP01_BUSINESS_STOP_FILE']=str(evidence/'stop-business.txt')
client_env=dict(os.environ,AP01_LIVE_EVIDENCE=str(evidence),AP01_LIVE_UDP='127.0.0.1:18767')
server=client=ui=None;streams=[]
def wsl_path(path):return '/mnt/'+path.drive[0].lower()+'/'+path.as_posix()[3:]
def start_server():
    stream=(evidence/f'device-{len(streams)}.log').open('w');streams.append(stream)
    process=subprocess.Popen([str(binary),'--listen','127.0.0.1:19761','--target','cuktech-ap01-0041',
        '--mac','02000000A001','--data-dir',str(evidence/'device')],env=environment,stdout=stream,stderr=subprocess.STDOUT,creationflags=flags)
    deadline=time.monotonic()+10
    while time.monotonic()<deadline:
        if process.poll() is not None:raise RuntimeError(f'AP01 simulator exited: inspect {stream.name}')
        with socket.socket() as sock:
            if sock.connect_ex(('127.0.0.1',19761))==0:return process
        time.sleep(.05)
    raise TimeoutError('AP01 HTTP startup')
def stop(process):
    if process and process.poll() is None:
        process.terminate()
        try:process.wait(timeout=5)
        except subprocess.TimeoutExpired:process.kill();process.wait(timeout=5)
try:
    ui_directory=Path(environment['CODEX_STATUS_AP01_UI_ROOT'])
    for name in ('live-ui.txt','live-frame.palette4','live-events.txt','live-activate-ack.txt'):
        (ui_directory/name).unlink(missing_ok=True)
    ui_log=(evidence/'ui-harness.log').open('w');streams.append(ui_log)
    ui=subprocess.Popen(['wsl.exe','-d','Ubuntu-24.04','--','python3','-B',wsl_path(repo/'tools/test-ap01-platform-ui.py'),root,wsl_path(native),wsl_path(evidence)],stdout=ui_log,stderr=subprocess.STDOUT,creationflags=flags)
    deadline=time.monotonic()+35
    while not (evidence/'ui-ready.txt').exists():
        if ui.poll() is not None:raise RuntimeError('NuttX UI startup failed; inspect ui-harness.log')
        if time.monotonic()>deadline:raise TimeoutError('NuttX readiness')
        time.sleep(.05)
    server=start_server()
    client_log=(evidence/'bridge-client.log').open('w');streams.append(client_log)
    client=subprocess.Popen([str(client_binary),'--ignored','--nocapture','--test-threads=1'],env=client_env,
        stdout=client_log,stderr=subprocess.STDOUT,creationflags=flags)
    restarted=False;deadline=time.monotonic()+200
    while client.poll() is None or ui.poll() is None:
        if client.poll() not in (None,0):raise RuntimeError('Bridge integration failed; inspect bridge-client.log')
        if ui.poll() not in (None,0):raise RuntimeError('Native UI integration failed; inspect ui-harness.log')
        try:stage=json.loads((evidence/'client-stage.json').read_text())
        except (FileNotFoundError,json.JSONDecodeError):stage={}
        if stage.get('stage')=='restart' and not restarted:
            stop(server);server=start_server();restarted=True
            (evidence/'client-continue.txt').write_text('restart')
        if stage.get('stage')=='business-stop':
            (evidence/'stop-business.txt').write_text('stop')
            time.sleep(.2)
            (evidence/'client-continue.txt').write_text('business-stop')
        if time.monotonic()>deadline:raise TimeoutError('live integration total deadline')
        time.sleep(.05)
    assert client.returncode==0 and ui.returncode==0
    report=json.loads((evidence/'live-report.json').read_text());assert report['passed']
    assert report['client_result']['independent_maintenance_http']
    report['checks'].append('separate authenticated maintenance HTTP stays available after business HTTP stops; business routes are absent from maintenance')
    report['binaries']=[{'path':str(path),'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
        for path in (binary,client_binary,repo/'bridge/target-note4-deploy/debug/bridge-app.exe')]
    report['installed_content_bytes']=(evidence/'device/littlefs-2f62756e646c652f636f6e74656e742e62696e').stat().st_size
    (evidence/'live-report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
    (repo/'artifacts/ap01-platform-integration/latest.txt').write_text(str(evidence))
    print(json.dumps({'passed':True,'evidence':str(evidence),'checks':len(report['checks']),'screenshots':len(report['screenshots'])}))
finally:
    (evidence/"stop-request.txt").write_text("stop")
    if ui and ui.poll() is None:
        try:ui.wait(timeout=10)
        except subprocess.TimeoutExpired:pass
    for process in (client,server,ui):stop(process)
    for stream in streams:stream.close()
