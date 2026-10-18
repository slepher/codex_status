// Portable screen_confirm_v1 state machine: used by the AP01 port and host.
#include "ap01_binding.h"
#include <string.h>

uint32_t ap01BindingCrc(const uint8_t *p,size_t n) {
    uint32_t c=~0u;
    while(n--) {c^=*p++;for(unsigned i=0;i<8;i++)c=(c>>1)^((0u-(c&1))&0xedb88320u);}
    return ~c;
}
static uint32_t rd32(const uint8_t *p) {return p[0]|(uint32_t)p[1]<<8|(uint32_t)p[2]<<16|(uint32_t)p[3]<<24;}
static void wr32(uint8_t *p,uint32_t n) {for(unsigned i=0;i<4;i++)p[i]=(uint8_t)(n>>(8*i));}
static bool bounded(const char *s,size_t cap,bool id) {
    if (!s || !*s) return false;
    for(size_t i=0;i<cap;i++) {
        unsigned c=(unsigned char)s[i];if(!c)return true;
        if(c<32 || c==127 || (id && !((c>='a'&&c<='z')||(c>='A'&&c<='Z')||
            (c>='0'&&c<='9')||c=='-'||c=='_')))return false;
    }
    return false;
}
static bool valid(const Ap01Binding *b) {
    if(!b || !bounded(b->bridge,sizeof(b->bridge),true) || !bounded(b->request,sizeof(b->request),true) ||
       !bounded(b->name,sizeof(b->name),false) || !bounded(b->code,5,false) || strlen(b->code)!=4 ||
       !bounded(b->token,33,true) || strlen(b->token)!=32)return false;
    for(unsigned i=0;i<4;i++)if(b->code[i]<'0'||b->code[i]>'9')return false;
    for(unsigned i=0;i<32;i++)if(!((b->token[i]>='0'&&b->token[i]<='9')||
        (b->token[i]>='a'&&b->token[i]<='f')))return false;
    return true;
}
static void terminate(Ap01Bindings *s,unsigned state,const char *error) {
    s->state=state;s->pressed_event=0;s->revision++;
    memset(s->terminal_error,0,sizeof(s->terminal_error));
    if(error)memcpy(s->terminal_error,error,strlen(error));
}
static void expire(Ap01Bindings *s,uint64_t now) {
    if((s->state==1||s->state==2) && now>=s->deadline)terminate(s,4,"expired");
    if(s->owner_until && now>=s->owner_until){s->owner_until=0;s->owner[0]=0;}
}

bool ap01BindingsInit(Ap01Bindings *s,const Ap01BindingIo *io,uint64_t boot,const uint8_t *bytes,size_t n) {
    if(!s||!io||!io->allocate||!io->release||!io->save_verified||!boot)return false;
    memset(s,0,sizeof(*s));s->io=*io;s->boot_id=boot;
    if(!n)return true;
    if(!bytes||n<16||n>4096||memcmp(bytes,"APB1",4)||rd32(bytes+4)!=n||
       bytes[8]!=1||bytes[9]>8||bytes[10]||bytes[11]||rd32(bytes+12)!=ap01BindingCrc(bytes+16,n-16))return false;
    // Validate the complete shape before allocating a repository.
    size_t pos=16;Ap01Binding record;
    for(unsigned i=0;i<bytes[9];i++) {
        memset(&record,0,sizeof(record));
        char *fields[]={record.bridge,record.name,record.code,record.token,record.request};
        const size_t caps[]={65,97,5,33,65};
        for(unsigned j=0;j<5;j++) {
            if(pos>=n)return false;unsigned len=bytes[pos++];
            if(!len||len>=caps[j]||len>n-pos)return false;
            memcpy(fields[j],bytes+pos,len);pos+=len;
        }
        if(!valid(&record))return false;
    }
    if(pos!=n)return false;
    unsigned count=bytes[9];
    if(!count)return true;
    Ap01Binding *records=(Ap01Binding *)io->allocate(count*sizeof(Ap01Binding));
    if(!records)return false;
    memset(records,0,count*sizeof(Ap01Binding));pos=16;
    for(unsigned i=0;i<count;i++) {
        char *fields[]={records[i].bridge,records[i].name,records[i].code,records[i].token,records[i].request};
        for(unsigned j=0;j<5;j++){unsigned len=bytes[pos++];memcpy(fields[j],bytes+pos,len);pos+=len;}
        for(unsigned j=0;j<i;j++)if(!strcmp(records[i].bridge,records[j].bridge)||
            !strcmp(records[i].token,records[j].token)) {io->release(records);return false;}
    }
    s->bindings=records;s->count=count;return true;
}
void ap01BindingsFree(Ap01Bindings *s) {
    if(!s)return;if(s->bindings)s->io.release(s->bindings);memset(s,0,sizeof(*s));
}

const char *ap01BindingPrincipal(const Ap01Bindings *s,const char *token) {
    if(!s||!token)return NULL;
    for(unsigned i=0;i<s->count;i++) {
        unsigned difference=0;
        if(strlen(token)!=32)return NULL;
        for(unsigned j=0;j<32;j++)difference|=(unsigned char)token[j]^(unsigned char)s->bindings[i].token[j];
        if(!difference)return s->bindings[i].bridge;
    }
    return NULL;
}
const char *ap01PairBegin(Ap01Bindings *s,const Ap01Binding *b,uint64_t boot,uint32_t revision,uint64_t now) {
    if(!s||!valid(b))return "pairing_invalid";
    expire(s,now);
    const char *principal=ap01BindingPrincipal(s,b->token);
    if(principal)return !strcmp(principal,b->bridge)?"already_bound":"credential_conflict";
    if(s->storage_unknown)return "binding_commit_unknown";
    if(s->maintenance)return "maintenance_busy";
    if(s->owner_until>now)return "occupied";
    if(boot!=s->boot_id||revision!=s->revision)return "pairing_revision";
    if(s->state==1||s->state==2) {
        if(strcmp(s->pending.request,b->request))return "pairing_busy";
        if(strcmp(s->pending.bridge,b->bridge)||strcmp(s->pending.token,b->token)||
           strcmp(s->pending.name,b->name)||strcmp(s->pending.code,b->code))return "request_conflict";
        return s->state==1?"pending_display":"awaiting_confirmation";
    }
    if(s->pending.request[0]&&!strcmp(s->pending.request,b->request))return "request_closed";
    if(boot!=s->boot_id||revision!=s->revision)return "pairing_revision";
    bool replacement=false;
    for(unsigned i=0;i<s->count;i++)if(!strcmp(s->bindings[i].bridge,b->bridge))replacement=true;
    if(s->count==8&&!replacement)return "binding_capacity";
    s->pending=*b;s->deadline=now+120000;s->state=1;s->displayed_event=0;s->pressed_event=0;
    s->terminal_error[0]=0;return "pending_display";
}
const char *ap01PairResult(Ap01Bindings *s,const char *request,const char *token,uint64_t now) {
    if(!s||!request||!token)return "unauthorized";expire(s,now);
    for(unsigned i=0;i<s->count;i++)if(!strcmp(request,s->bindings[i].request)&&
        !strcmp(token,s->bindings[i].token))return "approved";
    if(strcmp(request,s->pending.request)||strcmp(token,s->pending.token))return "unauthorized";
    if(s->state==1)return "pending_display";
    if(s->state==2)return "awaiting_confirmation";
    if(s->state==3)return "cancelled";
    if(s->state==4)return "expired";
    return s->terminal_error[0]?s->terminal_error:"rejected";
}
const char *ap01PairCancel(Ap01Bindings *s,const char *request,const char *token,uint64_t now) {
    const char *result=ap01PairResult(s,request,token,now);
    if(!strcmp(result,"pending_display")||!strcmp(result,"awaiting_confirmation")) {
        terminate(s,3,"cancelled");return "cancelled";
    }
    return result;
}
bool ap01PairDisplayed(Ap01Bindings *s,const char *request,uint32_t revision,uint64_t event) {
    if(!s||s->state!=1||revision!=s->revision||strcmp(request,s->pending.request))return false;
    s->state=2;s->displayed_event=event;return true;
}
static const char *commit(Ap01Bindings *s) {
    unsigned at=s->count;
    for(unsigned i=0;i<s->count;i++)if(!strcmp(s->bindings[i].bridge,s->pending.bridge))at=i;
    unsigned count=s->count+(at==s->count?1:0);
    size_t n=16;
    for(unsigned i=0;i<count;i++) {
        const Ap01Binding *r=i==at?&s->pending:&s->bindings[i];
        n+=5+strlen(r->bridge)+strlen(r->name)+4+32+strlen(r->request);
    }
    uint8_t *bytes=(uint8_t *)s->io.allocate(n);if(!bytes)return "binding_oom";
    Ap01Binding *records=(Ap01Binding *)s->io.allocate(count*sizeof(Ap01Binding));
    if(!records){s->io.release(bytes);return "binding_oom";}
    if(s->count)memcpy(records,s->bindings,s->count*sizeof(Ap01Binding));records[at]=s->pending;
    memset(bytes,0,16);memcpy(bytes,"APB1",4);wr32(bytes+4,(uint32_t)n);bytes[8]=1;bytes[9]=(uint8_t)count;
    size_t pos=16;
    for(unsigned i=0;i<count;i++) {
        const char *fields[]={records[i].bridge,records[i].name,records[i].code,records[i].token,records[i].request};
        for(unsigned j=0;j<5;j++){size_t length=strlen(fields[j]);bytes[pos++]=(uint8_t)length;memcpy(bytes+pos,fields[j],length);pos+=length;}
    }
    wr32(bytes+12,ap01BindingCrc(bytes+16,n-16));
    int saved=s->io.save_verified(s->io.user,bytes,n);s->io.release(bytes);
    if(saved!=1){s->io.release(records);s->storage_unknown=saved<0;return saved<0?"binding_commit_unknown":"binding_storage";}
    if(s->bindings)s->io.release(s->bindings);s->bindings=records;s->count=count;return "approved";
}
const char *ap01PairButton(Ap01Bindings *s,bool pressed,uint64_t event,uint64_t now) {
    if(!s)return "pairing_invalid";expire(s,now);
    if(s->state!=2)return "no_confirmation";
    if(event<=s->displayed_event)return "stale_button";
    if(pressed){s->pressed_event=event;return "awaiting_release";}
    if(!s->pressed_event||event<=s->pressed_event)return "stale_button";
    if(s->maintenance||s->owner_until>now){terminate(s,5,"owner_changed");return "owner_changed";}
    const char *result=commit(s);terminate(s,5,result);return result;
}
const char *ap01BindingClaim(Ap01Bindings *s,const char *bridge,const char *token,uint64_t now) {
    if(!s||!bridge)return "unauthorized";expire(s,now);
    const char *principal=ap01BindingPrincipal(s,token);
    if(!principal||strcmp(principal,bridge))return "unauthorized";
    if(s->maintenance)return "maintenance_busy";
    if(s->owner_until>now&&strcmp(s->owner,bridge))return "occupied";
    if(s->state==1||s->state==2)terminate(s,5,"owner_changed");
    memcpy(s->owner,bridge,strlen(bridge)+1);s->owner_until=now+300000;return "applied";
}

bool ap01BindingsResolve(Ap01Bindings *s,const uint8_t *bytes,size_t length) {
    if(!s||!s->storage_unknown)return false;
    Ap01Bindings loaded;
    if(!ap01BindingsInit(&loaded,&s->io,s->boot_id,bytes,length))return false;
    bool matched=false;
    for(unsigned i=0;i<loaded.count;i++)if(!strcmp(loaded.bindings[i].request,s->pending.request)&&
        !strcmp(loaded.bindings[i].token,s->pending.token)&&!strcmp(loaded.bindings[i].bridge,s->pending.bridge))matched=true;
    if(!matched){ap01BindingsFree(&loaded);return false;}
    if(s->bindings)s->io.release(s->bindings);
    s->bindings=loaded.bindings;s->count=loaded.count;s->storage_unknown=false;
    return true;
}
