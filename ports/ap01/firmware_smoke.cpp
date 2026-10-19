// Host ASan/UBSan only. Uses a complete real candidate as the stream fixture;
// flash callbacks are a private byte array, never a physical device.
#include "http.h"
#include "local_json_allocator.h"
#include "v2_sync.h"
#include "v2_command_envelope.h"
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <assert.h>
#include <vector>
#include <string>
#include <fstream>
#include <iterator>

struct Disk {bool locked=false,loseRename=false,failSave=false;unsigned random=0;};
void put32(uint8_t *p,uint32_t n){for(unsigned i=0;i<4;++i)p[i]=uint8_t(n>>(8*i));}
int prepare(void *,const char *p){struct stat s;return stat(p,&s)||!S_ISDIR(s.st_mode)?-1:0;}
int openFile(void *,const char *p,unsigned m){return open(p,m?O_WRONLY|O_CREAT|(m==1?O_TRUNC:O_APPEND):O_RDONLY,0600);}
int closeFile(void *,int f){return close(f);}
int readFile(void *,int f,void *p,size_t n){return read(f,p,n);}
int writeFile(void *p,int f,const void *b,size_t n){if(((Disk *)p)->failSave)return -1;int r=write(f,b,n);return fsync(f)?-1:r;}
int32_t seekFile(void *,int f,int32_t n,unsigned w){return lseek(f,n,w);}
int32_t sizeFile(void *,int f){struct stat s;return fstat(f,&s)?-1:s.st_size;}
int exists(void *,const char *p){struct stat s;return stat(p,&s)?(errno==ENOENT?0:-1):1;}
int mkdirFile(void *,const char *p){return mkdir(p,0700);}
int removeFile(void *,const char *p){return unlink(p);}
int renameFile(void *p,const char *a,const char *b){int r=rename(a,b);if(!r&&((Disk *)p)->loseRename){((Disk *)p)->loseRename=false;return -1;}return r;}
int capacity(void *,const char *,uint64_t *t,uint64_t *u){*t=0x140000;*u=8192;return 0;}
bool randomBytes(void *p,uint8_t *out,size_t n){for(size_t i=0;i<n;++i)out[i]=++((Disk *)p)->random;return true;}
bool lock(void *p){auto &d=*(Disk *)p;assert(!d.locked);d.locked=true;return true;}
void unlock(void *p){auto &d=*(Disk *)p;assert(d.locked);d.locked=false;}
bool present(void *,uint8_t *p,uint32_t){delete[] p;return true;}
struct Flash {
    std::vector<uint8_t> bytes;
    std::string expected;
    char build[33]="00000000000000000000000000000000";
    bool busy=false,failVerify=false,unknown=false;
    unsigned opens=0,writes=0,verifies=0,commits=0,closes=0,reboots=0;
    static bool isBusy(void *p){return ((Flash *)p)->busy;}
    static bool begin(void *p,uint32_t n){auto &f=*(Flash *)p;++f.opens;f.bytes.assign(n,255);return true;}
    static bool write(void *p,uint32_t at,const uint8_t *data,size_t n){auto &f=*(Flash *)p;assert(at+n<=f.bytes.size());memcpy(f.bytes.data()+at,data,n);if(!at)f.bytes[0]=~f.bytes[0];++f.writes;return true;}
    static bool verify(void *p,uint32_t n){auto &f=*(Flash *)p;++f.verifies;if(f.failVerify)return false;assert(n==f.expected.size());assert(f.bytes[0]==uint8_t(~uint8_t(f.expected[0])));f.bytes[0]=~f.bytes[0];assert(!memcmp(f.bytes.data(),f.expected.data(),n));return true;}
    static int commit(void *p){auto &f=*(Flash *)p;++f.commits;return f.unknown?-1:1;}
    static void close(void *p){++((Flash *)p)->closes;}
    static void reboot(void *p){++((Flash *)p)->reboots;}
    Ap01FirmwarePort port(){return {this,isBusy,begin,write,verify,commit,close,build,reboot};}
};
std::string readAll(const char *p){std::ifstream f(p,std::ios::binary);return {std::istreambuf_iterator<char>(f),{}};}
std::string text(const String &s){return {s.c_str(),s.length()};}
Ap01HttpRequest request(const char *method,const char *path,const std::string &body,const char *token="0123456789abcdef0123456789abcdef"){
    Ap01HttpRequest r{};r.method=method;r.path=path;r.token=token;r.deviceToken=token;r.contentType="application/json";r.body=(const uint8_t *)body.data();r.length=body.size();return r;
}
std::string beginBody(const std::string &image,const char *id="fw-test",unsigned revision=0){
    uint32_t crc=0;for(unsigned i=0;i<4;++i)crc|=uint32_t(uint8_t(image[image.size()-4+i]))<<(8*i);char hex[9];snprintf(hex,9,"%08x",crc);
    return "{\"device_mac\":\"AABBCCDDEEFF\",\"request_id\":\""+std::string(id)+"\",\"expected_revision\":"+std::to_string(revision)+",\"length\":"+std::to_string(image.size())+",\"recovery_crc\":\""+hex+"\"}";
}
const std::string commitBody="{\"device_mac\":\"AABBCCDDEEFF\",\"request_id\":\"fw-test\"}";
void upload(Ap01Firmware &fw,const std::string &image,size_t first=0){
    for(size_t at=first;at<image.size();at+=32768){auto offset=std::to_string(at);auto body=image.substr(at,32768);auto r=request("POST","/api/firmware/chunk",body);r.contentType="application/octet-stream";r.requestId="fw-test";r.offset=offset.c_str();assert(fw.handle(r,"bridge-test",3).status==200);}
}
struct Stream {std::string input,output;size_t at=0;};
int receive(void *p,uint8_t *out,size_t n,uint64_t){auto &s=*(Stream *)p;n=std::min(n,std::min<size_t>(2048,s.input.size()-s.at));memcpy(out,s.input.data()+s.at,n);s.at+=n;return n;}
int send(void *p,const uint8_t *out,size_t n,uint64_t){((Stream *)p)->output.append((const char *)out,n);return n;}

int main(int argc,char **argv){
    assert(argc==3);std::string root=argv[1],image=readAll(argv[2]);assert(image.size()>=6769704);
    Disk disk;Ap01VfsOps ops{&disk,prepare,openFile,closeFile,readFile,writeFile,seekFile,sizeFile,exists,mkdirFile,removeFile,renameFile,capacity};
    // The preceding application smoke test created this controlled binding.
    Flash flash;flash.expected=image;
    {
        LittleFSClass fs(ops,root.c_str());Ap01Business business(fs,{nullptr,present},9);assert(business.begin(0));
        Ap01Application app(fs,business,{&disk,randomBytes,lock,unlock,flash.port()},"AABBCCDDEEFF",9);assert(app.begin());app.businessOnline(true);
        auto start=beginBody(image);
        assert(app.maintenanceHttp(request("POST","/api/firmware/begin",start,""),1).status==401);assert(!flash.opens);
        auto bad=start;bad.replace(bad.find("AABBCCDDEEFF"),12,"112233445566");assert(app.maintenanceHttp(request("POST","/api/firmware/begin",bad),1).status==400);assert(!flash.opens);
        flash.busy=true;assert(app.maintenanceHttp(request("POST","/api/firmware/begin",start),1).status==409);flash.busy=false;
        disk.loseRename=true;auto begin=app.maintenanceHttp(request("POST","/api/firmware/begin",start),1);assert(begin.status==200&&flash.opens==1);
        assert(app.maintenanceHttp(request("POST","/api/firmware/begin",start),2).status==200&&flash.opens==1);
        auto health=app.maintenanceHttp(request("GET","/health",{}),2);assert(text(health.bytes).find("\"business_online\":false")!=std::string::npos);
        auto offline=app.businessHttp(request("POST","/api/status",{}),2);assert(offline.status==503&&text(offline.bytes).find("rejected")!=std::string::npos);
        auto first=image.substr(0,32768);
        Stream stream;stream.input="POST /api/firmware/chunk?request_id=fw-test&offset=0 HTTP/1.1\r\nAuthorization: Bearer 0123456789abcdef0123456789abcdef\r\nContent-Type: application/octet-stream\r\nContent-Length: 32768\r\n\r\n"+first;
        assert(ap01ServeHttp(app,true,{&stream,receive,send},2,5000));assert(stream.output.rfind("HTTP/1.1 200 ",0)==0&&flash.writes==1);
        auto chunk=request("POST","/api/firmware/chunk",first);chunk.contentType="application/octet-stream";chunk.requestId="fw-test";chunk.offset="0";
        assert(app.maintenanceHttp(chunk,3).status==200&&flash.writes==1);
        first[100]^=1;chunk.body=(const uint8_t *)first.data();assert(app.maintenanceHttp(chunk,3).status==409&&flash.writes==1);
        assert(app.maintenanceHttp(request("POST","/api/firmware/commit",commitBody),3).status==409&&!flash.verifies);
        for(size_t at=32768;at<image.size();at+=32768){auto offset=std::to_string(at),body=image.substr(at,32768);auto r=request("POST","/api/firmware/chunk",body);r.contentType="application/octet-stream";r.requestId="fw-test";r.offset=offset.c_str();assert(app.maintenanceHttp(r,4).status==200);}
        auto result=app.maintenanceHttp(request("POST","/api/firmware/commit",commitBody),5);assert(result.status==200&&text(result.bytes).find("\"result\":\"applied\"")!=std::string::npos);
        assert(flash.commits==1&&flash.verifies==1&&flash.closes==1);
        assert(app.maintenanceHttp(request("POST","/api/firmware/commit",commitBody),6).status==200&&flash.commits==1);
        assert(app.maintenanceHttp(request("POST","/api/firmware/reboot",commitBody),7).status==200);app.maintenanceResponse(false);assert(!flash.reboots);
        assert(app.maintenanceHttp(request("POST","/api/firmware/reboot",commitBody),8).status==200);assert(!flash.reboots);app.maintenanceResponse(true);assert(flash.reboots==1);
    }
    const char *digits="0123456789abcdef";for(unsigned i=0;i<16;++i){uint8_t b=image[0x675000+36+i];flash.build[2*i]=digits[b>>4];flash.build[2*i+1]=digits[b&15];}
    {
        LittleFSClass fs(ops,root.c_str());Ap01Business business(fs,{nullptr,present},10);assert(business.begin(0));
        Ap01Application app(fs,business,{&disk,randomBytes,lock,unlock,flash.port()},"AABBCCDDEEFF",10);assert(app.begin());app.businessOnline(true);
        auto query=app.maintenanceHttp(request("GET","/api/firmware/fw-test",{}),10);assert(query.status==200&&text(query.bytes).find("\"boot_confirmed\":true")!=std::string::npos);
        assert(app.maintenanceHttp(request("POST","/api/firmware/reboot",commitBody),11).status==200);app.maintenanceResponse(true);assert(flash.reboots==1);
        auto health=app.maintenanceHttp(request("GET","/health",{}),12);assert(text(health.bytes).find("\"business_online\":true")!=std::string::npos);
    }
    for(unsigned scenario=0;scenario<4;++scenario){
        std::string folder=root+"/f"+std::to_string(scenario);LittleFSClass fs(ops,folder.c_str());Flash fake;fake.expected=image;
        {
            Ap01Firmware fw(fs,fake.port(),"AABBCCDDEEFF");assert(fw.begin());auto body=beginBody(image);
            if(scenario==3){disk.failSave=true;assert(fw.handle(request("POST","/api/firmware/begin",body),"bridge-test",1).status==503);disk.failSave=false;assert(!fake.opens);continue;}
            assert(fw.handle(request("POST","/api/firmware/begin",body),"bridge-test",1).status==200);
            if(scenario==0){auto first=image.substr(0,1024);auto r=request("POST","/api/firmware/chunk",first);r.contentType="application/octet-stream";r.requestId="fw-test";r.offset="0";assert(fw.handle(r,"bridge-test",2).status==200);}
            else {
                upload(fw,image);fake.failVerify=scenario==1;fake.unknown=scenario==2;
                auto result=fw.handle(request("POST","/api/firmware/commit",commitBody),"bridge-test",5);
                assert(result.status==(scenario==1?500:503));assert(fake.commits==(scenario==2?1u:0u));
                if(scenario==2){assert(fw.blocksBusiness());assert(fw.handle(request("POST","/api/firmware/commit",commitBody),"bridge-test",6).status==503&&fake.commits==1);auto next=beginBody(image,"next",1);assert(fw.handle(request("POST","/api/firmware/begin",next),"bridge-test",7).status==409);}
            }
        }
        Ap01Firmware restored(fs,fake.port(),"AABBCCDDEEFF");assert(restored.begin());
        auto reply=restored.handle(request("GET","/api/firmware/fw-test",{}),"bridge-test",20);
        if(scenario==0)assert(reply.status==200&&text(reply.bytes).find("interrupted_before_commit")!=std::string::npos&&!restored.blocksBusiness());
        if(scenario==2)assert(reply.status==503&&restored.blocksBusiness());
    }
    {
        std::string folder=root+"/f4";LittleFSClass fs(ops,folder.c_str());Flash fake;
        {File f=fs.open("/firmware.bin","w");const uint8_t bad[12]={'A','P','F','2',255,255,255,255,0,0,0,0};assert(f.write(bad,12)==12);}
        Ap01Firmware fw(fs,fake.port(),"AABBCCDDEEFF");assert(!fw.begin()&&fw.blocksBusiness());assert(fw.handle(request("GET","/api/firmware/status",{}),"bridge-test",1).status==503);assert(!fake.opens);
    }
    {
        std::string folder=root+"/f5";LittleFSClass fs(ops,folder.c_str());Flash fake;
        Ap01Firmware fw(fs,fake.port(),"AABBCCDDEEFF");assert(fw.begin());
        auto body=beginBody(image);assert(fw.handle(request("POST","/api/firmware/begin",body),"bridge-test",1).status==200);
        fw.tick(300001);assert(!fw.blocksBusiness()&&fake.closes==1);
        auto result=fw.handle(request("GET","/api/firmware/fw-test",{}),"bridge-test",300002);
        assert(result.status==200&&text(result.bytes).find("receive_timeout")!=std::string::npos);
    }
    {
        std::string folder=root+"/f6";LittleFSClass fs(ops,folder.c_str());Flash fake;
        auto binding=readAll((root+"/bindings.bin").c_str());std::ofstream(folder+"/bindings.bin",std::ios::binary).write(binding.data(),binding.size());
        std::ofstream(folder+"/bundle/state.bin",std::ios::binary)<<"BAD";
        Ap01Business business(fs,{nullptr,present},11);assert(!business.begin(0));
        Ap01Application app(fs,business,{&disk,randomBytes,lock,unlock,fake.port()},"AABBCCDDEEFF",11);assert(app.begin());app.businessOnline(true);
        CompactFrameHeader header;header.op=1;header.requestNo=1;header.payloadLen=2;header.session[0]=1;
        uint8_t raw[48];assert(compactFrameEncode(header,raw));std::string body((char *)raw,48);body+="{}";
        auto status=request("POST","/api/status",body);status.contentType="application/vnd.codex-status.frame";
        auto reply=app.businessHttp(status,1);assert(reply.status==200&&reply.frame);
        assert(text(reply.bytes).find("publish_complete_bundle")!=std::string::npos&&text(reply.bytes).find("\"bundle_state\":\"recovery\"")!=std::string::npos);
    }
    {
        struct Slots:Flash {
            int running=0,selected=0;unsigned selections=0;bool valid=true,healthy=true;
            Ap01SlotImage identity;
            static int current(void *p){return ((Slots *)p)->running;}
            static bool health(void *p){return ((Slots *)p)->healthy;}
            static bool inspect(void *p,unsigned slot,Ap01SlotImage &out){auto &s=*(Slots *)p;if(!s.valid||slot>1)return false;out=s.identity;return true;}
            static int select(void *p,unsigned slot){auto &s=*(Slots *)p;++s.selections;s.selected=slot;return s.unknown?-1:1;}
            Ap01FirmwarePort callbacks(){auto result=port();result.runningSlot=current;result.inspectSlot=inspect;result.selectSlot=select;return result;}
        } fake;
        fake.identity.length=image.size();fake.identity.crc=1234;memcpy(fake.identity.build,fake.build,33);
        std::string folder=root+"/f7";LittleFSClass fs(ops,folder.c_str());
        auto binding=readAll((root+"/bindings.bin").c_str());std::ofstream(folder+"/bindings.bin",std::ios::binary).write(binding.data(),binding.size());
        const std::string mark="{\"device_mac\":\"AABBCCDDEEFF\",\"request_id\":\"stable-test\",\"slot\":0,\"expected_stable_revision\":0,\"confirm\":true}";
        const std::string change="{\"device_mac\":\"AABBCCDDEEFF\",\"request_id\":\"switch-test\",\"target_slot\":0,\"expected_revision\":0,\"expected_stable_revision\":1,\"expected_image_crc\":1234}";
        const std::string reboot="{\"device_mac\":\"AABBCCDDEEFF\",\"request_id\":\"switch-test\"}";
        {
            // Business never starts: maintenance still authenticates and works.
            Ap01Business business(fs,{nullptr,present},20);
            Ap01Application app(fs,business,{&disk,randomBytes,lock,unlock,fake.callbacks()},"AABBCCDDEEFF",20);assert(app.begin());
            assert(app.maintenanceHttp(request("POST","/api/firmware/switch",change,""),1).status==401);
            assert(app.maintenanceHttp(request("POST","/api/firmware/stable",mark),1).status==200);
            assert(app.maintenanceHttp(request("POST","/api/firmware/stable",mark),2).status==200);
            assert(app.maintenanceHttp(request("POST","/api/firmware/switch",change),2).status==409&&!fake.selections);
        }
        fake.running=1;
        {
            Ap01Business business(fs,{nullptr,present},21);
            Ap01Application app(fs,business,{&disk,randomBytes,lock,unlock,fake.callbacks()},"AABBCCDDEEFF",21);assert(app.begin());
            auto protectedReply=app.maintenanceHttp(request("POST","/api/firmware/begin",beginBody(image)),2);
            assert(protectedReply.status==409&&text(protectedReply.bytes).find("stable_slot_protected")!=std::string::npos&&!fake.opens);
            fake.valid=false;assert(app.maintenanceHttp(request("POST","/api/firmware/switch",change),3).status==409&&!fake.selections);fake.valid=true;
            fake.identity.crc++;assert(app.maintenanceHttp(request("POST","/api/firmware/switch",change),3).status==409&&!fake.selections);fake.identity.crc--;
            auto reply=app.maintenanceHttp(request("POST","/api/firmware/switch",change),4);
            assert(reply.status==200&&fake.selections==1&&fake.selected==0&&!fake.writes&&!fake.opens);
            assert(app.maintenanceHttp(request("POST","/api/firmware/switch",change),5).status==200&&fake.selections==1);
            assert(app.maintenanceHttp(request("POST","/api/firmware/reboot",reboot),6).status==200);app.maintenanceResponse(false);assert(!fake.reboots);
            assert(app.maintenanceHttp(request("POST","/api/firmware/reboot",reboot),7).status==200);app.maintenanceResponse(true);assert(fake.reboots==1);
        }
        fake.running=0;
        {
            Ap01Business business(fs,{nullptr,present},22);
            Ap01Application app(fs,business,{&disk,randomBytes,lock,unlock,fake.callbacks()},"AABBCCDDEEFF",22);assert(app.begin());
            auto reply=app.maintenanceHttp(request("GET","/api/firmware/switch-test",{}),8);
            assert(reply.status==200&&text(reply.bytes).find("\"boot_confirmed\":true")!=std::string::npos);
            assert(app.maintenanceHttp(request("POST","/api/firmware/switch",change),9).status==200&&fake.selections==1);
            assert(app.maintenanceHttp(request("POST","/api/firmware/reboot",reboot),10).status==200);app.maintenanceResponse(true);assert(fake.reboots==1);
        }
        {
            // A lost switch result must not cause a second partition commit.
            std::string unknownFolder=root+"/f8";LittleFSClass unknownFs(ops,unknownFolder.c_str());
            auto stableBytes=readAll((folder+"/stable.bin").c_str());
            std::ofstream(unknownFolder+"/stable.bin",std::ios::binary).write(stableBytes.data(),stableBytes.size());
            fake.running=1;fake.unknown=true;unsigned before=fake.selections;
            {
                Ap01Firmware fw(unknownFs,fake.callbacks(),"AABBCCDDEEFF");assert(fw.begin());
                assert(fw.handle(request("POST","/api/firmware/switch",change),"bridge-test",1).status==503);
                assert(fw.handle(request("POST","/api/firmware/switch",change),"bridge-test",2).status==503&&fake.selections==before+1);
                assert(fw.blocksBusiness());
            }
            fake.running=0;fake.unknown=false;
            Ap01Firmware restored(unknownFs,fake.callbacks(),"AABBCCDDEEFF");assert(restored.begin()&&!restored.blocksBusiness());
            auto receipt=restored.handle(request("GET","/api/firmware/switch-test",{}),"bridge-test",3);
            assert(receipt.status==200&&text(receipt.bytes).find("\"boot_confirmed\":true")!=std::string::npos);
        }
        {
            std::string badFolder=root+"/f9";LittleFSClass badFs(ops,badFolder.c_str());
            std::ofstream(badFolder+"/stable.bin",std::ios::binary)<<"APS9-corrupt";
            Ap01Firmware fw(badFs,fake.callbacks(),"AABBCCDDEEFF");assert(fw.begin());
            auto reply=fw.handle(request("GET","/api/firmware/slots",{}),"bridge-test",1);
            assert(text(reply.bytes).find("stable_storage_unavailable")!=std::string::npos);
            assert(fw.handle(request("POST","/api/firmware/begin",beginBody(image)),"bridge-test",1).status==200);
            assert(fw.handle(request("DELETE","/api/firmware/fw-test",{}),"bridge-test",2).status==200);
            const std::string manual="{\"device_mac\":\"AABBCCDDEEFF\",\"request_id\":\"rescue-no-stable\",\"target_slot\":1,\"expected_revision\":1}";
            assert(fw.handle(request("POST","/api/firmware/switch",manual),"bridge-test",3).status==200);
            fake.opens=0;fake.writes=0;
        }
        {
            std::string rescuedFolder=root+"/f20";LittleFSClass rescuedFs(ops,rescuedFolder.c_str());
            auto receipt=readAll((folder+"/firmware.bin").c_str());
            std::ofstream(rescuedFolder+"/firmware.bin",std::ios::binary).write(receipt.data(),receipt.size());
            fake.running=1;
            Ap01Firmware rescued(rescuedFs,fake.callbacks(),"AABBCCDDEEFF");assert(rescued.begin()&&!rescued.blocksBusiness());
            auto result=rescued.handle(request("GET","/api/firmware/status",{}),"bridge-test",1);
            assert(text(result.bytes).find("booted_other_slot")!=std::string::npos);
        }
        for(unsigned scenario=0;scenario<8;++scenario) {
            std::string trialFolder=root+"/f"+std::to_string(10+scenario);LittleFSClass trialFs(ops,trialFolder.c_str());
            auto stableBytes=readAll((folder+"/stable.bin").c_str());
            if(scenario!=7)std::ofstream(trialFolder+"/stable.bin",std::ios::binary).write(stableBytes.data(),stableBytes.size());
            auto receipt=readAll((folder+"/firmware.bin").c_str());
            std::string body(receipt.data()+12,receipt.size()-12);auto at=body.find("\"target_slot\":0");assert(at!=std::string::npos);body[at+14]='1';
            uint8_t header[12]={'A','P','F','1'};put32(header+4,body.size());put32(header+8,v2Crc32((const uint8_t *)body.data(),body.size()));
            {std::ofstream file(trialFolder+"/firmware.bin",std::ios::binary);file.write((char *)header,12);file<<body;}
            fake.running=fake.selected=1;fake.unknown=scenario==3;fake.valid=scenario!=4;fake.healthy=true;
            unsigned before=fake.selections;auto callbacks=fake.callbacks();callbacks.bootHealthy=Slots::health;
            {
                Ap01Firmware fw(trialFs,callbacks,"AABBCCDDEEFF");assert(fw.begin(100));
                auto status=[&](uint64_t now){return text(fw.handle(request("GET","/api/firmware/switch-test",{}),"bridge-test",now).bytes);};
                if(scenario==7){assert(status(101).find("unconfigured")!=std::string::npos&&fake.selections==before);continue;}
                if(scenario==3||scenario==4){assert(status(101).find("\"boot_safety\":\"unknown\"")!=std::string::npos);fw.tick(10000);assert(fake.selections==before+(scenario==3));continue;}
                assert(fake.selected==0&&fake.selections==before+1&&!fw.blocksBusiness());
                assert(status(101).find("\"result\":\"trial\"")!=std::string::npos);
                assert(fw.handle(request("POST","/api/firmware/begin",beginBody(image)),"bridge-test",102).status==409);
                if(scenario==1||scenario==6) {
                    fake.healthy=false;fw.tick(10000);fw.tick(30100);
                    assert(fake.selected==0&&fake.selections==before+1&&status(30101).find("retained_stable")!=std::string::npos);
                } else if(scenario==5) {
                    std::string manual=change;auto revision=manual.find("\"expected_revision\":0");manual[revision+20]='1';
                    manual.replace(manual.find("switch-test"),11,"rescue-test");
                    assert(fw.handle(request("POST","/api/firmware/switch",manual),"bridge-test",103).status==200);
                    fw.tick(15000);auto slots=text(fw.handle(request("GET","/api/firmware/slots",{}),"bridge-test",15001).bytes);
                    assert(fake.selected==0&&slots.find("retained_stable")!=std::string::npos);
                } else {
                    fw.tick(9100);assert(fake.selected==0);
                    if(scenario==2){fake.healthy=false;fw.tick(9101);fake.healthy=true;fw.tick(9200);fw.tick(18199);assert(fake.selected==0);fw.tick(18200);}
                    else fw.tick(9101);
                    assert(fake.selected==1&&fake.selections==before+2&&status(19000).find("healthy_locked")!=std::string::npos);
                }
            }
            // Persisted trial remains Failed for the older stable service.
            if(scenario!=5) {
                fake.running=0;fake.unknown=false;fake.valid=true;
                Ap01Firmware fallback(trialFs,fake.callbacks(),"AABBCCDDEEFF");assert(fallback.begin()&&!fallback.blocksBusiness());
                auto result=text(fallback.handle(request("GET","/api/firmware/switch-test",{}),"bridge-test",1).bytes);
                assert(result.find("\"result\":\"rejected\"")!=std::string::npos&&result.find("boot_trial_pending")!=std::string::npos);
            }
        }
    }
    for(unsigned scenario=0;scenario<2;++scenario) {
        std::string opaque=scenario?std::string(296,'\0'):image;
        if(!scenario)memset(&opaque[0x675000],0,52); // No proprietary App manifest.
        opaque[0x10]^=0x5a;opaque[0xb0]^=0x33; // Different board config and entry.
        memcpy(&opaque[opaque.size()-40],"0x5245434f56455259544147",24);
        for(unsigned i=0;i<4;++i)opaque[opaque.size()-8+i]=char(opaque.size()>>(8*(3-i)));
        put32((uint8_t *)&opaque[252],v2Crc32((const uint8_t *)opaque.data(),252));
        uint32_t crc=0;for(size_t i=0;i<opaque.size()-4;++i){crc^=uint8_t(opaque[i]);for(unsigned j=0;j<8;++j)crc=(crc>>1)^(0xedb88320u&(0u-(crc&1)));}
        put32((uint8_t *)&opaque[opaque.size()-4],crc);
        Flash fake;fake.expected=opaque;std::string folder=root+"/f"+std::to_string(18+scenario);LittleFSClass fs(ops,folder.c_str());
        Ap01Firmware fw(fs,fake.port(),"AABBCCDDEEFF");assert(fw.begin());
        assert(fw.handle(request("POST","/api/firmware/begin",beginBody(opaque)),"bridge-test",1).status==200);
        for(size_t at=0;at<opaque.size();at+=32768) {
            auto offset=std::to_string(at),body=opaque.substr(at,32768);auto r=request("POST","/api/firmware/chunk",body);
            r.contentType="application/octet-stream";r.requestId="fw-test";r.offset=offset.c_str();
            assert(fw.handle(r,"bridge-test",2).status==200);
        }
        assert(fw.handle(request("POST","/api/firmware/commit",commitBody),"bridge-test",3).status==200&&fake.commits==1);
    }
    puts("firmware: authenticated OTA, replay/conflict, revision fences, readback, unknown/storage failures, stable-slot protection/recovery, 9s health/30s timeout/fault cases; opaque board/entry, no App manifest and minimal integrity-valid image accepted");
}
