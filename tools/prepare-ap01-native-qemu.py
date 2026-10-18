"""Make a private NOR/profile for the complete native candidate; no hardware.
Uses only existing output directories. Run elevated with the QEMU runner.
"""
from pathlib import Path
import json,hashlib,subprocess,argparse
from importlib.util import spec_from_file_location,module_from_spec
ROOT=Path(__file__).resolve().parents[1];out=ROOT/'artifacts/ap01-platform-integration/device'
firmware=ROOT.parent/'cuktech-ap01-firmware';evidence=firmware/'evidence/qemu/0041-20261017'
spec=spec_from_file_location('native_builder',ROOT/'tools/build-ap01-rom.py');builder=module_from_spec(spec);spec.loader.exec_module(builder)
parser=argparse.ArgumentParser();parser.add_argument('--candidate',default='01');parser.add_argument('--run',default='01');parser.add_argument('--resume-nor',type=Path);a=parser.parse_args()
assert a.candidate.isdigit() and a.run.isdigit()
image=out/('ap01-native-0041-candidate-'+a.candidate+'.bin');manifest=image.with_suffix('.manifest.json')
builder.validate_candidate(image.read_bytes(),manifest)
source=a.resume_nor.resolve() if a.resume_nor else evidence/'flash-synthetic-0041-v3.bin'
if a.resume_nor:assert source.parent==out.resolve() and source.name.startswith('native-qemu-') and source.name.endswith('.flash.bin')
flash=bytearray(source.read_bytes());assert len(flash)==0x1000000
flash[0x10000:0x710000]=b'\xff'*0x700000
flash[0x10000:0x10000+image.stat().st_size]=image.read_bytes()
target=out/('native-0041-nor-'+a.run+'.bin')
if target.exists():assert target.read_bytes()==flash,'private NOR identity mismatch'
else:target.write_bytes(flash)
profile=json.loads((evidence/'ui-empty-scan-continuous-125.json').read_text(encoding='utf-8'))
def linux(p):
    p=p.resolve();return '/mnt/'+p.drive[0].lower()+p.as_posix()[2:]
command=profile['command'];machine=command[command.index('-M')+1].split(',')
for i,field in enumerate(machine):
    if field.startswith('otp-fixture='):machine[i]='otp-fixture='+linux(evidence/'otp-synthetic-ap01.bin')
command[command.index('-M')+1]=','.join(machine)
profile_path=out/'native-0041-qemu-profile.json';profile_path.write_text(json.dumps({'command':command}),encoding='utf-8')
engine=out/'native-qemu-engine'
if not engine.exists():subprocess.run(['wsl','-d','Ubuntu-24.04','--','cp','/home/slepher/qemu-ap01-0041/build/qemu-system-riscv32',linux(engine)],check=True)
# Latest native model, recorded by sibling qemu-ota-0031-20261017.md P4/P5.
# This is deliberately not labelled as the older 0041 7246b77a engine.
assert hashlib.sha256(engine.read_bytes()).hexdigest()=='a549e66f1f7680e30a7d491733ff2b95ff2de5a9f972dd70b7fa306b1c50da18'
args=['wsl','-d','Ubuntu-24.04','--','python3','-B',linux(ROOT/'tools/run-ap01-native-qemu.py'),
      '--qemu',linux(engine),'--image',linux(image),'--manifest',linux(manifest),
      '--flash-fixture',linux(target),'--rom-shim',linux(evidence/'qemu-bootrom-shim-v12.bin'),
      '--profile',linux(profile_path),'--output',linux(out/('native-qemu-'+a.run+'.json')),
      '--port',str(21880+3*int(a.run)),'--safety-seconds','840','--timeout','900',
      '--observe-pc','0xa005d180']
(out/'native-qemu-command.json').write_text(json.dumps(args,indent=2),encoding='utf-8')
print('Private NOR/profile prepared; execute the recorded bounded command')
