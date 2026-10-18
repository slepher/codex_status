"""Reconstruct a complete 0041 candidate, preserving every stock resource.
An unverified candidate remains flashable=false. No tool here contacts hardware.
"""
from pathlib import Path
import argparse,hashlib,json,os,struct,subprocess,zlib,shutil
ROOT=Path(__file__).resolve().parents[1]
BASE=0x9ffff000
OFFSET=0x675000
HOOK=0x5e180
ORIGINAL_SHA='972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9'
FORMAT='ap01-native-app-0041-v1'
def sha(data):return hashlib.sha256(data).hexdigest()
def call(target):
    delta=target-(BASE+HOOK)
    hi=(delta+0x800)>>12;lo=delta-(hi<<12)
    return struct.pack('<II',((hi&0xfffff)<<12)|(1<<7)|0x17,((lo&0xfff)<<20)|(1<<15)|(1<<7)|0x67)
def assemble(original,payload,entry):
    if sha(original)!=ORIGINAL_SHA or len(original)!=6769704:raise ValueError('exact original 0041 required')
    if original[HOOK:HOOK+8]!=bytes.fromhex('97001500e780404d'):raise ValueError('0041 UI callsite mismatch')
    if payload[:8]!=b'AP01APP1' or len(payload)<=256 or entry&1 or not BASE+OFFSET+256<=entry<BASE+OFFSET+len(payload):raise ValueError('native payload/entry')
    length=((OFFSET+len(payload)+15)&~15)+40
    if length>0x700000:raise ValueError('FW slot capacity')
    out=bytearray(original[:-40]);out.extend(b'\xff'*(length-len(out)))
    out[OFFSET:OFFSET+len(payload)]=payload
    out[HOOK:HOOK+8]=call(entry)
    out[-40:]=original[-40:]
    struct.pack_into('>I',out,length-8,length)
    struct.pack_into('<I',out,length-4,zlib.crc32(out[:-4],0xffffffff)^0xffffffff)
    if out[:0x1000]!=original[:0x1000]:raise ValueError('boot/header region changed')
    if out[HOOK+8:len(original)-40]!=original[HOOK+8:-40] or out[:HOOK]!=original[:HOOK]:raise ValueError('unexpected stock byte modification')
    return bytes(out)
def validate_candidate(data,manifest_path):
    m=json.loads(Path(manifest_path).read_text(encoding='utf-8'))
    if m['format']!=FORMAT:raise ValueError('manifest format')
    original=Path(m['original']['path']).read_bytes();payload=Path(m['payload']['path']).read_bytes()
    if sha(payload)!=m['payload']['sha256']:raise ValueError('payload identity')
    h=bytearray(payload[:256]);stored=struct.unpack_from('<I',h,32)[0];struct.pack_into('<I',h,32,0)
    if zlib.crc32(h)!=stored or struct.unpack_from('<I',h,28)[0]!=zlib.crc32(payload[256:]):raise ValueError('payload checksums')
    if struct.unpack_from('<4I',h,8)!=(1,len(data),OFFSET,len(payload)):raise ValueError('payload bounds')
    entry=struct.unpack_from('<I',h,24)[0]
    if data!=assemble(original,payload,entry) or sha(data)!=m['image']['sha256']:raise ValueError('candidate reconstruction mismatch')
    return m
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
    output=a.output.resolve();out=ROOT/'artifacts/ap01-platform-integration/device'
    if not output.parent.is_dir():p.error('precreate output directory with elevation')
    original_path=ROOT.parent/'cuktech-ap01-firmware/roms/0041/ap01-1.0.2_0041.bin'
    original=original_path.read_bytes();payload=bytearray((out/'ap01-native-0041.module.bin').read_bytes())
    linked=json.loads((out/'native-link-report.json').read_text(encoding='utf-8'))
    audit=json.loads((out/'native-core-audit.json').read_text(encoding='utf-8'))
    for item in audit['objects']:
        if sha(Path(item['source']).read_bytes())!=item['source_sha256']:p.error('source changed: rerun native audit/link')
    for item in audit['input_headers']:
        if sha(Path(item['path']).read_bytes())!=item['sha256']:p.error('header changed: rerun native audit/link')
    if not linked.get('linked') or sha(payload)!=linked.get('module_sha256') or sha((out/'native-core-audit.json').read_bytes())!=linked.get('audit_sha256'):
        p.error('current module/audit/link fingerprints do not agree')
    if payload[:256]!=bytes(256):p.error('linked module must have a zero manifest')
    tool=Path(os.environ['USERPROFILE'])/'.platformio/packages/toolchain-riscv32-esp/bin/riscv32-esp-elf'
    listing=subprocess.check_output([str(tool)+'-nm.exe','-n',str(out/'ap01-native-0041.elf')],text=True)
    symbols={line.split()[2]:int(line.split()[0],16) for line in listing.splitlines() if len(line.split())==3 and line.split()[0]!='U'}
    entry=symbols['ap01Register0041'];assert symbols['ap01_native_manifest']==BASE+OFFSET
    abi_file=out/'native-runtime-abi.bin'
    subprocess.run([str(tool)+'-objcopy.exe','-O','binary','--only-section=.abi',str(out/'ap01-native-0041.elf'),str(abi_file)],check=True)
    abi=struct.unpack('<17I',abi_file.read_bytes());assert abi[0]==1
    length=((OFFSET+len(payload)+15)&~15)+40
    h=bytearray(256);h[:8]=b'AP01APP1';struct.pack_into('<7I',h,8,1,length,OFFSET,len(payload),entry,zlib.crc32(payload[256:]),0)
    h[36:52]=hashlib.sha256(payload[256:]).digest()[:16];struct.pack_into('<3I',h,52,7,18080,18081)
    h[64:64+len(b'cuktech-ap01-0041')]=b'cuktech-ap01-0041'
    h[96:96+len(b'lcd-jd9853-320x240-palette4')]=b'lcd-jd9853-320x240-palette4'
    h[128:160]=bytes.fromhex(ORIGINAL_SHA)
    h[160:192]=hashlib.sha256((out/'native-core-audit.json').read_bytes()).digest()
    struct.pack_into('<I',h,32,zlib.crc32(h));payload[:256]=h
    image=assemble(original,bytes(payload),entry)
    payload_path=output.with_suffix('.payload.bin');manifest_path=output.with_suffix('.manifest.json')
    elf_path=output.with_suffix('.elf');audit_path=output.with_suffix('.audit.json');link_path=output.with_suffix('.link.json');map_path=output.with_suffix('.map')
    if any(path.exists() for path in (output,payload_path,manifest_path,elf_path,audit_path,link_path,map_path)):p.error('candidate outputs must be new files')
    for source,destination in ((out/'ap01-native-0041.elf',elf_path),(out/'ap01-native-0041.map',map_path),
                               (out/'native-core-audit.json',audit_path),(out/'native-link-report.json',link_path)):
        shutil.copyfile(source,destination)
    payload_path.write_bytes(payload);output.write_bytes(image)
    manifest={'format':FORMAT,'firmware_target':'cuktech-ap01-0041','build_id':h[36:52].hex(),
        'flashable':False,'hardware_verified':False,'simulator_verified':False,
        'original':{'path':str(original_path.resolve()),'sha256':ORIGINAL_SHA,'bytes':len(original)},
        'payload':{'path':str(payload_path),'sha256':sha(payload),'bytes':len(payload),'entry':hex(entry)},
        'elf':{'path':str(elf_path),'sha256':sha(elf_path.read_bytes()),'bytes':elf_path.stat().st_size},
        'audit':{'path':str(audit_path),'sha256':sha(audit_path.read_bytes())},
        'link':{'path':str(link_path),'sha256':sha(link_path.read_bytes())},
        'native_abi':{'ui_timer':hex(symbols['_ZN12_GLOBAL__N_17Runtime5timerEPv']),
            'ui_event':hex(symbols['_ZN12_GLOBAL__N_17Runtime13eventCallbackEPv']),
            'lv_obj_send_event':'0xa00d831a',
            'instance_bytes':dict(zip(('runtime','business','bundle_store','application'),abi[1:5])),
            'runtime_offsets':dict(zip(('root','content','dialog','image','pairPending','pairWoken','focused','shownGeneration','templateCount','retired','ready','uiStage'),abi[5:]))},
        'image':{'path':str(output),'sha256':sha(image),'bytes':len(image)},
        'preserved':['boot header/entry','original ROMFS and rcS','all five pet GIFs','all original bytes outside one 8-byte UI registration hook'],
        'limitations':['native application/update/recovery/resource acceptance pending','physical BSP/LCD and first 0041 hardware boot unverified']}
    manifest_path.write_text(json.dumps(manifest,indent=2),encoding='utf-8')
    validate_candidate(image,manifest_path)
    print(json.dumps(manifest['image']))
if __name__=='__main__':main()
