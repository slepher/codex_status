// Compile the production callbacks, replacing only Arduino/hardware surfaces.
#include <cassert>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <initializer_list>
using String = std::string;
enum HTTPMethod { HTTP_GET, HTTP_POST };
enum { UPLOAD_FILE_START, UPLOAD_FILE_WRITE, UPLOAD_FILE_END, UPLOAD_FILE_ABORTED };
struct HTTPUpload { int status; size_t currentSize = 0, totalSize = 0; uint8_t *buf = nullptr; };
struct WebServer {
    bool target = false; String targetValue, body; int status = 0;
    bool hasArg(const char *) { return target; }
    String arg(const char *) { return targetValue; }
    void sendHeader(const char *, const char *) {}
    void send(int code, const char *, const char *text) { status = code; body = text; }
};
struct RequestHandler {
    virtual ~RequestHandler() = default;
    virtual bool canHandle(WebServer &, HTTPMethod, const String &) { return false; }
    virtual bool canUpload(WebServer &, const String &) { return false; }
    virtual bool handle(WebServer &, HTTPMethod, const String &) { return false; }
    virtual void upload(WebServer &, const String &, HTTPUpload &) {}
};
struct { WebServer server; bool otaInProgress = false, otaRebootPending = false; uint32_t otaLastDataMs = 0, otaRebootAt = 0; } state;
struct {
    bool running = false, failBegin = false, failWrite = false, failEnd = false;
    unsigned begins = 0, writes = 0, ends = 0, aborts = 0;
    bool isRunning() { return running; }
    unsigned getError() { return 0; } // Stale library error is never proof of success.
    void abort() { ++aborts; running = false; }
    bool begin(int) { ++begins; running = true; return !failBegin; }
    size_t write(uint8_t *, size_t n) { ++writes; return failWrite ? 0 : n; }
    bool end(bool) { ++ends; if (failEnd) return false; running = false; return true; }
} Update;
struct { template<class... T> void printf(const char *, T...) {} } DevLog;
namespace power { bool lock = false, awake = false; void setOtaLock(bool v) { lock = v; } void setOtaWifiAwake(bool v) { awake = v; } }
namespace display { struct { bool frame = true; } state; void renderCurrent() {} void screen(std::initializer_list<String>) {} }
namespace application { struct { String firmwareVersion = "fixture"; } state; }
static bool authorized = true;
bool requestAuthorized() { return authorized; }
static uint32_t nowMs = 0;
uint32_t millis() { return nowMs; }
void dataLedActivity() {}
constexpr int UPDATE_SIZE_UNKNOWN = -1;
constexpr const char *FW_TARGET_ID = "zectrix-note4-400x300";
#include "../src/firmware/ota_http.inc"
static OtaHttpHandler handler;
static void reset() { state = {}; Update = {}; otaHttp = {}; authorized = true; nowMs = 0; power::lock = power::awake = false; }
static void request() { assert(handler.canHandle(state.server, HTTP_POST, "/doUpdate")); }
static void event(int status, size_t bytes = 0) { HTTPUpload up{status,bytes,bytes}; handler.upload(state.server,"/doUpdate",up); }
static void complete() { handler.handle(state.server,HTTP_POST,"/doUpdate"); }
static void released() { assert(!state.otaInProgress && !Update.running && !power::lock && !power::awake); }
static void success(bool target = true) { state.server.target=target; state.server.targetValue=FW_TARGET_ID; request(); event(UPLOAD_FILE_START); event(UPLOAD_FILE_WRITE,10); event(UPLOAD_FILE_END,10); complete(); assert(state.server.status==200 && state.server.body=="UPDATE OK"); assert(state.otaRebootPending); released(); }
int main() {
    // No upload callback: auth is per request, independent of earlier outcomes.
    reset(); request(); complete(); assert(state.server.status==400); assert(Update.begins==0);
    authorized=false; request(); complete(); assert(state.server.status==401); authorized=true; success();
    state.otaRebootPending=false; request(); complete(); assert(state.server.status==400);
    for (int failure=0;failure<7;++failure) {
        reset(); request();
        if (failure==0) authorized=false;
        if (failure==1) { state.server.target=true; state.server.targetValue="wrong"; }
        Update.failBegin=failure==2; Update.failWrite=failure==3; Update.failEnd=failure==4;
        event(UPLOAD_FILE_START); event(UPLOAD_FILE_WRITE,10);
        if (failure==5) event(UPLOAD_FILE_ABORTED); else event(UPLOAD_FILE_END,failure==6?0:10);
        complete(); assert(state.server.status==(failure==0?401:(failure==1||failure==6)?400:500));
        assert(!state.otaRebootPending); released();
        authorized=true; state.server.target=false; Update.failBegin=Update.failWrite=Update.failEnd=false;
        success();
    }
    // Another native upload owns Update/locks: refusal must not release them.
    for (bool auth : {false,true}) {
        reset(); Update.running=true; power::lock=power::awake=true; authorized=auth;
        request(); event(UPLOAD_FILE_START); event(UPLOAD_FILE_WRITE,10); event(UPLOAD_FILE_END,10); complete();
        assert(state.server.status==(auth?409:401)); assert(Update.running&&power::lock&&power::awake);
        assert(Update.aborts==0&&Update.begins==0&&Update.writes==0&&Update.ends==0);
    }
    reset(); state.otaInProgress=true; Update.running=true; power::lock=power::awake=true;
    request(); event(UPLOAD_FILE_START); event(UPLOAD_FILE_ABORTED); complete();
    assert(state.server.status==409 && state.otaInProgress && Update.running && power::lock && power::awake);
    assert(Update.aborts==0);
    // No first-tick hole, exact 20s boundary, unsigned millis wraparound.
    for (uint32_t start : {0u, UINT32_MAX-10000u}) {
        reset(); nowMs=start; request(); event(UPLOAD_FILE_START);
        nowMs=start+19999u; otaUploadTick(); assert(state.otaInProgress);
        nowMs=start+20000u; otaUploadTick(); released(); complete(); assert(state.server.status==408);
        success();
    }
    reset(); request(); event(UPLOAD_FILE_START); complete(); assert(state.server.status==500); released(); success();
    reset(); request(); event(UPLOAD_FILE_START); event(UPLOAD_FILE_START); complete(); assert(state.server.status==400); released(); success();
    reset(); success(false); request(); event(UPLOAD_FILE_START); complete(); assert(state.server.status==409);
    puts("OTA production handler: auth/no-file/target/busy/begin/write/end/abort/empty/stall/retry passed");
}
