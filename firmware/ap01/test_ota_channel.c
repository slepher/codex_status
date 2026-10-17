#include "ota_channel.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>
typedef struct {char h[1800],o[512];size_t hn,p,n,a,on,rc,wc;uint64_t t;unsigned reads,begins,chunks,commits,aborts,tick,at;int rf,wf,bs,cs,fs,mutate,late_commit,write_tick;} F;
static const char key[]="0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
static unsigned char val(size_t i,size_t n){return n==9?(unsigned char)"123456789"[i]:(unsigned char)(i*17+3);}
static uint32_t crc(size_t n){uint32_t c=~0u;for(size_t i=0;i<n;i++){c^=val(i,n);for(unsigned j=0;j<8;j++)c=(c>>1)^((c&1)?0xedb88320u:0);}return ~c;}
static void init(F*f,size_t n,const char*x){memset(f,0,sizeof(*f));f->n=n;f->rc=1024;f->wc=512;int k=snprintf(f->h,sizeof(f->h),"POST /firmware HTTP/1.1\r\nAuthorization: Bearer %s\r\nContent-Length: %zu\r\nX-ROM-CRC32: %08x\r\n%s\r\n",key,n,crc(n),x);assert(k>0&&(size_t)k<sizeof(f->h));f->hn=(size_t)k;}
static int rd(void*x,void*d,size_t n,uint32_t t){F*f=x;assert(t&&f->t<1000);f->reads++;if(f->at==f->reads)return f->rf;f->t+=f->tick;if(n>f->rc)n=f->rc;if(n>f->hn+f->n-f->p)n=f->hn+f->n-f->p;for(size_t i=0;i<n;i++,f->p++)((unsigned char*)d)[i]=f->p<f->hn?(unsigned char)f->h[f->p]:val(f->p-f->hn,f->n);return(int)n;}
static int wr(void*x,const void*d,size_t n,uint32_t t){F*f=x;assert(t);f->t+=(unsigned)f->write_tick;if(f->wf)return f->wf==1?0:-1;if(n>f->wc)n=f->wc;assert(f->on+n<sizeof(f->o));memcpy(f->o+f->on,d,n);f->on+=n;f->o[f->on]=0;return(int)n;}
static uint64_t now(void*x){return((F*)x)->t;}
static int begin(void*x,uint32_t n,uint32_t t){F*f=x;assert(t&&n==f->n);f->begins++;return f->bs;}
static int chunk(void*x,const void*d,size_t n,uint32_t t){F*f=x;assert(t&&n<=1024);f->chunks++;for(size_t i=0;i<n;i++)assert(((const unsigned char*)d)[i]==val(f->a+i,f->n));if(!f->cs)f->a+=n;if(f->mutate&&n)((unsigned char*)d)[0]^=255;return f->cs;}
static int commit(void*x,uint32_t t){F*f=x;assert(t&&f->a==f->n);f->commits++;if(f->late_commit)f->t=1001;return f->fs;}
static void abort_tx(void*x){((F*)x)->aborts++;}
static ota_channel_result run(F*f){ota_channel_io io={f,rd,wr,now,begin,chunk,commit,abort_tx};return ota_channel_handle(&io,key,1000);}
static void bad(F*f,enum ota_error e){ota_channel_result r=run(f);assert(r.error==e&&!f->begins&&!f->chunks&&!f->commits&&!f->aborts);assert(strstr(f->o,"\"stage\":\"header\"")&&!strstr(f->o,key));}
int main(void){F f;ota_channel_result r;unsigned k=0;assert(crc(9)==0xcbf43926u);
 const size_t sizes[]={1,9,4097,OTA_CHANNEL_MAX_ROM};for(unsigned i=0;i<4;i++){init(&f,sizes[i],"");r=run(&f);assert(r.error==OTA_OK&&r.committed&&r.bytes==sizes[i]&&f.commits==1&&!f.aborts);assert(strstr(f.o,"200 OK")&&strstr(f.o,"Connection: close"));k++;}
 const size_t caps[]={1,2,3,7,1024};for(unsigned i=0;i<5;i++){init(&f,2051,"");f.rc=caps[i];f.wc=caps[i];r=run(&f);assert(r.error==OTA_OK&&f.a==2051&&!f.aborts);k++;}
 init(&f,2051,"");f.mutate=1;r=run(&f);assert(r.error==OTA_OK&&r.committed);k++;
 init(&f,0,"");strcpy(f.h,"GET /health HTTP/1.1\r\n\r\n");f.hn=strlen(f.h);r=run(&f);assert(r.error==OTA_OK&&!f.begins&&strstr(f.o,OTA_CHANNEL_VERSION));k++;
 init(&f,9,"");strstr(f.h,"Bearer ")[7]='x';bad(&f,OTA_UNAUTHORIZED);k++;
 const char*x[]={"Content-Length: 9\r\n","Transfer-Encoding: chunked\r\n","Connection: keep-alive\r\n","Content-Length: -1\r\n","X-ROM-CRC32: cbf43926\r\n","Bad Name: x\r\n","X-A: x\ny\r\n"};for(unsigned i=0;i<sizeof(x)/sizeof(x[0]);i++){init(&f,9,x[i]);bad(&f,OTA_BAD_HEADER);k++;}
 init(&f,0,"");bad(&f,OTA_BAD_HEADER);k++;init(&f,OTA_CHANNEL_MAX_ROM+1u,"");bad(&f,OTA_TOO_LARGE);k++;
 init(&f,9,"");memset(f.h,'x',1024);f.hn=1024;bad(&f,OTA_BAD_HEADER);k++;
 init(&f,9,"");strstr(f.h,"X-ROM-CRC32: ")[13]='z';bad(&f,OTA_BAD_HEADER);k++;
 init(&f,9,"");strstr(f.h,"cbf43926")[0]='0';r=run(&f);assert(r.error==OTA_BAD_CRC&&f.begins==1&&f.aborts==1&&!f.commits&&!r.committed);k++;
 for(int fault=-1;fault<=0;fault++){init(&f,9,"");f.at=3;f.rf=fault;r=run(&f);assert(r.error==(fault?OTA_IO:OTA_EOF)&&!f.begins);k++;init(&f,9,"");f.at=(unsigned)f.hn+1;f.rf=fault;r=run(&f);assert(r.error==(fault?OTA_IO:OTA_EOF)&&f.aborts==1&&!f.commits);k++;}
 init(&f,9,"");f.rc=1;f.tick=7;r=run(&f);assert(r.error==OTA_TIMEOUT&&f.t>=1000&&f.t<1007&&!f.commits);k++;
 init(&f,2051,"");f.rc=1;f.tick=1;r=run(&f);assert(r.error==OTA_TIMEOUT&&f.begins==1&&f.aborts==1&&!f.commits&&f.a<2051);k++;
 init(&f,9,"");f.bs=OTA_STORE_BUSY;r=run(&f);assert(r.error==OTA_BUSY&&f.aborts==1&&!f.chunks);k++;
 init(&f,9,"");f.cs=OTA_STORE_ERROR;r=run(&f);assert(r.error==OTA_WRITER&&f.aborts==1&&!f.commits);k++;
 for(int s=OTA_STORE_BUSY;s<=OTA_STORE_UNKNOWN;s++){init(&f,9,"");f.fs=s;r=run(&f);assert(r.error!=OTA_OK&&!r.committed&&f.aborts==1&&!strstr(f.o,"200 OK"));if(s==OTA_STORE_UNKNOWN){assert(r.error==OTA_COMMIT_UNKNOWN&&strstr(f.o,"result_unknown"));}k++;}
 for(int s=1;s<=2;s++){init(&f,9,"");f.wf=s;r=run(&f);assert(r.error==OTA_IO&&r.response_error==OTA_IO&&r.committed&&!f.aborts);k++;}
 init(&f,9,"");f.late_commit=1;r=run(&f);assert(r.error==OTA_TIMEOUT&&r.committed&&!f.aborts);k++;
 init(&f,9,"");f.late_commit=1;f.fs=OTA_STORE_UNKNOWN;r=run(&f);assert(r.error==OTA_COMMIT_UNKNOWN&&!r.committed&&f.aborts==1);k++;
 init(&f,9,"");f.wc=1;f.write_tick=30;r=run(&f);assert(r.error==OTA_TIMEOUT&&r.committed&&!f.aborts);k++;
 init(&f,1,"");size_t padlen=1024-f.hn-9;char padding[1100];memcpy(padding,"X-Pad: ",7);memset(padding+7,'a',padlen);memcpy(padding+7+padlen,"\r\n",3);init(&f,1,padding);assert(f.hn==1024);r=run(&f);assert(r.error==OTA_OK);k++;
 padding[7+padlen]='a';memcpy(padding+8+padlen,"\r\n",3);init(&f,1,padding);assert(f.hn==1025);bad(&f,OTA_BAD_HEADER);k++;
 printf("{\"passed\":true,\"checks\":%u,\"body_buffer_bytes\":1024,\"max_rom_bytes\":%u}\n",k,OTA_CHANNEL_MAX_ROM);return 0;}
