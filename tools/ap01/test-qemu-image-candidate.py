#!/usr/bin/env python3
"""Frozen unsigned-OTA candidate selection/FW0/XIP checks; no device or OTA."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import time
import uuid
sys.dont_write_bytecode = True


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ('qemu','original','candidate','output'):
        parser.add_argument('--'+key,type=Path,required=True)
    args=parser.parse_args()
    qemu,original,candidate,output=(getattr(args,k).resolve() for k in ('qemu','original','candidate','output'))
    if output.exists() or not output.parent.is_dir():parser.error('new output in existing directory required')
    root=Path(__file__).parent
    runner=load('probe',root/'run-qemu-ap01.py')
    fixture=load('fixture',root/'build-qemu-flash-fixture.py')
    token='candidate-'+uuid.uuid4().hex
    bad=output.parent/(token+'-bad.bin')
    flash=output.parent/(token+'-fw0.bin')
    probe_output=output.parent/(token+'-probe.json')
    report=dict(device_accessed=False,ota_performed=False,checks=[])
    def expect(ok,message):
        if not ok:raise AssertionError(message)
    def command(argv,success):
        result=subprocess.run([sys.executable,'-B',*map(str,argv)],capture_output=True,text=True,timeout=15)
        expect((result.returncode==0)==success,'unexpected command result: '+result.stderr[-800:])
        return result
    process=gs=None
    gdb=Path('/tmp')/(token+'.gdb')
    try:
        data=candidate.read_bytes()
        damaged=bytearray(data);damaged[0x1100]^=1
        with bad.open('xb') as file:file.write(damaged)
        for module in (runner,fixture):
            expect(module.validate_running_image(original.read_bytes())['kind']=='original0041','default original acceptance')
            for image,reference in ((data,None),(bytes(damaged),original)):
                try:module.validate_running_image(image,reference)
                except ValueError:pass
                else:raise AssertionError('invalid/default candidate accepted')
            identity=module.validate_running_image(data,original)
            expect(identity['current_sha256']==runner.UNSIGNED_OTA_SHA256 and identity['patch_manifest'],'candidate identity')
        report['checks'].append('both validators retain original default and require exact recomputed candidate')
        base=[root/'run-qemu-ap01.py','--qemu',qemu,'--image',candidate,'--output',probe_output,'--steps','1','--timeout','5']
        command(base,False)
        command([root/'run-qemu-ap01.py','--qemu',qemu,'--image',bad,'--output',probe_output,'--unsigned-ota-candidate',original],False)
        command(base+['--unsigned-ota-candidate',original],True)
        probe=json.loads(probe_output.read_text())
        expect(probe['source_unchanged'] and probe['image_sha256']==runner.UNSIGNED_OTA_SHA256,'runner current-image identity')
        report['checks'].append('runner CLI default/bad reject and explicit candidate executes bounded native probe')
        fixture_base=[root/'build-qemu-flash-fixture.py','--image',candidate,'--output',flash]
        command(fixture_base,False)
        command(fixture_base+['--unsigned-ota-candidate',original],True)
        expect(flash.read_bytes()[0x10000:0x10000+len(data)]==data,'FW0 does not match selected candidate')
        manifest=json.loads(flash.with_suffix('.manifest.json').read_text())
        expect(manifest['running_image']['current_sha256']==runner.UNSIGNED_OTA_SHA256,'fixture manifest identity')
        report['checks'].append('fixture default rejects candidate; explicit FW0 matches every selected byte')
        deadline=time.monotonic()+8
        with (output.parent/(token+'.log')).open('x') as log:
            process=subprocess.Popen([str(qemu),'-M','ap01','-cpu','rv32','-kernel',str(candidate),'-S','-display','none','-serial','none','-monitor','none','-gdb',f'unix:{gdb},server=on,wait=off'],stdin=subprocess.DEVNULL,stdout=log,stderr=log,preexec_fn=runner.child_limits)
            gs=runner.connect(gdb,process,deadline)
            rsp=runner.RSP(gs,deadline);rsp.request('?')
            # Verify actual mapped patch instruction and refreshed recovery CRC.
            for offset,length in ((0x707fc,2),(len(data)-4,4)):
                mapped=bytes.fromhex(rsp.request(f'm{0x9ffff000+offset:x},{length:x}'))
                expect(mapped==data[offset:offset+length],'native XIP differs from selected patch bytes')
        report['checks'].append('QEMU native XIP contains patched branch and candidate recovery CRC')
        report.update(passed=True,runner_probe=str(probe_output),fixture=str(flash),running_image=identity,qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest())
    except Exception as exc:
        report.update(passed=False,error=f'{type(exc).__name__}: {exc}')
    finally:
        if gs is not None:gs.close()
        if process is not None:
            if process.poll() is None:
                process.terminate()
                try:process.wait(timeout=2)
                except subprocess.TimeoutExpired:process.kill();process.wait(timeout=2)
            if gdb.exists():gdb.unlink()
        with output.open('x') as file:json.dump(report,file,indent=2);file.write('\n')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__=='__main__':sys.exit(main())
