"""Host ASan/UBSan test of native application code; not a ROM execution claim.
Run elevated: precreates its private state directory and compiler temp files.
"""
from pathlib import Path
import hashlib, json, struct, subprocess, zlib, time, argparse
repo=Path(__file__).resolve().parents[1]
out=repo/'artifacts/ap01-platform-integration/device'
parser=argparse.ArgumentParser();parser.add_argument('--image',type=Path,default=out/'ap01-native-0041-candidate-13.bin');parser.add_argument('--bundle',type=Path);parser.add_argument('--out',type=Path,default=out);parser.add_argument('--linux-state',action='store_true');args=parser.parse_args();out=args.out.resolve()
assert out.is_dir()
if args.linux_state:
    create="import tempfile,pathlib; p=pathlib.Path(tempfile.mkdtemp(prefix='ap01-host-')); [(p/('f'+str(i))).mkdir() for i in range(22)]; (p/'f6/bundle').mkdir(); print(p)"
    state=subprocess.check_output(['wsl','-d','Ubuntu-24.04','--','python3','-c',create],text=True).strip()
    assert state.startswith('/tmp/ap01-host-')
else:
    state=out/('t'+str(time.time_ns()%1000000))
    state.mkdir(exist_ok=True)
    assert not any(state.iterdir()), 'Use a fresh private test directory; never delete runtime data'
    for i in range(22):(state/('f'+str(i))).mkdir()
    (state/'f6/bundle').mkdir()

def linux(p):
    if isinstance(p,str) and p.startswith('/tmp/ap01-host-'):return p
    p=p.resolve(); return '/mnt/'+p.drive[0].lower()+p.as_posix()[2:]

prior=repo/'artifacts/ap01-platform-integration/run-20261018-195827/device'
meta=json.loads((prior/'littlefs-2f62756e646c652f73746174652e62696e').read_bytes()[12:])
content=(prior/'littlefs-2f62756e646c652f636f6e74656e742e62696e').read_bytes()
payload=content[meta['payload_offset']:meta['payload_offset']+meta['payload_length']]
if payload[:4]==b'CSZ1':
    decoded,crc,block,count,reserved=struct.unpack_from('<5I',payload,4)
    assert block==4096 and not reserved
    lengths=struct.unpack_from('<'+str(count)+'I',payload,24)
    at=24+4*count; parts=[]
    for length in lengths:
        parts.append(zlib.decompress(payload[at:at+length])); at+=length
    payload=b''.join(parts); assert len(payload)==decoded and zlib.crc32(payload)==crc
bundle=json.loads(args.bundle.read_text(encoding='utf-8') if args.bundle else payload); bundle['bridge_id']='bridge-test'
payload=json.dumps(bundle,ensure_ascii=False,separators=(',',':')).encode()
bundle_file=out/'native-app-bundle.json';bundle_file.write_bytes(payload)
sources=['template_engine.cpp','GUI_Paint.cpp','font_asset.cpp','bundle_store.cpp','bundle_payload.cpp',
         'v2_runtime.cpp','v2_data_command.cpp','v2_bundle_command.cpp','v2_activate_command.cpp',
         'v2_command_envelope.cpp','v2_status_snapshot.cpp','ap01_binding.cpp','ap01_pairing.cpp','sha256.cpp']
sources+=['font'+str(n)+'.cpp' for n in (8,12,16,20,24)]
binary=out/'native-app-smoke';report=out/'native-app-report.json'
report.write_text(json.dumps({'passed':False,'stage':'building','flashable':False}),encoding='utf-8')
(out/'native-firmware-host-report.json').write_text(json.dumps({'passed':False,'stage':'building','flashable':False}),encoding='utf-8')
cmd=['wsl','-d','Ubuntu-24.04','--','g++','-std=c++17','-O0','-g','-fno-omit-frame-pointer',
     '-fsanitize=address,undefined','-DCODEX_AP01_NATIVE=1','-DCODEX_RENDER_NOTE4_FONTS=1',
     '-DARDUINOJSON_ENABLE_ARDUINO_STRING=1','-DARDUINOJSON_ENABLE_STD_STRING=0',
     '-DARDUINOJSON_ENABLE_STD_STREAM=0','-Wno-deprecated-declarations']
for folder in ('ports/ap01','bridge/crates/render/shim','src','.pio/libdeps/esp32-s3-epaper-154g/ArduinoJson/src'):
    cmd+=['-I',linux(repo/folder)]
source_paths=[repo/'src'/s for s in sources]+[repo/'ports/ap01'/s for s in ('business.cpp','application.cpp','http.cpp','native_smoke.cpp')]
platform=repo.parent/'cuktech-ap01-firmware/tooling/firmware-modules'
source_paths += [platform/'native_maintenance_0041.cpp',platform/'native_http_transport_0041.cpp',platform/'native_wifi_profiles_0041.cpp',platform/'native_diagnostic_0041.cpp',platform/'native_calendar_0041.cpp',platform/'native_firmware_0041.cpp',platform/'native_ota_image_0041.c']
cmd+=['-I',linux(platform)]
objects=[]
headers=list(platform.glob('*.h'))+list((repo/'ports/ap01').glob('*.h'))+list((repo/'src').glob('*.h'))
newest=max(p.stat().st_mtime_ns for p in headers)
for source in source_paths:
    obj=out/('native-host-'+source.stem+'.o');objects.append(obj)
    if not obj.exists() or obj.stat().st_mtime_ns < max(source.stat().st_mtime_ns,newest):
        compile_cmd=cmd if source.suffix!='.c' else [s.replace('g++','gcc').replace('-std=c++17','-std=c11') for s in cmd]
        subprocess.run(compile_cmd+['-c',linux(source),'-o',linux(obj)],check=True)
subprocess.run(cmd+[linux(p) for p in objects]+['-o',linux(binary)],check=True)
result=subprocess.run(['wsl','-d','Ubuntu-24.04','--',linux(binary),linux(state),linux(bundle_file),
                       linux(out/'native-app-activation.json')],capture_output=True,text=True,timeout=180)
(out/'native-app-tests.log').write_text(result.stdout+result.stderr,encoding='utf-8')
assert not result.returncode,result.stderr
firmware_source=platform/'test_native_maintenance_0041.cpp';firmware_object=out/'native-host-firmware_smoke.o'
subprocess.run(cmd+['-c',linux(firmware_source),'-o',linux(firmware_object)],check=True)
firmware_binary=out/'native-firmware-smoke'
subprocess.run(cmd+[linux(p) for p in objects if p.stem!='native-host-native_smoke']+[linux(firmware_object),'-o',linux(firmware_binary)],check=True)
firmware_result=subprocess.run(['wsl','-d','Ubuntu-24.04','--',linux(firmware_binary),linux(state),linux(args.image)],capture_output=True,text=True,timeout=180)
(out/'native-firmware-host-tests.log').write_text(firmware_result.stdout+firmware_result.stderr,encoding='utf-8')
assert not firmware_result.returncode,firmware_result.stderr
# Compile the same menu-code OTA/restart/switch scenarios without App or
# business/render objects: firmware maintenance must not link the application.
independent_object=out/'native-host-maintenance-only.o'
subprocess.run(cmd+['-DAP01_MAINTENANCE_ONLY_TEST=1','-c',linux(firmware_source),'-o',linux(independent_object)],check=True)
independent_binary=out/'native-maintenance-independent'
independent_names={'native_maintenance_0041','native_http_transport_0041','native_wifi_profiles_0041',
                   'native_diagnostic_0041','native_firmware_0041','native_ota_image_0041','sha256'}
independent_objects=[obj for source,obj in zip(source_paths,objects) if source.stem in independent_names]
assert len(independent_objects)==len(independent_names)
subprocess.run(cmd+[linux(p) for p in independent_objects]+[linux(independent_object),'-o',linux(independent_binary)],check=True)
symbols=subprocess.check_output(['wsl','-d','Ubuntu-24.04','--','nm','-C',linux(independent_binary)],text=True)
assert 'Ap01Application' not in symbols and 'Ap01Business' not in symbols
create="import tempfile,pathlib; p=pathlib.Path(tempfile.mkdtemp(prefix='ap01-independent-')); (p/'f21').mkdir(); print(p)"
independent_state=subprocess.check_output(['wsl','-d','Ubuntu-24.04','--','python3','-c',create],text=True).strip()
independent_result=subprocess.run(['wsl','-d','Ubuntu-24.04','--',linux(independent_binary),independent_state,linux(args.image)],capture_output=True,text=True,timeout=180)
(out/'native-maintenance-independent.log').write_text(independent_result.stdout+independent_result.stderr,encoding='utf-8')
assert not independent_result.returncode,independent_result.stderr
(out/'native-maintenance-independent.json').write_text(json.dumps({'passed':True,'application_linked':False,
    'binary_sha256':hashlib.sha256(independent_binary.read_bytes()).hexdigest(),
    'checks':independent_result.stdout.strip(),'physical_verified':False},indent=2),encoding='utf-8')
(out/'native-firmware-host-report.json').write_text(json.dumps({'passed':True,
    'scope':'host ASan/UBSan exact native firmware service; fake flash callbacks, no ROM ABI proof',
    'image_sha256':hashlib.sha256(args.image.read_bytes()).hexdigest(),
    'test_source_sha256':hashlib.sha256(firmware_source.read_bytes()).hexdigest(),
    'binary_sha256':hashlib.sha256(firmware_binary.read_bytes()).hexdigest(),
    'checks':firmware_result.stdout.strip()},indent=2),encoding='utf-8')
report.write_text(json.dumps({'passed':True,'flashable':False,'scope':'native application on Linux host ASan/UBSan; no recovered VFS/UI ABI execution',
    'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'bundle_bytes':len(payload),
    'checks':result.stdout.strip()},indent=2),encoding='utf-8')
print(report)
