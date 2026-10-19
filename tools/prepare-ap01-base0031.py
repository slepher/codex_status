"""Prepare isolated native 0031 maintenance/recovery fixtures, never hardware."""
from pathlib import Path
import json, subprocess

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT/'artifacts/ap01-base0031-20261019'
FW = ROOT.parent/'cuktech-ap01-firmware'

def linux(path):
    path = path.resolve()
    return '/mnt/'+path.drive[0].lower()+path.as_posix()[2:]

for prefix in ('base0031-upload-v4','base0031-recovery-v4'):
    image = OUT/(prefix+'-rom.bin')
    if not image.exists():
        subprocess.run(['wsl','-d','Ubuntu-24.04','--','python3','-B',
            linux(FW/'tooling/scripts/build-ota-expanded-0031.py'),
            '--original',linux(FW/'roms/0031/ap01-1.0.2_0031.bin'),
            '--payload',linux(OUT/(prefix+'.bin')),'--elf',linux(OUT/(prefix+'.elf')),
            '--output',linux(image),'--auth-key-file',linux(OUT/'auth.key'),
            '--mode','http','--label',prefix],check=True)

release = (OUT/'ap01-base0031-v2-rom.bin').read_bytes()
upload = (OUT/'base0031-upload-v4-rom.bin').read_bytes()
assert release[:0x67e000] == upload[:0x67e000], 'native upload prefix fixture mismatch'
template = (ROOT/'artifacts/ap01-platform-integration/ota-minimal-20261019/minimal-0031-nor.bin').read_bytes()
for index, (name, running_name, inactive) in enumerate((
    ('upload-production','base0031-upload-v4-rom.bin',None),
    ('recovery-good-v4','base0031-recovery-v4-rom.bin',release),
    ('recovery-extra-marker-v4','base0031-recovery-v4-rom.bin',
     (ROOT/'artifacts/ap01-platform-integration/ota-minimal-20261019/minimal-0031-v1-rom.bin').read_bytes()),
)):
    running = (OUT/running_name).read_bytes()
    flash = bytearray(template)
    flash[0x10000:0x710000] = b'\xff'*0x700000
    flash[0x10000:0x10000+len(running)] = running
    if inactive is not None:
        flash[0x7b0000:0xeb0000] = b'\xff'*0x700000
        flash[0x7b0000:0x7b0000+len(inactive)] = inactive
    fixture = OUT/(name+'-nor.bin')
    if fixture.exists():
        assert fixture.read_bytes() == flash
    else:
        fixture.write_bytes(flash)
    command = ['wsl','-d','Ubuntu-24.04','--','python3','-B',
        linux(FW/'tooling/scripts/run-qemu-ota-channel.py'),
        '--qemu',linux(ROOT/'artifacts/ap01-platform-integration/device/native-qemu-engine'),
        '--image',linux(OUT/running_name),'--manifest',linux((OUT/running_name).with_suffix('.manifest.json')),
        '--flash-fixture',linux(fixture),'--rom-shim',linux(FW/'evidence/qemu/0041-20261017/qemu-bootrom-shim-v12.bin'),
        '--profile',linux(OUT/'profile.json'),'--output',linux(OUT/('native-'+name+'.json')),
        '--port',str(39310+index*3),'--timeout','235','--safety-seconds','220',
        '--suppress-scan','--native-loopback']
    if name == 'upload-production':
        command.append('--require-diagnostic-probe')
    (OUT/('command-'+name+'.json')).write_text(json.dumps(command,indent=2),encoding='utf-8')
print('Prepared production-upload and actual0031 recovery fixtures')
