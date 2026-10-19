"""Validate native OTA/recovery evidence without changing raw simulator results."""
from pathlib import Path
import hashlib, importlib.util, json, re

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT/'artifacts/ap01-base0031-20261019'
FW = ROOT.parent/'cuktech-ap01-firmware'

def read(name):
    return json.loads((OUT/name).read_text())

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def tables(data):
    import zlib
    result = []
    for offset in (0xe000,0xf000):
        p = data[offset:offset+596]
        count = int.from_bytes(p[6:8],'little')
        assert p[:4] == b'BFPT' and 1 <= count <= 16
        assert zlib.crc32(p[:12]) == int.from_bytes(p[12:16],'little')
        entries = p[16:16+count*36]
        assert zlib.crc32(entries) == int.from_bytes(p[16+count*36:20+count*36],'little')
        fw = next(entries[i:i+36] for i in range(0,len(entries),36)
                  if entries[i] == 0 and entries[i+3:i+6] == b'FW\0')
        result.append({'age':int.from_bytes(p[8:12],'little'),'slot':fw[2]})
    return sorted(result,key=lambda p:p['age'])

image_path = OUT/'ap01-base0031-v2-rom.bin'
image = image_path.read_bytes()
assert read('host-checks.json')['passed']
assert read('request-tests.json')['passed'] and read('diagnostic-tests.json')['passed']
ota = read('native-upload-verification.json')
assert ota['passed'] and ota['validated_images'] == [sha(image_path)]
upload = read('native-upload-production.json')
assert upload['native_loopback'] and upload['provider_calls'] == {}
uart = (OUT/'native-upload-production.json.uart.log').read_bytes()
for marker in (b'DIAG_STATUS_BUSY_13_OK',b'DIAG_FLASH_BUSY_OK',b'DIAG_CONCURRENT_WRITE_BUSY_OK'):
    assert marker in uart, marker
assert bytes(v^85 for v in (OUT/'base0031-upload-v4.upload-tail.bin').read_bytes()) == image[0x67e000:]
assert (OUT/'base0031-upload-v4-rom.bin').read_bytes()[:0x67e000] == image[:0x67e000]

recovery = {}
for name, expected in (('recovery-good-v4',0),('recovery-extra-marker-v4',-1)):
    report = read('native-'+name+'.json')
    assert not report.get('error') and not report.get('first_guest_exception')
    assert report['native_loopback'] and report['provider_calls'] == {}
    log = (OUT/('native-'+name+'.json.uart.log')).read_bytes()
    # UART lines can interleave after the numeric return; table/slot evidence
    # independently proves whether the original function committed anything.
    match = re.search(rb'DIAG_RECOVERY\s*\{\s*"result"\s*:\s*(-?\d+)',log)
    assert match and int(match[1]) == expected
    before = (OUT/(name+'-nor.bin')).read_bytes()
    snapshot = Path(report['flash_snapshot']['path'])
    after = snapshot.read_bytes()
    assert sha(snapshot) == report['flash_snapshot']['sha256']
    assert before[0x10000:0x710000] == after[0x10000:0x710000]
    assert before[0x7b0000:0xeb0000] == after[0x7b0000:0xeb0000]
    assert before[:0xe000] == after[:0xe000]
    previous, selected = tables(before)[-1], tables(after)[-1]
    if expected == 0:
        assert selected == {'age':previous['age']+1,'slot':1}
        assert len(report['native_ff13_calls']) == 1
        assert after[0x7b0000:0x7b0000+len(image)] == image
    else:
        assert before[0xe000:0x10000] == after[0xe000:0x10000]
        assert not report['native_ff13_calls'] and selected == previous
    recovery[name] = {'result':expected,'previous':previous,'selected':selected,
                      'both_rom_slots_unchanged':True,'snapshot_sha256':sha(snapshot)}

for name in ('native-production-boot-slot1.json','native-recovery-boot-slot1.json'):
    boot = read(name)
    assert boot['passed'] and boot['active_slot'] == 1 and boot['native_ui_started']
    assert boot['image_sha256'] == sha(image_path) and not boot['reset_requested']
    assert not boot.get('error') and not boot.get('first_guest_exception')

spec = importlib.util.spec_from_file_location('builder',FW/'tooling/scripts/build-ota-expanded-0031.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
manifest_path = image_path.with_suffix('.manifest.json')
manifest = builder.validate_candidate(image,manifest_path)
assert manifest['original_gif_preserved'] and manifest['cloud_ota_dispatch_modified'] is False
assert builder.recovery_markers(image)[1] == len(image)-40
assert (OUT/'base0031-v2.bin').read_bytes() == (OUT/'base0031-repro-v2.bin').read_bytes()
assert (OUT/'base0031-v2.elf').read_bytes() == (OUT/'base0031-repro-v2.elf').read_bytes()

evidence_names = ('host-checks.json','diagnostic-tests.json','request-tests.json',
                 'native-upload-verification.json','native-upload-production.json',
                 'native-recovery-good-v4.json','native-recovery-extra-marker-v4.json',
                 'native-production-boot-slot1.json','native-recovery-boot-slot1.json','original-recovery.json',
                 'resources.json','release-provenance.json')
result = {'passed':True,'scope':'0031 maintenance-only ROM; native TCP OTA, busy diagnostics, slot commit, original recovery and new-slot boot',
          'image_sha256':sha(image_path),'image_bytes':len(image),'elf_sha256':sha(OUT/'base0031-v2.elf'),
          'recovery':recovery,'recovery_marker_offsets':builder.recovery_markers(image),
          'no_business_application_added':True,'device_accessed':False,'physical_device_verified':False,
          'evidence':{name:sha(OUT/name) for name in evidence_names},
          'limitations':['QEMU radio, Boot2, GPIO and Flash peripherals use explicit fixtures; physical LCD/power-loss/timing remain unverified.',
                         'Native upload/recovery raw safety reports stop on intentional reset and remain false; domain checks above validate actual commit and snapshots.',
                         'The failed host socket proxy run and the initial recovery probe with missing task environment remain preserved as failed evidence.',
                         'Traditional OTA and independent OTA must be used sequentially; this guard coordinates independent OTA and diagnostic physical reads.']}
(OUT/'acceptance.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
manifest.update(simulator_verified=True,flashable=True,physical_device_verified=False,
                acceptance={'path':str(OUT/'acceptance.json'),'sha256':sha(OUT/'acceptance.json')})
manifest_path.write_text(json.dumps(manifest,indent=2),encoding='utf-8')
(OUT/'ap01-base0031-v2-rom.sha256').write_text(sha(image_path)+'  '+image_path.name+'\n',encoding='ascii')
print(json.dumps({k:result[k] for k in ('passed','image_sha256','image_bytes','elf_sha256','device_accessed')}))
