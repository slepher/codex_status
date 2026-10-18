#include "ap01_binding.h"
#include <vector>
#include <cstdlib>
#include <cstring>

struct Store {std::vector<uint8_t> bytes;int outcome=1;};
static int save(void *user,const uint8_t *bytes,size_t size) {
    Store &store=*(Store *)user;
    if(store.outcome)store.bytes.assign(bytes,bytes+size);
    return store.outcome;
}
static Ap01Binding candidate(unsigned number) {
    Ap01Binding b={};
    strcpy(b.bridge,"test-bridge-0");b.bridge[12]=(char)('0'+number);
    strcpy(b.request,"request-0");b.request[8]=(char)('0'+number);
    strcpy(b.name,"Test computer");strcpy(b.code,"4827");
    memset(b.token,"0123456789abcdef"[number%16],32);b.token[32]=0;return b;
}
#define CHECK(x) do{if(!(x))return __LINE__;}while(0)
extern "C" int codex_ap01_binding_check() {
    Store store;Ap01BindingIo io={&store,malloc,free,save};Ap01Bindings state;
    CHECK(ap01BindingsInit(&state,&io,1,NULL,0));
    auto b=candidate(0);
    CHECK(!strcmp(ap01PairBegin(&state,&b,1,0,100),"pending_display"));
    CHECK(!ap01BindingPrincipal(&state,b.token));
    CHECK(!strcmp(ap01PairButton(&state,false,1,101),"no_confirmation"));
    CHECK(ap01PairDisplayed(&state,b.request,0,4));
    CHECK(!strcmp(ap01PairButton(&state,false,5,102),"stale_button"));
    CHECK(!strcmp(ap01PairButton(&state,true,4,102),"stale_button"));
    CHECK(!strcmp(ap01PairButton(&state,true,5,102),"awaiting_release"));
    CHECK(!strcmp(ap01PairButton(&state,false,6,103),"approved"));
    CHECK(state.count==1 && store.bytes.size()>16);
    CHECK(!strcmp(ap01PairResult(&state,b.request,b.token,104),"approved"));
    CHECK(!strcmp(ap01BindingPrincipal(&state,b.token),b.bridge));
    CHECK(!strcmp(ap01BindingClaim(&state,b.bridge,b.token,105),"applied"));
    auto other=candidate(1);
    CHECK(!strcmp(ap01PairBegin(&state,&other,1,state.revision,106),"occupied"));
    // Reboot restores bindings, not owner or an in-flight confirmation.
    ap01BindingsFree(&state);
    CHECK(ap01BindingsInit(&state,&io,2,store.bytes.data(),store.bytes.size()));
    CHECK(!state.owner_until && !strcmp(ap01PairResult(&state,b.request,b.token,0),"approved"));
    CHECK(!strcmp(ap01PairBegin(&state,&other,2,0,0),"pending_display"));
    auto competing=candidate(2);
    CHECK(!strcmp(ap01PairBegin(&state,&competing,2,0,1),"pairing_busy"));
    CHECK(!strcmp(ap01PairBegin(&state,&other,2,0,119999),"pending_display"));
    CHECK(state.deadline==120000);
    CHECK(!strcmp(ap01PairResult(&state,other.request,other.token,120000),"expired"));
    CHECK(!ap01PairDisplayed(&state,other.request,0,2));
    other=candidate(2);
    CHECK(!strcmp(ap01PairBegin(&state,&other,2,state.revision,120001),"pending_display"));
    CHECK(ap01PairDisplayed(&state,other.request,state.revision,7));
    store.outcome=0;
    ap01PairButton(&state,true,8,120002);
    CHECK(!strcmp(ap01PairButton(&state,false,9,120003),"binding_storage"));
    CHECK(state.count==1 && !ap01BindingPrincipal(&state,other.token));
    // A commit with a lost readback fences new transactions until the original
    // persisted request is independently recovered, never a new token/ID retry.
    other=candidate(3);
    CHECK(!strcmp(ap01PairBegin(&state,&other,2,state.revision,120004),"pending_display"));
    CHECK(ap01PairDisplayed(&state,other.request,state.revision,10));store.outcome=-1;
    ap01PairButton(&state,true,11,120005);
    CHECK(!strcmp(ap01PairButton(&state,false,12,120006),"binding_commit_unknown"));
    competing=candidate(4);
    CHECK(!strcmp(ap01PairBegin(&state,&competing,2,state.revision,120007),"binding_commit_unknown"));
    CHECK(ap01BindingsResolve(&state,store.bytes.data(),store.bytes.size()));
    CHECK(!strcmp(ap01PairResult(&state,other.request,other.token,120008),"approved"));
    CHECK(state.count==2);
    // A known principal's claim atomically cancels an outstanding popup.
    CHECK(!strcmp(ap01PairBegin(&state,&competing,2,state.revision,120009),"pending_display"));
    CHECK(!strcmp(ap01BindingClaim(&state,b.bridge,b.token,120010),"applied"));
    CHECK(!strcmp(ap01PairResult(&state,competing.request,competing.token,120011),"owner_changed"));
    // Corruption, truncation and undeclared trailing bytes never authenticate.
    auto valid=store.bytes;ap01BindingsFree(&state);
    valid.back()^=1;CHECK(!ap01BindingsInit(&state,&io,3,valid.data(),valid.size()));
    CHECK(!ap01BindingsInit(&state,&io,3,store.bytes.data(),store.bytes.size()-1));
    valid=store.bytes;valid.push_back(0);CHECK(!ap01BindingsInit(&state,&io,3,valid.data(),valid.size()));
    CHECK(ap01BindingsInit(&state,&io,4,NULL,0));store.outcome=1;
    for(unsigned i=0;i<8;i++) {
        auto next=candidate(i);
        CHECK(!strcmp(ap01PairBegin(&state,&next,4,state.revision,i*10),"pending_display"));
        CHECK(ap01PairDisplayed(&state,next.request,state.revision,i*3+1));
        ap01PairButton(&state,true,i*3+2,i*10+1);
        CHECK(!strcmp(ap01PairButton(&state,false,i*3+3,i*10+2),"approved"));
        CHECK(state.count==i+1);
    }
    auto ninth=candidate(8);
    CHECK(!strcmp(ap01PairBegin(&state,&ninth,4,state.revision,100),"binding_capacity"));
    CHECK(state.count==8);
    auto replacement=candidate(0);strcpy(replacement.request,"replacement");memset(replacement.token,'f',32);
    CHECK(!strcmp(ap01PairBegin(&state,&replacement,4,state.revision,101),"pending_display"));
    CHECK(ap01PairDisplayed(&state,replacement.request,state.revision,30));
    ap01PairButton(&state,true,31,102);
    CHECK(!strcmp(ap01PairButton(&state,false,32,103),"approved"));
    CHECK(state.count==8);
    auto retired=candidate(0);CHECK(!ap01BindingPrincipal(&state,retired.token));
    CHECK(ap01BindingPrincipal(&state,replacement.token));
    ap01BindingsFree(&state);
    return 0;
}
