#include "firmware.h"
#include "application.h"
#include "local_json_allocator.h"
#include "v2_sync.h"
#include <new>
extern "C" {
#include "ota_image.h"
}

namespace {
constexpr uint32_t MIN_ROM=296, MAX_ROM=0x700000, MANIFEST=0x675000;
enum Phase { Receiving=1, Committing=2, Applied=3, Failed=4, Unknown=5 };
uint32_t le32(const uint8_t *p) {return uint32_t(p[0])|uint32_t(p[1])<<8|uint32_t(p[2])<<16|uint32_t(p[3])<<24;}
void put32(uint8_t *p,uint32_t n) {for(unsigned i=0;i<4;++i)p[i]=uint8_t(n>>(8*i));}
bool id(const char *p) {
    size_t n=p?strlen(p):0;if(!n||n>64)return false;
    for(;*p;++p)if(!((*p>='a'&&*p<='z')||(*p>='A'&&*p<='Z')||(*p>='0'&&*p<='9')||*p=='-'||*p=='_'))return false;
    return true;
}
bool hex32(const char *p,uint32_t &value) {
    if(!p||strlen(p)!=8)return false;value=0;
    for(unsigned i=0;i<8;++i){char c=p[i];unsigned d=c>='0'&&c<='9'?c-'0':c>='a'&&c<='f'?c-'a'+10:c>='A'&&c<='F'?c-'A'+10:16;if(d==16)return false;value=(value<<4)|d;}return true;
}
Ap01HttpReply jsonReply(JsonDocument &doc,int status) {
    Ap01HttpReply out;out.status=status;
    if(doc.overflowed()||measureJson(doc)>2048||!serializeJson(doc,out.bytes)) {
        out.status=503;out.bytes="{\"result\":\"unknown\",\"error\":\"response_oom\",\"stage\":\"response\",\"action\":\"query_original_request\"}";
    }return out;
}
Ap01HttpReply reject(int status,const char *reason,const char *stage="firmware_admission",bool unknown=false) {
    LocalJsonAllocator a;JsonDocument doc(&a);doc["result"]=unknown?"unknown":"rejected";doc["error"]=reason;doc["stage"]=stage;
    doc["action"]=unknown?"restart_then_query_slots":"read_maintenance_status";return jsonReply(doc,status);
}
bool matches(File &f,const uint8_t *p,size_t n) {
    if(!f||f.size()!=n)return false;for(size_t i=0;i<n;++i)if(f.read()!=p[i])return false;return true;
}
}
struct Ap01Firmware::Record {
    String owner,request,targetBuild,error;
    uint32_t revision=0,length=0,recoveryCrc=0,position=0,lastOffset=0,lastBytes=0;
    uint64_t deadline=0;
    Phase phase=Receiving;
    bool switching=false,trialPending=false;
    int targetSlot=-1;
    uint8_t lastDigest[32]={},manifest[52]={};
    ota_image image{};
};
struct Ap01Firmware::Stable {
    unsigned slot=0,revision=0;
    Ap01SlotImage image;
    String owner,request;
};
bool Ap01Firmware::saveStable(const Stable &value) {
    LocalJsonAllocator a;JsonDocument doc(&a);
    doc["device_mac"]=mac;doc["slot"]=value.slot;doc["revision"]=value.revision;
    doc["length"]=value.image.length;doc["crc"]=value.image.crc;doc["build"]=value.image.build;
    doc["owner"]=value.owner;doc["request_id"]=value.request;
    uint8_t bytes[525];size_t n=measureJson(doc);
    if(doc.overflowed()||!n||n>512||serializeJson(doc,(char *)bytes+12,513)!=n)return false;
    memcpy(bytes,"APS1",4);put32(bytes+4,n);put32(bytes+8,v2Crc32(bytes+12,n));
    File f=fs.open("/stable.pending","w");if(!f||f.write(bytes,n+12)!=n+12)return false;
    f.close();f=fs.open("/stable.pending","r");if(!matches(f,bytes,n+12))return false;
    f.close();fs.rename("/stable.pending","/stable.bin");f=fs.open("/stable.bin","r");
    return matches(f,bytes,n+12);
}
bool Ap01Firmware::loadStable() {
    File f=fs.open("/stable.bin","r");if(!f)return fs.inspect("/stable.bin")==0;
    uint8_t bytes[525];size_t n=f.size();
    if(n<=12||n>524||f.read(bytes,n)!=n||memcmp(bytes,"APS1",4)||le32(bytes+4)!=n-12||v2Crc32(bytes+12,n-12)!=le32(bytes+8))return false;
    LocalJsonAllocator a;JsonDocument doc(&a);
    if(deserializeJson(doc,bytes+12,n-12)||strcmp(doc["device_mac"]|"",mac)||!doc["slot"].is<unsigned>()||doc["slot"].as<unsigned>()>1||
       !doc["revision"].is<unsigned>()||!doc["revision"].as<unsigned>()||!doc["length"].is<uint32_t>()||
       doc["length"].as<uint32_t>()<MIN_ROM||doc["length"].as<uint32_t>()>MAX_ROM||!doc["crc"].is<uint32_t>()||
       !doc["owner"].is<const char *>()||!strlen(doc["owner"]|"")||strlen(doc["owner"]|"")>64||!id(doc["request_id"]|""))return false;
    const char *build=doc["build"]|"";if(strlen(build)!=32)return false;
    for(const char *p=build;*p;++p)if(!((*p>='0'&&*p<='9')||(*p>='a'&&*p<='f')))return false;
    for(const char *field:{"device_mac","owner","request_id","build"}) {
        JsonString value=doc[field].as<JsonString>();if(value.size()!=strlen(value.c_str()))return false;
    }
    stable.reset(new(std::nothrow) Stable);if(!stable)return false;
    stable->slot=doc["slot"];stable->revision=doc["revision"];stable->image.length=doc["length"];stable->image.crc=doc["crc"];
    memcpy(stable->image.build,build,33);stable->owner=doc["owner"].as<const char *>();stable->request=doc["request_id"].as<const char *>();
    return stable->owner.length()&&stable->request.length();
}
Ap01HttpReply Ap01Firmware::slots(const Ap01HttpRequest &request,const char *principal,uint64_t) {
    if(!port.runningSlot||!port.inspectSlot||!port.selectSlot)return reject(501,"slot_port_unavailable");
    int running=port.runningSlot(port.context);if(running<0||running>1)return reject(503,"running_slot_unknown","slot_inspect");
    bool marking=!strcmp(request.path,"/api/firmware/stable");
    if(marking) {
        if(stableCorrupt)return reject(503,"stable_storage_unavailable","firmware_storage");
        if(strcmp(request.method,"POST"))return reject(405,"method");
        if(safety==Armed||safety==RetainedStable||safety==SafetyUnknown)return reject(409,"boot_safety_pending","boot_safety");
        if(blocksBusiness()||port.busy(port.context))return reject(409,"firmware_busy");
        LocalJsonAllocator a;JsonDocument input(&a);
        if(!request.length||request.length>1024||memchr(request.body,0,request.length)||deserializeJson(input,request.body,request.length)||
           strcmp(input["device_mac"]|"",mac)||!id(input["request_id"]|"")||!input["slot"].is<unsigned>()||
           input["slot"].as<unsigned>()!=unsigned(running)||input["confirm"]!=true||!input["expected_stable_revision"].is<unsigned>())return reject(400,"stable_envelope");
        const char *requestId=input["request_id"];unsigned revision=input["expected_stable_revision"];
        if(input["device_mac"].as<JsonString>().size()!=12||input["request_id"].as<JsonString>().size()!=strlen(requestId))return reject(400,"stable_envelope");
        if(stable&&stable->owner==principal&&stable->request==requestId) {
            if(stable->slot!=unsigned(running)||revision!=stable->revision-1)return reject(409,"request_conflict");
        } else {
            if(revision!=(stable?stable->revision:0)||revision==UINT32_MAX)return reject(409,"stable_revision_conflict");
            std::unique_ptr<Stable> value(new(std::nothrow) Stable);if(!value)return reject(503,"slot_oom");
            value->slot=running;value->revision=revision+1;value->owner=principal;value->request=requestId;
            if(value->owner!=principal||value->request!=requestId)return reject(503,"slot_oom");
            if(!port.inspectSlot(port.context,running,value->image))return reject(409,value->image.error,"slot_inspect");
            if(!saveStable(*value)){stableCorrupt=true;return reject(503,"stable_storage_unknown","firmware_storage",true);}
            stable=std::move(value);
        }
    } else if(strcmp(request.method,"GET"))return reject(405,"method");
    LocalJsonAllocator a;JsonDocument out(&a);out["result"]=marking?"applied":"ok";out["device_mac"]=mac;
    out["running_slot"]=running;out["stable_revision"]=stable?stable->revision:0;
    if(stableCorrupt)out["stable_error"]="stable_storage_unavailable";
    out["boot_safety"]=safetyName();out["health_window_ms"]=9000;out["trial_timeout_ms"]=30000;
    if(stable){out["stable_slot"]=stable->slot;out["stable_image_crc"]=stable->image.crc;out["stable_image_length"]=stable->image.length;out["stable_build"]=stable->image.build;out["stability"]="explicitly_confirmed";}
    else {out["stable_slot"]=nullptr;out["stability"]="unconfirmed";}
    out["service"]="ap01-maintenance";return jsonReply(out,200);
}
Ap01Firmware::Ap01Firmware(LittleFSClass &filesystem,const Ap01FirmwarePort &callbacks,const char *identity)
    :fs(filesystem),port(callbacks) {memcpy(mac,identity,13);}
Ap01Firmware::~Ap01Firmware() {close();}
void Ap01Firmware::afterResponse(bool sent) {bool requested=rebootPending;rebootPending=false;if(requested&&sent&&port.reboot)port.reboot(port.context);}
bool Ap01Firmware::available() const {return port.busy&&port.open&&port.write&&port.verify&&port.commit&&port.close&&port.build;}
void Ap01Firmware::close() {if(opened){port.close(port.context);opened=false;}}
bool Ap01Firmware::blocksBusiness() const {
    return corrupt||(record&&(record->phase==Receiving||record->phase==Committing||record->phase==Unknown||(record->phase==Applied&&!booted)));
}
const char *Ap01Firmware::safetyName() const {
    switch(safety){case StableRunning:return "stable_running";case Armed:return "armed";case Healthy:return "healthy_locked";
        case RetainedStable:return "retained_stable";case SafetyUnknown:return "unknown";default:return "unconfigured";}
}
void Ap01Firmware::armBoot(uint64_t now) {
    if(!port.bootHealthy||!port.runningSlot||!port.inspectSlot||!port.selectSlot||!stable)return;
    int running=port.runningSlot(port.context);
    if(running<0||running>1){safety=SafetyUnknown;return;}
    if(unsigned(running)==stable->slot){safety=StableRunning;return;}
    Ap01SlotImage image;
    if(!port.inspectSlot(port.context,stable->slot,image)||image.length!=stable->image.length||image.crc!=stable->image.crc||strcmp(image.build,stable->image.build)) {
        safety=SafetyUnknown;return;
    }
    if(record&&record->targetSlot==running&&record->targetBuild==port.build&&
       (record->phase==Applied||record->phase==Committing||record->phase==Unknown||record->trialPending)) {
        // Old stable ROMs understand Failed, and remain usable after fallback.
        // Keep this durable trial receipt until a new explicit transaction.
        record->trialPending=true;record->phase=Failed;record->error="boot_trial_pending";booted=true;
        if(!save()){corrupt=true;safety=SafetyUnknown;return;}
    }
    safety=port.selectSlot(port.context,stable->slot)==1?Armed:SafetyUnknown;
    armedAt=now;
}
void Ap01Firmware::tick(uint64_t now) {
    if(record&&record->phase==Receiving&&now>=record->deadline)failed("receive_timeout");
    if(safety!=Armed)return;
    if(now>=armedAt&&now-armedAt>=30000){safety=RetainedStable;if(record&&record->trialPending)record->error="boot_health_timeout";return;}
    if(port.bootHealthy(port.context)) {
        if(!healthyObserved){healthySince=now;healthyObserved=true;}
        if(now>=healthySince&&now-healthySince>=9000) {
            int running=port.runningSlot(port.context);
            safety=running>=0&&running<=1&&port.selectSlot(port.context,unsigned(running))==1?Healthy:SafetyUnknown;
            if(safety==Healthy&&record&&record->trialPending){record->phase=Applied;record->error="";booted=true;}
            return;
        }
    } else healthyObserved=false;
}
bool Ap01Firmware::save() {
    if(!record)return false;
    LocalJsonAllocator a;JsonDocument doc(&a);
    doc["device_mac"]=mac;doc["owner"]=record->owner;doc["request_id"]=record->request;
    doc["revision"]=record->revision;doc["length"]=record->length;doc["recovery_crc"]=record->recoveryCrc;
    doc["received"]=record->position;doc["phase"]=unsigned(record->phase);
    doc["target_build"]=record->targetBuild;doc["error"]=record->error;
    doc["operation"]=record->switching?"switch":"upload";doc["target_slot"]=record->targetSlot;
    doc["boot_trial_pending"]=record->trialPending;
    size_t n=measureJson(doc);if(doc.overflowed()||!n||n>1024)return false;
    std::unique_ptr<uint8_t[]> bytes(new(std::nothrow) uint8_t[n+13]);if(!bytes)return false;
    memcpy(bytes.get(),"APF1",4);put32(bytes.get()+4,n);
    if(serializeJson(doc,(char *)bytes.get()+12,n+1)!=n)return false;
    put32(bytes.get()+8,v2Crc32(bytes.get()+12,n));
    File file=fs.open("/firmware.pending","w");if(!file||file.write(bytes.get(),n+12)!=n+12)return false;
    file.close();file=fs.open("/firmware.pending","r");if(!matches(file,bytes.get(),n+12))return false;
    file.close();fs.rename("/firmware.pending","/firmware.bin");file=fs.open("/firmware.bin","r");
    return matches(file,bytes.get(),n+12);
}
bool Ap01Firmware::load() {
    File file=fs.open("/firmware.bin","r");if(!file)return fs.inspect("/firmware.bin")==0;
    size_t total=file.size();uint8_t header[12];
    if(total<=12||total>1036||file.read(header,12)!=12||memcmp(header,"APF1",4)||le32(header+4)!=total-12)return false;
    size_t n=total-12;std::unique_ptr<char[]> bytes(new(std::nothrow) char[n+1]);
    if(!bytes||file.read((uint8_t *)bytes.get(),n)!=n||v2Crc32((const uint8_t *)bytes.get(),n)!=le32(header+8))return false;
    bytes[n]=0;LocalJsonAllocator a;JsonDocument doc(&a);
    if(deserializeJson(doc,bytes.get(),n)||strcmp(doc["device_mac"]|"",mac)||!id(doc["request_id"]|"")||
       !doc["revision"].is<uint32_t>()||!doc["length"].is<uint32_t>()||!doc["recovery_crc"].is<uint32_t>()||!doc["received"].is<uint32_t>()||
       !doc["phase"].is<unsigned>()||!doc["owner"].is<const char *>()||!doc["target_build"].is<const char *>()||!doc["error"].is<const char *>())return false;
    const char *owner=doc["owner"],*build=doc["target_build"],*error=doc["error"];
    for(const char *field:{"owner","request_id","target_build","error"}) {
        JsonString value=doc[field].as<JsonString>();if(value.size()!=strlen(value.c_str()))return false;
    }
    unsigned phase=doc["phase"];uint32_t length=doc["length"],position=doc["received"];
    if(!*owner||strlen(owner)>64||(strlen(build)!=0&&strlen(build)!=32)||strlen(error)>40||phase<Receiving||phase>Unknown||
       !doc["revision"].as<uint32_t>()||length<MIN_ROM||length>MAX_ROM||position>length||
       ((phase==Committing||phase==Applied||phase==Unknown)&&position!=length))return false;
    for(const char *p=build;*p;++p)if(!((*p>='0'&&*p<='9')||(*p>='a'&&*p<='f')))return false;
    record.reset(new(std::nothrow) Record);if(!record)return false;
    record->owner=owner;record->request=doc["request_id"].as<const char *>();record->targetBuild=build;record->error=error;
    if(record->owner.length()!=strlen(owner)||record->targetBuild.length()!=strlen(build)||!record->request.length())return false;
    record->revision=doc["revision"];record->length=length;record->recoveryCrc=doc["recovery_crc"];record->position=position;record->phase=Phase(phase);
    const char *operation=doc["operation"]|"upload";
    if((!doc["operation"].isNull()&&(!doc["operation"].is<const char *>()||doc["operation"].as<JsonString>().size()!=strlen(operation)))||
       (!doc["target_slot"].isNull()&&!doc["target_slot"].is<int>()))return false;
    if(strcmp(operation,"upload")&&strcmp(operation,"switch"))return false;
    record->switching=!strcmp(operation,"switch");record->targetSlot=doc["target_slot"]|-1;
    if(!doc["boot_trial_pending"].isNull()&&!doc["boot_trial_pending"].is<bool>())return false;
    record->trialPending=doc["boot_trial_pending"]|false;
    if(record->trialPending&&(record->targetSlot<0||position!=length||strlen(build)!=32||phase!=Failed))return false;
    if(record->targetSlot < -1||record->targetSlot>1||(record->switching&&record->targetSlot<0))return false;
    return true;
}
void Ap01Firmware::failed(const char *reason) {close();record->phase=Failed;record->error=reason;save();}
bool Ap01Firmware::begin(uint64_t now) {
    __attribute__((used,section(".ap01_firmware_budget"))) static const uint32_t budget[]={
        sizeof(Ap01Firmware),sizeof(Record),32768,6145,1036,1024};
    if(!available())return true;
    if(!load()){corrupt=true;record.reset();return false;}
    if(!loadStable()){stableCorrupt=true;stable.reset();}
    if(record&&record->phase==Receiving)failed("interrupted_before_commit");
    else if(record&&!record->trialPending&&(record->phase==Committing||record->phase==Unknown||record->phase==Applied)) {
        if(record->targetBuild.length()==32&&record->targetBuild==port.build&&
           (record->targetSlot<0||(port.runningSlot&&port.runningSlot(port.context)==record->targetSlot))) {
            record->phase=Applied;record->error="";booted=true;
            if(!save()){record->phase=Unknown;record->error="receipt_storage";booted=false;}
        } else if(record->phase==Applied&&record->targetSlot>=0&&port.runningSlot&&
                  port.runningSlot(port.context)>=0&&port.runningSlot(port.context)!=record->targetSlot) {
            // A confirmed historical selection must not fence a later boot
            // reached through the independent rescue ROM's update channel.
            record->phase=Failed;record->error="booted_other_slot";save();
        } else if(record->phase==Committing) {record->phase=Unknown;record->error="commit_unknown";save();}
    }
    armBoot(now);return true;
}
Ap01HttpReply Ap01Firmware::handle(const Ap01HttpRequest &request,const char *principal,uint64_t now) {
    if(!available())return reject(501,"firmware_port_unavailable");
    if(corrupt)return reject(503,"firmware_state_unavailable","firmware_storage");
    tick(now);
    if(!strcmp(request.path,"/api/firmware/slots")||!strcmp(request.path,"/api/firmware/stable"))return slots(request,principal,now);
    bool starting=!strcmp(request.path,"/api/firmware/begin");
    bool switching=!strcmp(request.path,"/api/firmware/switch");
    bool chunk=!strcmp(request.path,"/api/firmware/chunk");
    bool committing=!strcmp(request.path,"/api/firmware/commit");
    bool rebooting=!strcmp(request.path,"/api/firmware/reboot");
    bool status=!strcmp(request.path,"/api/firmware/status");
    if((starting||committing||chunk)&&(safety==Armed||safety==RetainedStable||safety==SafetyUnknown))return reject(409,"boot_safety_pending","boot_safety",safety==SafetyUnknown);
    if(switching&&safety==SafetyUnknown)return reject(503,"boot_safety_unknown","boot_safety",true);
    const char *queryId=!strncmp(request.path,"/api/firmware/",14)?request.path+14:"";
    if((starting||switching||committing||rebooting)&&strcmp(request.method,"POST"))return reject(405,"method");
    LocalJsonAllocator a;JsonDocument input(&a);
    if(starting||switching||committing||rebooting) {
        if(!request.length||request.length>1024||memchr(request.body,0,request.length)||deserializeJson(input,request.body,request.length)||
           strcmp(input["device_mac"]|"",mac)||!id(input["request_id"]|""))return reject(400,"identity_or_body");
        queryId=input["request_id"];
        if(input["device_mac"].as<JsonString>().size()!=12||input["request_id"].as<JsonString>().size()!=strlen(queryId))return reject(400,"identity_or_body");
    } else if(chunk)queryId=request.requestId;
    if(switching) {
        if(!port.runningSlot||!port.inspectSlot||!port.selectSlot)return reject(501,"slot_port_unavailable");
        if(!input["target_slot"].is<unsigned>()||input["target_slot"].as<unsigned>()>1||!input["expected_revision"].is<unsigned>()||
           (!input["expected_image_crc"].isNull()&&!input["expected_image_crc"].is<uint32_t>()))return reject(400,"switch_envelope");
        unsigned target=input["target_slot"],revision=input["expected_revision"];
        bool same=record&&record->owner==principal&&record->request==queryId;
        if(same) {
            if(!record->switching||record->targetSlot!=int(target)||revision!=record->revision-1)return reject(409,"request_conflict");
        } else {
            if(blocksBusiness()||port.busy(port.context))return reject(409,"firmware_busy");
            if(revision!=(record?record->revision:0)||revision==UINT32_MAX)return reject(409,"revision_conflict");
            int running=port.runningSlot(port.context);if(running<0||running>1)return reject(503,"running_slot_unknown");
            if(target==unsigned(running))return reject(409,"already_running");
            Ap01SlotImage image;
            if(!port.inspectSlot(port.context,target,image))return reject(409,image.error,"slot_inspect");
            if(!input["expected_image_crc"].isNull()&&input["expected_image_crc"]!=image.crc)return reject(409,"image_crc_conflict","slot_inspect");
            std::unique_ptr<Record> next(new(std::nothrow) Record);if(!next)return reject(503,"slot_oom");
            next->owner=principal;next->request=queryId;next->targetBuild=image.build;
            if(next->owner!=principal||next->request!=queryId||next->targetBuild!=image.build)return reject(503,"slot_oom");
            next->switching=true;next->targetSlot=target;next->length=next->position=image.length;next->recoveryCrc=image.crc;next->revision=revision+1;next->phase=Committing;
            record=std::move(next);booted=commitConfirmed=false;
            if(!save()){failed("receipt_prepare");return reject(503,"receipt_prepare","firmware_storage");}
            commitConfirmed=port.selectSlot(port.context,target)==1;
            if(commitConfirmed&&(safety==Armed||safety==RetainedStable))safety=RetainedStable;
            record->phase=commitConfirmed?Applied:Unknown;record->error=commitConfirmed?"":"commit_unknown";
            if(!save()){record->phase=Unknown;record->error="receipt_commit";}
        }
    }
    if(starting) {
        uint32_t checksum=0;
        if(!input["length"].is<uint32_t>()||!input["expected_revision"].is<uint32_t>()||!hex32(input["recovery_crc"]|"",checksum))return reject(400,"firmware_envelope");
        uint32_t length=input["length"],revision=input["expected_revision"];
        if(length<MIN_ROM||length>MAX_ROM||now>UINT64_MAX-300000)return reject(413,"firmware_bounds");
        bool same=record&&record->owner==principal&&record->request==queryId;
        if(same) {
            if(record->switching||record->length!=length||record->recoveryCrc!=checksum||revision!=record->revision-1)return reject(409,"request_conflict");
        } else {
            if(blocksBusiness())return reject(409,"firmware_busy");
            uint32_t current=record?record->revision:0;
            if(revision!=current||current==UINT32_MAX)return reject(409,"revision_conflict");
            if(port.busy(port.context))return reject(409,"traditional_ota_busy");
            int target=-1;
            if(port.runningSlot) {
                int running=port.runningSlot(port.context);if(running<0||running>1)return reject(503,"running_slot_unknown");target=running^1;
                if(stable&&stable->slot==unsigned(target))return reject(409,"stable_slot_protected");
            }
            std::unique_ptr<Record> next(new(std::nothrow) Record);if(!next)return reject(503,"firmware_oom");
            next->owner=principal;next->request=queryId;
            if(next->owner!=principal||next->request!=queryId)return reject(503,"firmware_oom");
            next->revision=current+1;next->length=length;next->recoveryCrc=checksum;next->deadline=now+300000;
            next->targetSlot=target;
            ota_image_begin(&next->image,length);record=std::move(next);booted=commitConfirmed=false;
            if(!save()){failed("receipt_prepare");return reject(503,"receipt_prepare","firmware_storage");}
            opened=port.open(port.context,length);
            if(!opened){failed("flash_open");return reject(500,"flash_open","firmware_open");}
        }
    } else if(!status&&!switching) {
        if(!record||record->request!=queryId)return reject(404,"request_retired");
        if(record->owner!=principal)return reject(403,"principal");
    }
    if(chunk) {
        if(strcmp(request.method,"POST")||strcmp(request.contentType,"application/octet-stream")||!request.length||request.length>32768)return reject(400,"chunk_shape");
        if(record->phase!=Receiving||!opened)return reject(409,"receive_closed");
        uint64_t offset=0;if(!*request.offset||strlen(request.offset)>10)return reject(400,"offset");
        for(const char *p=request.offset;*p;++p){if(*p<'0'||*p>'9')return reject(400,"offset");offset=offset*10+*p-'0';}
        uint8_t digest[32];syncSha256(request.body,request.length,digest);
        if(offset==record->lastOffset&&record->lastBytes&&request.length==record->lastBytes) {
            if(memcmp(record->lastDigest,digest,32))return reject(409,"chunk_conflict");
        } else {
            if(offset!=record->position||request.length>record->length-record->position)return reject(409,"chunk_offset");
            uint32_t lo=record->position>MANIFEST?record->position:MANIFEST;
            uint32_t hi=record->position+request.length<MANIFEST+52?record->position+request.length:MANIFEST+52;
            if(lo<hi)memcpy(record->manifest+lo-MANIFEST,request.body+lo-record->position,hi-lo);
            if(ota_image_chunk(&record->image,request.body,request.length)||!port.write(port.context,record->position,request.body,request.length)) {
                failed("flash_write");return reject(500,"flash_write","firmware_write");
            }
            record->lastOffset=record->position;record->lastBytes=request.length;memcpy(record->lastDigest,digest,32);record->position+=request.length;
        }
    } else if(committing&&record->phase==Receiving) {
        if(!opened||record->position!=record->length)return reject(409,"incomplete");
        if(!ota_image_valid(&record->image)||record->image.recovery_crc!=record->recoveryCrc) {
            failed("image_integrity");return reject(400,"image_integrity","firmware_verify");
        }
        // Optional receipt metadata, never an image-admission requirement.
        if(record->length>=MANIFEST+52&&!memcmp(record->manifest,"AP01APP1",8)&&le32(record->manifest+8)==1&&
           le32(record->manifest+12)==record->length&&le32(record->manifest+16)==MANIFEST) {
            char build[33];const char *hex="0123456789abcdef";
            for(unsigned i=0;i<16;++i){build[2*i]=hex[record->manifest[36+i]>>4];build[2*i+1]=hex[record->manifest[36+i]&15];}build[32]=0;record->targetBuild=build;
            if(record->targetBuild.length()!=32){failed("firmware_oom");return reject(503,"firmware_oom");}
        }
        record->phase=Committing;
        if(!save()){failed("receipt_precommit");return reject(503,"receipt_precommit","firmware_storage");}
        if(!port.verify(port.context,record->length)){failed("flash_verify");return reject(500,"flash_verify","firmware_verify");}
        commitConfirmed=port.commit(port.context)==1;close();
        record->phase=commitConfirmed?Applied:Unknown;record->error=commitConfirmed?"":"commit_unknown";
        if(!save()){record->phase=Unknown;record->error="receipt_commit";}
    } else if(rebooting) {
        if(record->phase!=Applied)return reject(409,"not_committed");
        if(!port.reboot)return reject(501,"reboot_unavailable");
        if(!booted)rebootPending=true;
    } else if(!starting&&!switching&&!chunk&&!committing&&strcmp(request.method,"GET")) {
        if(!strcmp(request.method,"DELETE")&&record&&record->phase==Receiving)failed("cancelled");
        else return reject(405,"method");
    }
    if(record&&record->phase==Unknown&&commitConfirmed) {record->phase=Applied;record->error="";if(!save()){record->phase=Unknown;record->error="receipt_commit";}}
    LocalJsonAllocator allocator;JsonDocument out(&allocator);out["device_mac"]=mac;out["protocol"]=1;
    out["boot_safety"]=safetyName();out["boot_health_confirmed"]=safety==Healthy||safety==StableRunning;
    out["revision"]=record?record->revision:0;out["max_chunk_bytes"]=32768;out["min_rom_bytes"]=MIN_ROM;out["max_rom_bytes"]=MAX_ROM;
    if(!record){out["result"]="idle";out["phase"]="idle";return jsonReply(out,200);}
    if(record->owner!=principal) {
        if(!status)return reject(403,"principal");
        out["result"]=blocksBusiness()?"occupied":"idle";out["phase"]=blocksBusiness()?"occupied":"idle";
        return jsonReply(out,200);
    }
    out["request_id"]=record->request;out["received"]=record->position;out["length"]=record->length;
    out["result"]=record->phase==Applied?"applied":record->phase==Failed?"rejected":record->phase==Receiving?"receiving":"unknown";
    out["phase"]=record->phase==Applied?"committed":record->phase==Failed?"failed":record->phase==Receiving?"receiving":"unknown";
    if(record->trialPending&&safety==Armed){out["result"]="trial";out["phase"]="boot_trial";}
    out["target_build"]=record->targetBuild;out["boot_confirmed"]=booted;out["reboot_required"]=record->phase==Applied&&!booted;
    out["operation"]=record->switching?"switch":"upload";out["target_slot"]=record->targetSlot;
    if(record->error.length()){out["error"]=record->error;out["stage"]="firmware_commit";out["action"]=record->phase==Unknown?"query_original_request":"read_maintenance_status";}
    return jsonReply(out,record->phase==Unknown?503:200);
}
