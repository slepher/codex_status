"""Check native I4 -> original LVGL RGB565 pixels; do not substitute LCD output."""
from pathlib import Path
import argparse,json,struct,zlib,hashlib

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'artifacts/ap01-platform-integration/device'
PALETTE=(0x0000,0xffff,0x56f9,0xfd48,0x7c51,0x2144,0xfaeb,0x4208,
         0x8410,0xbdd7,0x1947,0x2b8d,0x4493,0x5142,0xa384,0x10a2)

def png(path,pixels):
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
    rows=[]
    for y in range(240):
        row=bytearray([0])
        for x in range(320):
            c=int.from_bytes(pixels[(y*320+x)*2:(y*320+x)*2+2],'little')
            row.extend(((c>>11)*255//31,(c>>5&63)*255//63,(c&31)*255//31))
        rows.append(row)
    path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',320,240,8,2,0,0,0))+
                     chunk(b'IDAT',zlib.compress(b''.join(rows)))+chunk(b'IEND',b''))

def main():
    p=argparse.ArgumentParser();p.add_argument('--run',required=True);p.add_argument('--scene',default='codex');a=p.parse_args()
    assert a.run.isdigit() and a.scene.replace('-','').isalnum()
    prefix=OUT/f'native-qemu-{a.run}.{a.scene}'
    raw=prefix.with_suffix(prefix.suffix+'.palette4').read_bytes();assert len(raw)==38400
    expected=b''.join(struct.pack('<H',PALETTE[i]) for byte in raw for i in (byte>>4,byte&15))
    differences=[]
    for i in (0,1):
        actual=Path(str(prefix)+f'.fb{i}.rgb565').read_bytes();assert len(actual)==153600
        differences.append(sum(actual[at:at+2]!=expected[at:at+2] for at in range(0,len(actual),2)))
        png(Path(str(prefix)+f'.fb{i}.png'),actual)
    png(Path(str(prefix)+'.engine.png'),expected)
    result={'passed':min(differences)==0,'scene':a.scene,'different_pixels':differences,
            'native_palette4_crc':zlib.crc32(raw),'native_palette4_sha256':hashlib.sha256(raw).hexdigest(),
            'scope':'RV32 shared renderer pixels versus original LVGL framebuffer; not LCD/SPI completion'}
    Path(str(prefix)+'.pixels.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
    print(json.dumps(result));assert result['passed'],'original LVGL framebuffer differs from native template frame'
if __name__=='__main__':main()
