"""Offline 0041 ABI work with the frozen private THead model, never a device.
Run elevated: Ghidra creates project/cache directories. Original ROM is read only.
"""
from pathlib import Path
import hashlib
import json
import subprocess
import sys
import shutil

repo = Path(__file__).resolve().parents[1]
firmware = repo.parent / 'cuktech-ap01-firmware'
private = firmware / 'evidence/rea-0041-20261017'
out = repo / 'artifacts/ap01-platform-integration/device/ghidra-native-0041'
assert out.is_dir(), 'Precreate the isolated output directory with elevation'
rom = firmware / 'roms/0041/ap01-1.0.2_0041.bin'
assert hashlib.sha256(rom.read_bytes()).hexdigest() == '972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9'
profile = json.loads((private / 'profile.json').read_text(encoding='utf-8'))
verified = []
for item in profile['inputs']:
    path = item['path'].replace('\\', '/')
    marker = '/private-0041-20261017/'
    relative = path.split(marker, 1)[-1]
    if not relative.startswith('ghidra/'):
        continue
    actual = private / relative
    assert hashlib.sha256(actual.read_bytes()).hexdigest() == item['sha256'], actual
    verified.append({'path': str(actual), 'sha256': item['sha256']})
assert len(verified) >= 10
addresses = sys.argv[1:] or ['0xa005a736', '0xa008bc64', '0xa00d91c6', '0xa00dd144', '0xa00afc88']
assert all(a.startswith('0x') and 0x9ffff000 <= int(a, 16) < 0xa0673c28 for a in addresses)
seed = out / 'native-seeds.tsv'
seed.write_text(''.join(a + '\tfunction\n' for a in addresses), encoding='utf-8', newline='\n')
repair_seed = out / 'native-seeds.json'
repair_seed.write_text(json.dumps({'seeds': addresses}), encoding='utf-8')
scripts = private / 'rea/bridge/ghidra'
for source in [scripts / name for name in ('ReaGhidraPrepareRaw.java', 'ReaGhidraApplySeeds.java',
                                         'Ap01NoReturnRepair.java')]:
    shutil.copyfile(source, out / source.name)
shutil.copyfile(repo / 'ports/ap01/Ap01NativeExports.java', out / 'Ap01NativeExports.java')
command = [str(private / 'ghidra/support/analyzeHeadless.bat'), str(out), 'native0041']
if (out / 'native0041.gpr').exists():
    command += ['-process', rom.name, '-noanalysis']
else:
    command += ['-import', str(rom), '-loader', 'BinaryLoader', '-loader-baseAddr', '0x9ffff000',
                '-processor', 'RISCV:LE:32:ap01thead2', '-cspec', 'gcc',
                '-preScript', 'ReaGhidraPrepareRaw.java', 'RISCV:LE:32:ap01thead2', '0x9ffff000', '0xa0000000']
command += ['-scriptPath', str(out),
            '-preScript', 'ReaGhidraApplySeeds.java', str(seed),
            '-analysisTimeoutPerFile', '300', '-postScript', 'Ap01NoReturnRepair.java',
            str(out / 'native-repair.json'), str(repair_seed),
            '-postScript', 'Ap01NativeExports.java', str(out / 'native-exports.json'), *addresses]
(out / 'analysis-inputs.json').write_text(json.dumps({'verified_model_inputs': verified,
    'command': command, 'rom_sha256': profile['source_sha256']}, indent=2), encoding='utf-8')
(out / 'native-exports.json').unlink(missing_ok=True)
with (out / 'headless.log').open('w', encoding='utf-8') as log:
    subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=True)
assert (out / 'native-exports.json').is_file(), 'See headless.log for script errors'
print(out / 'native-exports.json')
