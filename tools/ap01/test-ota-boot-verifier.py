#!/usr/bin/env python3
"""In-memory negative tests of committed synthetic NOR selection; no QEMU."""
import argparse
import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import struct
import sys
from types import SimpleNamespace
from unittest.mock import patch
import zlib
sys.dont_write_bytecode = True


class MemoryFile:
    def __init__(self, data=b''):
        self.data = data
        self.parent = self
        self.written = None
    def read_bytes(self): return self.data
    def exists(self): return False
    def is_dir(self): return True
    def __str__(self): return '<memory-report>'
    @contextlib.contextmanager
    def open(self, *args, **kwargs):
        stream = io.StringIO()
        yield stream
        self.written = stream.getvalue()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('before','after','image','manifest','output'):
        parser.add_argument('--'+name,type=Path,required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists() or not output.parent.is_dir(): parser.error('new report in existing directory required')
    tool = Path(__file__).with_name('verify-qemu-ota-boot.py')
    spec = importlib.util.spec_from_file_location('verifier',tool)
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)
    old, new, image = args.before.read_bytes(), args.after.read_bytes(), args.image.read_bytes()
    report = dict(qemu_executed=False,hardware_verified=False,checks=[],
                  verifier_sha256=hashlib.sha256(tool.read_bytes()).hexdigest(),
                  original_snapshot_sha256=hashlib.sha256(old).hexdigest(),
                  committed_snapshot_sha256=hashlib.sha256(new).hexdigest())
    previous,_=verifier.select(old)
    current,_=verifier.select(new)
    table=current['address']; primary=previous['address']

    def entry_field(data, address, offset, value):
        struct.pack_into('<I',data,address+16+offset,value)
        struct.pack_into('<I',data,address+52,zlib.crc32(data[address+16:address+52]))
    def table_age(data,address,value):
        struct.pack_into('<I',data,address+8,value)
        struct.pack_into('<I',data,address+12,zlib.crc32(data[address:address+12]))
    def check(name,before=old,after=new,upload=image,accepted=False):
        sink=MemoryFile()
        namespace=SimpleNamespace(before=MemoryFile(before),after=MemoryFile(after),image=MemoryFile(upload),manifest=args.manifest,output=sink)
        try:
            with patch.object(verifier.argparse.ArgumentParser,'parse_args',return_value=namespace), contextlib.redirect_stdout(io.StringIO()):
                verifier.main()
        except ValueError as exc:
            if accepted: raise
            assert sink.written is None, 'failure wrote success report'
            report['checks'].append(dict(name=name,passed=True,rejected=str(exc)))
        else:
            assert accepted, name+' unexpectedly accepted'
            assert json.loads(sink.written)['passed']
            report['checks'].append(dict(name=name,passed=True,accepted=True))
    try:
        check('actual successful127 snapshot passes current strict verifier',accepted=True)
        for value in (1,len(image),0xffffffff):
            data=bytearray(new);entry_field(data,table,28,value)
            check('preserved zero entry.length rejects '+str(value),after=data)
        for value in (0,2,0xffffffff):
            data=bytearray(new);entry_field(data,table,32,value)
            check('entry.age must advance exactly once: '+str(value),after=data)
        for value in (0,previous['age'],previous['age']+2,0xffffffff):
            data=bytearray(new);table_age(data,table,value)
            check('table.age tie/backward/skip rejected: '+str(value),after=data)
        before=bytearray(old);after=bytearray(new)
        entry_field(before,primary,32,0xffffffff);entry_field(after,primary,32,0xffffffff);entry_field(after,table,32,0)
        check('entry.age FFFFFFFF to0 wrap rejected',before,after)
        before=bytearray(old);after=bytearray(new)
        table_age(before,primary,0xffffffff);table_age(after,primary,0xffffffff);table_age(after,table,0)
        check('unsigned table.age FFFFFFFF to0 wrap rejected',before,after)
        data=bytearray(new);data[current['image_offset']+0x1000]^=1
        check('selected NOR image differs despite unchanged authenticated upload',after=data)
        data=bytearray(image);data[0x1000]^=1
        check('wrong uploaded image fails manifest identity',upload=data)
        for label,address in (('Boot2',0),('activePT',primary+0x80),('activeFW',previous['image_offset']+0x1000),('gap',0x720000),('reservedTail',0xff0000)):
            data=bytearray(new);data[address]^=1
            check('protected '+label+' mutation rejected',after=data)
        report['passed']=True
    except Exception as exc:
        report.update(passed=False,error=f'{type(exc).__name__}: {exc}')
    with output.open('x') as stream:json.dump(report,stream,indent=2);stream.write('\n')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__=='__main__':sys.exit(main())
