/* AP01 0041-only independent OTA port. No radio or Boot2 equivalence claim.
 * Builder must disable miIO.ota dispatch: its busy query is not an exclusion
 * claim. This single server owns one transaction; uncertain commits fence it.
 * All ROM imports were checked against the matching0041 image, ilp32f.
 */
#include "ota_channel.h"
#include <stdint.h>
#include <stddef.h>

#define FN(addr,type) ((type)(uintptr_t)(addr))
#define IMAGE_LEN 6769704u
#define MAN_OFF 0x1c7000u
#define PAYLOAD_MAX (0x223d96u-MAN_OFF)
#define BUILTIN_OFF 0x1be990u
#define RCS_OFF 0x1bef04u
#define RCS_LEN 97u
#define LIMIT_MS 120000u

extern const unsigned char ota_0041_manifest[256];

void *memcpy(void *d,const void *s,size_t n) { unsigned char *p=d;const unsigned char *q=s;while(n--)*p++=*q++;return d; }
void *memset(void *d,int c,size_t n) { unsigned char *p=d;while(n--)*p++=(unsigned char)c;return d; }
int memcmp(const void *a,const void *b,size_t n) { const unsigned char *p=a,*q=b;while(n--){if(*p!=*q)return (int)*p-*q;p++;q++;}return 0; }
size_t strlen(const char *s) { const char *p=s;while(*p)p++;return (size_t)(p-s); }
int strcmp(const char *a,const char *b) { while(*a&&*a==*b){a++;b++;}return (unsigned char)*a-(unsigned char)*b; }
char *strchr(const char *s,int c) { do {if(*s==(char)c)return (char *)s;}while(*s++);return NULL; }
char *strstr(const char *s,const char *n) { if(!*n)return (char *)s;for(;*s;s++){size_t i=0;while(n[i]&&s[i]==n[i])i++;if(!n[i])return (char *)s;}return NULL; }

typedef struct { int32_t fd,flip; } flash_io;
typedef struct { int32_t fd;uint32_t events,revents;void *priv[3]; } poll_fd;
typedef struct { uint16_t family,port;uint32_t addr;unsigned char pad[8]; } ip_addr;
_Static_assert(sizeof(poll_fd)==24,"0041 poll ABI");
_Static_assert(sizeof(ip_addr)==16,"0041 IPv4 ABI");
typedef struct {
    int listen_fd,fenced,reboot_returned;
    uint32_t last_ms;
    uint64_t high_ms;
} server;
typedef struct {
    server *owner;
    int fd,started,finished;
    flash_io flash;
    uint32_t position,code_crc,recovery_crc;
    unsigned char header[256],manifest[256],builtin[4],rcs[RCS_LEN],tail[40];
    unsigned char scratch[1024];
} connection;

static uint32_t le32(const unsigned char *p)
{ return (uint32_t)p[0]|(uint32_t)p[1]<<8|(uint32_t)p[2]<<16|(uint32_t)p[3]<<24; }
static uint32_t be32(const unsigned char *p)
{ return (uint32_t)p[3]|(uint32_t)p[2]<<8|(uint32_t)p[1]<<16|(uint32_t)p[0]<<24; }
static uint32_t crc(uint32_t value,const unsigned char *p,size_t n)
{ while(n--){value^=*p++;for(unsigned i=0;i<8;i++)value=(value>>1)^(0xedb88320u&(0u-(value&1)));}return value; }
static uint32_t manifest_crc(const unsigned char *m)
{ uint32_t v=0xffffffff;for(unsigned i=0;i<256;i++){unsigned char b=i>=32&&i<36?0:m[i];v=crc(v,&b,1);}return ~v; }
static int hexkey(const unsigned char *key)
{ for(unsigned i=0;i<64;i++)if(!((key[i]>='0'&&key[i]<='9')||(key[i]>='a'&&key[i]<='f')||(key[i]>='A'&&key[i]<='F')))return 0;return key[64]==0; }
static int manifest_ok(const unsigned char *m)
{
    uint32_t n=le32(m+20),entry=le32(m+24);
    if(memcmp(m,"AP01OTA1",8)||le32(m+8)!=1||le32(m+12)!=IMAGE_LEN||
       le32(m+16)!=MAN_OFF||n<=256||n>PAYLOAD_MAX||
       (entry&1)||entry<0x9ffff000u+MAN_OFF+256||
       entry>=0x9ffff000u+MAN_OFF+n||le32(m+52)!=8192||
       le32(m+56)!=18080||le32(m+60)||!hexkey(m+64)||
       manifest_crc(m)!=le32(m+32))return 0;
    for(unsigned i=129;i<256;i++)if(m[i])return 0;
    return 1;
}
static uint64_t now(void *opaque)
{
    connection *c=opaque;server *s=c->owner;
    uint32_t n=FN(0xa005f9e2,uint32_t(*)(void))();
    if(n<s->last_ms)s->high_ms+=(uint64_t)1<<32;
    s->last_ms=n;return s->high_ms+n;
}
static int error_number(void)
{ return *FN(0xa00267c0,int *(*)(void))(); }
static int wait_fd(connection *c,unsigned events,uint64_t deadline)
{
    for(;;){
        uint64_t t=now(c);if(t>=deadline)return -1;
        uint64_t left=deadline-t;poll_fd p;memset(&p,0,sizeof(p));p.fd=c->fd;p.events=events;
        int r=FN(0xa003f546,int(*)(poll_fd *,unsigned,int))(&p,1,left>1000?1000:(int)left);
        if(r<0){if(error_number()==4)continue;return -1;}
        if(p.revents&(8|32))return -1;
        if(r>0&&(p.revents&events))return 0;
        if(p.revents&16)return -1;
    }
}
static int transfer(void *opaque,void *buffer,size_t length,uint32_t timeout,int output)
{
    connection *c=opaque;uint64_t deadline=now(c)+timeout;
    while(length){
        if(wait_fd(c,output?4:1,deadline))return -1;
        int n=output?FN(0xa003949c,int(*)(int,const void*,size_t,int,const void*,unsigned))
                        (c->fd,buffer,length,0x40,NULL,0):
                     FN(0xa010d6f6,int(*)(int,void*,size_t,int))(c->fd,buffer,length,0x40);
        if(n>=0)return n;
        int e=error_number();if(e!=4&&e!=11)return -1;
    }
    return 0;
}
static int receive(void *p,void *b,size_t n,uint32_t t) { return transfer(p,b,n,t,0); }
static int transmit(void *p,const void *b,size_t n,uint32_t t) { return transfer(p,(void *)b,n,t,1); }
static void close_fd(int fd) { if(fd>=0)FN(0xa0026888,int(*)(int))(fd); }
static int begin(void *opaque,uint32_t length,uint32_t timeout)
{
    connection *c=opaque;
    if(!timeout||length!=IMAGE_LEN||c->started)return OTA_STORE_ERROR;
    if(c->owner->fenced)return OTA_STORE_BUSY;
    c->owner->fenced=1;c->started=1;c->flash.fd=-1;c->flash.flip=0;
    c->code_crc=0xffffffff;c->recovery_crc=0;
    if(FN(0xa00b4f54,int(*)(flash_io*,uint32_t))(&c->flash,length)!=0||c->flash.fd<=0){
        if(c->flash.fd==0){close_fd(0);c->flash.fd=-1;}
        return OTA_STORE_ERROR;
    }
    return OTA_STORE_OK;
}
static void capture(unsigned char *dst,uint32_t base,uint32_t bytes,
                    const unsigned char *src,uint32_t pos,uint32_t n)
{
    uint32_t start=pos>base?pos:base,end=pos+n<base+bytes?pos+n:base+bytes;
    if(start<end)memcpy(dst+start-base,src+start-pos,end-start);
}
static int chunk(void *opaque,const void *buffer,size_t bytes,uint32_t timeout)
{
    connection *c=opaque;const unsigned char *p=buffer;
    if(!timeout||!c->started||c->finished||bytes>1024||bytes>IMAGE_LEN-c->position)return OTA_STORE_ERROR;
    uint32_t pos=c->position,n=(uint32_t)bytes;
    capture(c->header,0,256,p,pos,n);capture(c->manifest,MAN_OFF,256,p,pos,n);
    capture(c->builtin,BUILTIN_OFF,4,p,pos,n);capture(c->rcs,RCS_OFF,RCS_LEN,p,pos,n);
    capture(c->tail,IMAGE_LEN-40,40,p,pos,n);
    if(pos+n>=256&&memcmp(c->header,(const void *)0x9ffff000,256))return OTA_STORE_ERROR;
    /* CRC of incoming unchanged bytes, before original writer flips byte0. */
    uint32_t end=pos+n<IMAGE_LEN-4?pos+n:IMAGE_LEN-4;
    if(pos<end)c->recovery_crc=crc(c->recovery_crc,p,end-pos);
    if(pos+n>=MAN_OFF+256){
        if(!manifest_ok(c->manifest))return OTA_STORE_ERROR;
        uint32_t lo=pos>MAN_OFF+256?pos:MAN_OFF+256;
        uint32_t limit=MAN_OFF+le32(c->manifest+20);
        uint32_t hi=pos+n<limit?pos+n:limit;
        if(lo<hi)c->code_crc=crc(c->code_crc,p+lo-pos,hi-lo);
    }
    memcpy(c->scratch,p,n);
    if(FN(0xa00b5122,int(*)(flash_io*,uint32_t,void*,uint32_t))(&c->flash,pos,c->scratch,n))return OTA_STORE_ERROR;
    c->position+=n;return OTA_STORE_OK;
}
static int contains(const unsigned char *p,size_t n,const char *text)
{ size_t k=strlen(text);for(size_t i=0;i+k<=n;i++)if(!memcmp(p+i,text,k))return 1;return 0; }
static int commit(void *opaque,uint32_t timeout)
{
    connection *c=opaque;uint64_t deadline=now(c)+timeout;
    if(!timeout||!c->started||c->finished||c->position!=IMAGE_LEN)return OTA_STORE_ERROR;
    if(memcmp(c->header,(const void*)0x9ffff000,256)||
       (~crc(0xffffffff,c->header,252))!=le32(c->header+252)||
       !manifest_ok(c->manifest)||le32(c->manifest+24)!=le32(c->builtin)||
       ~c->code_crc!=le32(c->manifest+28)||
       memcmp(c->tail,"0x5245434f56455259544147",24)||
       be32(c->tail+32)!=IMAGE_LEN||c->recovery_crc!=le32(c->tail+36)||
       !contains(c->rcs,RCS_LEN,"setlogmask &\n"))return OTA_STORE_ERROR;
    /* One-shot validation restores byte0 only on real CRC success. */
    c->finished=1;
    if(FN(0xa00b51d8,int(*)(flash_io*,uint32_t,uint32_t))(&c->flash,0,IMAGE_LEN))return OTA_STORE_ERROR;
    if(now(c)>=deadline)return OTA_STORE_ERROR;
    int r=FN(0xa0055a14,int(*)(uint32_t,void*))(0xff13,NULL);
    if(r!=0)return OTA_STORE_UNKNOWN;
    FN(0xa01addc6,void(*)(flash_io*))(&c->flash);
    return OTA_STORE_OK;
}
static void abort_store(void *opaque)
{
    connection *c=opaque;
    if(c->flash.fd>0)FN(0xa01addc6,void(*)(flash_io*))(&c->flash);
    if(c->started&&!c->finished)c->owner->fenced=0;
    /* Once first-byte restoration or commit began, retain the fence even on
     * timeout: a new request must not overwrite a possibly committed image. */
}
static void *serve(void *opaque)
{
    server *s=opaque;
    for(;;){
        poll_fd p;memset(&p,0,sizeof(p));p.fd=s->listen_fd;p.events=1;
        int r=FN(0xa003f546,int(*)(poll_fd*,unsigned,int))(&p,1,1000);
        if(r<0&&error_number()!=4)break;
        if(p.revents&(8|16|32))break;
        if(r<=0||!(p.revents&1))continue;
        int fd=FN(0xa01a192a,int(*)(int,void*,void*))(s->listen_fd,NULL,NULL);
        if(fd<0)continue;
        connection *c=FN(0xa002fcc8,void*(*)(size_t))(sizeof(*c));
        if(!c){close_fd(fd);continue;}
        memset(c,0,sizeof(*c));c->owner=s;c->fd=fd;c->flash.fd=-1;
        int32_t tv[2]={1,0};
        if(FN(0xa010e532,int(*)(int,int,int,const void*,unsigned))(fd,1,10,tv,8)||
           FN(0xa010e532,int(*)(int,int,int,const void*,unsigned))(fd,1,14,tv,8)){
            close_fd(fd);FN(0xa002e14e,void(*)(void*))(c);continue;
        }
        ota_channel_io io={c,receive,transmit,now,begin,chunk,commit,abort_store};
        ota_channel_result result=ota_channel_handle(&io,(const char*)ota_0041_manifest+64,now(c)+LIMIT_MS);
        close_fd(fd);
        if(c->flash.fd>0)abort_store(c);
        FN(0xa002e14e,void(*)(void*))(c);
        /* Matching0041 miIO delayed reboot invokes board-app0xff04. Only
         * reboot after durable commit, a fully sent reply and socket close.
         * This command should not return; if it does, retain the fence. */
        if(result.committed && result.error==OTA_OK && result.response_error==OTA_OK){
            FN(0xa0055a14,int(*)(uint32_t,void*))(0xff04,NULL);
            s->reboot_returned=1;
        }
    }
    close_fd(s->listen_fd);FN(0xa002e14e,void(*)(void*))(s);return NULL;
}
__attribute__((section(".text.entry"),used))
int ota_0041_main(int argc,char **argv)
{
    (void)argc;(void)argv;
    if(!manifest_ok(ota_0041_manifest))return -1;
    server *s=FN(0xa002fcc8,void*(*)(size_t))(sizeof(*s));if(!s)return -1;
    memset(s,0,sizeof(*s));s->listen_fd=FN(0xa00268c4,int(*)(int,int,int))(2,1,0);
    if(s->listen_fd<0)goto failed;
    ip_addr address;memset(&address,0,sizeof(address));address.family=2;
    address.port=(uint16_t)((18080u>>8)|(18080u<<8));
    if(FN(0xa003f446,int(*)(int,const void*,unsigned))(s->listen_fd,&address,sizeof(address))||
       FN(0xa01a18ce,int(*)(int))(s->listen_fd))goto failed;
    uint32_t tid=0;
    if(FN(0xa019750e,int(*)(uint32_t*,const char*,void*(*)(void*),unsigned,void*))
       (&tid,"ap01-ota",serve,8192,s))goto failed;
    /* This builtin owns the pthread task group. Returning from its main lets
     * NuttX tear down the just-created server before it is scheduled. Keep
     * the small builtin stack asleep; HTTP work uses the8192-byte child. */
    for(;;)FN(0xa003f546,int(*)(poll_fd*,unsigned,int))(NULL,0,1000);
failed:
    close_fd(s->listen_fd);FN(0xa002e14e,void(*)(void*))(s);return -1;
}
