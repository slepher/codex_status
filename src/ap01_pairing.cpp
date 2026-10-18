#include "ap01_pairing.h"
#include <ArduinoJson.h>
#include <string.h>
#include <stdlib.h>

// XIP ports cannot use ArduinoJson's writable static allocator singleton.
// Each request owns this four-byte vtable handle for the document lifetime.
class Ap01JsonAllocator final : public ArduinoJson::Allocator {
public:
    void *allocate(size_t n) override {return malloc(n);}
    void deallocate(void *p) override {free(p);}
    void *reallocate(void *p,size_t n) override {return realloc(p,n);}
};

static bool field(JsonObject object,const char *key,char *out,size_t capacity) {
    const char *value=object[key].as<const char *>();
    if(!value||strlen(value)>=capacity)return false;
    memcpy(out,value,strlen(value)+1);return true;
}
static int writeJson(JsonDocument &document,char *out,size_t capacity,int status) {
    if(document.overflowed()) {
        const char error[]="{\"result\":\"unknown\",\"error\":\"reply_oom\",\"stage\":\"response\",\"action\":\"query_original_request\"}";
        if(!out||capacity<sizeof(error))return -1;
        memcpy(out,error,sizeof(error));return 503;
    }
    if(!out||!capacity||measureJson(document)>=capacity)return -1;
    serializeJson(document,out,capacity);return status;
}
int ap01PairingHttp(Ap01Bindings *s,const char *mac,const char *method,const char *path,
                   const char *token,const char *body,uint64_t now,char *out,size_t capacity) {
    if(!s||!mac||!method||!path||!token)return -1;
    // Polling also services expiry; it cannot renew a confirmation or owner.
    if(s->pending.request[0])ap01PairResult(s,s->pending.request,s->pending.token,now);
    Ap01JsonAllocator allocator;
    JsonDocument reply(&allocator);
    reply["device_mac"]=mac;reply["boot_id"]=s->boot_id;reply["pairing_revision"]=s->revision;
    if(!strcmp(method,"GET")&&!strcmp(path,"/api/pairing/info")) {
        reply["mac"]=mac;reply["model"]="njcuk.enstor.ap01";
        reply["binding_method"]="screen_confirm_v1";
        // At capacity a known Bridge may still replace its own relationship;
        // Begin performs the principal-aware capacity check before any popup.
        reply["can_pair"]=!s->maintenance&&s->owner_until<=now&&s->state!=1&&s->state!=2&&!s->storage_unknown;
        reply["binding_count"]=s->count;reply["binding_capacity"]=8;
        reply["reason"]=s->storage_unknown?"binding_commit_unknown":s->maintenance?"maintenance_busy":
            s->owner_until>now?"occupied":(s->state==1||s->state==2)?"pairing_busy":s->count==8?"binding_capacity":"ready";
        reply["owner_remaining_s"]=s->owner_until>now?(s->owner_until-now+999)/1000:0;
        return writeJson(reply,out,capacity,200);
    }
    const char *result="not_found";int status=404;
    if(!strcmp(method,"POST")&&!strcmp(path,"/api/pairing")) {
        JsonDocument request(&allocator);
        Ap01Binding candidate={};
        if(!body||strlen(body)>4096||deserializeJson(request,body)||!request.is<JsonObject>())result="pairing_invalid";
        else if(strcmp(request["device_mac"]|"",mac))result="device_identity";
        else if(!field(request.as<JsonObject>(),"bridge_id",candidate.bridge,sizeof(candidate.bridge))||
            !field(request.as<JsonObject>(),"name",candidate.name,sizeof(candidate.name))||
            !field(request.as<JsonObject>(),"display_code",candidate.code,sizeof(candidate.code))||
            !field(request.as<JsonObject>(),"binding_token",candidate.token,sizeof(candidate.token))||
            !field(request.as<JsonObject>(),"request_id",candidate.request,sizeof(candidate.request))||
            !request["boot_id"].is<uint64_t>()||!request["pairing_revision"].is<uint32_t>())result="pairing_invalid";
        else result=ap01PairBegin(s,&candidate,request["boot_id"].as<uint64_t>(),request["pairing_revision"].as<uint32_t>(),now);
        if(candidate.request[0])reply["request_id"]=candidate.request;
        status=400;
    } else if(!strncmp(path,"/api/pairing/",13)&&path[13]&&strlen(path+13)<=64) {
        reply["request_id"]=path+13;
        if(!strcmp(method,"GET"))result=ap01PairResult(s,path+13,token,now);
        else if(!strcmp(method,"DELETE"))result=ap01PairCancel(s,path+13,token,now);
        status=200;
    }
    if(!strcmp(result,"pending_display")||!strcmp(result,"awaiting_confirmation"))status=!strcmp(method,"POST")?202:200;
    else if(!strcmp(result,"approved")||!strcmp(result,"already_bound")||!strcmp(result,"cancelled")||!strcmp(result,"expired"))status=200;
    else if(!strcmp(result,"unauthorized"))status=401;
    else if(!strcmp(result,"binding_storage"))status=500;
    else if(!strcmp(result,"binding_commit_unknown"))status=503;
    else if(strcmp(result,"pairing_invalid")&&strcmp(result,"device_identity")&&strcmp(result,"not_found"))status=409;
    reply["result"]=result;reply["state"]=result;reply["pairing_revision"]=s->revision;
    if(status>=400){reply["error"]=result;reply["stage"]=s->storage_unknown?"storage":"pairing";}
    return writeJson(reply,out,capacity,status);
}
int ap01PairingUi(Ap01Bindings *s,uint64_t now,char *out,size_t capacity) {
    if(!s)return -1;
    if(s->pending.request[0])ap01PairResult(s,s->pending.request,s->pending.token,now);
    Ap01JsonAllocator allocator;
    JsonDocument reply(&allocator);reply["pending"]=s->state==1||s->state==2;
    reply["displayed"]=s->state==2;reply["revision"]=s->revision;
    if(s->state==1||s->state==2) {
        reply["request_id"]=s->pending.request;reply["bridge_name"]=s->pending.name;
        reply["display_code"]=s->pending.code;
        reply["remaining_s"]=s->deadline>now?(s->deadline-now+999)/1000:0;
    }
    return writeJson(reply,out,capacity,200);
}
