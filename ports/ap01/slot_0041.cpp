// Exact 0041 partition table/posix ABI. No erase or write of firmware slots.
#include "slot_0041.h"
#include "reference_0041.h"
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
    int fd;
    explicit Reader(unsigned slot):fd(FN(0xa003f4f6,int (*)(const char *,int,...))(slot?"/dev/fw2":"/dev/fw1",1,0)){}
    ~Reader(){if(fd>=0)FN(0xa0026888,int (*)(int))(fd);}
    bool read(uint32_t offset,uint8_t *bytes,unsigned length) {
        if(fd<0||FN(0xa00281d8,int32_t (*)(int,int32_t,int))(fd,offset,0)!=int32_t(offset))return false;
        while(length){int n=FN(0xa003f6a2,int (*)(int,void *,size_t))(fd,bytes,length);if(n<=0||unsigned(n)>length)return false;bytes+=n;length-=n;}
        return true;
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
    if(slot>1)return false;Reader file(slot);uint8_t manifest[52];
    if(!file.read(0x675000,manifest,sizeof(manifest)))return false;
    uint32_t length=6769704;bool custom=!memcmp(manifest,"AP01APP1",8);
    if(custom){if(le32(manifest+8)!=1||le32(manifest+16)!=0x675000)return false;length=le32(manifest+12);}
    if(running&&custom&&memcmp(manifest,(const void *)(uintptr_t)0xa0674000,sizeof(manifest)))return false;
    if(length<6769704||length>0x700000)return false;
    ota_image image;if(ota_image_begin(&image,length))return false;
    uint8_t bytes[1024];
    for(uint32_t at=0;at<length;) {
        unsigned n=length-at>sizeof(bytes)?sizeof(bytes):length-at;
        if(!file.read(at,bytes,n)||(running&&!at&&memcmp(bytes,(const void *)(uintptr_t)0x9ffff000,256))||ota_image_chunk(&image,bytes,n))return false;
        at+=n;
    }
    if(!ota_image_valid(&image,ap01ReferenceHeader))return false;
    out.length=length;out.crc=image.recovery_crc;
    const char *hex="0123456789abcdef";
    // An all-zero build explicitly represents unmodified stock 0041.
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
