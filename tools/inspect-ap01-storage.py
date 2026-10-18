"""Read-only 0041 partition/GIF evidence. No ROM rewriting or device access."""
from pathlib import Path
import hashlib,json,struct
repo=Path(__file__).resolve().parents[1]
rom_path=repo.parent/'cuktech-ap01-firmware/roms/0041/ap01-1.0.2_0041.bin'
out=repo/'artifacts/ap01-platform-integration/storage'
assert out.is_dir(), 'Precreate the evidence directory with elevation'
rom=rom_path.read_bytes();digest=hashlib.sha256(rom).hexdigest()
assert digest=='972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9'
base=0x9ffff000
def offset(pointer,size):
 value=pointer-base
 assert 0<=value<=len(rom)-size
 return value
def gif_length(start):
 assert rom[start:start+6] in (b'GIF87a',b'GIF89a')
 at=start+13
 if rom[start+10]&128:at+=3*(1<<((rom[start+10]&7)+1))
 frames=0
 def blocks(at):
  while True:
   assert at<len(rom);n=rom[at];at+=1
   if not n:return at
   at+=n;assert at<=len(rom)
 while True:
  assert at<len(rom);kind=rom[at];at+=1
  if kind==0x3b:return at-start,frames
  if kind==0x21:at+=1;at=blocks(at)
  elif kind==0x2c:
   assert at+9<=len(rom);packed=rom[at+8];at+=9
   if packed&128:at+=3*(1<<((packed&7)+1))
   at+=1;at=blocks(at);frames+=1
  else:raise ValueError('not a valid contiguous GIF')
pets=[]
for i in range(5):
 wrapper=struct.unpack_from('<I',rom,offset(0xa055fe44+i*4,4))[0]
 size,pointer=struct.unpack_from('<II',rom,offset(wrapper,20)+12)
 at=offset(pointer,size);length,frames=gif_length(at)
 assert length==size and struct.unpack_from('<HH',rom,at+6)==(320,240)
 pets.append(dict(index=i,descriptor_va=hex(wrapper),file_offset=hex(at),bytes=size,frames=frames,
                  sha256=hashlib.sha256(rom[at:at+size]).hexdigest()))
all_gifs=[];at=0
while at<len(rom):
 choices=[v for magic in (b'GIF87a',b'GIF89a') if (v:=rom.find(magic,at))>=0]
 if not choices:break
 start=min(choices);at=start+6
 try:length,frames=gif_length(start)
 except (ValueError,AssertionError,IndexError):continue
 all_gifs.append(dict(file_offset=hex(start),bytes=length,frames=frames,
     pet_page=any(p['file_offset']==hex(start) for p in pets)))
 at=start+length
report=dict(firmware_target='cuktech-ap01-0041',original=dict(path=str(rom_path),bytes=len(rom),sha256=digest),
 resource_partition=dict(physical_start='0xeb0000',bytes=0x140000,end_exclusive='0xff0000',mount='/resource',filesystem='littlefs',
  source_function_va='0xa0059762',setup_window_va=['0xa0059afa','0xa0059b4e'],
  setup_window_sha256=hashlib.sha256(rom[0x5aafa:0x5ab4e]).hexdigest(),actual_used_bytes=None,actual_free_bytes=None),
 data_partition=dict(physical_start='0x710000',bytes=0x10000,mount='/data'),
 pet_gif_table_va='0xa055fe44',pet_gifs=pets,pet_gif_total_bytes=sum(p['bytes'] for p in pets),
 all_valid_contiguous_gifs=all_gifs,
 distinction='GIF extents are in the firmware XIP image. Deleting them does not increase /resource filesystem capacity or free bytes.',
 limits=['Resource geometry is verified from 0041 static initialization, not a live statfs sample.',
         'The official update container contains firmware; it does not contain a physical /resource partition dump.',
         'Unpopulated FF resource in QEMU fixtures is a model assumption, never physical free-space evidence.',
         'A whole pet-page removal must also remove navigation/selection/descriptor/timer callbacks; a GIF placeholder is not that removal.'])
(out/'rom-storage.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(dict(resource_total_bytes=0x140000,resource_actual_free_bytes=None,pet_gif_bytes=report['pet_gif_total_bytes'],
                    pet_gif_count=len(pets),all_valid_gif_count=len(all_gifs),report=str(out/'rom-storage.json'))))
