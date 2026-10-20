#include "application.h"
#include "http.h"
#include "slot_0041.h"
#include "vfs_0041.h"
#include "ap01_template_display.h"
#include "local_json_allocator.h"
#include <atomic>
#include <new>
#include <stddef.h>
#define ROM_FN(address,type) ((type)(uintptr_t)(address))
bool ap01Calendar(time_t epoch, struct tm &result);
extern "C" const uint8_t ap01_native_manifest[256];

namespace {
struct PollFd { int32_t fd; uint32_t events,revents; void *privateFields[3]; };
struct Address { uint16_t family,port; uint32_t ip; uint8_t padding[8]; };
struct Clock { int32_t seconds,nanos; };
static_assert(sizeof(PollFd)==24 && sizeof(Address)==16 && sizeof(Clock)==8,"0041 kernel ABI");
uint64_t nowMs() {
    Clock value{};
    if (ROM_FN(0xa002696a,int (*)(unsigned,Clock *))(1,&value) || value.seconds < 0 || value.nanos < 0) return 0;
    return uint64_t(value.seconds)*1000+value.nanos/1000000;
}
void pause(unsigned ms) { ROM_FN(0xa003f546,int (*)(PollFd *,unsigned,int))(nullptr,0,ms); }
void closeFd(int fd) { if(fd>=0)ROM_FN(0xa0026888,int (*)(int))(fd); }
bool randomBytes(void *,uint8_t *out,size_t length) {
    int fd=ROM_FN(0xa003f4f6,int (*)(const char *,int,...))("/dev/urandom",1);
    if(fd<0)return false;
    size_t done=0;
    while(done<length) {int n=ROM_FN(0xa003f6a2,int (*)(int,void *,size_t))(fd,out+done,length-done);if(n<=0)break;done+=n;}
    closeFd(fd);return done==length;
}
bool wifiMac(char (&text)[13]) {
    uint8_t bytes[6]={}; ROM_FN(0xa005f240,void (*)(uint8_t *))(bytes);
    unsigned any=0,notFF=0;for(unsigned i=0;i<6;++i){any|=bytes[i];notFF|=bytes[i]^255;}
    if(!any||!notFF||(bytes[0]&1))return false;
    const char *hex="0123456789ABCDEF";
    for(unsigned i=0;i<6;++i){text[2*i]=hex[bytes[i]>>4];text[2*i+1]=hex[bytes[i]&15];}text[12]=0;return true;
}
bool waitFd(int fd,unsigned events,uint64_t deadline) {
    while(nowMs()<deadline) {
        uint64_t left=deadline-nowMs();PollFd p{};p.fd=fd;p.events=events;
        int n=ROM_FN(0xa003f546,int (*)(PollFd *,unsigned,int))(&p,1,left>1000?1000:left);
        if(n<0){int code=*ROM_FN(0xa00267c0,int *(*)())();if(code!=4)return false;}
        if(n>0){if(p.revents&events)return true;if(p.revents&(8|16|32))return false;}
    }
    return false;
}
int receive(void *p,uint8_t *bytes,size_t length,uint64_t deadline) {
    int fd=*(int *)p;
    while(waitFd(fd,1,deadline)) {
        int n=ROM_FN(0xa010d6f6,int (*)(int,void *,size_t,int))(fd,bytes,length,0x40);
        if(n>=0)return n;int code=*ROM_FN(0xa00267c0,int *(*)())();if(code!=4&&code!=11)return -1;
    }return -1;
}
int send(void *p,const uint8_t *bytes,size_t length,uint64_t deadline) {
    int fd=*(int *)p;
    while(waitFd(fd,4,deadline)) {
        int n=ROM_FN(0xa003949c,int (*)(int,const void *,size_t,int,const void *,unsigned))(fd,bytes,length,0x40,nullptr,0);
        if(n>=0)return n;int code=*ROM_FN(0xa00267c0,int *(*)())();if(code!=4&&code!=11)return -1;
    }return -1;
}
int listener(unsigned port,bool udp=false) {
    int fd=ROM_FN(0xa00268c4,int (*)(int,int,int))(2,udp?2:1,0);if(fd<0)return -1;
    Address address{};address.family=2;address.port=(port>>8)|(port<<8);
    Clock timeout{2,0};
    if(ROM_FN(0xa010e532,int (*)(int,int,int,const void *,unsigned))(fd,1,10,&timeout,sizeof(timeout)) ||
       ROM_FN(0xa003f446,int (*)(int,const Address *,unsigned))(fd,&address,sizeof(address)) ||
       (!udp&&ROM_FN(0xa01a18ce,int (*)(int))(fd))) {closeFd(fd);return -1;}
    return fd;
}
struct Frame {uint8_t *pixels;uint32_t generation;uint8_t count,index,tries=0;};
struct ImageHeader {uint8_t magic,format;uint16_t flags,width,height,stride,reserved;};
struct Image {ImageHeader header;uint32_t size;const uint8_t *data;const void *reserved;};
struct PanelTag {uint32_t kind=0xfe;void *state=nullptr;};
static_assert(sizeof(Image)==24,"0041 LVGL image ABI");

struct Runtime {
    struct Flash {int32_t fd=-1,flip=0;} updateFlash;
    char firmwareBuild[33]={};
    int bootSlot=-1;
    LittleFSClass fs;
    Ap01Business business;
    std::unique_ptr<Ap01Application> app;
    std::atomic<uint32_t> ready{0}; // 0 initializing, 1 live, 2 root retired.
    std::atomic<uint32_t> uiHeartbeat{0};
    std::atomic<bool> maintenanceListening{false};
    std::atomic<bool> stop{false};
    std::atomic_flag bindingsLock=ATOMIC_FLAG_INIT,businessLock=ATOMIC_FLAG_INIT;
    std::atomic<Frame *> pending{nullptr};
    std::atomic<unsigned> templateCount{0};
    std::atomic<bool> retired{false};
    const uint64_t bootId;
    void *root,*content=nullptr,*image=nullptr,*dialog=nullptr;
    uint8_t *imageBytes=nullptr;
    Image descriptor{};
    PanelTag contentTag,dialogTag;
    void (*original)(void *);
    uint64_t event=0;
    uint32_t shownGeneration=0,pairRevision=0;
    uint32_t uiStage=0; // UI-thread diagnostic only; never used for authorization.
    unsigned previousNative=0,followingNative=0;
    uint8_t contentPage=255,dialogPage=255;
    char pairRequest[65]={};
    bool focused=false,pairPending=false,pairWoken=false,contentLeaving=false;
    uint8_t virtualIndex=0;
    explicit Runtime(void *window,void (*callback)(void *),uint64_t boot)
        :fs(ap01RomVfs(),"/resource/codex-status"),business(fs,{this,present,retire},boot),bootId(boot),root(window),original(callback) {}
    static bool retire(void *p) {
        Runtime &r=*(Runtime *)p;r.retired.store(false,std::memory_order_release);
        if(!present(p,nullptr,0))return false;
        uint64_t deadline=nowMs()+1000;
        while(nowMs()<deadline) {if(r.retired.load(std::memory_order_acquire))return true;pause(5);}
        return false;
    }
    static bool present(void *p,uint8_t *pixels,uint32_t generation) {
        Runtime &r=*(Runtime *)p;Frame *frame=new(std::nothrow) Frame{pixels,generation,uint8_t(pixels?r.business.count():0),r.business.active()};if(!frame)return false;
        r.templateCount.store(frame->count,std::memory_order_release);
        Frame *old=r.pending.exchange(frame,std::memory_order_acq_rel);
        if(old){delete[] old->pixels;delete old;}return true;
    }
    static bool lockBindings(void *p) {return !((Runtime *)p)->bindingsLock.test_and_set(std::memory_order_acquire);}
    static void unlockBindings(void *p) {((Runtime *)p)->bindingsLock.clear(std::memory_order_release);}
    static bool firmwareBusy(void *) {return *(volatile uint8_t *)(uintptr_t)0x62fc4053!=0;}
    static void firmwareClose(void *p) {
        Flash &f=((Runtime *)p)->updateFlash;
        if(f.fd>=0){ROM_FN(0xa01addc6,void (*)(Flash *))(&f);f.fd=-1;}
    }
    static bool firmwareOpen(void *p,uint32_t length) {
        Flash &f=((Runtime *)p)->updateFlash;if(f.fd>=0)return false;
        const char *slot=ROM_FN(0xa0026e00,const char *(*)(const char *))("ro.boot.update_slot");
        if(!slot||(strcmp(slot,"/dev/fw1")&&strcmp(slot,"/dev/fw2")))return false;
        Runtime &r=*(Runtime *)p;
        if(r.bootSlot<0||ap01SelectedSlot0041()!=r.bootSlot||strcmp(slot,r.bootSlot?"/dev/fw1":"/dev/fw2"))return false;
        f.flip=0;
        int result=ROM_FN(0xa00b4f54,int (*)(Flash *,uint32_t))(&f,length);
        if(result||f.fd<=0){firmwareClose(p);return false;}return true;
    }
    static bool firmwareWrite(void *p,uint32_t offset,const uint8_t *bytes,size_t length) {
        Flash &f=((Runtime *)p)->updateFlash;if(f.fd<0||!length||length>32768)return false;
        uint8_t scratch[1024];
        while(length){unsigned n=length>sizeof(scratch)?sizeof(scratch):length;memcpy(scratch,bytes,n);
            if(ROM_FN(0xa00b5122,int (*)(Flash *,uint32_t,void *,uint32_t))(&f,offset,scratch,n))return false;
            offset+=n;bytes+=n;length-=n;
        }return true;
    }
    static bool firmwareVerify(void *p,uint32_t length) {
        Flash &f=((Runtime *)p)->updateFlash;
        return f.fd>=0&&!ROM_FN(0xa00b51d8,int (*)(Flash *,uint32_t,uint32_t))(&f,0,length);
    }
    static int firmwareCommit(void *p) {int slot=((Runtime *)p)->bootSlot;return slot<0?-1:ap01SelectSlot0041(unsigned(slot)^1);}
    static void firmwareReboot(void *) {ROM_FN(0xa0055a14,int (*)(uint32_t,void *))(0xff04,nullptr);}
    static int runningSlot(void *p) {return ((Runtime *)p)->bootSlot;}
    static bool inspectSlot(void *p,unsigned slot,Ap01SlotImage &out) {return ap01InspectSlot0041(slot,int(slot)==((Runtime *)p)->bootSlot,out);}
    static int selectSlot(void *,unsigned slot) {return ap01SelectSlot0041(slot);}
    static bool bootHealthy(void *p) {
        auto &r=*(Runtime *)p;uint32_t beat=r.uiHeartbeat.load(std::memory_order_acquire);
        return r.ready.load(std::memory_order_acquire)==1&&r.maintenanceListening.load(std::memory_order_acquire)&&beat&&uint32_t(nowMs())-beat<=1000;
    }
    bool mainRootVisible() const {
        // theme+0x4c is actual UI initialization. 62fc4038 is the original
        // MiHome onboarding gate; Bridge pairing must not depend on that account.
        return !*(volatile uint8_t *)(uintptr_t)0x62fc4053 &&
            *(volatile uint32_t *)(uintptr_t)0x62fc95bc==*(uint32_t *)((uint8_t *)root+0x30) &&
            !*((uint8_t *)root+0x4d);
    }
    bool mainRootActive() const {return *((uint8_t *)root+0x4c)&&mainRootVisible();}
    bool mainFocused() const { return mainRootActive() && *(volatile uint8_t *)(uintptr_t)0x62fc4039==1; }
    void hidden(void *object,bool value) {
        if(!object)return;
        if(value)ROM_FN(0xa00d8f7c,void (*)(void *,uint32_t))(object,1);
        else ROM_FN(0xa00d9044,void (*)(void *,uint32_t))(object,1);
    }
    unsigned nativePage() const {return ROM_FN(0xa00b2ff4,unsigned (*)(void *))(root);}
    void *panel(PanelTag &tag,unsigned page) {
        // Native screen-level traversal treats every child as its custom
        // window class (+0x3c kind). Ordinary panels belong inside that window.
        if(page>8)return nullptr;
        void *parent=ROM_FN(0xa00dcee4,void *(*)(void *,int))(root,page);if(!parent)return nullptr;
        uiStage=10;
        void *object=ROM_FN(0xa00d91c6,void *(*)(void *))(parent);if(!object)return nullptr;
        *(void **)((uint8_t *)object+0x10)=&tag; // Verified public LVGL user_data field.
        // Floating/ignore-layout: keep the overlay in the page viewport while
        // preserving all nine original window children and their fixed indices.
        uiStage=11;ROM_FN(0xa00d8f7c,void (*)(void *,uint32_t))(object,(1u<<17)|(1u<<18));
        uiStage=12;
        ROM_FN(0xa00d8e96,void (*)(void *,int,int))(object,320,240);
        uiStage=13;
        ROM_FN(0xa00d92ec,void (*)(void *,unsigned,int,int))(object,1,0,0);
        uiStage=14;
        ROM_FN(0xa00a5c0c,void (*)(void *,int,unsigned))(object,0,0);
        uiStage=15;
        ROM_FN(0xa00dfb70,void (*)(void *,uint32_t,unsigned))(object,0,0);
        uiStage=16;
        ROM_FN(0xa00dfbca,void (*)(void *,int,unsigned))(object,0,0);
        uiStage=17;
        ROM_FN(0xa00d9044,void (*)(void *,uint32_t))(object,2|0x10);
        uiStage=18;hidden(object,true);uiStage=19;return object;
    }
    bool ensureContent(unsigned page) {
        if(content&&contentPage==page&&image)return true;
        if(content&&contentPage!=page){ROM_FN(0xa00ded6c,void (*)(void *))(content);content=image=nullptr;contentPage=255;}
        if(!content){content=panel(contentTag,page);if(!content)return false;contentPage=page;}
        image=ROM_FN(0xa00ed0a8,void *(*)(void *))(content);if(!image)return false;
        ROM_FN(0xa01ad654,void (*)(void *,void (*)(void *),unsigned,void *))(image,imageDrawn,0x1e,this);
        if(imageBytes)ROM_FN(0xa00ed560,void (*)(void *,const Image *))(image,&descriptor);
        return true;
    }
    void *label(void *parent,const char *text,uintptr_t font,int x,int y) {
        // Same six verified operations as the original a019d38e helper, with
        // an explicit allocation check and an observable failure stage.
        uiStage=100;void *object=ROM_FN(0xa009090e,void *(*)(void *))(parent);if(!object)return nullptr;
        uiStage=101;ROM_FN(0xa0091b3a,void (*)(void *,const char *))(object,text);
        uiStage=102;ROM_FN(0xa019d0ae,void (*)(void *,const void *))(object,(void *)font);
        uiStage=103;ROM_FN(0xa019d0ca,void (*)(void *,unsigned))(object,0);
        uiStage=104;ROM_FN(0xa019d0e6,void (*)(void *,uint32_t))(object,0xffffff);
        uiStage=105;ROM_FN(0xa00d92ec,void (*)(void *,unsigned,int,int))(object,1,x,y);
        uiStage=106;return object;
    }
    bool activate(unsigned index) {
        if(businessLock.test_and_set(std::memory_order_acquire))return false;
        bool ok=business.active()==index || business.localActivate(index,nowMs());
        businessLock.clear(std::memory_order_release);return ok;
    }
    void tick();
    void key(void *);
    static void timer(void *timer) {auto *r=*(Runtime **)((uint8_t *)timer+12);if(r){r->tick();r->uiHeartbeat.store(uint32_t(nowMs()),std::memory_order_release);}}
    static void eventCallback(void *event) {
        auto *r=*(Runtime **)((uint8_t *)event+12);if(!r)return;
        uint32_t code=*(uint32_t *)((uint8_t *)event+8)&0xffff7fff;
        if(code==0x29&&*(void **)event==r->root) {
            bool wasReady=r->ready.exchange(2,std::memory_order_acq_rel)==1;
            if(wasReady&&r->app)r->app->businessOnline(false);
            r->root=nullptr;return;
        }
        r->key(event);
    }
    static void imageDrawn(void *event) {
        auto *r=*(Runtime **)((uint8_t *)event+12);
        if(r&&r->ready.load(std::memory_order_acquire)==1&&r->focused&&r->mainFocused()&&!r->app->maintenanceActive())r->business.displayed(r->shownGeneration);
    }
    static void dialogDrawn(void *event) {
        auto *r=*(Runtime **)((uint8_t *)event+12);
        if(r&&r->ready.load(std::memory_order_acquire)==1&&r->pairPending&&r->mainFocused())
            r->app->pairingDisplayed(r->pairRequest,r->pairRevision,++r->event);
    }
    static void *bootstrap(void *);
    static void *maintenance(void *);
    static void *udp(void *);
    void serve(bool maintenance);
};
extern "C" {
__attribute__((section(".ap01_abi"),used))
extern const uint32_t ap01_runtime_abi[] = {
    1,sizeof(Runtime),sizeof(Ap01Business),sizeof(BundleStore),sizeof(Ap01Application),
    offsetof(Runtime,root),offsetof(Runtime,content),offsetof(Runtime,dialog),offsetof(Runtime,image),
    offsetof(Runtime,pairPending),offsetof(Runtime,pairWoken),offsetof(Runtime,focused),
    offsetof(Runtime,shownGeneration),offsetof(Runtime,templateCount),offsetof(Runtime,retired),offsetof(Runtime,ready),offsetof(Runtime,uiStage),
};
}
bool ap01ImageReplace(Runtime &r,const uint8_t *pixels) {
    uintptr_t cache=*(volatile uint32_t *)(uintptr_t)0x62fc9698;
    if(!cache||*(uint32_t *)(cache+8)<1280)return false;
    std::unique_ptr<uint8_t[]> candidate(new(std::nothrow) uint8_t[38464]);
    if(!candidate)return false;
    for(unsigned i=0;i<16;++i) {
        uint16_t c=ap01Palette565(i);unsigned red=(c>>11)&31,green=(c>>5)&63,blue=c&31;
        uint32_t argb=0xff000000|(((red<<3)|(red>>2))<<16)|(((green<<2)|(green>>4))<<8)|(blue<<3)|(blue>>2);
        memcpy(candidate.get()+4*i,&argb,4);
    }
    memcpy(candidate.get()+64,pixels,38400);
    // 0041 lv_image_cache_drop(src), verified against both header/pixel caches.
    ROM_FN(0xa00890e8,void (*)(const void *))(&r.descriptor);
    Image previous=r.descriptor;
    r.descriptor.header={0x19,0x09,0,320,240,160,0};
    r.descriptor.size=38464;r.descriptor.data=candidate.get();r.descriptor.reserved=nullptr;
    ROM_FN(0xa00ed560,void (*)(void *,const Image *))(r.image,&r.descriptor);
    if(*(const void **)((uint8_t *)r.image+0x2c)!=&r.descriptor ||
       *(uint32_t *)((uint8_t *)r.image+0x3c)!=320 || *(uint32_t *)((uint8_t *)r.image+0x40)!=240) {
        r.descriptor=previous;return false;
    }
    delete[] r.imageBytes;r.imageBytes=candidate.release();return true;
}
void Runtime::tick() {
    if(ready.load(std::memory_order_acquire)!=1)return;
    Frame *frame=pending.exchange(nullptr,std::memory_order_acq_rel);
    if(frame&&frame->pixels&&!content&&!mainRootActive()&&!focused) {
        Frame *empty=nullptr;
        if(pending.compare_exchange_strong(empty,frame,std::memory_order_acq_rel))frame=nullptr;
        else {delete[] frame->pixels;delete frame;frame=nullptr;}
    }
    if(frame) {
        if(!frame->pixels) {hidden(content,true);focused=contentLeaving=false;retired.store(true,std::memory_order_release);}
        else {
            // Native cache preparation/drop is bound by the matching linker
            // script only after its 0041 ABI has been independently recovered.
            extern bool ap01ImageReplace(Runtime &,const uint8_t *);
            if(ensureContent(contentLeaving?contentPage:nativePage())&&ap01ImageReplace(*this,frame->pixels)) {shownGeneration=frame->generation;virtualIndex=frame->index;}
            else if(++frame->tries<3) {
                Frame *empty=nullptr;if(pending.compare_exchange_strong(empty,frame,std::memory_order_acq_rel))frame=nullptr;
            } else {business.displayFailed(frame->generation);focused=false;hidden(content,true);}
        }
        if(frame){delete[] frame->pixels;delete frame;}
    }
    String state;
    if(app->pairingUi(nowMs(),state)) {
        LocalJsonAllocator allocator;JsonDocument doc(&allocator);
        if(!deserializeJson(doc,state)) {
            bool pendingPair=doc["pending"]==true;
            const char *request=doc["request_id"]|"";
            unsigned page=nativePage();
            if(pendingPair&&mainRootActive()&&(strcmp(request,pairRequest)||dialogPage!=page)) {
                pairWoken=false;
                if(dialog)ROM_FN(0xa00ded6c,void (*)(void *))(dialog);
                dialog=panel(dialogTag,page);dialogPage=page;
                if(dialog) {
                    // These ROM fonts are page-specific subsets with a self
                    // fallback. Every character below is covered by 49f4;
                    // selecting a missing glyph would loop in native LVGL.
                    void *title=label(dialog,"PAIR CODE",0xa02549f4,100,28);
                    void *code=title?label(dialog,doc["display_code"]|"----",0xa02549f4,40,80):nullptr;
                    void *hint=code?label(dialog,"Press then release",0xa02549f4,45,192):nullptr;
                    if(title&&code&&hint) {
                        ROM_FN(0xa01ad654,void (*)(void *,void (*)(void *),unsigned,void *))(code,dialogDrawn,0x1e,this);
                        if(strlen(request)<sizeof(pairRequest))strcpy(pairRequest,request);
                        pairRevision=doc["revision"]|0u;uiStage=24;
                    } else {
                        ROM_FN(0xa00ded6c,void (*)(void *))(dialog);dialog=nullptr;dialogPage=255;
                    }
                }
            }
            pairPending=pendingPair;
        }
    }
    if(pairPending&&dialog&&mainRootActive()&&!pairWoken) {
        uiStage=25;
        ROM_FN(0xa00a769a,int (*)())();
        *(volatile uint8_t *)(uintptr_t)0x62fc403a=1;
        pairWoken=true;
        uiStage=26;
    }
    if(!pairPending)pairWoken=false;
    if(contentLeaving&&mainRootActive())contentLeaving=false;
    if(focused&&mainRootVisible())ensureContent(nativePage());
    // +0x4c is cleared for the complete native slide. The image must already
    // be visible in its moving window, including the outgoing virtual page.
    bool visible=mainRootVisible()&&*(volatile uint8_t *)(uintptr_t)0x62fc4039==1;
    hidden(content,(!focused&&!contentLeaving)||pairPending||!visible||app->maintenanceActive());
    hidden(dialog,!pairPending||!mainFocused());
    uiStage=27;
}
void Runtime::key(void *eventPointer) {
    uint32_t code=*(uint32_t *)((uint8_t *)eventPointer+8)&0xffff7fff;
    // Root receives draw/style events too. Hiding objects from those callbacks
    // invalidates LVGL while rendering; only input events may change focus.
    if(code!=1&&code!=0xb&&code!=0x11)return;
    if(ready.load(std::memory_order_acquire)==1&&app->maintenanceActive()) {
        focused=contentLeaving=false;hidden(content,true);if(code==0x11)original(eventPointer);return;
    }
    if(ready.load(std::memory_order_acquire)==1&&pairPending&&mainFocused()) {
        if(code==1||code==0xb||code==0x11)*(volatile uint8_t *)(uintptr_t)0x62fc403a=1;
        if(code==1||code==0xb)app->button(code==1,++event,nowMs());
        if(code==1||code==0xb||code==0x11)return;
    }
    if(code!=0x11)return;
    // Do not let the stock handler navigate under an in-flight virtual slide.
    if(ready.load(std::memory_order_acquire)==1&&mainRootVisible()&&!*((uint8_t *)root+0x4c))return;
    if(ready.load(std::memory_order_acquire)!=1||!mainFocused()) {original(eventPointer);return;}
    *(volatile uint8_t *)(uintptr_t)0x62fc403a=1;
    int key=ROM_FN(0xa00d8af6,int (*)(void *))(eventPointer);
    int direction=key==0x13?1:key==0x14?-1:0;
    unsigned count=templateCount.load(std::memory_order_acquire);
    if(!direction||!count) {if(!focused)original(eventPointer);return;}
    if(focused) {
        int next=int(virtualIndex)+direction;
        if(next<0||unsigned(next)>=count) {
            focused=false;contentLeaving=content!=nullptr;
            unsigned target=direction>0?followingNative:previousNative;
            if(target==nativePage()) {
                // The virtual page occupies the incoming stock window. Move
                // its image to the other neighbour before animating back to
                // the exposed stock page; selecting the same index is a no-op.
                unsigned outgoing=direction>0?previousNative:followingNative;
                if(outgoing==target)outgoing=target==0?8:0;
                if(!ensureContent(outgoing)){focused=true;contentLeaving=false;return;}
                ROM_FN(0xa00b3106,void (*)(void *,int,int))(root,outgoing,0);
            }
            // Match the stock key handler: 0x13 slides with 2, 0x14 with 1.
            ROM_FN(0xa00b3106,void (*)(void *,int,int))(root,target,direction>0?2:1);
            tick();
        } else if(activate(next))virtualIndex=next;
        return;
    }
    unsigned before=ROM_FN(0xa00b2ff4,unsigned (*)(void *))(root);
    if(before==8&&!*(volatile uint8_t *)(uintptr_t)0x62fc4038) {
        unsigned index=direction>0?0:count-1;
        if(activate(index)) {previousNative=followingNative=8;focused=true;virtualIndex=index;tick();}
        return;
    }
    original(eventPointer);
    unsigned after=ROM_FN(0xa00b2ff4,unsigned (*)(void *))(root);
    // Insert only on the stock edge into Pet (7), and its reverse edge.
    // Numeric decreases also include ordinary Pet(7)->Settings(6) and the
    // Settings->Power/Time edges; those must never enter the virtual group.
    if(before!=after&&((direction>0&&after==7)||(direction<0&&before==7))) {
        unsigned index=direction>0?0:count-1;
        if(activate(index)) {
            previousNative=direction>0?before:after;followingNative=direction>0?after:before;
            focused=true;contentLeaving=false;virtualIndex=index;tick();
        }
    }
}
void Runtime::serve(bool maintenancePort) {
    int fd=listener(maintenancePort?18080:18081);if(fd<0)return;
    if(maintenancePort)maintenanceListening.store(true,std::memory_order_release);
    if(!maintenancePort)app->businessOnline(true);
    while(!stop.load(std::memory_order_acquire)) {
        if(maintenancePort)app->maintenanceTick(nowMs());
        else if(!app->maintenanceActive()&&!businessLock.test_and_set(std::memory_order_acquire)) {
            time_t epoch=time(nullptr);struct tm local{};char hhmm[6]="--:--";
            if(epoch>1600000000&&ap01Calendar(epoch,local)&&local.tm_hour>=0&&local.tm_hour<24&&local.tm_min>=0&&local.tm_min<60)
                snprintf(hhmm,sizeof(hhmm),"%02u:%02u",unsigned(local.tm_hour),unsigned(local.tm_min));
            business.clock(hhmm);businessLock.clear(std::memory_order_release);
        }
        if(!waitFd(fd,1,nowMs()+1000))continue;
        int client=ROM_FN(0xa01a192a,int (*)(int,void *,void *))(fd,nullptr,nullptr);if(client<0)continue;
        Ap01Stream io{&client,receive,send};uint64_t now=nowMs();
        bool maintenanceSent=false;
        if(maintenancePort)maintenanceSent=ap01ServeHttp(*app,true,io,now,now+5000);
        else if(!businessLock.test_and_set(std::memory_order_acquire)) {
            ap01ServeHttp(*app,false,io,now,now+5000);businessLock.clear(std::memory_order_release);
        } else {
            const char body[]="{\"result\":\"rejected\",\"error\":\"busy\",\"stage\":\"admission\",\"action\":\"read_status\"}";
            char headers[192];int length=snprintf(headers,sizeof(headers),"HTTP/1.1 503 Busy\r\nContent-Length: %u\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n",unsigned(sizeof(body)-1));
            for(auto part:{String(headers,length),String(body)}) {
                size_t at=0;while(at<part.length()){int n=send(&client,(const uint8_t *)part.c_str()+at,part.length()-at,now+5000);if(n<=0)break;at+=n;}
            }
        }
        closeFd(client);
        if(maintenancePort)app->maintenanceResponse(maintenanceSent);
    }
    if(maintenancePort)maintenanceListening.store(false,std::memory_order_release);
    if(!maintenancePort)app->businessOnline(false);closeFd(fd);
}
void *Runtime::maintenance(void *p) {((Runtime *)p)->serve(true);return nullptr;}
void *Runtime::udp(void *p) {
    Runtime &r=*(Runtime *)p;int fd=listener(8767,true);if(fd<0)return nullptr;
    uint64_t nextReply=0;
    while(!r.stop.load(std::memory_order_acquire)) {
        if(!waitFd(fd,1,nowMs()+1000))continue;
        uint8_t bytes[257];Address source{};unsigned length=sizeof(source);
        int n=ROM_FN(0xa003942c,int (*)(int,void *,size_t,int,Address *,unsigned *))(fd,bytes,sizeof(bytes),0x40,&source,&length);
        uint64_t now=nowMs();if(n<=0||n>256||length!=sizeof(source)||now<nextReply)continue;
        String reply;if(!r.app->discovery(bytes,n,reply))continue;
        ROM_FN(0xa003949c,int (*)(int,const void *,size_t,int,const void *,unsigned))(fd,reply.c_str(),reply.length(),0x40,&source,sizeof(source));
        nextReply=now+200;
    }closeFd(fd);return nullptr;
}
void *Runtime::bootstrap(void *p) {
    Runtime &r=*(Runtime *)p;char mac[13];
    if(!wifiMac(mac)||!r.fs.begin(false,"/resource",10,"resource"))return nullptr;
    r.bootSlot=ap01SelectedSlot0041();
    const char *hex="0123456789abcdef";
    for(unsigned i=0;i<16;++i){uint8_t v=ap01_native_manifest[36+i];r.firmwareBuild[2*i]=hex[v>>4];r.firmwareBuild[2*i+1]=hex[v&15];}
    Ap01ApplicationPort callbacks{&r,randomBytes,lockBindings,unlockBindings,
        {&r,firmwareBusy,firmwareOpen,firmwareWrite,firmwareVerify,firmwareCommit,firmwareClose,r.firmwareBuild,firmwareReboot,runningSlot,inspectSlot,selectSlot,bootHealthy}};
    r.app.reset(new(std::nothrow) Ap01Application(r.fs,r.business,callbacks,mac,r.bootId));
    if(!r.app||!r.app->begin(nowMs()))return nullptr;
    uint32_t initializing=0;
    if(!r.ready.compare_exchange_strong(initializing,1,std::memory_order_acq_rel))return nullptr;
    uint32_t thread=0;
    ROM_FN(0xa019750e,int (*)(uint32_t *,const char *,void *(*)(void *),unsigned,void *))(&thread,"ap01-maint",maintenance,16384,&r);
    ROM_FN(0xa019750e,int (*)(uint32_t *,const char *,void *(*)(void *),unsigned,void *))(&thread,"ap01-discover",udp,8192,&r);
    // Maintenance/discovery are already running; a failed business restore
    // must not take their listeners or pairing lifecycle down with it.
    // A damaged Bundle must remain repairable through an authenticated full
    // publish. The service exposes recovery state and the native UI remains.
    r.business.begin(nowMs());r.serve(false);
    return nullptr;
}
}

extern "C" __attribute__((used))
void ap01Register0041(void *root,void (*callback)(void *),unsigned filter,void *user) {
    if((uintptr_t)callback!=0xa00afc88||filter!=0x11) {
        ROM_FN(0xa01ad654,void (*)(void *,void (*)(void *),unsigned,void *))(root,callback,filter,user);return;
    }
    uint64_t boot=0;
    if(!randomBytes(nullptr,(uint8_t *)&boot,sizeof(boot))||!boot) {
        ROM_FN(0xa01ad654,void (*)(void *,void (*)(void *),unsigned,void *))(root,callback,filter,user);return;
    }
    Runtime *runtime=new(std::nothrow) Runtime(root,callback,boot);
    if(!runtime) {ROM_FN(0xa01ad654,void (*)(void *,void (*)(void *),unsigned,void *))(root,callback,filter,user);return;}
    ROM_FN(0xa01ad654,void (*)(void *,void (*)(void *),unsigned,void *))(root,Runtime::eventCallback,0,runtime);
    ROM_FN(0xa008bc64,void *(*)(void (*)(void *),unsigned,void *))(Runtime::timer,50,runtime);
    uint32_t thread=0;
    ROM_FN(0xa019750e,int (*)(uint32_t *,const char *,void *(*)(void *),unsigned,void *))
        (&thread,"ap01-business",Runtime::bootstrap,32768,runtime);
}
