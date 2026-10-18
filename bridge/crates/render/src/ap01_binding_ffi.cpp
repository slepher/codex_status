#include "ap01_pairing.h"
#include <stdlib.h>
extern "C" {
void *codex_ap01_binding_new(uint64_t boot,const uint8_t *bytes,size_t length,
                           int (*save)(void *,const uint8_t *,size_t),void *user) {
    Ap01Bindings *state=(Ap01Bindings *)malloc(sizeof(Ap01Bindings));
    Ap01BindingIo io={user,malloc,free,save};
    if(!state)return NULL;
    if(!ap01BindingsInit(state,&io,boot,bytes,length)){free(state);return NULL;}
    return state;
}
void codex_ap01_binding_free(void *state) {if(state){ap01BindingsFree((Ap01Bindings *)state);free(state);}}
int codex_ap01_pairing_http(void *state,const char *mac,const char *method,const char *path,
    const char *token,const char *body,uint64_t now,char *out,size_t cap) {
    return ap01PairingHttp((Ap01Bindings *)state,mac,method,path,token,body,now,out,cap);
}
int codex_ap01_pairing_ui(void *state,uint64_t now,char *out,size_t cap) {
    return ap01PairingUi((Ap01Bindings *)state,now,out,cap);
}
int codex_ap01_pairing_displayed(void *state,const char *request,uint32_t revision,uint64_t event) {
    return ap01PairDisplayed((Ap01Bindings *)state,request,revision,event)?1:0;
}
const char *codex_ap01_pairing_button(void *state,int pressed,uint64_t event,uint64_t now) {
    return ap01PairButton((Ap01Bindings *)state,pressed!=0,event,now);
}
const char *codex_ap01_principal(void *state,const char *token) {
    return ap01BindingPrincipal((Ap01Bindings *)state,token);
}
const char *codex_ap01_claim(void *state,const char *bridge,const char *token,uint64_t now) {
    return ap01BindingClaim((Ap01Bindings *)state,bridge,token,now);
}
int codex_ap01_binding_resolve(void *state,const uint8_t *bytes,size_t n) {
    return ap01BindingsResolve((Ap01Bindings *)state,bytes,n)?1:0;
}
int codex_ap01_owner(void *state,uint64_t now,char *out,size_t cap) {
    Ap01Bindings *s=(Ap01Bindings *)state;
    if(!s||!out||cap<65)return -1;
    if(s->owner_until<=now){out[0]=0;return 0;}
    size_t n=0;while(s->owner[n]){out[n]=s->owner[n];n++;}out[n]=0;
    return (int)((s->owner_until-now+999)/1000);
}
}

extern "C" void codex_ap01_pairing_cancel_ui(void *state,uint64_t now) {
    Ap01Bindings *s=(Ap01Bindings *)state;
    if(s && (s->state==1||s->state==2))ap01PairCancel(s,s->pending.request,s->pending.token,now);
}
