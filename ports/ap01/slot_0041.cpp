// Exact 0041 partition table/raw Flash read ABI. No erase or write of slots.
#include "slot_0041.h"
#include "v2_sync.h"
#include "v2_runtime.h"
#include <string.h>
extern "C" {
#include "ota_image.h"
}
#define FN(address,type) ((type)(uintptr_t)(address))
namespace {
uint32_t le32(const uint8_t *p){return uint32_t(p[0])|uint32_t(p[1])<<8|uint32_t(p[2])<<16|uint32_t(p[3])<<24;}
struct Reader {
    uint32_t base;
    explicit Reader(unsigned slot):base(slot?0x7b0000:0x10000){}
    bool read(uint32_t offset,uint8_t *bytes,unsigned length) {
        if(offset>0x700000||length>0x700000-offset)return false;
        // The original partition-table reader handles the physical Flash/cache
        // critical section. Avoid the VFS temporary block proxy for slot reads.
        return !FN(0xa0058e12,int (*)(uint32_t,void *,unsigned))(base+offset,bytes,length);
    }
};
}
int ap01SelectedSlot0041() {
    // 0041 ff13 table; copy index at 62fc8cf4 is separate from FW activeIndex.
    const uint8_t *p=(const uint8_t *)(uintptr_t)0x62fc8cf8;
    unsigned count=unsigned(p[6])|unsigned(p[7])<<8;
    if(le32(p)!=0x54504642||!count||count>16||v2Crc32(p,12)!=le32(p+12)||v2Crc32(p+16,count*36)!=le32(p+16+count*36))return -1;
    for(unsigned i=0;i<count;++i) {
        const uint8_t *entry=p+16+i*36;if(entry[0])continue;
        if(entry[2]>1||le32(entry+12)!=0x10000||le32(entry+16)!=0x7b0000||le32(entry+20)!=0x700000||le32(entry+24)!=0x700000)return -1;
        return entry[2];
    }
    return -1;
}
bool ap01InspectSlot0041(unsigned slot,bool running,Ap01SlotImage &out) {
    out=Ap01SlotImage{};
    if(slot>1){out.error="slot_bounds";return false;}Reader file(slot);
    // Derive framing from the checked native recovery record, not an optional
    // application manifest or a firmware-version-specific image size.
    static const uint8_t encoded[]={0xb0,0xf8,0xb5,0xb2,0xb4,0xb5,0xb4,0xb3,0xb4,0xe6,0xb5,0xb6,
        0xb4,0xb5,0xb5,0xb2,0xb5,0xb9,0xb5,0xb4,0xb4,0xb1,0xb4,0xb7};
    uint8_t bytes[1063];uint32_t length=0;
    for(uint32_t at=0;at+40<=0x700000&&!length;at+=1024) {
        unsigned n=0x700000-at>sizeof(bytes)?sizeof(bytes):0x700000-at;
        if(!file.read(at,bytes,n)){out.error="slot_read";return false;}
        for(unsigned i=0;i+40<=n;++i) {
            unsigned j=0;for(;j<sizeof(encoded);++j)if(uint8_t(bytes[i+j]^0x80)!=encoded[j])break;
            if(j!=sizeof(encoded))continue;
            const uint8_t *p=bytes+i+32;
            uint32_t framed=uint32_t(p[0])<<24|uint32_t(p[1])<<16|uint32_t(p[2])<<8|p[3];
            if(framed==at+i+40&&framed>=296){length=framed;break;}
        }
    }
    if(!length){out.error="slot_recovery_length";return false;}
    ota_image image;if(ota_image_begin(&image,length))return false;
    for(uint32_t at=0;at<length;) {
        unsigned n=length-at>1024?1024:length-at;
        if(!file.read(at,bytes,n)){out.error="slot_read";return false;}
        if(running&&!at&&memcmp(bytes,(const void *)(uintptr_t)0x9ffff000,256)){out.error="slot_running_mapping";return false;}
        if(ota_image_chunk(&image,bytes,n)){out.error="slot_length";return false;}
        at+=n;
    }
    if(!ota_image_valid(&image)){out.error="slot_header_or_crc";return false;}
    out.length=length;out.crc=image.recovery_crc;
    uint8_t manifest[52];bool custom=length>=0x675000+sizeof(manifest)&&file.read(0x675000,manifest,sizeof(manifest))&&
        !memcmp(manifest,"AP01APP1",8)&&le32(manifest+8)==1&&le32(manifest+12)==length&&le32(manifest+16)==0x675000;
    const char *hex="0123456789abcdef";
    // Unknown optional build metadata does not prevent a valid rescue image.
    for(unsigned i=0;i<16;++i){uint8_t b=custom?manifest[36+i]:0;out.build[2*i]=hex[b>>4];out.build[2*i+1]=hex[b&15];}out.build[32]=0;
    return true;
}
int ap01SelectSlot0041(unsigned slot) {
    int selected=ap01SelectedSlot0041();
    volatile uint8_t *copy=(volatile uint8_t *)(uintptr_t)0x62fc8cf4;
    if(slot>1||selected<0||*copy>1||le32((const uint8_t *)(uintptr_t)0x62fc8d00)==UINT32_MAX)return -1;
    if(selected==int(slot))return 1;
    uint8_t previous=*copy;
    const char *path=slot?"/dev/fw2":"/dev/fw1";
    int result=FN(0xa0055a14,int (*)(uint32_t,const void *))(0xff13,path);
    if(result||ap01SelectedSlot0041()!=int(slot))return -1;
    // ff13 writes the other table but leaves this RAM selector unchanged.
    // Advance only after success, so the next commit preserves that table.
    *copy=previous^1;
    return 1;
}
