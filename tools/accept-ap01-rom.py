"""Release an existing 0041 image only after matching native simulation evidence.
This tool never contacts a device or authorizes flashing. Run after all probes.
"""
from pathlib import Path
import argparse, hashlib, json, shutil, zlib
from importlib.util import spec_from_file_location, module_from_spec

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'artifacts/ap01-platform-integration/device'

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def read(name):
    return json.loads((OUT / name).read_text(encoding='utf-8'))

def partition_tables(data):
    tables = []
    for offset in (0xe000, 0xf000):
        table = data[offset:offset+596]
        count = int.from_bytes(table[6:8], 'little')
        if table[:4] != b'BFPT' or not 1 <= count <= 16:
            continue
        if zlib.crc32(table[:12]) != int.from_bytes(table[12:16], 'little'):
            continue
        entries = table[16:16+count*36]
        if zlib.crc32(entries) != int.from_bytes(table[16+count*36:20+count*36], 'little'):
            continue
        fw = next(entries[i:i+36] for i in range(0, len(entries), 36) if entries[i] == 0)
        tables.append({'offset':hex(offset), 'age':int.from_bytes(table[8:12], 'little'), 'slot':fw[2]})
    assert len(tables) == 2, 'both partition table copies must remain valid'
    return sorted(tables, key=lambda table:table['age'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', required=True)
    parser.add_argument('--fresh', required=True)
    parser.add_argument('--update', required=True)
    parser.add_argument('--recovery', required=True)
    parser.add_argument('--switch', help='native return-to-stable-slot report')
    parser.add_argument('--trial-cut', help='native boot interrupted immediately after stable prearm')
    parser.add_argument('--fallback', help='exact interrupted NOR stable-slot recovery report')
    parser.add_argument('--image-admission-report', type=Path, help='cross-version integrity-only validator regression report')
    parser.add_argument('--name', required=True)
    args = parser.parse_args()
    assert all(x.isdigit() for x in (args.candidate,args.fresh,args.update,args.recovery))
    assert args.switch is None or args.switch.isdigit()
    assert bool(args.trial_cut) == bool(args.fallback)
    assert not args.trial_cut or args.switch and args.trial_cut.isdigit() and args.fallback.isdigit()
    assert Path(args.name).name == args.name and args.name.endswith('.bin')
    source = OUT / f'ap01-native-0041-candidate-{args.candidate}.bin'
    spec = spec_from_file_location('ap01_builder', ROOT/'tools/build-ap01-rom.py')
    builder = module_from_spec(spec); spec.loader.exec_module(builder)
    manifest = builder.validate_candidate(source.read_bytes(),source.with_suffix('.manifest.json'))
    image_hash = digest(source)
    evidence = []
    def record(path):
        evidence.append({'path':str(path.relative_to(ROOT)), 'sha256':digest(path)})
    for key in ('elf','audit','link'):
        path = Path(manifest[key]['path'])
        assert digest(path) == manifest[key]['sha256'], key
        record(path)
    audit = json.loads(Path(manifest['audit']['path']).read_text(encoding='utf-8'))
    assert audit['passed']
    for item in audit['objects']:
        assert digest(Path(item['source'])) == item['source_sha256'], item['source']
    for item in audit['input_headers']:
        assert digest(Path(item['path'])) == item['sha256'], item['path']
    reports = {}
    for phase, number in (('fresh',args.fresh),('update',args.update),('recovery',args.recovery)):
        path = OUT/f'native-qemu-{number}.json'; report = read(path.name)
        assert report['passed'] and report['image_sha256'] == image_hash, phase
        assert report['native_runner_sha256'] == digest(ROOT/'tools/run-ap01-native-qemu.py'), phase
        assert report['native_execution_clean'] and report['native_service_probe_passed'], phase
        reports[phase] = report; record(path)
        if phase != 'update':
            assert report['native_application_smoke_passed']
            nav = report['native_navigation']
            assert nav['template_count'] == 8 and nav['forward_reverse_and_original_return']
            for scene in ('codex','weekly-only','zero','full'):
                pixels = OUT/f'native-qemu-{number}.{scene}.pixels.json'
                assert json.loads(pixels.read_text())['passed'], pixels
                record(pixels)
    recovery = reports['recovery']
    assert recovery['full_application_verified'] and recovery['native_firmware_receipt_recovered']
    assert recovery['active_slot'] == (0 if args.switch else 1)
    update = reports['update']
    assert update['native_firmware_protocol_passed']
    transaction = update['native_firmware_transaction']
    assert transaction['image_sha256'] == image_hash
    assert transaction['full_inactive_image_matches'] and transaction['protected_regions_unchanged']
    before = (OUT/f'native-qemu-{args.update}.before-ota.flash.bin').read_bytes()
    after_path = OUT/f'native-qemu-{args.update}.after-ota.flash.bin'
    after = after_path.read_bytes(); assert len(before) == len(after) == 0x1000000
    target = int(transaction['target_offset'],16)
    assert target == 0x7b0000 and after[target:target+source.stat().st_size] == source.read_bytes()
    for start,end in ((0,0xe000),(0x10000,0x7b0000),(0xff0000,0x1000000)):
        assert before[start:end] == after[start:end], 'protected region'
    record(after_path)
    if args.switch:
        path=OUT/f'native-qemu-{args.switch}.json';switch=read(path.name)
        assert switch['passed'] and switch['image_sha256']==image_hash and switch['active_slot']==1
        assert switch['native_slot_switch_passed'] and switch['native_firmware_receipt_recovered']
        assert recovery['native_slot_receipt_recovered']
        fixture=OUT/f'native-0041-nor-{args.switch}.bin'
        assert fixture.read_bytes()==after
        record(path)
        before_switch=(OUT/f'native-qemu-{args.switch}.before-slot-switch.flash.bin').read_bytes()
        after_path=OUT/f'native-qemu-{args.switch}.after-slot-switch.flash.bin'
        after=after_path.read_bytes()
        for start,end in ((0,0xe000),(0x10000,0xeb0000),(0xff0000,0x1000000)):
            assert before_switch[start:end]==after[start:end], 'stable switch changed firmware/protected bytes'
        assert switch['native_slot_transaction']['single_commit_after_replay']
        record(after_path)
        abi_path=OUT/'slots-0041-boardctl-evidence.json'
        abi=json.loads(abi_path.read_text(encoding='utf-8'))
        assert abi['source_sha256']==builder.ORIGINAL_SHA
        assert any(f['address']=='0xa0055a14' and 'pseudocode' in f for f in abi['functions'])
        record(abi_path)
        if args.trial_cut:
            model_path = OUT/'boot-safety-qemu-model.json'
            model = read(model_path.name)
            assert model['passed'] and digest(OUT/'native-qemu-boot-safety-engine') == model['engine_sha256']
            assert digest(ROOT/'tools/build-ap01-boot-safety-qemu.py') == model['builder_sha256']
            model_tests = OUT/'boot-safety-flash-model-tests.json'
            assert digest(model_tests) == model['test_report_sha256'] and read(model_tests.name)['passed']
            source_check = OUT/'boot-safety-model-source-check.json'
            checked = read(source_check.name)
            differences = [item for item in checked['inputs'] if not item['matches_frozen']]
            assert checked['passed'] and len(differences) == 1
            assert differences[0]['file'] == 'qemu-ap01-flash.h' and differences[0]['sha256'] == model['flash_header_sha256']
            for report in (reports['fresh'],update,recovery,switch):
                assert report['qemu_sha256'] == model['engine_sha256']
            record(model_path); record(model_tests); record(source_check)
            assert switch['native_runner_sha256'] == digest(ROOT/'tools/run-ap01-native-qemu.py')
            assert switch['native_execution_clean'] and switch['native_service_probe_passed']
            assert switch['native_application_smoke_passed'] and switch['native_boot_safety'] == 'healthy_locked'
            armed_path = OUT/f'native-qemu-{args.switch}.trial-armed.flash.bin'
            locked_path = OUT/f'native-qemu-{args.switch}.healthy-locked.flash.bin'
            armed, locked = armed_path.read_bytes(), locked_path.read_bytes()
            armed_tables, locked_tables = partition_tables(armed), partition_tables(locked)
            assert armed_tables == switch['native_boot_prearmed']['tables']
            assert locked_tables == switch['native_boot_health_verified']['tables']
            assert armed_tables[-1]['slot'] == 0 and locked_tables[-1]['slot'] == 1
            assert locked_tables[-1]['age'] == armed_tables[-1]['age']+1
            assert locked_tables[0] == armed_tables[-1], 'healthy commit must preserve the prearmed stable table'
            calls = switch['native_ff13_calls']
            assert len(calls) == 3 and all(call['result'] == 0 for call in calls)
            elapsed = calls[1]['guest_uptime_ms']-calls[0]['guest_uptime_ms']
            assert 9000 <= elapsed < 30000 and elapsed == switch['native_boot_health_verified']['elapsed_ms']
            for start,end in ((0,0xe000),(0x10000,0xeb0000),(0xff0000,0x1000000)):
                assert armed[start:end] == locked[start:end], 'boot health changed firmware/protected bytes'
            record(armed_path); record(locked_path)
            cut_path = OUT/f'native-qemu-{args.trial_cut}.json'
            fallback_path = OUT/f'native-qemu-{args.fallback}.json'
            cut, fallback = read(cut_path.name), read(fallback_path.name)
            for name, report in (('trial cut',cut), ('fallback',fallback)):
                assert report['passed'] and report['image_sha256'] == image_hash, name
                assert report['native_runner_sha256'] == digest(ROOT/'tools/run-ap01-native-qemu.py'), name
                assert report['native_execution_clean'], name
                assert report['qemu_sha256'] == model['engine_sha256'], name
            assert cut['active_slot'] == 1 and cut['native_boot_trial_cut']
            assert len(cut['native_ff13_calls']) == 1 and cut['native_ff13_calls'][0]['result'] == 0
            original_fixture = fixture.read_bytes()
            assert (OUT/f'native-0041-nor-{args.trial_cut}.bin').read_bytes() == original_fixture
            interrupted_path = OUT/f'native-qemu-{args.trial_cut}.trial-armed.flash.bin'
            interrupted = interrupted_path.read_bytes()
            assert partition_tables(interrupted)[-1]['slot'] == 0
            assert (OUT/f'native-0041-nor-{args.fallback}.bin').read_bytes() == interrupted
            assert fallback['flash_fixture_sha256'] == digest(OUT/f'native-0041-nor-{args.fallback}.bin')
            assert fallback['active_slot'] == 0 and fallback['full_application_verified']
            assert fallback['native_service_probe_passed'] and fallback['native_boot_fallback_receipt']
            assert not fallback['native_ff13_calls'], 'stable boot must not auto-select the failed working slot'
            for start,end in ((0,0xe000),(0x10000,0xeb0000),(0xff0000,0x1000000)):
                assert interrupted[start:end] == original_fixture[start:end]
            for number in (args.switch,args.fallback):
                for scene in ('codex','weekly-only','zero','full'):
                    pixels = OUT/f'native-qemu-{number}.{scene}.pixels.json'
                    assert json.loads(pixels.read_text())['passed'], pixels
                    record(pixels)
            record(cut_path); record(interrupted_path); record(fallback_path)
    recovery_fixture = OUT/f'native-0041-nor-{args.recovery}.bin'
    assert digest(recovery_fixture) == recovery['flash_fixture_sha256']
    assert recovery_fixture.read_bytes() == after, 'recovery must boot the exact updated NOR'
    for filename in ('native-app-report.json','native-firmware-host-report.json'):
        report = read(filename); assert report['passed'], filename
        if 'image_sha256' in report: assert report['image_sha256'] == image_hash
        if args.trial_cut and filename == 'native-firmware-host-report.json':
            assert digest(ROOT/'ports/ap01/firmware_smoke.cpp') == report['test_source_sha256']
        binary = OUT/('native-firmware-smoke' if 'firmware' in filename else 'native-app-smoke')
        assert digest(binary) == report['binary_sha256'], filename
        record(OUT/filename)
    if args.image_admission_report:
        path = args.image_admission_report.resolve()
        admission = json.loads(path.read_text())
        assert admission['passed'] and len(admission['validators']) == 2
        for index, name in enumerate(('0041','0031')):
            item = admission['validators'][index]
            validator_source = ROOT/'ports/ap01/ota_image.c' if name == '0041' else ROOT.parent/'cuktech-ap01-firmware/tooling/firmware-modules/ota_image.c'
            assert item['passed'] and item['source_sha256'] == digest(validator_source) and item['checks'] == 29
            assert item['test_source_sha256'] == digest(ROOT/'ports/ap01/ota_image_smoke.c')
            assert item['binary_sha256'] == digest(path.parent/('image-host-'+name))
        record(path); record(ROOT/'ports/ap01/ota_image_smoke.c')
    for filename, expected in (('native-core-final-tests.log','162 passed; 0 failed'),
                               ('native-render-final-tests.log','15 passed; 0 failed'),
                               ('native-v2-state-final-tests.log','21 passed; 0 failed'),
                               ('native-bridge-final-build.log','Finished `dev` profile')):
        path = OUT/filename
        assert expected in path.read_text(encoding='utf-8-sig',errors='replace'), filename
        record(path)
    storage = ROOT/'artifacts/ap01-platform-integration/storage/bundle-storage.json'
    measurements = json.loads(storage.read_text())['measurements']
    assert len(measurements) == 8 and all(x['first_install']=='applied' and x['second_install']=='applied' for x in measurements)
    record(storage)
    bridge = ROOT/'bridge/target-note4-deploy/debug/bridge-app.exe'
    destination = OUT/args.name
    assert digest(source) == image_hash and source.stat().st_size == manifest['image']['bytes'], 'packaging source identity changed'
    if destination.exists(): assert digest(destination) == image_hash
    else: shutil.copyfile(source,destination)
    assert digest(destination) == image_hash and destination.stat().st_size == manifest['image']['bytes'], 'packaged ROM differs from candidate'
    limitations = [
        'Physical AP01 first 0041 boot, LCD/SPI completion and actual storage free space are unverified; no device was flashed.',
        'QEMU uses explicit synthetic MAC, loopback sockets, LVGL input, fbdev readiness and Boot2/peripheral fixtures.',
        'Old 0031 independent maintenance receiver uses another model offset; direct 0031-to-0041 transfer through that receiver is not validated.',
        'Traditional and independent OTA channels must be used sequentially; there is no shared cross-channel lock.',
        'Chinese fonts and templates are delivered by an explicitly published complete Bundle, not embedded in the ROM.'
    ]
    if args.trial_cut:
        limitations.append('Automatic boot protection starts only after the maintenance service prearms a confirmed stable slot; earlier Boot2/startup failures and physical power loss are unverified.')
    acceptance = {'passed':True,'scope':'complete 0041 image format and native application/update/recovery simulation',
                  'acceptance_tool_sha256':digest(Path(__file__)),
                  'hardware_verified':False,'device_flashed':False,'image_sha256':image_hash,
                  'stable_slot_switch_verified':bool(args.switch),
                  'boot_safety_window_verified':bool(args.trial_cut),
                  'integrity_only_image_admission_verified':bool(args.image_admission_report),
                  'matching_elf_sha256':manifest['elf']['sha256'],'evidence':evidence,
                  'bridge_candidate':{'path':str(bridge),'bytes':bridge.stat().st_size,'sha256':digest(bridge),'deployed':False},
                  'budget':audit['budget'],'limitations':limitations}
    acceptance_path = destination.with_suffix('.acceptance.json')
    acceptance_path.write_text(json.dumps(acceptance,indent=2,ensure_ascii=False),encoding='utf-8')
    manifest.update(flashable=True,simulator_verified=True,hardware_verified=False,
                    stable_slot_switch_verified=bool(args.switch),
                    boot_safety_window_verified=bool(args.trial_cut),
                    integrity_only_image_admission_verified=bool(args.image_admission_report),
                    release_scope='image-format/native-simulation accepted; awaiting separately authorized hardware validation',
                    limitations=limitations,acceptance={'path':str(acceptance_path),'sha256':digest(acceptance_path)})
    manifest['image']['path'] = str(destination)
    destination.with_suffix('.manifest.json').write_text(json.dumps(manifest,indent=2,ensure_ascii=False),encoding='utf-8')
    destination.with_suffix('.sha256').write_text(image_hash+'  '+destination.name+'\n',encoding='ascii')
    print(json.dumps({'rom':str(destination),'bytes':destination.stat().st_size,'sha256':image_hash,
                      'simulator_verified':True,'hardware_verified':False},ensure_ascii=False))

if __name__ == '__main__':
    main()
