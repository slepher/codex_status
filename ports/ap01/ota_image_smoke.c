/* Host-only positive/negative image admission checks, shared with 0031. */
#include "ota_image.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static unsigned checks;
static uint32_t crc(uint32_t v,const unsigned char *p,size_t n) {
    while(n--){v^=*p++;for(unsigned i=0;i<8;++i)v=(v>>1)^(0xedb88320u&(0u-(v&1)));}return v;
}
static void put(unsigned char *p,uint32_t n,int big) {
    for(unsigned i=0;i<4;++i)p[i]=(unsigned char)(n>>(8*(big?3-i:i)));
}
static void fix(unsigned char *p,size_t n) {
    put(p+252,crc(0xffffffffu,p,252)^0xffffffffu,0);
    put(p+n-4,crc(0,p,n-4),0);
}
static void check(const unsigned char *p,uint32_t n,size_t available,size_t width,int expected) {
    ota_image image;int valid=ota_image_begin(&image,n)==0;
    for(size_t at=0;valid&&at<available;){size_t count=available-at;if(count>width)count=width;
        valid=ota_image_chunk(&image,p+at,count)==0;at+=count;}
    if(valid)valid=ota_image_valid(&image);
    if(valid!=expected)fprintf(stderr,"case=%u length=%u available=%zu valid=%d expected=%d\n",checks,n,available,valid,expected);
    assert(valid==expected);++checks;
}
static unsigned char *fixture(uint32_t n) {
    unsigned char *p=calloc(1,n);assert(p);
    const char tag[]="0x5245434f56455259544147";
    memcpy(p+n-40,tag,sizeof(tag)-1);put(p+n-8,n,1);fix(p,n);return p;
}
int main(int argc,char **argv) {
    assert(argc>=2);
    for(int a=1;a<argc;++a) {
        FILE *f=fopen(argv[a],"rb");assert(f);assert(!fseek(f,0,SEEK_END));long size=ftell(f);
        assert(size>=296&&size<=0x700000);assert(!fseek(f,0,SEEK_SET));
        unsigned char *p=malloc((size_t)size);assert(p);assert(fread(p,1,(size_t)size,f)==(size_t)size);assert(!fclose(f));
        check(p,(uint32_t)size,(size_t)size,7,1);
        /* Board/config/entry/span and both historical model positions are opaque. */
        const unsigned positions[]={0,8,0x10,0x64,0x78,0x7c,0x84,0xb0,0x4d48,0x4f00};
        for(unsigned i=0;i<sizeof(positions)/sizeof(positions[0]);++i)p[positions[i]]^=0x5a;
        fix(p,(size_t)size);check(p,(uint32_t)size,(size_t)size,1024,1);
        p[0]^=1;put(p+size-4,crc(0,p,(size_t)size-4),0);
        check(p,(uint32_t)size,(size_t)size,1024,0); /* header CRC */
        fix(p,(size_t)size);p[512]^=1;
        check(p,(uint32_t)size,(size_t)size,1024,0); /* body CRC */
        fix(p,(size_t)size);p[size-40]^=1;fix(p,(size_t)size);
        check(p,(uint32_t)size,(size_t)size,1024,0); /* recovery framing */
        p[size-40]^=1;fix(p,(size_t)size);put(p+size-8,(uint32_t)size-1,1);fix(p,(size_t)size);
        check(p,(uint32_t)size,(size_t)size,1024,0); /* declared length */
        put(p+size-8,(uint32_t)size,1);fix(p,(size_t)size);
        check(p,(uint32_t)size,(size_t)size-1,1024,0);
        check(p,(uint32_t)size-1,(size_t)size,1024,0);free(p);
    }
    const uint32_t sizes[]={296,327,0x700000};
    for(unsigned i=0;i<3;++i){unsigned char *p=fixture(sizes[i]);check(p,sizes[i],sizes[i],i?1024:1,1);free(p);}
    ota_image image;assert(ota_image_begin(&image,295));assert(ota_image_begin(&image,0x700001));checks+=2;
    printf("{\"passed\":true,\"checks\":%u,\"scope\":\"capacity, framing and integrity only; cross-version layouts accepted\"}\n",checks);
    return 0;
}
