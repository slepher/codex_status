"""Check AP01 source ownership; no device, network or build side effects."""
from pathlib import Path
import argparse, json, runpy

repo=Path(__file__).resolve().parents[1]
platform=repo.parent/'cuktech-ap01-firmware/tooling/firmware-modules'
parser=argparse.ArgumentParser()
parser.add_argument('--audit',type=Path)
args=parser.parse_args()

for name in ('application.cpp','application.h','native_0041.cpp','http.cpp','http.h'):
    source=(repo/'ports/ap01'/name).read_text(encoding='utf-8')
    for token in ('/api/firmware/','/api/wifi/','/api/diagnostic/','Ap01Firmware',
                  'Ap01WifiSwitch0041','manualAuth','flashOpen(','ap01SelectSlot0041('):
        assert token not in source,(name,token)
for name in ('firmware.h','firmware_smoke.cpp','ota_image.h','slot_0041.h','native_wifi_switch_0041.h'):
    assert not (repo/'ports/ap01'/name).exists(),name

owned=('native_firmware_0041.cpp','native_slot_0041.cpp','native_wifi_switch_0041.cpp',
       'native_wifi_switch_port_0041.cpp','native_device_code_menu_0041.cpp',
       'native_maintenance_0041.cpp','native_services_0041.cpp','native_http_transport_0041.cpp')
for name in owned:
    assert (platform/name).is_file(),name
    source=(platform/name).read_text(encoding='utf-8')
    assert '#include "application.h"' not in source,name
    assert '#include "business.h"' not in source,name

if args.audit:
    audit=json.loads(args.audit.read_text(encoding='utf-8'))
    assert audit['passed']
    # Manifest source paths may be emitted by Windows or WSL.
    paths={Path(row['source'].replace('\\','/')).name:row['source'].replace('\\','/') for row in audit['objects']}
    for name in owned:
        assert '/cuktech-ap01-firmware/tooling/firmware-modules/' in paths[name],name

# App-side build entries may forward arguments, never compile/link platform code.
for name in ('check-ap01-native.py','link-ap01-native.py','ap01_firmware_build.py'):
    text=(repo/'tools'/name).read_text(encoding='utf-8')
    for token in ('subprocess','-march=','--gc-sections','native_services_0041.cpp'):
        assert token not in text,(name,token)
config=json.loads((repo/'ports/ap01/build-0041.json').read_text(encoding='utf-8'))
assert config['format']==1
for relative in config['sources']+config['plain_sources']+config['include_dirs']:
    assert (repo/relative).resolve().is_relative_to(repo),relative
    assert (repo/relative).exists(),relative

builder=runpy.run_path(str(repo/'tools/build-ap01-rom.py'))
assert Path(builder['main'].__code__.co_filename).resolve().is_relative_to(platform.parent/'scripts')
print('AP01 boundary passed: App protocol/display only; OTA/slots/Wi-Fi/menu/HTTP policies and ROM packaging owned by firmware')
