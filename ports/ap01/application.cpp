#include "application.h"
#include "local_json_allocator.h"
#include "v2_command_envelope.h"
#include "v2_sync.h"
#include "platform_target.h"
#include <new>
#if defined(__riscv)
extern "C" const uint8_t ap01_native_manifest[256];
#endif

namespace {
constexpr const char *FRAME_TYPE = "application/vnd.codex-status.frame";
void imageIdentity(JsonDocument &out) {
    out["fw"] = "1.0.2_0041";
#if defined(__riscv)
    char build[33]; const char *hex="0123456789abcdef";
    for (unsigned i=0;i<16;++i) {uint8_t v=ap01_native_manifest[36+i];build[2*i]=hex[v>>4];build[2*i+1]=hex[v&15];}
    build[32]=0;out["app_build"] = String(build);
    const uint8_t *p=ap01_native_manifest+12;
    out["rom_bytes"] = uint32_t(p[0])|uint32_t(p[1])<<8|uint32_t(p[2])<<16|uint32_t(p[3])<<24;
#else
    out["app_build"] = "host-native-port";
#endif
}
class BindingGuard {
    const Ap01ApplicationPort &port;
    bool held;
public:
    explicit BindingGuard(const Ap01ApplicationPort &p) : port(p), held(p.lockBindings && p.lockBindings(p.context)) {}
    ~BindingGuard() { if (held) port.unlockBindings(port.context); }
    void release() {if(held){port.unlockBindings(port.context);held=false;}}
    explicit operator bool() const { return held; }
};
bool hex(const char *text, size_t length) {
    if (!text || strlen(text) != length) return false;
    for (size_t i = 0; i < length; ++i)
        if (!((text[i] >= '0' && text[i] <= '9') || (text[i] >= 'a' && text[i] <= 'f') ||
              (text[i] >= 'A' && text[i] <= 'F'))) return false;
    return true;
}
void error(Ap01HttpReply &reply, int status, const char *code, const char *stage, bool unknown = false) {
    LocalJsonAllocator allocator; JsonDocument out(&allocator);
    out["result"] = unknown ? "unknown" : "rejected";
    out["error"] = code; out["stage"] = stage;
    out["action"] = unknown ? "query_original_request" : "read_device_status";
    reply.status = status; reply.bytes = "";
    if (out.overflowed() || !serializeJson(out, reply.bytes))
        reply.bytes = "{\"result\":\"unknown\",\"error\":\"reply_oom\",\"stage\":\"response\"}";
}
bool encode(JsonDocument &doc, Ap01HttpReply &reply) {
    size_t expected = measureJson(doc); reply.bytes = "";
    if (doc.overflowed() || expected > 4048 || !reply.bytes.reserve(expected) ||
        serializeJson(doc, reply.bytes) != expected || reply.bytes.length() != expected) {
        error(reply, 503, "reply_limit", "response", true); return false;
    }
    return true;
}
bool matches(File &file, const uint8_t *bytes, size_t length) {
    if (!file || file.size() != length) return false;
    for (size_t i = 0; i < length; ++i) if (file.read() != bytes[i]) return false;
    return true;
}
uint16_t operation(const char *path) {
    if (!strcmp(path, "/api/status")) return 1;
    if (!strcmp(path, "/api/data")) return 2;
    if (!strcmp(path, "/api/plan")) return 3;
    if (!strcmp(path, "/claim")) return 4;
    if (!strcmp(path, "/api/bundle/begin")) return 10;
    if (!strcmp(path, "/api/bundle/chunk")) return 11;
    if (!strcmp(path, "/api/bundle/commit")) return 12;
    if (!strcmp(path, "/api/activate")) return 13;
    return 0;
}
}
struct Ap01Application::Session {
    char principal[65] = {};
    uint8_t open[16] = {}, id[16] = {}, digest[32] = {};
    uint64_t highest = 0;
    int lastStatus = 0;
    String response;
    std::unique_ptr<Session> next;
};
Ap01Application::Ap01Application(BundleFilesystem &filesystem, Ap01Business &content,
    const Ap01ApplicationPort &callbacks, const char *wifiMac, uint64_t bootId)
    : fs(filesystem), business(content), port(callbacks), boot(bootId) {
    mac[0] = 0;
    if (hex(wifiMac, 12)) memcpy(mac, wifiMac, 13);
}
Ap01Application::~Ap01Application() { ap01BindingsFree(&bindings); }
int Ap01Application::save(void *context, const uint8_t *bytes, size_t length) {
    auto &self = *(Ap01Application *)context;
    File pending = self.fs.open("/bindings.pending", "w");
    if (!pending || pending.write(bytes, length) != length) return 0;
    pending.close(); pending = self.fs.open("/bindings.pending", "r");
    if (!matches(pending, bytes, length)) return 0;
    pending.close();
    bool renamed = self.fs.rename("/bindings.pending", "/bindings.bin");
    File committed = self.fs.open("/bindings.bin", "r");
    // A failed rename may still have committed: only exact final readback wins.
    if (matches(committed, bytes, length)) return 1;
    (void)renamed; return -1;
}
bool Ap01Application::begin(uint64_t now) {
    if (!*mac || !port.random || !port.lockBindings || !port.unlockBindings) return false;
    uint8_t random[16]; if (!port.random(port.context, random, sizeof(random))) return false;
    const char *digits = "0123456789abcdef";
    for (unsigned i = 0; i < 16; ++i) { nonce[2*i] = digits[random[i] >> 4]; nonce[2*i+1] = digits[random[i] & 15]; }
    File file = fs.open("/bindings.bin", "r");
    if(!file&&fs.inspect("/bindings.bin")!=0)return false;
    size_t length = file ? file.size() : 0;
    if ((file && !length) || length > 2136) return false;
    std::unique_ptr<uint8_t[]> bytes(length ? new (std::nothrow) uint8_t[length] : nullptr);
    if (length && (!bytes || file.read(bytes.get(), length) != length)) return false;
    Ap01BindingIo io{this, malloc, free, save};
    ready = ap01BindingsInit(&bindings, &io, boot, bytes.get(), length);
    if(ready&&port.firmware.open) {
        firmware.reset(new(std::nothrow) Ap01Firmware(fs,port.firmware,mac));
        if(firmware)firmware->begin(now);
        maintenanceGate.store(!firmware||firmware->blocksBusiness(),std::memory_order_release);
    }
    return ready;
}
Ap01Application::Session *Ap01Application::session(const char *principal) {
    unsigned count = 0;
    for (Session *s = sessions.get(); s; s = s->next.get(), ++count)
        if (!strcmp(s->principal, principal)) return s;
    if (count >= 8) return nullptr;
    std::unique_ptr<Session> value(new (std::nothrow) Session);
    if (!value) return nullptr;
    strcpy(value->principal, principal); value->next = std::move(sessions);
    sessions = std::move(value); return sessions.get();
}
Ap01HttpReply Ap01Application::maintenanceHttp(const Ap01HttpRequest &request, uint64_t now) {
    Ap01HttpReply reply;
    maintenanceTick(now);
    BindingGuard guard(port);
    if (!ready || !guard) { error(reply, 503, "maintenance_busy", "bindings"); return reply; }
    bindings.maintenance=maintenanceGate.load(std::memory_order_acquire);
    if (!strncmp(request.path, "/api/pairing", 12)) {
        if (request.length > 4096 || (request.length && memchr(request.body, 0, request.length))) {
            error(reply, 400, "body", "pairing"); return reply;
        }
        String input((const char *)request.body, request.length);
        std::unique_ptr<char[]> output(new (std::nothrow) char[4096]);
        if (!output || input.length() != request.length) { error(reply, 503, "oom", "pairing"); return reply; }
        if (bindings.storage_unknown) {
            File file = fs.open("/bindings.bin", "r"); size_t n = file ? file.size() : 0;
            std::unique_ptr<uint8_t[]> bytes(n && n <= 2136 ? new (std::nothrow) uint8_t[n] : nullptr);
            if (bytes && file.read(bytes.get(), n) == n) ap01BindingsResolve(&bindings, bytes.get(), n);
        }
        reply.status = ap01PairingHttp(&bindings, mac, request.method, request.path,
            request.token, input.c_str(), now, output.get(), 4096);
        if (reply.status < 0) error(reply, 503, "reply_limit", "pairing");
        else reply.bytes = output.get();
        return reply;
    }
    const char *bound=ap01BindingPrincipal(&bindings, request.token);
    if (!bound) { error(reply, 401, "unauthorized", "maintenance_auth"); return reply; }
    if(!strncmp(request.path,"/api/firmware/",14)) {
        if(!firmware){error(reply,port.firmware.open?503:501,"firmware_port_unavailable","maintenance_router");return reply;}
        char principal[65];strcpy(principal,bound);
        bool start=(!strcmp(request.path,"/api/firmware/begin")||!strcmp(request.path,"/api/firmware/switch"))&&!strcmp(request.method,"POST");
        if(start) {
            maintenanceGate.store(true,std::memory_order_release);bindings.maintenance=true;
        }
        guard.release();reply=firmware->handle(request,principal,now);
        maintenanceGate.store(firmware->blocksBusiness(),std::memory_order_release);
        return reply;
    }
    if (!strcmp(request.method, "GET") && (!strcmp(request.path, "/health") || !strcmp(request.path, "/api/maintenance/status"))) {
        LocalJsonAllocator allocator; JsonDocument out(&allocator);
        out["device_mac"] = mac; out["fw_target"] = FW_TARGET_ID; out["boot_id"] = boot;
        imageIdentity(out);
        out["service"] = "ap01-maintenance"; out["http_port"] = 18080;
        out["business_online"] = online.load(std::memory_order_acquire)&&!maintenanceGate.load(std::memory_order_acquire);
        out["firmware_ota"] = firmware&&firmware->available()?"chunked-v1":"unavailable";
        out["firmware_update_policy"]="sequential_with_traditional_ota";
        reply.status = 200; encode(out, reply); return reply;
    }
    error(reply, 404, "unsupported_endpoint", "maintenance_router"); return reply;
}
void Ap01Application::maintenanceTick(uint64_t now) {
    if(firmware){firmware->tick(now);maintenanceGate.store(firmware->blocksBusiness(),std::memory_order_release);}
}
void Ap01Application::dispatch(const Ap01HttpRequest &request, const char *principal,
                               uint16_t op, uint64_t now, Ap01HttpReply &reply) {
    LocalJsonAllocator allocator; JsonDocument doc(&allocator);
    if (op == 1) {
        business.status(doc); doc["device_mac"] = mac; doc["mac"] = mac;
        imageIdentity(doc);
        doc["boot_id"] = boot; doc["uptime_ms"] = now; doc["session_nonce"] = nonce;
        {
            BindingGuard guard(port);
            if (!guard) { error(reply, 503, "bindings_busy", "owner"); return; }
            doc["owner"]["id"] = bindings.owner_until > now ? bindings.owner : "";
            doc["owner"]["expires_in_s"] = bindings.owner_until > now ? (bindings.owner_until-now+999)/1000 : 0;
        }
        reply.status = 200; encode(doc, reply); return;
    }
    if (op != 11 && (deserializeJson(doc, request.body, request.length) || !doc.is<JsonObject>())) {
        error(reply, 400, "json", "command"); return;
    }
    if (op != 11 && (strcmp(doc["bridge_id"] | "", principal) || strcmp(doc["device_mac"] | "", mac))) {
        error(reply, 403, "identity", "binding"); return;
    }
    if (op != 11 && op != 4 && strcmp(doc["session_nonce"] | "", nonce)) { error(reply, 409, "session", "command"); return; }
    if (op == 10 || op == 12 || op == 13) {
        String macText(mac), nonceText(nonce);
        if (!v2CheckCommandSession(doc, macText, &nonceText).accepted) {
            error(reply, 409, "session", "command"); return;
        }
    }
    {
        BindingGuard guard(port);
        if (!guard) { error(reply, 503, "bindings_busy", "owner"); return; }
        bindings.maintenance=maintenanceGate.load(std::memory_order_acquire);
        if (op == 4) {
            if (strcmp(request.deviceToken, request.token)) { error(reply, 401, "device_token", "claim"); return; }
            if (doc["force"] == true || doc["release"] == true) { error(reply, 409, "unsupported", "claim"); return; }
            const char *result = ap01BindingClaim(&bindings, principal, request.token, now);
            if (strcmp(result, "applied")) { error(reply, 409, result, "claim"); return; }
            doc.clear(); doc["op"] = "claim"; doc["result"] = "applied";
            doc["device_mac"] = mac; doc["owner_id"] = principal; doc["lease_s"] = 300;
            doc["expires_in_s"] = 300; doc["owner"]["id"] = principal; doc["owner"]["expires_in_s"] = 300;
            reply.status = 200; encode(doc, reply); return;
        }
        if (bindings.maintenance || bindings.owner_until <= now || strcmp(bindings.owner, principal)) {
            error(reply, 409, "claim_required", "owner"); return;
        }
    }
    if (op == 11 && strcmp(request.nonce, nonce)) { error(reply, 409, "session", "bundle"); return; }
    if (op == 11) doc["bridge_id"] = principal;
    reply.status = business.command(op, doc, nonce, now, request.body, request.length,
                                    request.requestId, request.offset, reply.bytes);
    if (reply.status < 400 && (op == 2 || op == 10 || op == 11 || op == 12 || op == 13)) {
        BindingGuard guard(port);
        // Refresh only the same live owner; never create/transfer ownership.
        if (guard && bindings.owner_until > now && !strcmp(bindings.owner, principal)) bindings.owner_until = now + 300000;
    }
}
Ap01HttpReply Ap01Application::businessHttp(const Ap01HttpRequest &request, uint64_t now) {
    Ap01HttpReply reply;
    if (!ready || !online.load(std::memory_order_acquire)||maintenanceGate.load(std::memory_order_acquire)) { error(reply, 503, "business_offline", "lifecycle"); return reply; }
    uint16_t op = operation(request.path);
    if (!op) { error(reply, 404, "unsupported_endpoint", "business_router"); return reply; }
    if (strcmp(request.method, "POST") || strcmp(request.contentType, FRAME_TYPE)) {
        error(reply, 415, "content_type_or_method", "frame"); return reply;
    }
    CompactFrameHeader frame;
    if (!compactFrameDecode(request.body, request.length, frame, 4096) || frame.kind || frame.op != op) {
        error(reply, 400, "protocol_mismatch", "frame"); return reply;
    }
    char principal[65];
    {
        BindingGuard guard(port);
        const char *name = guard ? ap01BindingPrincipal(&bindings, request.token) : nullptr;
        if (!name) { error(reply, guard ? 401 : 503, guard ? "unauthorized" : "bindings_busy", "binding"); return reply; }
        strcpy(principal, name);
    }
    Session *s = session(principal);
    if (!s) { error(reply, 503, "session_oom", "frame"); return reply; }
    uint8_t zero[16] = {}, digest[32];
    String covered;
    bool coveredOk = covered.concat((const char *)request.body, request.length);
    for (const char *field : {request.method, request.path, request.requestId, request.offset, request.nonce, request.deviceToken}) {
        size_t n = strlen(field); uint8_t size[2] = {uint8_t(n), uint8_t(n >> 8)};
        coveredOk = coveredOk && n <= 512 && covered.concat((const char *)size, 2) && covered.concat(field, n);
    }
    if (!coveredOk) { error(reply, 503, "oom", "frame"); return reply; }
    syncSha256((const uint8_t *)covered.c_str(), covered.length(), digest);
    bool retain = false;
    if (!memcmp(frame.session, zero, 16)) error(reply, 409, "session", "frame");
    else {
        bool opening = op == 1 && memcmp(frame.session, s->id, 16) && memcmp(frame.session, s->open, 16);
        if (opening) {
            uint8_t random[16];
            if (!port.random(port.context, random, 16) || !memcmp(random, zero, 16)) {
                error(reply, 503, "entropy", "session");
            } else {
                memcpy(s->id, random, 16); memcpy(s->open, frame.session, 16);
                s->highest = 0; s->response = "";
            }
        }
        if (reply.bytes.length()) {}
        else if (memcmp(frame.session, s->id, 16) && !(op == 1 && !memcmp(frame.session, s->open, 16)))
            error(reply, 409, "session", "frame");
        else if (frame.requestNo <= s->highest) {
            if (frame.requestNo == s->highest && !memcmp(digest, s->digest, 32) && s->response.length()) {
                reply.status = s->lastStatus; reply.frame = true; reply.bytes = s->response; return reply;
            }
            error(reply, 409, frame.requestNo == s->highest && s->response.length() ? "request_conflict" : "retired", "frame");
        } else if (memcmp(frame.context, zero, 16) &&
            (strlen(business.context()) != 16 || memcmp(frame.context, business.context(), 16)))
            error(reply, 409, "context", "frame");
        else {
            retain = true; Ap01HttpRequest payload = request;
            payload.body += 48; payload.length -= 48;
            dispatch(payload, principal, op, now, reply);
        }
    }
    if (!reply.bytes.length() || reply.bytes.length() > 4048) error(reply, 503, "reply_limit", "response", true);
    CompactFrameHeader response = frame; response.kind = 1;
    response.result = reply.status < 400 ? 0 : reply.status == 503 ? 3 : 2;
    response.payloadLen = reply.bytes.length(); memcpy(response.session, s->id, 16);
    uint8_t header[48];
    if (!compactFrameEncode(response, header)) { error(reply, 503, "encode", "frame", true); return reply; }
    String encoded;
    if (!encoded.reserve(48 + reply.bytes.length()) || !encoded.concat((const char *)header, 48) || !encoded.concat(reply.bytes)) {
        error(reply, 503, "oom", "response", true); return reply;
    }
    reply.bytes = std::move(encoded); reply.frame = true;
    if (retain) {
        s->highest = frame.requestNo; memcpy(s->digest, digest, 32); s->response = "";
        if (reply.bytes.length() <= 512) s->response = reply.bytes;
        s->lastStatus = reply.status;
    }
    return reply;
}
bool Ap01Application::discovery(const uint8_t *bytes, size_t length, String &reply) {
    if (!ready || length > 256) return false;
    LocalJsonAllocator allocator; JsonDocument input(&allocator), out(&allocator);
    if (deserializeJson(input, bytes, length) || input["magic"] != "codex-status" || input["kind"] != "discover" ||
        input["discovery_v"] != 1 || !hex(input["nonce"] | "", 32) ||
        (!input["target_mac"].isNull() && strcmp(input["target_mac"] | "", mac))) return false;
    out["magic"] = "codex-status"; out["discovery_v"] = 1; out["kind"] = "discovery_reply";
    out["nonce"] = input["nonce"]; out["mac"] = mac; out["name"] = "CUKTECH AP01";
    out["model"] = "njcuk.enstor.ap01"; out["http_port"] = 18081; out["maintenance_port"] = 18080;
    out["boot_id"] = boot; out["binding_method"] = "screen_confirm_v1";
    Ap01HttpReply response; if (!encode(out, response)) return false;
    reply = std::move(response.bytes); return true;
}
bool Ap01Application::pairingUi(uint64_t now, String &reply) {
    BindingGuard guard(port); if (!ready || !guard) return false;
    bindings.maintenance=maintenanceGate.load(std::memory_order_acquire);
    // After confirmation, the UI only needs to hide the prompt. Avoid a JSON
    // document allocation while holding the shared binding lock on every tick.
    if (bindings.state != 1 && bindings.state != 2) {
        reply = "{\"pending\":false}"; return reply.length() != 0;
    }
    char output[768]; int status = ap01PairingUi(&bindings, now, output, sizeof(output));
    if (status != 200) return false; reply = output; return true;
}
bool Ap01Application::pairingDisplayed(const char *request, uint32_t revision, uint64_t event) {
    BindingGuard guard(port); return ready && guard && ap01PairDisplayed(&bindings, request, revision, event);
}
bool Ap01Application::button(bool pressed, uint64_t event, uint64_t now) {
    BindingGuard guard(port); if (!ready || !guard) return false;
    bindings.maintenance=maintenanceGate.load(std::memory_order_acquire);
    bool pending = bindings.state == 1 || bindings.state == 2;
    if (pending) ap01PairButton(&bindings, pressed, event, now);
    return pending;
}
