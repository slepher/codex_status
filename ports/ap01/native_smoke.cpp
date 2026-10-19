// Host-only integration of the exact native application/VFS surface. Never
// linked into a ROM. Tests the protocol and persistence, not the recovered ABI.
#include "http.h"
#include "local_json_allocator.h"
#include "v2_command_envelope.h"
#include "v2_sync.h"
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <assert.h>
#include <vector>
#include <string>
#include <fstream>
#include <iterator>

struct Host {
    bool locked = false, lostRename = false, corruptReadback = false,failPresent=false;
    unsigned randomCounter = 1, displays = 0;
    std::vector<uint8_t> frame;
};
int prepare(void *, const char *path) { struct stat s; return stat(path, &s) || !S_ISDIR(s.st_mode) ? -1 : 0; }
int openFile(void *, const char *p, unsigned mode) { return open(p, mode == 0 ? O_RDONLY : O_WRONLY|O_CREAT|(mode == 1 ? O_TRUNC : O_APPEND), 0600); }
int closeFile(void *, int fd) { return close(fd); }
int readFile(void *, int fd, void *p, size_t n) { return read(fd, p, n); }
int writeFile(void *, int fd, const void *p, size_t n) { int result = write(fd, p, n); return fsync(fd) ? -1 : result; }
int32_t seekFile(void *, int fd, int32_t at, unsigned origin) { return lseek(fd, at, origin); }
int32_t sizeFile(void *, int fd) { struct stat s; return fstat(fd, &s) ? -1 : s.st_size; }
int exists(void *, const char *p) { struct stat s; return stat(p, &s) ? (errno == ENOENT ? 0 : -1) : 1; }
int mkdirFile(void *, const char *p) { return mkdir(p, 0700); }
int removeFile(void *, const char *p) { return unlink(p); }
int renameFile(void *context, const char *a, const char *b) {
    int result = rename(a, b); auto &host = *(Host *)context;
    if (!result && host.lostRename && strstr(b, "state.bin")) { host.lostRename = false; return -1; }
    return result;
}
int statFs(void *, const char *, uint64_t *total, uint64_t *used) { *total = 0x140000; *used = 8192; return 0; }
bool randomBytes(void *p, uint8_t *out, size_t n) { auto &h = *(Host *)p; for (size_t i = 0; i < n; ++i) out[i] = ++h.randomCounter; return true; }
bool lock(void *p) { auto &h = *(Host *)p; assert(!h.locked); h.locked = true; return true; }
void unlock(void *p) { auto &h = *(Host *)p; assert(h.locked); h.locked = false; }
bool present(void *p, uint8_t *bytes, uint32_t) { auto &h = *(Host *)p; if(h.failPresent)return false;if (bytes) { h.frame.assign(bytes, bytes+38400); delete[] bytes; ++h.displays; } return true; }
std::string readAll(const char *path) { std::ifstream f(path, std::ios::binary); return {std::istreambuf_iterator<char>(f), {}}; }
std::string text(const String &s) { return {s.c_str(), s.length()}; }
Ap01HttpRequest req(const char *method, const char *path, const char *token, const std::string &body) {
    Ap01HttpRequest r{}; r.method=method; r.path=path; r.token=token; r.deviceToken=token;
    r.contentType="application/json"; r.body=(const uint8_t *)body.data(); r.length=body.size(); return r;
}
struct Client {
    Ap01Application &app;
    uint64_t number = 0;
    uint8_t session[16] = {1};
    std::string nonce, token="0123456789abcdef0123456789abcdef";
    Ap01HttpReply send(unsigned op, const char *path, const std::string &payload, const char *id="", const char *offset="") {
        CompactFrameHeader h; h.op=op; h.requestNo=++number; h.payloadLen=payload.size(); memcpy(h.session, session, 16);
        uint8_t header[48]; assert(compactFrameEncode(h, header));
        std::string bytes((const char *)header, 48); bytes+=payload;
        auto r=req("POST",path,token.c_str(),bytes); r.contentType="application/vnd.codex-status.frame";
        r.requestId=id; r.offset=offset; r.nonce=nonce.c_str();
        auto reply=app.businessHttp(r,1000+number);
        if (reply.frame) { CompactFrameHeader response; assert(compactFrameDecode((const uint8_t *)reply.bytes.c_str(),reply.bytes.length(),response,4096)); memcpy(session,response.session,16); }
        return reply;
    }
    std::string body(const Ap01HttpReply &r) { return text(r.bytes).substr(r.frame?48:0); }
    std::string command(const char *id) { return std::string("\"bridge_id\":\"bridge-test\",\"device_mac\":\"AABBCCDDEEFF\",\"request_id\":\"")+id+"\""; }
    void status() {
        auto reply=send(1,"/api/status","{}"); assert(reply.status==200);
        LocalJsonAllocator a; JsonDocument doc(&a); assert(!deserializeJson(doc,body(reply).c_str())); nonce=doc["session_nonce"].as<const char *>();
    }
    void claim() { auto reply=send(4,"/claim","{\"device_mac\":\"AABBCCDDEEFF\",\"bridge_id\":\"bridge-test\"}"); assert(reply.status==200); }
};
struct MemoryStream { std::string input, output; size_t offset=0, fragment=17; };
int streamRead(void *p, uint8_t *out, size_t n, uint64_t) { auto &s=*(MemoryStream *)p; n=std::min(n,std::min(s.fragment,s.input.size()-s.offset)); memcpy(out,s.input.data()+s.offset,n); s.offset+=n; return n; }
int streamWrite(void *p,const uint8_t *out,size_t n,uint64_t) { auto &s=*(MemoryStream *)p; n=std::min(n,s.fragment); s.output.append((const char *)out,n); return n; }
void http(Ap01Application &app) {
    struct Example {const char *request;unsigned status;};
    for (auto example : {
        Example{"GET /api/pairing/info HTTP/1.1\r\nHost: ap01\r\n\r\n",200},
        Example{"POST /api/pairing HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\n{",400},
        Example{"POST /api/pairing HTTP/1.1\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\n{",400},
        Example{"POST /api/pairing HTTP/1.1\r\nContent-Length: 4097\r\n\r\n",413}}) {
        MemoryStream s{example.request}; Ap01Stream io{&s,streamRead,streamWrite}; assert(ap01ServeHttp(app,true,io,1,5000));
        assert(s.output.rfind("HTTP/1.1 "+std::to_string(example.status)+" ",0)==0);
    }
}
void binaryHttp(Ap01Application &app) {
    CompactFrameHeader header;header.op=1;header.requestNo=1;header.payloadLen=2;header.session[0]=9;
    uint8_t raw[48];assert(compactFrameEncode(header,raw));
    MemoryStream stream;stream.fragment=8192;
    stream.input="POST /api/status HTTP/1.1\r\nContent-Length: 50\r\nContent-Type: application/vnd.codex-status.frame\r\nAuthorization: Bearer 0123456789abcdef0123456789abcdef\r\n\r\n";
    stream.input.append((const char *)raw,48);stream.input+="{}";
    Ap01Stream io{&stream,streamRead,streamWrite};assert(ap01ServeHttp(app,false,io,10,5000));
    assert(stream.output.rfind("HTTP/1.1 200 ",0)==0);
    size_t body=stream.output.find("\r\n\r\n");assert(body!=std::string::npos);
    CompactFrameHeader response;assert(compactFrameDecode((const uint8_t *)stream.output.data()+body+4,stream.output.size()-body-4,response,4096));
    assert(response.kind==1&&response.op==1&&response.requestNo==1);
}
int main(int argc,char **argv) {
    assert(argc==4); Host host;
    Ap01VfsOps io{&host,prepare,openFile,closeFile,readFile,writeFile,seekFile,sizeFile,exists,mkdirFile,removeFile,renameFile,statFs};
    LittleFSClass fs(io,argv[1]); std::string bundle=readAll(argv[2]); assert(!bundle.empty());
    std::string recoveredContext;
    {
        Ap01Business content(fs,{&host,present},1); assert(content.begin(0));
        Ap01Application app(fs,content,{&host,randomBytes,lock,unlock},"AABBCCDDEEFF",1); assert(app.begin()); app.businessOnline(true);
        http(app);
        const std::string pairing="{\"device_mac\":\"AABBCCDDEEFF\",\"bridge_id\":\"bridge-test\",\"name\":\"native-test\",\"display_code\":\"1234\",\"binding_token\":\"0123456789abcdef0123456789abcdef\",\"request_id\":\"pair-test\",\"boot_id\":1,\"pairing_revision\":0}";
        auto reply=app.maintenanceHttp(req("POST","/api/pairing","",pairing),1); assert(reply.status==202);
        assert(app.button(false,1,2)); // A held old release cannot approve.
        assert(app.pairingDisplayed("pair-test",0,2));
        assert(app.button(false,3,3));
        auto result=app.maintenanceHttp(req("GET","/api/pairing/pair-test","0123456789abcdef0123456789abcdef",{}),4);
        assert(text(result.bytes).find("approved")==std::string::npos);
        assert(app.button(true,4,5)); assert(app.button(false,5,6));
        binaryHttp(app);
        Client client{app}; client.status(); client.claim();
        char crc[9]; snprintf(crc,9,"%08x",v2Crc32((const uint8_t *)bundle.data(),bundle.size()));
        std::string envelope="{"+client.command("bundle-test")+",\"length\":"+std::to_string(bundle.size())+",\"content_crc\":\""+crc+"\"}";
        auto begin=client.send(10,"/api/bundle/begin",envelope); assert(begin.status==200);
        for (size_t at=0;at<bundle.size();) {
            size_t n=std::min<size_t>(3072,bundle.size()-at); auto offset=std::to_string(at);
            auto chunk=client.send(11,"/api/bundle/chunk",bundle.substr(at,n),"bundle-test",offset.c_str());
            if (chunk.status!=200) fprintf(stderr,"chunk %s\n",client.body(chunk).c_str()); assert(chunk.status==200); at+=n;
        }
        auto commit=client.send(12,"/api/bundle/commit",envelope);
        if(commit.status!=200)fprintf(stderr,"commit %s\n",client.body(commit).c_str()); assert(commit.status==200); assert(content.count()==2);
        auto replay=client.send(12,"/api/bundle/commit",envelope); assert(replay.status==200);
        LocalJsonAllocator a; JsonDocument status(&a); content.status(status);
        bool hasClock=false;
        for(JsonObject requirement:status["data_contract"]["requirements"].as<JsonArray>())
            if(!strcmp(requirement["field"]|"","device.now"))hasClock=true;
        unsigned displays=host.displays;std::string clockContext=content.context();
        content.clock("00:59");assert(host.displays==displays+unsigned(hasClock));
        content.clock("00:59");assert(host.displays==displays+unsigned(hasClock));
        content.clock("01:00");assert(host.displays==displays+2*unsigned(hasClock));
        assert(clockContext==content.context());content.status(status);assert((status["applied_seq"]|0u)==0);
        const char *second=status["template_ids"][1] | "";
        std::string activate="{"+client.command("activate-test")+",\"template_id\":\""+second+"\",\"expected_active_context_id\":\""+content.context()+"\"}";
        host.lostRename=true;
        auto switched=client.send(13,"/api/activate",activate); assert(switched.status==200); assert(content.active()==1);
        host.failPresent=true;assert(!content.localActivate(content.active(),2000));content.status(status);assert(!strcmp(status["display_state"]|"","failed"));
        host.failPresent=false;assert(content.localActivate(content.active(),2001));content.status(status);content.displayed(status["frame_generation"]|0u);content.status(status);assert(!strcmp(status["display_state"]|"","displayed"));
        std::ofstream output(argv[3],std::ios::binary); output<<activate;
        app.businessOnline(false);
        auto health=app.maintenanceHttp(req("GET","/health",client.token.c_str(),{}),4000); assert(health.status==200);
        assert(text(health.bytes).find("\"business_online\":false")!=std::string::npos);
        assert(app.businessHttp(req("POST","/api/status",client.token.c_str(),{}),4000).status==503);
    }
    {
        Ap01Business content(fs,{&host,present},2); assert(content.begin(0)); assert(content.active()==1);
        auto receipt=content.store.bsActivationReceipt(); assert(receipt && !strcmp(receipt->request,"activate-test"));
        Ap01Application app(fs,content,{&host,randomBytes,lock,unlock},"AABBCCDDEEFF",2); assert(app.begin()); app.businessOnline(true);
        Client client{app}; client.status(); client.claim();
        LocalJsonAllocator a; JsonDocument old(&a); auto activation=readAll(argv[3]); assert(!deserializeJson(old,activation.c_str())); old.remove("session_nonce");
        String refreshed; serializeJson(old,refreshed);
        auto replay=client.send(13,"/api/activate",text(refreshed)); assert(replay.status==200); assert(content.active()==1);
        std::string discover="{\"magic\":\"codex-status\",\"kind\":\"discover\",\"discovery_v\":1,\"nonce\":\"0123456789abcdef0123456789abcdef\"}";
        String discovered; assert(app.discovery((const uint8_t *)discover.data(),discover.size(),discovered));
        assert(text(discovered).find("18081")!=std::string::npos);
        recoveredContext=content.context();
    }
    {
        Ap01Business content(fs,{&host,present},2);assert(content.begin(0));
        assert(content.active()==1&&recoveredContext!=content.context());
        auto receipt=content.store.bsActivationReceipt();assert(receipt&&!strcmp(receipt->request,"activate-test"));
    }
    fprintf(stdout,"native application: physical confirmation, full Bundle chunks/commit replay, durable activation/lost rename ACK, restart, independent maintenance, discovery and HTTP parsing passed\n");
}
