#include "application.h"
#include "native_services_0041.h"
#include "http.h"
#include "screen_port_0041.h"
#include "native_io_0041.h"
#include "vfs_0041.h"
#include "ap01_template_display.h"
#include "local_json_allocator.h"
#include <atomic>
#include <new>
#include <stddef.h>
#ifdef AP01_SCREEN_HOST_PROBE
#include "test_screen_peer_0041.h"
#endif

bool ap01Calendar(time_t epoch, struct tm &result);
extern "C" const uint8_t ap01_native_manifest[256];

namespace {
using namespace ap01_native0041;
struct Frame {uint8_t *pixels;uint32_t generation,transition,epoch;uint8_t count,index,tries=0;char ids[8][BS_ID_LEN]={};};

struct Activation {unsigned index;uint32_t transition;};

struct Runtime {
    LittleFSClass fs;
    Ap01Business business;
    std::unique_ptr<Ap01Application> app;
    Ap01NativeServices0041 services;
    std::atomic<bool> initialized{false};
    std::atomic<uint32_t> uiEpoch{1};
    uint32_t renderingEpoch=1; // Captured by the business executor before each operation.
    std::atomic<uint32_t> ready{0}; // 0 initializing, 1 live, 2 root retired.
    std::atomic<uint32_t> uiHeartbeat{0};
    std::atomic<bool> stop{false};
    std::atomic_flag bindingsLock=ATOMIC_FLAG_INIT,businessLock=ATOMIC_FLAG_INIT;
    std::atomic<Frame *> pending{nullptr};
    std::atomic<Activation *> activation{nullptr};
    std::atomic<uint32_t> uiToken{0},failedToken{0};
    uint32_t renderingToken=0; // Business executor only; copied into each frame.
    int rollbackIndex=-1; // UI executor only.

    std::atomic<unsigned> templateCount{0};
    std::atomic<bool> retired{false};
    // Published by the UI executor; the HTTP thread never reads LVGL fields.
    std::atomic<unsigned> displayWait{0};
    const uint64_t bootId;
    void *root,*content=nullptr,*image=nullptr,*dialog=nullptr;
    void (*original)(void *);
    uint64_t event=0;
    uint32_t shownGeneration=0,pairRevision=0,uiStage=0;
    uint8_t dialogPage=255,virtualIndex=0,lastFrameIndex=255;
    char pairRequest[65]={};
    bool focused=false,pairPending=false,pairWoken=false,resumeContent=false;
    bool replacing=false,uiMounted=false;
    uint32_t resumeInput=0,prepareToken=0;
    ap01_screen_slot *prepareSlot=nullptr;
    std::unique_ptr<ap01_screen_slot[]> slots;
    std::unique_ptr<ap01_page_0041[]> pages;
    ap01_screen_host host{};
    #ifdef AP01_SCREEN_HOST_PROBE
    ap01_screen_app apps[2]{};
    ap01_test_peer_0041 peer{};
    Frame *staleFixture=nullptr;
    void *retiredRoot=nullptr;
    uint32_t deleteAt=0;
    unsigned deletedPhase=0,deletedX=0;
    uint32_t staleRevision=0;
    bool probeBlocked=false;
#else
    ap01_screen_app apps[1]{};
#endif
    ap01_window_0041 window{};
    explicit Runtime(void *window,void (*callback)(void *),uint64_t boot)
        :fs(ap01RomVfs(),"/resource/codex-status"),business(fs,{this,present,retire,displayReason},boot),bootId(boot),root(window),original(callback) {}
    static const char *displayReason(void *p) {
        switch(((Runtime *)p)->displayWait.load(std::memory_order_acquire)) {
            case 1:return "waiting_for_draw";
            case 2:return "screen_not_visible";
            case 3:return "pairing_dialog";
            case 4:return "maintenance_active";
            default:return "waiting_for_template_page";
        }
    }
    static bool retire(void *p) {
        Runtime &r=*(Runtime *)p;if(r.ready.load(std::memory_order_acquire)==2)return true;r.retired.store(false,std::memory_order_release);
        if(!present(p,nullptr,0))return false;
        uint64_t deadline=nowMs()+1000;
        while(nowMs()<deadline) {if(r.retired.load(std::memory_order_acquire))return true;pause(5);}
        return false;
    }
    static bool present(void *p,uint8_t *pixels,uint32_t generation) {
        Runtime &r=*(Runtime *)p;if(r.ready.load(std::memory_order_acquire)==2||r.renderingEpoch!=r.uiEpoch.load(std::memory_order_acquire))return false;Frame *frame=new(std::nothrow) Frame{pixels,generation,r.renderingToken,r.renderingEpoch,uint8_t(pixels?r.business.count():0),r.business.active()};if(!frame)return false;
        for(unsigned i=0;i<frame->count;++i)strcpy(frame->ids[i],r.business.screenId(i));
        r.templateCount.store(frame->count,std::memory_order_release);
        Frame *old=r.pending.exchange(frame,std::memory_order_acq_rel);
        if(old){delete[] old->pixels;delete old;}
        if(r.ready.load(std::memory_order_acquire)==2) {
            Frame *mine=frame;
            if(r.pending.compare_exchange_strong(mine,nullptr,std::memory_order_acq_rel)){delete[] frame->pixels;delete frame;}
        }
        return true;
    }
    static bool lockBindings(void *p) {return !((Runtime *)p)->bindingsLock.test_and_set(std::memory_order_acquire);}
    static void unlockBindings(void *p) {((Runtime *)p)->bindingsLock.clear(std::memory_order_release);}
    static bool systemBusy(void *p) {return ((Runtime *)p)->services.busy();}
    static bool healthy(void *p) {
        auto &r=*(Runtime *)p;uint32_t beat=r.uiHeartbeat.load(std::memory_order_acquire);
        return r.ready.load(std::memory_order_acquire)==1&&beat&&uint32_t(nowMs())-beat<=1000;
    }
    bool mainRootVisible() const {return ap01_window_visible(const_cast<ap01_window_0041 *>(&window));}
    bool mainRootActive() const {return mainRootVisible()&&ap01_window_idle(const_cast<ap01_window_0041 *>(&window));}
    bool mainFocused() const {return ap01_window_awake(const_cast<ap01_window_0041 *>(&window));}
    unsigned nativePage() {return ap01_window_active(&window);}
    int pageIndex(ap01_screen_slot *s) {if(slots)for(unsigned i=0;i<8;++i)if(s==&slots[i])return int(i);return -1;}
    bool requestActivation(unsigned index,uint32_t token) {
        auto *next=new(std::nothrow) Activation{index,token};if(!next)return false;
        delete activation.exchange(next,std::memory_order_acq_rel);return true;
    }
    void processActivation() {
        if(app->maintenanceActive()||ready.load(std::memory_order_acquire)!=1)return;
        Activation *next=activation.exchange(nullptr,std::memory_order_acq_rel);if(!next)return;
        if(next->transition&&next->transition!=uiToken.load(std::memory_order_acquire)){delete next;return;}
        if(businessLock.test_and_set(std::memory_order_acquire)) {
            Activation *empty=nullptr;
            if(!activation.compare_exchange_strong(empty,next,std::memory_order_acq_rel))delete next;
            return;
        }
        renderingToken=next->transition;renderingEpoch=uiEpoch.load(std::memory_order_acquire);
        unsigned index=next->index==~0u?business.active():next->index;
        bool ok=business.active()==index?business.redraw():business.localActivate(index,nowMs());
        renderingToken=0;
        businessLock.clear(std::memory_order_release);
        if(!ok&&next->transition)failedToken.store(next->transition,std::memory_order_release);
        delete next;
    }
    static int prepare(void *p,ap01_screen_slot *slot,uint32_t token) {
        auto &r=*(Runtime *)p;
        int index=r.pageIndex(slot);
        if(index<0||r.replacing)return AP01_ENTER_FAILED;
        auto &surface=r.pages[index].surface;
        // The current front buffer is already rendered. Re-enter it without
        // blocking on the business executor or rendering the same data again.
        if(index==r.lastFrameIndex&&surface.image&&surface.bytes&&surface.generation==r.shownGeneration&&
           r.business.canReuseFrame(surface.generation)&&r.rollbackIndex<0&&
           !r.pending.load(std::memory_order_acquire)&&!r.activation.load(std::memory_order_acquire))
            return AP01_ENTER_READY;
        r.prepareToken=token;r.prepareSlot=slot;r.rollbackIndex=-1;
        r.uiToken.store(token,std::memory_order_release);
        if(r.requestActivation(slot->order,token))return AP01_ENTER_PENDING;
        r.uiToken.store(0,std::memory_order_release);r.prepareSlot=nullptr;return AP01_ENTER_FAILED;
    }
    static void leave(void *p,ap01_screen_slot *s) {
        auto &r=*(Runtime *)p;int i=r.pageIndex(s);
        // Retain only the latest front buffer while visiting stock pages.
        // Switching to another app template still releases the outgoing one.
        if(i>=0&&i!=r.lastFrameIndex)ap01_lvgl_0041.i4_retire(&r.pages[i].surface);
    }
    static void unbind(void *p,ap01_screen_slot *s) {
        auto &r=*(Runtime *)p;int i=r.pageIndex(s);
        if(i>=0)ap01_lvgl_0041.i4_retire(&r.pages[i].surface);
    }
    static void imageDrawn(void *event) {
        auto *r=(Runtime *)ap01_event_context(event);
        if(!r||r->ready.load(std::memory_order_acquire)!=1||!r->mainFocused()||
           r->pairPending||r->app->maintenanceActive())return;
        auto *s=ap01_host_current(&r->host);int i=r->pageIndex(s);
        if(i>=0&&r->pages[i].surface.image==ap01_event_target(event)&&
           r->pages[i].surface.generation==r->shownGeneration)
            r->business.displayed(r->shownGeneration);
    }
    static void dialogDrawn(void *event) {
        auto *r=(Runtime *)ap01_event_context(event);
        if(r&&r->ready.load(std::memory_order_acquire)==1&&r->pairPending&&r->mainFocused())
            r->app->pairingDisplayed(r->pairRequest,r->pairRevision,++r->event);
    }
    bool initUi() {
        ready.store(0,std::memory_order_release);
        window.win=root;window.stock=original;window.context=this;window.event=eventCallback;window.poll=timer;window.services=&services;
        uint32_t token=host.transition;
        uint32_t revisions[sizeof(apps)/sizeof(apps[0])];
        for(unsigned i=0;i<sizeof(apps)/sizeof(apps[0]);++i)revisions[i]=apps[i].revision;
        slots.reset(new(std::nothrow) ap01_screen_slot[8]{});
        pages.reset(new(std::nothrow) ap01_page_0041[8]{});
        auto port=ap01_window_port(&window);
        if(slots&&pages&&ap01_window_mount(&window,slots.get(),pages.get(),8)) {
            apps[0].app_id="codex_status";apps[0].slots=slots.get();apps[0].capacity=8;apps[0].context=this;apps[0].poll=pollApp;
            unsigned count=1;
#ifdef AP01_SCREEN_HOST_PROBE
            peer={};if(ap01_test_peer_mount(&peer,&apps[1],&window))count=2;
#endif
            uiMounted=ap01_host_init(&host,&port,apps,count)==AP01_HOST_OK;
        }
        if(!uiMounted)ap01_host_init(&host,&port,nullptr,0);
        host.transition=token;
        if(uiEpoch.load(std::memory_order_acquire)>1)
            for(unsigned i=0;i<host.app_count;++i){apps[i].revision=revisions[i]+1;if(!apps[i].revision)++apps[i].revision;}
        if(ap01_window_bind(&window)) {
            if(initialized.load(std::memory_order_acquire)) {
                ready.store(1,std::memory_order_release);app->businessOnline(true);
                requestActivation(~0u,0);
            }
            return true;
        }
        if(uiMounted)ap01_window_unmount(slots.get(),pages.get(),8);
        return false;
    }
    static void timer(void *p) {auto &r=*(Runtime *)p;r.tick();r.uiHeartbeat.store(uint32_t(nowMs()),std::memory_order_release);}
    static void eventCallback(void *p,void *e) {
        auto &r=*(Runtime *)p;
        if(ap01_event_code(e)==0x29&&ap01_event_target(e)==r.root) {
#ifdef AP01_SCREEN_HOST_PROBE
            r.deleteAt=0;
#endif
            r.ready.store(2,std::memory_order_release);r.uiEpoch.fetch_add(1,std::memory_order_acq_rel);
            if(r.app)r.app->businessOnline(false);
            ap01_host_destroy(&r.host);r.retired.store(true,std::memory_order_release);
            r.uiToken.store(0,std::memory_order_release);r.failedToken.store(0,std::memory_order_release);
            delete r.activation.exchange(nullptr,std::memory_order_acq_rel);
            Frame *pending=r.pending.exchange(nullptr,std::memory_order_acq_rel);
            if(pending){delete[] pending->pixels;delete pending;}
            r.prepareSlot=nullptr;r.rollbackIndex=-1;r.uiMounted=false;r.resumeContent=false;r.replacing=false;
            r.content=r.image=r.dialog=nullptr;r.dialogPage=255;r.pairRevision=0;r.pairRequest[0]=0;r.focused=false;
            ap01_window_detach(&r.window);r.root=nullptr;return;
        }
        r.key(e);
    }
    static void pollApp(void *p,uint32_t) {((Runtime *)p)->processFrames();}
    void processFrames();
    void tick();
    void key(void *);
    static void *bootstrap(void *);
    static void *udp(void *);
    void serve();
};
extern "C" {
__attribute__((section(".ap01_abi"),used))
extern const uint32_t ap01_runtime_abi[] = {
    1,sizeof(Runtime),sizeof(Ap01Business),sizeof(BundleStore),sizeof(Ap01Application),
    offsetof(Runtime,root),offsetof(Runtime,content),offsetof(Runtime,dialog),offsetof(Runtime,image),
    offsetof(Runtime,pairPending),offsetof(Runtime,pairWoken),offsetof(Runtime,focused),
    offsetof(Runtime,shownGeneration),offsetof(Runtime,templateCount),offsetof(Runtime,retired),offsetof(Runtime,ready),offsetof(Runtime,uiStage),
    offsetof(Runtime,host),offsetof(ap01_screen_host,phase),sizeof(ap01_screen_slot),offsetof(ap01_screen_slot,root),offsetof(ap01_screen_slot,child),offsetof(Runtime,slots),
};
}
void Runtime::processFrames() {
    if(ready.load(std::memory_order_acquire)!=1)return;
    Frame *frame=pending.exchange(nullptr,std::memory_order_acq_rel);if(!frame)return;
    if(frame->epoch!=uiEpoch.load(std::memory_order_acquire)) {delete[] frame->pixels;delete frame;return;}
    if(frame->pixels&&((frame->transition&&(host.phase!=1||frame->transition!=host.transition||frame->transition!=uiToken.load(std::memory_order_acquire)))||
       (!frame->transition&&rollbackIndex>=0&&frame->index!=unsigned(rollbackIndex)))) {
        delete[] frame->pixels;delete frame;return;
    }
    if(frame->pixels&&rollbackIndex>=0&&frame->index==unsigned(rollbackIndex))rollbackIndex=-1;

    auto defer=[&](){Frame *empty=nullptr;if(pending.compare_exchange_strong(empty,frame,std::memory_order_acq_rel))frame=nullptr;};
    if(!frame->pixels) {
        auto *current=ap01_host_current(&host);resumeContent=pageIndex(current)>=0&&mainRootVisible();
        resumeInput=host.input_revision;replacing=true;prepareSlot=nullptr;uiToken.store(0,std::memory_order_release);rollbackIndex=-1;
        if(uiMounted)ap01_host_retire(&apps[0]);
        content=image=nullptr;focused=false;retired.store(true,std::memory_order_release);
    } else if(!uiMounted) {
        business.displayFailed(frame->generation);
    } else {
        bool changed=replacing||apps[0].count!=frame->count;
        for(unsigned i=0;i<frame->count&&!changed;++i) {
            auto *s=ap01_host_find(&apps[0],frame->ids[i]);changed=!s||s->order!=i;
        }
        if(changed) {
            ap01_screen_desc descriptions[8]{};
            for(unsigned i=0;i<frame->count;++i)
                descriptions[i]={AP01_SCREEN_ABI_0041,sizeof(ap01_screen_desc),frame->ids[i],this,prepare,nullptr,leave,unbind,nullptr,1};
            int result=ap01_host_api.replace_screen_set(&apps[0],apps[0].revision,descriptions,frame->count);
            if(result==AP01_HOST_BUSY)defer();
            else if(result!=AP01_HOST_OK){business.displayFailed(frame->generation);delete[] frame->pixels;delete frame;return;}
        }
        if(frame) {
            auto *slot=ap01_host_find(&apps[0],frame->ids[frame->index]);int index=pageIndex(slot);
            if(host.phase==2&&slot!=host.target)defer();
            else if(index>=0) {
                for(unsigned i=0;i<8;++i)if(&slots[i]!=slot&&&slots[i]!=host.source&&&slots[i]!=ap01_host_current(&host))
                    ap01_lvgl_0041.i4_retire(&pages[i].surface);
                uint16_t palette[16];for(unsigned i=0;i<16;++i)palette[i]=ap01Palette565(i);
                if(ap01_lvgl_0041.i4_present(&pages[index].surface,slot->root,frame->pixels,palette,frame->generation,imageDrawn,this)) {
                    replacing=false;shownGeneration=frame->generation;virtualIndex=lastFrameIndex=frame->index;
                    if(prepareSlot==slot) {
                        if(ap01_host_enter_ready(&host,prepareToken,1)==AP01_HOST_OK){prepareSlot=nullptr;uiToken.store(0,std::memory_order_release);}
                    } else if(!host.phase&&(pageIndex(ap01_host_current(&host))>=0||(resumeContent&&resumeInput==host.input_revision))&&mainFocused()&&!pairPending&&!app->maintenanceActive()) {
                        if(!resumeContent||resumeInput==host.input_revision)
                            ap01_host_api.request_switch_to(&apps[0],slot->id,2,uint32_t(nowMs()));
                        resumeContent=false;
                    }
                } else if(++frame->tries<3)defer();
                else {business.displayFailed(frame->generation);if(prepareSlot==slot){ap01_host_enter_ready(&host,prepareToken,0);}resumeContent=false;}
            } else business.displayFailed(frame->generation);
        }
    }
    if(frame){delete[] frame->pixels;delete frame;}
}
void Runtime::tick() {
#ifdef AP01_SCREEN_HOST_PROBE
    if(deleteAt&&ready.load(std::memory_order_acquire)==1&&(int32_t)(uint32_t(nowMs())-deleteAt)>=0) {
        deleteAt=0;deletedPhase=host.phase;deletedX=ap01_test_peer_diagnostic(&peer,9);
        retiredRoot=root;ap01_lvgl_0041.obj_delete(root);return;
    }
#endif
    if(ready.load(std::memory_order_acquire)!=1)return;
    String state;
    if(app->pairingUi(nowMs(),state)) {
        LocalJsonAllocator allocator;JsonDocument doc(&allocator);
        if(!deserializeJson(doc,state)) {
            bool pendingPair=doc["pending"]==true;const char *request=doc["request_id"]|"";
            unsigned page=nativePage();
            if(pendingPair&&mainRootActive()&&(strcmp(request,pairRequest)||dialogPage!=page)) {
                pairWoken=false;if(dialog)ap01_lvgl_0041.obj_delete(dialog);
                dialog=ap01_window_overlay(&window);dialogPage=page;
                if(dialog) {
                    void *title=ap01_lvgl_0041.label(dialog,"PAIR CODE",100,28);
                    void *code=title?ap01_lvgl_0041.label(dialog,doc["display_code"]|"----",40,80):nullptr;
                    void *hint=code?ap01_lvgl_0041.label(dialog,"Press then release",45,192):nullptr;
                    if(title&&code&&hint){ap01_lvgl_0041.on_draw(code,dialogDrawn,this);
                        if(strlen(request)<sizeof(pairRequest))strcpy(pairRequest,request);pairRevision=doc["revision"]|0u;}
                    else {ap01_lvgl_0041.obj_delete(dialog);dialog=nullptr;dialogPage=255;}
                }
            }
            pairPending=pendingPair;
        }
    }
    if(pairPending&&dialog&&mainRootActive()&&!pairWoken){ap01_window_wake();pairWoken=true;}
    if(!pairPending)pairWoken=false;
    bool system=pairPending||app->maintenanceActive();
#ifdef AP01_SCREEN_HOST_PROBE
    system=system||probeBlocked;
#endif
    ap01_host_set_blocked(&host,system);
    if(system)resumeContent=false;
    if(!uiMounted)processFrames();
    ap01_host_poll(&host,uint32_t(nowMs()));
    uint32_t failed=failedToken.exchange(0,std::memory_order_acq_rel);
    if(prepareSlot&&failed==prepareToken)ap01_host_enter_ready(&host,prepareToken,0);
    if(prepareSlot&&host.transition!=prepareToken) {
        uiToken.store(0,std::memory_order_release);
        leave(this,prepareSlot);auto *current=ap01_host_current(&host);
        if(pageIndex(current)<0)prepareSlot=nullptr;
        else if(requestActivation(current->order,0)){rollbackIndex=int(current->order);prepareSlot=nullptr;}
    }
    auto *slot=ap01_host_current(&host);int index=pageIndex(slot);
    focused=index>=0&&!replacing;content=index>=0?slot->root:nullptr;image=index>=0?pages[index].surface.image:nullptr;
    for(unsigned i=0;uiMounted&&i<8;++i)ap01_lvgl_0041.obj_hide(pages[i].surface.image,system||!mainFocused());
    ap01_lvgl_0041.obj_hide(dialog,!pairPending||!mainFocused());
    displayWait.store(app->maintenanceActive()?4:pairPending?3:(index<0&&!resumeContent)?0:!mainFocused()?2:1,std::memory_order_release);
    uiStage=27;
}
void Runtime::key(void *e) {
    unsigned code=ap01_event_code(e);
    if(code!=1&&code!=0xb&&code!=0x11)return;
    resumeContent=false;
    if(code==1||code==0xb)ap01_host_button(&host,code==1,uint32_t(nowMs()));
    if(ready.load(std::memory_order_acquire)==1&&app->maintenanceActive()) {
        ap01_host_cancel(&host);if(code==0x11&&!ap01_host_current(&host))original(e);return;
    }
    if(ready.load(std::memory_order_acquire)==1&&pairPending&&mainFocused()) {
        ap01_window_activity();if(code==1||code==0xb)app->button(code==1,++event,nowMs());return;
    }
    if(code!=0x11)return;
    if(ready.load(std::memory_order_acquire)!=1||!mainFocused()){original(e);return;}
    unsigned key=ap01_event_key(e);ap01_window_activity();
    int consumed=key==0x13?ap01_host_rotate(&host,1,uint32_t(nowMs())):
        key==0x14?ap01_host_rotate(&host,-1,uint32_t(nowMs())):key==10?ap01_host_short(&host):0;
    if(!consumed)original(e);
}

void Runtime::serve() {
    int fd=listener(18081);if(fd<0)return;
    app->businessOnline(true);
    while(!stop.load(std::memory_order_acquire)) {
        processActivation();
        if(!app->maintenanceActive()&&!businessLock.test_and_set(std::memory_order_acquire)) {
            renderingEpoch=uiEpoch.load(std::memory_order_acquire);
            time_t epoch=time(nullptr);struct tm local{};char hhmm[6]="--:--";
            if(epoch>1600000000&&ap01Calendar(epoch,local)&&local.tm_hour>=0&&local.tm_hour<24&&local.tm_min>=0&&local.tm_min<60)
                snprintf(hhmm,sizeof(hhmm),"%02u:%02u",unsigned(local.tm_hour),unsigned(local.tm_min));
            business.clock(hhmm);businessLock.clear(std::memory_order_release);
        }
        if(!waitFd(fd,1,nowMs()+1000))continue;
        int client=ap01_native0041::accept(fd);if(client<0)continue;
        Ap01Stream io{&client,receive,send};uint64_t now=nowMs();
        if(!businessLock.test_and_set(std::memory_order_acquire)) {
            renderingEpoch=uiEpoch.load(std::memory_order_acquire);
            ap01ServeHttp(*app,io,now,now+5000);businessLock.clear(std::memory_order_release);
        } else {
            const char body[]="{\"result\":\"rejected\",\"error\":\"busy\",\"stage\":\"admission\",\"action\":\"read_status\"}";
            char headers[192];int length=snprintf(headers,sizeof(headers),"HTTP/1.1 503 Busy\r\nContent-Length: %u\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n",unsigned(sizeof(body)-1));
            for(auto part:{String(headers,length),String(body)}) {
                size_t at=0;while(at<part.length()){int n=send(&client,(const uint8_t *)part.c_str()+at,part.length()-at,now+5000);if(n<=0)break;at+=n;}
            }
        }
        closeFd(client);
    }
    app->businessOnline(false);closeFd(fd);
}
void *Runtime::udp(void *p) {
    Runtime &r=*(Runtime *)p;int fd=listener(8767,true);if(fd<0)return nullptr;
    uint64_t nextReply=0;
    while(!r.stop.load(std::memory_order_acquire)) {
        if(!waitFd(fd,1,nowMs()+1000))continue;
        uint8_t bytes[257];Address source{};unsigned length=sizeof(source);
        int n=receiveFrom(fd,bytes,sizeof(bytes),&source,&length);
        uint64_t now=nowMs();if(n<=0||n>256||length!=sizeof(source)||now<nextReply)continue;
        String reply;if(!r.app->discovery(bytes,n,reply))continue;
        sendTo(fd,reply.c_str(),reply.length(),&source,sizeof(source));
        nextReply=now+200;
    }closeFd(fd);return nullptr;
}
void *Runtime::bootstrap(void *p) {
    Runtime &r=*(Runtime *)p;char mac[13];
    if(!wifiMac(mac)||!r.fs.begin(false,"/resource",10,"resource"))return nullptr;
    Ap01ApplicationPort callbacks{&r,randomBytes,lockBindings,unlockBindings,systemBusy};
    r.app.reset(new(std::nothrow) Ap01Application(r.fs,r.business,callbacks,mac,r.bootId));
    if(!r.app)return nullptr;
    bool appReady=r.app->begin(nowMs());
    Ap01MaintenanceBridge0041 bridge{r.app.get(),
        [](void *p,const char *token,char out[65]){return ((Ap01Application *)p)->bindingPrincipal(token,out);},
        [](void *p,const Ap01HttpRequest &q,uint64_t now){return ((Ap01Application *)p)->pairingHttp(q,now);},
        [](void *p){return ((Ap01Application *)p)->businessAvailable();}};
    if(!r.services.start(bridge,r.bootId,&r,healthy))return nullptr;
    if(!appReady)return nullptr; // System maintenance already has an independent executor.
    r.initialized.store(true,std::memory_order_release);
    uint32_t initializing=0;
    r.ready.compare_exchange_strong(initializing,1,std::memory_order_acq_rel);
    uint32_t thread=0;
    ap01_native0041::thread(&thread,"ap01-discover",udp,8192,&r);
    // Maintenance/discovery are already running; a failed business restore
    // must not take their listeners or pairing lifecycle down with it.
    // A damaged Bundle must remain repairable through an authenticated full
    // publish. The service exposes recovery state and the native UI remains.
    r.renderingEpoch=r.uiEpoch.load(std::memory_order_acquire);r.business.begin(nowMs());r.serve();
    return nullptr;
}
}

extern "C" __attribute__((used))
void ap01Register0041(void *root,void (*callback)(void *),unsigned filter,void *user) {
    if(!ap01_native0041::mainHook(callback,filter)) {
        ap01_native0041::registerStock(root,callback,filter,user);return;
    }
    if(auto *runtime=(Runtime *)ap01_window_find(Runtime::timer)) {
        if(runtime->root==root)return; // Duplicate registration is idempotent.
        if(runtime->root){ap01_native0041::registerStock(root,callback,filter,user);return;}
        runtime->root=root;runtime->original=callback;
        if(!runtime->initUi()) {
            runtime->ready.store(2,std::memory_order_release);runtime->root=nullptr;
            ap01_window_detach(&runtime->window);
            ap01_native0041::registerStock(root,callback,filter,user);
        }
        return;
    }
    uint64_t boot=0;
    if(!randomBytes(nullptr,(uint8_t *)&boot,sizeof(boot))||!boot) {
        ap01_native0041::registerStock(root,callback,filter,user);return;
    }
    Runtime *runtime=new(std::nothrow) Runtime(root,callback,boot);
    if(!runtime) {ap01_native0041::registerStock(root,callback,filter,user);return;}
    if(!runtime->initUi()) {
        delete runtime;
        ap01_native0041::registerStock(root,callback,filter,user);return;
    }
    uint32_t thread=0;
    ap01_native0041::thread
        (&thread,"ap01-business",Runtime::bootstrap,32768,runtime);
}

#ifdef AP01_SCREEN_HOST_PROBE
// Invoked only by the offline runner, on the original UI executor.
extern "C" __attribute__((used,noinline))
unsigned ap01HostProbe0041(void *context,unsigned operation) {
    auto &r=*(Runtime *)context;
    switch(operation) {
    case 0:return (uintptr_t)ap01_window_find(Runtime::timer);
    case 1:return ap01_host_api.request_switch_to(&r.apps[1],"peer-a",2,uint32_t(nowMs()));
    case 2:return ap01_host_api.request_switch_to(&r.apps[1],"peer-b",2,uint32_t(nowMs()));
    case 3:r.retiredRoot=r.root;if(r.root)ap01_lvgl_0041.obj_delete(r.root);return r.ready.load(std::memory_order_acquire);
    case 5: {
        for(unsigned i=0;i<8;++i)if(r.slots[i].id[0]&&r.slots[i].order==0)
            return ap01_host_api.request_switch_to(&r.apps[0],r.slots[i].id,2,uint32_t(nowMs()));
        return AP01_HOST_INVALID;
    }
    case 6: {
        if(r.staleFixture)return 0;
        auto *f=new(std::nothrow) Frame{};if(!f)return 0;
        f->pixels=new(std::nothrow) uint8_t[38400];if(!f->pixels){delete f;return 0;}
        memset(f->pixels,0xbb,38400);f->generation=0xdeadbeef;f->epoch=r.uiEpoch.load(std::memory_order_acquire);
        f->count=r.apps[0].count;f->index=r.virtualIndex;
        for(unsigned i=0;i<8;++i)if(r.slots[i].id[0])strcpy(f->ids[r.slots[i].order],r.slots[i].id);
        r.staleFixture=f;return 1;
    }
    case 7: {
        if(!r.staleFixture)return 0;
        auto *old=r.pending.exchange(r.staleFixture,std::memory_order_acq_rel);r.staleFixture=nullptr;
        if(old){delete[] old->pixels;delete old;}return 1;
    }
    case 8:ap01Register0041(r.root,r.original,0x11,nullptr);return 1;
    case 9:return r.peer.polls;
    case 10:return r.peer.draws;
    case 11:return r.host.locked;
    case 12:return r.peer.escapes;
    case 13:return r.peer.inputs;
    case 14:return ap01_test_peer_replace(&r.peer,2,1);
    case 15:return ap01_test_peer_replace(&r.peer,2,0);
    case 16:return ap01_test_peer_replace(&r.peer,1,0);
    case 17:return r.peer.slots[0].child;
    case 18:return r.peer.slots[1].child;
    case 20:return (uintptr_t)r.window.timer;
    case 21:return r.uiEpoch.load(std::memory_order_acquire);
    case 22:return r.ready.load(std::memory_order_acquire);
    case 23:r.staleRevision=r.apps[0].revision;return r.staleRevision;
    case 24:return r.apps[0].revision;
    case 25:return ap01_host_replace(&r.apps[0],r.staleRevision,nullptr,0);
    case 26:r.probeBlocked=true;return 1;
    case 27:r.probeBlocked=false;return 1;
    case 28:return ap01_test_peer_hidden(&r.peer);
    case 50:return ap01_window_binding_count(r.root);
    case 51:return ap01_window_binding_count(r.retiredRoot);
    case 52: {
        int result=ap01_host_api.request_switch_to(&r.apps[1],"peer-a",2,uint32_t(nowMs()));
        if(result==AP01_HOST_OK)r.deleteAt=uint32_t(nowMs())+75;
        return result;
    }
    case 53:return r.deletedPhase;
    case 54:return r.deletedX;
    default:if(operation>=40&&operation<50)return ap01_test_peer_diagnostic(&r.peer,operation-40);return ~0u;
    }
}
#endif
