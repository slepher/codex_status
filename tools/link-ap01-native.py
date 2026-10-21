"""Link the owned native application into an XIP ELF. No device/flash action.
This script does NOT call an unverified module a flashable firmware.
Run elevated: compiler/linker may create private temporary directories.
"""
from pathlib import Path
import os,subprocess,json,hashlib,argparse
repo=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser();parser.add_argument('--out',type=Path,default=repo/'artifacts/ap01-platform-integration/device');args=parser.parse_args()
out=args.out.resolve();assert out.is_dir()
audit=json.loads((out/'native-core-audit.json').read_text(encoding='utf-8'))
assert audit['passed']
for item in audit['objects']:
    assert hashlib.sha256(Path(item['source']).read_bytes()).hexdigest()==item['source_sha256'],'source changed: rerun native audit'
for item in audit['input_headers']:
    assert hashlib.sha256(Path(item['path']).read_bytes()).hexdigest()==item['sha256'],'header changed: rerun native audit'
(out/'native-link-report.json').write_text(json.dumps({'linked':False,'flashable':False}),encoding='utf-8')
tool=Path(os.environ['USERPROFILE'])/'.platformio/packages/toolchain-riscv32-esp/bin/riscv32-esp-elf'
sources=[Path(x['file']) for x in audit['objects'] if not x['file'].endswith('abi_budget.o')]
elf=out/'ap01-native-0041.elf'
cmd=[str(tool)+'-g++.exe','-march=rv32imafc','-mabi=ilp32f','-nostdlib','-mno-relax',
     '-Wl,--no-relax,--gc-sections','-Wl,-T,'+str(repo.parent/'cuktech-ap01-firmware/tooling/firmware-modules/native_application_0041.ld'),
     '-Wl,-Map,'+str(out/'ap01-native-0041.map'),*[str(p) for p in sources],
     '-Wl,--start-group','-lm','-lgcc','-Wl,--end-group','-o',str(elf)]
subprocess.run(cmd,check=True)
undefined=subprocess.check_output([str(tool)+'-nm.exe','-u',str(elf)],text=True).strip()
assert not undefined,undefined
sections=subprocess.check_output([str(tool)+'-size.exe','-A',str(elf)],text=True)
subprocess.run([str(tool)+'-objcopy.exe','-O','binary',str(elf),str(out/'ap01-native-0041.module.bin')],check=True)
report={'linked':True,'flashable':False,'scope':'fully linked native XIP module; ROM packaging/boot/UI acceptance still required',
    'audit_sha256':hashlib.sha256((out/'native-core-audit.json').read_bytes()).hexdigest(),
    'module_sha256':hashlib.sha256((out/'ap01-native-0041.module.bin').read_bytes()).hexdigest(),
    'elf':{'path':str(elf),'bytes':elf.stat().st_size,'sha256':hashlib.sha256(elf.read_bytes()).hexdigest()},
    'sections':sections,'undefined_imports':undefined,'command':cmd}
(out/'native-link-report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
print(json.dumps(report['elf']))
