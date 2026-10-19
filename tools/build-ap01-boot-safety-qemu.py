"""Build an isolated QEMU variant admitting both boot partition tables.

WSL only, elevated caller: creates private source/build trees. Firmware and
frozen sibling sources stay unchanged. Active firmware remains write-protected.
"""
from pathlib import Path
import argparse, hashlib, json, shutil, subprocess, time

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT/'artifacts/ap01-platform-integration/device'
SCRIPTS = ROOT.parent/'cuktech-ap01-firmware/tooling/scripts'

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source', type=Path, required=True)
    p.add_argument('--work', type=Path, required=True)
    a = p.parse_args()
    assert OUT.is_dir() and a.source.is_dir()
    header = a.source/'hw/riscv/qemu-ap01-flash.h'
    original = header.read_text()
    assert sha(header) == sha(SCRIPTS/'qemu-ap01-flash.h'), 'baseline model differs from frozen evidence'
    old = '''        uint32_t table = s->fw1_active ? 0xe000 : 0xf000;
        if ((address >= inactive && (uint64_t)address + length <= inactive + 0x700000) ||
            (address >= table && (uint64_t)address + length <= table + 0x1000)) {'''
    new = '''        /* Both redundant boot tables are writable; active FW is still protected. */
        if ((address >= inactive && (uint64_t)address + length <= inactive + 0x700000) ||
            (address >= 0xe000 && (uint64_t)address + length <= 0xf000) ||
            (address >= 0xf000 && (uint64_t)address + length <= 0x10000)) {'''
    assert original.count(old) == 1
    source, build = a.work/'source', a.work/'build'
    if not a.work.exists():
        shutil.copytree(a.source, source, symlinks=True)
        build.mkdir()
        (source/'hw/riscv/qemu-ap01-flash.h').write_text(original.replace(old,new))
    assert (source/'hw/riscv/qemu-ap01-flash.h').read_text() == original.replace(old,new)
    if not (build/'build.ninja').exists():
        subprocess.run([str(source/'configure'),'--target-list=riscv32-softmmu',
            '--disable-docs','--disable-werror','--disable-tools','--disable-guest-agent',
            '--disable-user','--disable-gtk','--disable-sdl','--disable-opengl',
            '--disable-vnc','--disable-curses','--without-default-devices'],cwd=build,check=True)
    subprocess.run(['ninja','-j','8','qemu-system-riscv32'],cwd=build,check=True)
    engine = OUT/'native-qemu-boot-safety-engine'
    if engine.exists(): assert sha(engine) == sha(build/'qemu-system-riscv32')
    else: shutil.copyfile(build/'qemu-system-riscv32',engine)
    engine.chmod(0o755)
    # Reuse every frozen native guard/reset/snapshot check, extending only PTs.
    test = (SCRIPTS/'test-qemu-ota-flash.py').read_text()
    test = test.replace('sys.dont_write_bytecode = True', 'sys.dont_write_bytecode = True\nsys.path.insert(0,'+repr(str(SCRIPTS))+')')
    test = test.replace("Path(__file__).with_name('run-qemu-ap01.py')",repr(str(SCRIPTS/'run-qemu-ap01.py')))
    test = test.replace('(inactive,inactive_end-0x1000,inactive_pt)', '(inactive,inactive_end-0x1000,inactive_pt,active_pt)')
    test = test.replace('protected=(0,active_pt,active,', 'protected=(0,active,')
    test = test.replace('(inactive-4,inactive_pt-4)', '(inactive-2,0xdffe)')
    test_path = OUT/'boot-safety-flash-model-test.py'
    test_path.write_text(test)
    report_path = OUT/f'boot-safety-flash-model-tests-{time.time_ns()}.json'
    subprocess.run(['python3','-B',str(test_path),'--qemu',str(engine),'--rom-shim',
        str(ROOT.parent/'cuktech-ap01-firmware/evidence/qemu/0041-20261017/qemu-bootrom-shim-v12.bin'),
        '--output',str(report_path)],check=True)
    assert json.loads(report_path.read_text())['passed']
    shutil.copyfile(report_path,OUT/'boot-safety-flash-model-tests.json')
    report = {'passed':True,'scope':'explicit both-PT aperture; active FW/Boot2/protected NOR still denied',
        'engine_sha256':sha(engine),'baseline_flash_header_sha256':sha(header),
        'flash_header_sha256':sha(source/'hw/riscv/qemu-ap01-flash.h'),
        'builder_sha256':sha(Path(__file__)),'test_sha256':sha(test_path),
        'test_report_sha256':sha(report_path),'source':str(source),'build':str(build)}
    (OUT/'boot-safety-qemu-model.json').write_text(json.dumps(report,indent=2))
    assert sha(header) == report['baseline_flash_header_sha256']
    print(json.dumps(report))

if __name__ == '__main__':
    main()
