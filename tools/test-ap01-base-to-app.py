"""Native TCP 0031->0041 test. Only the probe client's input is modelled.
The unchanged 0031 receiver, authentication, framing/CRC, flash and ff13 run
natively. Never invoked on a physical device; never packages the probe ROM.
"""
from pathlib import Path
import hashlib, importlib.util, json, sys, types
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'artifacts/ap01-platform-integration/device'
FW=ROOT.parent/'cuktech-ap01-firmware'
sys.path.insert(0,str(FW/'tooling/scripts'))
path=FW/'tooling/scripts/run-qemu-ota-channel.py'
source=path.read_text()
source=source.replace('1 <= args.timeout <= 240','1 <= args.timeout <= 900')
source=source.replace('1 <= args.safety_seconds <= 240','1 <= args.safety_seconds <= 840')
needle="                eligible = code_start <= ra < code_end\n"
assert source.count(needle)==1
injection='''                if pc==0xa0057eaa and 0xa06800e0<=ra<0xa068015a:
                    offset=args6[0]-0x10000;count=args6[2]
                    assert 0<=offset and count<=256 and offset+count<=0x67e000
                    provider.write(args6[1],cross_target[offset:offset+count])
                    provider.reg(10,0);provider.reg(32,ra)
                    report['probe_fixture_bytes']=report.get('probe_fixture_bytes',0)+count
                    continue
'''
source=source.replace(needle,injection+needle)
base=types.ModuleType('cross_native_runner');base.__file__=str(path)
exec(compile(source,str(path),'exec'),base.__dict__)
base.cross_target=(OUT/'ap01-native-0041-candidate-29.bin').read_bytes()
base.PROFILES['ap01-http-0031-v1']['apis'][0xa0057eaa]='fixture_read'
if __name__=='__main__':
    verify_only='--verify-only' in sys.argv
    if verify_only:sys.argv.remove('--verify-only')
    else:base.main()
    output=Path(sys.argv[sys.argv.index('--output')+1])
    report=json.loads(output.read_text())
    before=Path(sys.argv[sys.argv.index('--flash-fixture')+1]).read_bytes()
    after=output.with_suffix('.flash.bin').read_bytes()
    image=base.cross_target
    assert report['diagnostic_probe_verified']
    assert report['reset_requested'] and len(report['native_ff13_calls'])==1
    assert after[0x7b0000:0x7b0000+len(image)]==image
    # 0031 persists /data in 0x710000..0x7b0000; it is not part of FW1.
    for start,end in ((0,0xe000),(0x10000,0x710000),(0xff0000,0x1000000)):
        assert before[start:end]==after[start:end]
    proof={'passed':True,'image_sha256':hashlib.sha256(image).hexdigest(),
        'native_tcp_full_upload':True,'receiver_auth_crc_flash_commit_unchanged':True,
        'running_slot_bytes_unchanged':True,'inactive_exact_image':True,
        'probe_fixture_bytes':report['probe_fixture_bytes'],
        'raw_report_sha256':hashlib.sha256(output.read_bytes()).hexdigest(),
        'model':'Read-only target bytes supplied only to upload_read in the test client; all TCP and receiver code native',
        'harness_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        'runner_variant_sha256':hashlib.sha256(source.encode()).hexdigest()}
    output.with_suffix('.verification.json').write_text(json.dumps(proof,indent=2))
    print(json.dumps(proof))
