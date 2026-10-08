#include "transport_internal.h"
#include "application_internal.h"
#include "clock_internal.h"
#include "diagnostics_internal.h"
#include "display_internal.h"
#include "platform_internal.h"
#include "power_internal.h"
#include "radio_internal.h"
#include "sync_internal.h"

namespace firmware::transport {

State state;

static char        otaPasswordBuf[40] = {0};

static bool        otaUploadDenied = false;

static String         v2SessionNonce;

static bool v2BleFrameActive = false;

static bool v2BleCacheReply = false;

static CompactFrameHeader v2BleFrame;

static uint32_t v2BleSessionGeneration = 0;

static uint64_t v2BleHighestRequest = 0;

static uint8_t v2BleLastDigest[32] = {};

static uint8_t v2BleCurrentDigest[32] = {};

static String v2BleLastFrame;

static bool v2HttpRawOverflow = false;

static uint8_t v2HttpSession[16] = {};

static uint8_t v2HttpOpenNonce[16] = {};

static bool v2HttpOpenValid = false;

static uint64_t v2HttpHighestRequest = 0;

static uint8_t v2HttpLastDigest[32] = {}, v2HttpCurrentDigest[32] = {};

static bool v2HttpCacheReply = false;

static void randomHex(char *out, size_t bytes);
static void setOtaPassword(const char *value);
static void persistAuthToken();
static void rotateAuthToken();
static bool authValid();
static void handleClaim();
static void handleUpdatePage();
static void compactDigest(const String &bytes, uint8_t digest[32]);
static void v2BleNotify(JsonDocument &doc);
static bool v2OwnerOk(const char *bridgeId);
static bool v2HttpBegin(uint16_t op);
static void handleOtaImage();
static void __attribute__((noinline)) handleBleClaim(JsonDocument &doc);
static void __attribute__((noinline)) handleBleWifiCapabilities(
    JsonDocument &doc, const String &identity, uint32_t generation);
static void __attribute__((noinline)) handleBleWifiMigration(
    JsonDocument &doc, const String &identity, uint64_t now, bool allowed);

// Business HTTP requests use the endpoint token provisioned over bonded BLE.
bool endpointTokenAuthorized(String &mac) {
    String header = state.server.header("Authorization");
    if (!header.startsWith("Bearer ")) return false;
    String token = header.substring(7);
    token.trim();
    if (!token.length()) return false;
    EndpointRec rec;
    if (!storeAuthenticate(token,rec)) return false;
    mac=rec.mac; return true;
}

void handleBleEndpoint(const String &json) {
    JsonDocument doc;
    if (deserializeJson(doc, json)) {
        bleNotifyStatus(bleBuildEndpoint(false));
        return;
    }
    if(doc["schema"]!=2 || !doc["port"].is<uint16_t>()){bleNotifyStatus(bleBuildEndpoint(false));return;}
    for(const char *key:{"host","token","bridge_id"}){JsonString value=doc[key].as<JsonString>();if(!value || value.size()!=strlen(value.c_str())){bleNotifyStatus(bleBuildEndpoint(false));return;}}
    const char *host = doc["host"] | "";
    uint16_t port = doc["port"] | 0;
    const char *token = doc["token"] | "";
    if (!strlen(host) || !port) {
        bleNotifyStatus(bleBuildEndpoint(false));
        return;
    }
    String mac = blePeerAddress();
    if (!mac.length()) mac = "unknown";
    String identity=blePeerIdentity(), principal=doc["bridge_id"]|"";
    if (!blePeerIsBonded() || !blePeerIsEncrypted() || !identity.length() ||
        !bleWireText(principal,64,false) || !bleWireText(String(host),64,false) || !bleWireText(String(token),128,false)) {
        bleNotifyStatus(bleBuildEndpoint(false)); return;
    }
    char ns[17];snprintf(ns,sizeof(ns),"%08lx%08lx",(unsigned long)esp_random(),(unsigned long)esp_random());
    const char *failure=storeUpsert(mac,host,port,token,principal,identity,String(ns),power::state.bleRecovery.until>clock::v2NowMs());
    if (failure) { bleNotifyStatus(bleBuildEndpoint(false));return; }
    diagnostics::traceSetTransport(1);
    diagnostics::traceSetStage(TRACE_STAGE_COMMAND);
    diagnostics::traceSetStage(TRACE_STAGE_REPLY);
    bleNotifyStatus(bleBuildEndpoint(true));
}

static void randomHex(char *out, size_t bytes) {
    static const char *hex = "0123456789abcdef";
    for (size_t i = 0; i < bytes; i++) {
        uint32_t r = esp_random();
        out[i * 2]     = hex[(r >> 4) & 0x0F];
        out[i * 2 + 1] = hex[r & 0x0F];
    }
    out[bytes * 2] = '\0';
}

static void setOtaPassword(const char *value) {
    strncpy(otaPasswordBuf, value, sizeof(otaPasswordBuf) - 1);
    otaPasswordBuf[sizeof(otaPasswordBuf) - 1] = '\0';
    ArduinoOTA.setPassword(otaPasswordBuf);
}

// OTA/Wi-Fi operation token. Issued once on the device and persisted in NVS so
// reboots and deep sleep no longer invalidate it; it is disclosed only over the
// bonded BLE link (never over HTTP or serial).
static void persistAuthToken() {
    Preferences p;
    p.begin("auth", false);
    p.putString("token", state.authToken);
    p.end();
}

void loadOrIssueAuthToken() {
    Preferences p;
    p.begin("auth", true);
    state.authToken = p.getString("token", "");
    p.end();
    if (state.authToken.length() != 32) {
        char buf[33];
        randomHex(buf, 16);
        state.authToken = buf;
        persistAuthToken();
        DevLog.println("[auth] token initialized in NVS");
    } else {
        DevLog.println("[auth] token loaded from NVS");
    }
    setOtaPassword(state.authToken.c_str());
}

static void rotateAuthToken() {
    char buf[33];
    randomHex(buf, 16);
    state.authToken = buf;
    persistAuthToken();
    setOtaPassword(state.authToken.c_str());
    DevLog.println("[auth] token rotated");
}

static bool authValid() {
    // No expiry: the token lives in NVS across reboots and deep sleep.
    return state.authToken.length() > 0;
}

// Wi-Fi operations require a token that was negotiated over the bonded BLE
// link; accepted as "Authorization: Bearer <token>" or ?token=<token>.
bool requestAuthorized() {
    if (!authValid()) return false;
    if (state.server.hasHeader("Authorization")) {
        if (state.server.header("Authorization") == String("Bearer ") + state.authToken) return true;
    }
    if (state.server.hasArg("token") && state.server.arg("token") == state.authToken) return true;
    return false;
}

static void handleClaim() {
    if (!authValid() || state.server.header("X-Device-Token") != state.authToken) {
        v2Response(401, "{\"result\":\"rejected\",\"error\":\"unauthorized\",\"stage\":\"claim\"}");
        return;
    }
    JsonDocument doc;
    if (deserializeJson(doc, state.v2HttpBody)) {
        v2Response(400, "{\"result\":\"rejected\",\"error\":\"payload\",\"stage\":\"claim\"}");
        return;
    }
    V2ClaimArgs args = v2PrepareClaim(
        doc["id"].as<String>(), doc["name"].as<String>(), doc["host"].as<String>(),
        doc["port"].as<String>(), doc["lease"].as<String>(), !doc["lease"].isNull(),
        doc["force"] | false, doc["release"] | false);
    if (!args.validId) {
        v2Response(400, "{\"result\":\"rejected\",\"error\":\"args\",\"stage\":\"claim\"}");
        return;
    }
    const OwnerClaimResult reply = ownerApplyClaim(args, true);
    if (reply.newClaim) power::noteActivity("claim");
    JsonDocument result;
    deserializeJson(result, reply.body);
    result["result"] = reply.status == 200 ? "applied" : "rejected";
    result["stage"] = "claim";
    String payload; serializeJson(result, payload);
    v2Response(reply.status, payload);
}

void handleBleAuth(const String &json) {
    JsonDocument doc;
    if (deserializeJson(doc, json) || doc["cmd"].isNull() || strcmp(doc["cmd"] | "", "token")) {
        bleNotifyStatusQuiet(bleBuildAuth(false));
        return;
    }
    if (doc["rotate"] | false) rotateAuthToken();
    else if (!authValid()) loadOrIssueAuthToken();
    diagnostics::traceSetTransport(1);
    diagnostics::traceSetStage(TRACE_STAGE_COMMAND);
    diagnostics::traceSetStage(TRACE_STAGE_REPLY);
    bleNotifyStatusQuiet(bleBuildAuth(true, state.authToken));
}

void handleBleV2Ctrl(const String &json) {
    if (!state.v2BleQueue || json.length() > 8192 || !blePeerIsBonded() || !blePeerIsEncrypted()) return;
    auto *message = new V2BleMessage{json, blePeerAddress(), blePeerIdentity(), bleConnectionGeneration()};
    if (xQueueSend(state.v2BleQueue, &message, 0) != pdTRUE) delete message;
}

static void handleUpdatePage() {
    if (!requestAuthorized()) {
        state.server.send(401, "text/plain", "unauthorized: negotiate a token over BLE first");
        return;
    }
    String page = F("<!DOCTYPE html><html><head><meta charset='utf-8'><title>OTA</title></head><body>"
                    "<h2>Firmware OTA</h2>"
                    "<form method='POST' action='/doUpdate?token=");
    page += state.authToken;
    page += F("' enctype='multipart/form-data'>"
              "<input type='file' name='firmware' accept='.bin'>"
              "<button type='submit'>Upload</button></form></body></html>");
    state.server.send(200, "text/html", page);
}

// Abort a running/half-finished OTA and restore the normal UI. Without this an
// aborted upload (client drop / stall) leaves UpdateClass "already running"
// (every later begin() fails), the OTA PM lock held (no light sleep) and the
// screen stuck on the OTA view -- the 0.14.10 battery OTA hang.
void otaUploadCleanup(const char *reason) {
    DevLog.printf("[ota] abort (%s) running=%d err=%u\n", reason, Update.isRunning() ? 1 : 0,
                  (unsigned)Update.getError());
    Update.abort();
    state.otaInProgress = false;
    power::setOtaLock(false);
    power::setOtaWifiAwake(false);
    otaUploadDenied = false;
    if (display::state.frame) display::renderCurrent();
}

static void compactDigest(const String &bytes, uint8_t digest[32]) {
    mbedtls_sha256(reinterpret_cast<const unsigned char *>(bytes.c_str()), bytes.length(), digest, 0);
}

bool compactSessionFromHex(const String &text, uint8_t out[16]) {
    if (text.length() != 32) return false;
    for (unsigned i = 0; i < 16; ++i) {
        const char a = text[i * 2], b = text[i * 2 + 1];
        if (!isxdigit((unsigned char)a) || !isxdigit((unsigned char)b)) return false;
        auto nibble = [](char c) -> uint8_t {
            return c <= '9' ? c - '0' : (c | 0x20) - 'a' + 10;
        };
        out[i] = (nibble(a) << 4) | nibble(b);
    }
    return true;
}

static void v2BleNotify(JsonDocument &doc) {
    String reply = bleBuildReply(doc, state.v2RequestId, diagnostics::retained.rtcWakeGeneration, diagnostics::retained.rtcTraceCurrentSeq);
    if (v2BleFrameActive) {
        const bool limited = !reply.length() || reply.length() > 512 - COMPACT_FRAME_HEADER;
        if (limited) {
            JsonDocument limit;
            limit["ack"] = doc["ack"] | "command";
            limit["op"] = doc["op"] | "command";
            limit["request_id"] = state.v2RequestId;
            limit["result"] = "unknown";
            limit["error"] = "reply_limit";
            limit["stage"] = "frame";
            limit["query"] = "original_business_id";
            reply = "";
            serializeJson(limit, reply);
        }
        CompactFrameHeader response = v2BleFrame;
        response.kind = 1;
        const char *result = doc["result"] | "";
        response.result = limited ? 3 : !strcmp(result, "accepted") ? 1 :
            !strcmp(result, "unknown") ? 3 :
            !strcmp(result, "rejected") || doc["ok"] == false ? 2 : 0;
        response.payloadLen = reply.length();
        compactSessionFromHex(v2Nonce(), response.session);
        uint8_t header[COMPACT_FRAME_HEADER];
        if (!compactFrameEncode(response, header)) return;
        String bytes(reinterpret_cast<const char *>(header), COMPACT_FRAME_HEADER);
        bytes.concat(reply.c_str(), reply.length());
        if (v2BleCacheReply) {
            v2BleHighestRequest = v2BleFrame.requestNo;
            memcpy(v2BleLastDigest, v2BleCurrentDigest, sizeof(v2BleLastDigest));
            v2BleLastFrame = bytes;
        }
        bleNotifyStatusQuiet(bytes);
        return;
    }
    const char *op = doc["op"] | (doc["ack"] == "wifi" ? "wifi" : "command");
    const bool known = !strcmp(op, "status") || !strcmp(op, "data") || !strcmp(op, "plan") ||
        !strcmp(op, "sync_config") || !strcmp(op, "sync_open") || !strcmp(op, "wifi");
    String correlation = doc["rid"] | "";
    DevLog.printf("[ble-wire] phase=encode op=%s correlation=%08x encoded_bytes=%u reply_bytes=%u max=512 wake=%u/%u result=%s\n",
        known ? op : "command", (unsigned)v2Crc32((const uint8_t *)correlation.c_str(), correlation.length()),
        (unsigned)measureJson(doc), (unsigned)reply.length(), (unsigned)diagnostics::retained.rtcWakeGeneration, (unsigned)diagnostics::retained.rtcTraceCurrentSeq,
        !reply.length() ? "invalid" : measureJson(doc) > BLE_WIRE_MAX ? "fallback_unknown" : "ready");
    bleNotifyStatusQuiet(reply);
}

void v2Response(int status, const String &body) {
    if (!state.v2ReplyOverBle) {
        if (!state.v2HttpFrameActive) { state.server.send(status, "application/json", body); return; }
        String payload = body;
        if (payload.length() > 4096 - COMPACT_FRAME_HEADER) {
            payload = "{\"result\":\"unknown\",\"error\":\"reply_limit\",\"stage\":\"frame\",\"query\":\"original_business_id\"}";
        }
        CompactFrameHeader response = state.v2HttpFrame;
        response.kind = 1;
        response.payloadLen = payload.length();
        memcpy(response.session, v2HttpSession, 16);
        JsonDocument result;
        deserializeJson(result, payload);
        const char *value = result["result"] | "";
        response.result = !strcmp(value, "accepted") ? 1 :
            !strcmp(value, "unknown") ? 3 :
            status >= 400 || !strcmp(value, "rejected") || result["ok"] == false ? 2 : 0;
        uint8_t header[COMPACT_FRAME_HEADER];
        if (!compactFrameEncode(response, header)) {
            state.server.send(500, "application/json", "{\"error\":\"frame_encode\"}"); return;
        }
        String bytes(reinterpret_cast<const char *>(header), COMPACT_FRAME_HEADER);
        bytes.concat(payload.c_str(), payload.length());
        if (v2HttpCacheReply) {
            v2HttpHighestRequest = state.v2HttpFrame.requestNo;
            memcpy(v2HttpLastDigest, v2HttpCurrentDigest, sizeof(v2HttpLastDigest));
            state.v2HttpLastFrame = bytes.length() <= 512 ? bytes : String();
        }
        state.server.send(status, "application/vnd.codex-status.frame", bytes);
        return;
    }
    JsonDocument doc;
    if (deserializeJson(doc, body)) return;
    diagnostics::traceSetTransport(1);
    diagnostics::traceSetStage(TRACE_STAGE_REPLY);
    v2BleNotify(doc);
}

static bool v2OwnerOk(const char *bridgeId) {
    if (!ownerAllows(bridgeId)) {
        v2Response(409,
                    String("{\"result\":\"rejected\",\"error\":\"occupied\",\"owner\":") +
                        ownerJson() + "}");
        DevLog.printf("[platform] rejected: owner conflict id=%s\n", bridgeId ? bridgeId : "?");
        return false;
    }
    return true;
}

void v2Ack(const char *op, const char *result, const char *display,
                  const char *retention, const char *error, int64_t seq, uint64_t planId,
                  const char *context, uint32_t acceptedRemainingS, bool fullSeq) {
    v2Response(200, v2BuildAck(op, result, display, retention, error, seq,
                               planId, context, acceptedRemainingS, FW_TARGET_ID, fullSeq));
}

// Only authenticated status exposes this boot session nonce.
const String &v2Nonce() {
    if (!v2SessionNonce.length()) {
        char nonce[33];
        snprintf(nonce, sizeof(nonce), "%08x%08x%08x%08x", (unsigned)esp_random(),
                 (unsigned)esp_random(), (unsigned)esp_random(), (unsigned)esp_random());
        v2SessionNonce = nonce;
    }
    return v2SessionNonce;
}

bool v2Command(const String &body, JsonDocument &doc) {
    if (v2ParseCommand(body, doc)) {
        v2Ack("command", "rejected", "unchanged", "ram", "json", -1, 0, nullptr, UINT32_MAX);
        return false;
    }
    const char *request = doc["request_id"] | "";
    state.v2RequestId = request;
    String currentMac = radio::macText();
    V2CommandSessionDecision session = v2CheckCommandSession(doc, currentMac, nullptr);
    if (session.needsNonce) {
        const String &nonce = v2Nonce();
        session = v2CheckCommandSession(doc, currentMac, &nonce);
    }
    if (!session.accepted) {
        v2Ack("command", "rejected", "unchanged", "ram", "session", -1, 0, nullptr, UINT32_MAX);
        return false;
    }
    if (!v2OwnerOk(doc["bridge_id"] | "")) return false;
    diagnostics::traceSetTransport(state.v2ReplyOverBle ? 1 : 2);
    diagnostics::traceSetRequest(request);
    diagnostics::traceSetStage(TRACE_STAGE_COMMAND);
    if (!state.v2ReplyOverBle) diagnostics::traceMarkHttp(true, 200);
    return true;
}

void v2HttpRawHandler() {
    HTTPRaw &raw = state.server.raw();
    if (raw.status == RAW_START) { state.v2HttpRaw = ""; v2HttpRawOverflow = false; return; }
    if (raw.status == RAW_ABORTED) { state.v2HttpRaw = ""; v2HttpRawOverflow = true; return; }
    if (raw.status == RAW_WRITE) {
        if (v2HttpRawOverflow) return;
        if (state.v2HttpRaw.length() + raw.currentSize > 4096 ||
            !state.v2HttpRaw.concat(reinterpret_cast<const char *>(raw.buf), raw.currentSize)) {
            state.v2HttpRaw = "";
            v2HttpRawOverflow = true;
        }
    }
}

static bool v2HttpBegin(uint16_t op) {
    state.v2HttpFrameActive = false;
    v2HttpCacheReply = false;
    if (state.server.header("Content-Type") != "application/vnd.codex-status.frame") {
        state.server.send(415, "application/json", "{\"error\":\"content_type\",\"stage\":\"frame\"}");
        return false;
    }
    CompactFrameHeader request;
    if (v2HttpRawOverflow || !compactFrameDecode(reinterpret_cast<const uint8_t *>(state.v2HttpRaw.c_str()),
                            state.v2HttpRaw.length(), request, 4096) ||
        request.kind != 0 || request.op != op) {
        state.server.send(400, "application/json", "{\"error\":\"protocol_mismatch\",\"stage\":\"frame\"}");
        return false;
    }
    String token = state.server.header("Authorization");
    if (!token.startsWith("Bearer ")) {
        state.server.send(401, "application/json", "{\"error\":\"unauthorized\"}"); return false;
    }
    token.remove(0, 7);
    EndpointRec endpoint;
    if (!storeAuthenticate(token, endpoint) &&
        !(op == 19 && authValid() && token == state.authToken)) {
        state.server.send(401, "application/json", "{\"error\":\"unauthorized\"}"); return false;
    }
    state.v2HttpFrame = request;
    state.v2HttpFrameActive = true;
    compactDigest(state.v2HttpRaw, v2HttpCurrentDigest);
    state.v2HttpBody = op == 11 ? String() :
        String(state.v2HttpRaw.c_str() + COMPACT_FRAME_HEADER, request.payloadLen);
    uint8_t zeroSession[16] = {};
    bool imageOpen = false;
    if (op == 19) {
        JsonDocument input;
        imageOpen = !deserializeJson(input, state.v2HttpBody) && input["open"] == true;
    }
    if (op == 1 || imageOpen) {
        uint8_t zero[16] = {};
        if (!memcmp(request.session, zero, 16)) {
            v2Response(409, "{\"result\":\"rejected\",\"error\":\"session\"}"); return false;
        }
        if (v2HttpOpenValid && state.v2HttpToken == token &&
            !memcmp(request.session, v2HttpOpenNonce, 16)) {
            if (request.requestNo == v2HttpHighestRequest && state.v2HttpLastFrame.length() &&
                !memcmp(v2HttpCurrentDigest, v2HttpLastDigest, sizeof(v2HttpLastDigest))) {
                state.server.send(200, "application/vnd.codex-status.frame", state.v2HttpLastFrame);
            } else {
                v2Response(409, request.requestNo == v2HttpHighestRequest ?
                    "{\"result\":\"rejected\",\"error\":\"request_conflict\",\"query\":\"original_business_id\"}" :
                    "{\"result\":\"rejected\",\"error\":\"retired\",\"query\":\"original_business_id\"}");
            }
            return false;
        }
        memcpy(v2HttpOpenNonce, request.session, 16);
        v2HttpOpenValid = true;
        for (size_t i = 0; i < 16; i += 4) {
            uint32_t random = esp_random();
            memcpy(v2HttpSession + i, &random, 4);
        }
        state.v2HttpToken = token;
        v2HttpHighestRequest = 0;
        state.v2HttpLastFrame = "";
    } else if (state.v2HttpToken != token || memcmp(request.session, v2HttpSession, 16)) {
        v2Response(409, "{\"result\":\"rejected\",\"error\":\"session\"}"); return false;
    }
    uint8_t zero[16] = {};
    if (memcmp(request.context, zero, 16) &&
        (strlen(platform::state.v2Profile.contextId) != 16 || memcmp(request.context, platform::state.v2Profile.contextId, 16))) {
        v2Response(409, "{\"result\":\"rejected\",\"error\":\"context\"}"); return false;
    }
    if (request.requestNo <= v2HttpHighestRequest) {
        if (request.requestNo == v2HttpHighestRequest && state.v2HttpLastFrame.length() &&
            !memcmp(v2HttpCurrentDigest, v2HttpLastDigest, sizeof(v2HttpLastDigest))) {
            state.server.send(200, "application/vnd.codex-status.frame", state.v2HttpLastFrame);
        } else {
            v2Response(409, request.requestNo == v2HttpHighestRequest ?
                "{\"result\":\"rejected\",\"error\":\"request_conflict\",\"query\":\"original_business_id\"}" :
                "{\"result\":\"rejected\",\"error\":\"retired\",\"query\":\"original_business_id\"}");
        }
        return false;
    }
    v2HttpCacheReply = true;
    return true;
}

void v2HttpRoute(uint16_t op, void (*handler)()) {
    if (v2HttpBegin(op)) handler();
    state.v2HttpFrameActive = false;
    v2HttpCacheReply = false;
    state.v2HttpBody = "";
    state.v2HttpRaw = "";
}

// Independent of sync diagnostics and claim ownership. The caller chooses the
// exact prefix length from the frozen ROM; the Bridge compares the returned
// hash with that file after reboot.
static void handleOtaImage() {
    if (!requestAuthorized()) {
        v2Response(401, "{\"error\":\"unauthorized\"}"); return;
    }
    const esp_partition_t *running = esp_ota_get_running_partition();
    JsonDocument input;
    if (deserializeJson(input, state.v2HttpBody) || !input["image_bytes"].is<uint32_t>()) {
        v2Response(400, "{\"error\":\"range\"}"); return;
    }
    uint32_t length = input["image_bytes"].as<uint32_t>();
    if (!running || !length || length > running->size) {
        v2Response(400, "{\"error\":\"range\"}"); return;
    }
    mbedtls_sha256_context sha;
    mbedtls_sha256_init(&sha);
    mbedtls_sha256_starts(&sha, 0);
    std::vector<uint8_t> block(4096);
    uint8_t hash[32];
    bool okay = true;
    for (uint32_t offset = 0; offset < length; offset += block.size()) {
        size_t count = std::min((size_t)(length - offset), block.size());
        if (esp_partition_read(running, offset, block.data(), count) != ESP_OK) { okay = false; break; }
        mbedtls_sha256_update(&sha, block.data(), count);
    }
    if (okay) mbedtls_sha256_finish(&sha, hash);
    mbedtls_sha256_free(&sha);
    if (!okay) { v2Response(503, "{\"error\":\"storage\"}"); return; }
    char hex[65];
    syncHex(hash, 32, hex);
    JsonDocument reply;
    reply["algorithm"] = "sha256-running-prefix-v1";
    reply["image_bytes"] = length;
    reply["sha256"] = hex;
    reply["device_mac"] = radio::macText();
    reply["running_slot"] = running->label;
    reply["fw_target"] = FW_TARGET_ID;
    reply["fw"] = application::state.firmwareVersion;
    String body;
    serializeJson(reply, body);
    v2Response(200, body);
}

// Keep claim temporaries off the already busy BLE dispatcher stack frame.
static void __attribute__((noinline)) handleBleClaim(JsonDocument &doc) {
    const V2CommandSessionDecision session = v2CheckCommandSession(doc, radio::macText(), &v2Nonce());
    const JsonString operationToken = doc["operation_token"].as<JsonString>();
    const bool allowed = session.accepted && authValid() && operationToken &&
        operationToken.size() == strlen(operationToken.c_str()) &&
        state.authToken == operationToken.c_str() &&
        doc["force"].isNull() && doc["release"].isNull();
    const V2ClaimArgs args = v2PrepareClaim(doc["bridge_id"] | "",
        doc["name"] | "", doc["host"] | "", String(doc["port"] | 0u),
        String(doc["lease_s"] | 300u), true, false, false);
    const OwnerClaimResult claim = ownerApplyClaim(args, allowed);
    if (claim.newClaim) power::noteActivity("claim");
    JsonDocument out;
    out["op"] = "claim"; out["result"] = claim.status == 200 ? "applied" : "rejected";
    out["stage"] = "claim";
    if (claim.status != 200) {
        JsonDocument error; deserializeJson(error, claim.body);
        out["error"] = error["error"] | "claim_failed";
    }
    OwnerRec current;
    out["owner_id"] = ownerGet(current) ? current.id : "";
    out["expires_in_s"] = ownerRemainingS();
    v2BleNotify(out);
}

static void __attribute__((noinline)) handleBleWifiCapabilities(
    JsonDocument &doc, const String &identity, uint32_t generation) {
    JsonDocument caps;
    caps["fw_target"] = FW_TARGET_ID; caps["render_target"] = RENDER_TARGET_ID;
    caps["width"] = TARGET_WIDTH; caps["height"] = TARGET_HEIGHT;
    caps["pixel_format"] = TARGET_PIXEL_FORMAT; caps["colors"] = TARGET_COLORS;
    caps["partial"] = bool(TARGET_PARTIAL); caps["hardware_verified"] = bool(TARGET_VERIFIED);
    caps["max_templates"] = 8; caps["max_bundle_bytes"] = BS_MAX_BUNDLE_BYTES;
    caps["compiler_abi"] = CT_ABI;
#ifdef CODEX_TARGET_NOTE4
    caps["bundle_font_protocol"] = 1;
#endif
    String text; serializeJson(caps, text);
    JsonDocument reply;
    deserializeJson(reply, wifiSetupReadPage(doc, identity, generation, text));
    v2BleNotify(reply);
}

static void __attribute__((noinline)) handleBleWifiMigration(
    JsonDocument &doc, const String &identity, uint64_t now, bool allowed) {
    EndpointRec mapping;
    if (!storeAuthenticate(doc["token"] | "", mapping) || !allowed) {
        JsonDocument out;
        out["ack"] = "wifi"; out["request_id"] = doc["request_id"];
        out["device_mac"] = radio::macText(); out["ok"] = false; out["error"] = "claim_required";
        v2BleNotify(out); return;
    }
    String endpointNamespace = mapping.nameSpace;
    // Resolve an earlier uncertain commit from durable storage before retry.
    if (!wifiSetupBusy()) wifiSchemesBegin();
    std::unique_ptr<WifiNetwork[]> slots(new(std::nothrow) WifiNetwork[MAX_SLOTS]{});
    if (!slots) {
        JsonDocument out; out["ack"] = "wifi"; out["request_id"] = doc["request_id"];
        out["device_mac"] = radio::macText(); out["ok"] = false; out["error"] = "allocation";
        v2BleNotify(out); return;
    }
    size_t count = 0;
    String preferred = WiFi.status() == WL_CONNECTED ? WiFi.SSID() : String();
    Preferences oldPrefs;
    bool opened = oldPrefs.begin("wifi", true);
    if (opened) {
        uint8_t last = oldPrefs.getUChar("last", 0);
        for (int i = 0; i < MAX_SLOTS; ++i) {
            String ssid = oldPrefs.getString(("s" + String(i)).c_str(), "");
            if (!ssid.length()) continue;
            String password = oldPrefs.getString(("p" + String(i)).c_str(), "");
            auto &n = slots[count++];
            n.id.set((String("wifi-slot-") + String(i)).c_str());
            // Length errors must reject rather than truncate credentials.
            if (ssid.length() > 32 || password.length() > 64 ||
                strlen(ssid.c_str()) != ssid.length() || strlen(password.c_str()) != password.length()) n.id.clear();
            else { n.ssid.set(ssid.c_str()); n.password.set(password.c_str()); }
            if (!preferred.length() && i == last) preferred = ssid;
            if (password.length()) memset(const_cast<char *>(password.c_str()), 0, password.length());
        }
        oldPrefs.end();
    }
    if (!preferred.length() && count) preferred = slots[0].ssid.c_str();
    JsonDocument out;
    if (!opened) {
        out["ack"] = "wifi"; out["request_id"] = doc["request_id"];
        out["device_mac"] = radio::macText(); out["ok"] = false; out["error"] = "storage";
    } else deserializeJson(out, wifiSchemesMigrate(doc, endpointNamespace, slots.get(), count,
        preferred, now, power::state.bleRecovery.until, allowed));
    if (out["persisted"] == true) {
        bool retired = oldPrefs.begin("wifi", false);
        if (retired) {
            for (int i = 0; i < MAX_SLOTS; ++i) {
                for (const char *prefix : {"s", "p"}) {
                    String key = String(prefix) + String(i);
                    if (oldPrefs.isKey(key.c_str()) && !oldPrefs.remove(key.c_str())) retired = false;
                }
            }
            oldPrefs.end();
        }
        out["retired"] = retired;
        if (!retired) { out["ok"] = false; out["error"] = "retirement_pending"; }
    }
    v2BleNotify(out);
}

void serviceV2Ble() {
    if (!state.v2BleQueue) return;
    V2BleMessage *message = nullptr;
    if (xQueueReceive(state.v2BleQueue, &message, 0) != pdTRUE) return;
    String body = message->body, peer = message->peer;
    String identity = message->identity;
    uint32_t generation = message->generation;
    delete message;
    if (!blePeerIsBonded() || !blePeerIsEncrypted() || peer != blePeerAddress() ||
        identity != blePeerIdentity() || generation != bleConnectionGeneration()) return;
    struct FrameScope { ~FrameScope() { v2BleFrameActive = false; v2BleCacheReply = false; state.v2ReplyOverBle = false; } } frameScope;
    if (generation != v2BleSessionGeneration) {
        v2BleSessionGeneration = generation;
        v2SessionNonce = "";
        v2BleHighestRequest = 0;
        memset(v2BleLastDigest, 0, sizeof(v2BleLastDigest));
        v2BleLastFrame = "";
    }
    CompactFrameHeader frame;
    if (!compactFrameDecode(reinterpret_cast<const uint8_t *>(body.c_str()), body.length(), frame, 8192) ||
        frame.kind != 0) return;
    v2BleFrame = frame;
    v2BleFrameActive = true;
    state.v2ReplyOverBle = true;
    compactDigest(body, v2BleCurrentDigest);
    body = String(body.c_str() + COMPACT_FRAME_HEADER, frame.payloadLen);
    JsonDocument doc;
    if (deserializeJson(doc, body)) {
        v2Ack("command", "rejected", "unchanged", "ram", "payload", -1, 0, nullptr, UINT32_MAX);
        return;
    }
    const char *frameOp = doc["op"] | "";
    if (compactOperation(frameOp) != frame.op) {
        v2Ack("command", "rejected", "unchanged", "ram", "operation", -1, 0, nullptr, UINT32_MAX);
        return;
    }
    uint8_t expectedSession[16] = {};
    if (frame.op == 1) {
        uint8_t empty[16] = {};
        if (memcmp(frame.session, empty, 16)) {
            v2Ack("command", "rejected", "unchanged", "ram", "session", -1, 0, nullptr, UINT32_MAX);
            return;
        }
    } else if (!compactSessionFromHex(v2SessionNonce, expectedSession) ||
               memcmp(frame.session, expectedSession, 16)) {
        v2Ack("command", "rejected", "unchanged", "ram", "session", -1, 0, nullptr, UINT32_MAX);
        return;
    }
    uint8_t noContext[16] = {};
    if (memcmp(frame.context, noContext, 16) &&
        (strlen(platform::state.v2Profile.contextId) != 16 || memcmp(frame.context, platform::state.v2Profile.contextId, 16))) {
        v2Ack("command", "rejected", "unchanged", "ram", "context", -1, 0, nullptr, UINT32_MAX);
        return;
    }
    if (frame.requestNo <= v2BleHighestRequest) {
        if (frame.requestNo == v2BleHighestRequest && v2BleLastFrame.length() &&
            !memcmp(v2BleCurrentDigest, v2BleLastDigest, sizeof(v2BleLastDigest))) {
            bleNotifyStatusQuiet(v2BleLastFrame);
        } else {
            v2Ack("command", "rejected", "unchanged", "ram",
                frame.requestNo == v2BleHighestRequest ? "request_conflict" : "retired",
                -1, 0, nullptr, UINT32_MAX);
        }
        return;
    }
    v2BleCacheReply = true;
    JsonString request = doc["request_id"].as<JsonString>();
    if (!request || request.size() != strlen(request.c_str())) return;
    state.v2RequestId = doc["request_id"].as<String>();
    if (!bleWireText(state.v2RequestId, 64, false)) return;
    bool authenticated = false;
    for (int i = 0; i < storeCount(); ++i) {
        EndpointRec endpoint;
        if (storeGet(i, endpoint) && endpoint.token.length() &&
            endpoint.token == (doc["token"] | "") && endpoint.identity == identity &&
            endpoint.principal == (doc["bridge_id"]|"")) {
            authenticated = true; break;
        }
    }
    String operation = doc["op"] | "";
    if (operation.startsWith("wifi_")) {
        // Physical recovery is bond-authorized, independently of HTTP owner,
        // endpoint token and data contact. Ordinary commands remain unchanged.
        if (body.length() > 4096 || String(doc["device_mac"] | "") != radio::macText() ||
            !doc["protocol"].isNull() || !doc["rv"].isNull()) {
            JsonDocument out;
            out["ack"] = "wifi"; out["request_id"] = doc["request_id"];
            out["device_mac"] = radio::macText(); out["ok"] = false; out["error"] = "mac";
            v2BleNotify(out);
            return;
        }
        OwnerRec owner;
        bool allowed = authenticated && ownerGet(owner) && owner.id == (doc["bridge_id"] | "");
        const uint64_t now = clock::v2NowMs();
        const uint64_t admittedUntil = power::state.v2Plan.lightActive(now) ? power::state.v2Plan.deadlineMs() :
            sync::state.syncOpenThisWake ? sync::state.syncOpenAnsweredAt + 90000 : 0;
        const uint64_t wifiWindowUntil = radio::state.v2Rv.active() ? std::min(admittedUntil, radio::state.v2Rv.deadline()) : admittedUntil;
        if (!authenticated) {
            JsonDocument out; out["ack"]="wifi"; out["request_id"]=doc["request_id"];
            out["device_mac"]=radio::macText(); out["ok"]=false; out["error"]=authenticated ? "occupied" : "unauthorized";
            v2BleNotify(out); return;
        }
        if (operation=="wifi_command" || operation=="wifi_save" || operation=="wifi_result" || operation=="wifi_result_ack" ||
            operation=="wifi_match" || operation=="wifi_active" || operation=="wifi_trial" || operation=="wifi_activate" ||
            operation=="wifi_setup_status" || operation=="wifi_setup_cancel" || operation=="wifi_snapshot" || operation=="wifi_status" || operation=="wifi_scheme_get") {
            EndpointRec endpoint;JsonDocument out;
            if(!storeAuthenticate(doc["token"]|"",endpoint) || endpoint.identity!=identity || endpoint.principal!=(doc["bridge_id"]|"")) {
                out["ok"]=false;out["error"]="binding_required";out["device_mac"]=radio::macText();
            } else {
                auto context=radio::wifiControlContext(endpoint,doc);context.generation=generation;
                if(operation=="wifi_command")doc["op"]=doc["command"];
                if(operation=="wifi_status"){
                    JsonDocument status;radio::wifiDeviceStatus(status,&endpoint);status["session_nonce"]=v2Nonce();status["ok"]=true;
                    String text;serializeJson(status,text);deserializeJson(out,wifiSetupReadPage(doc,endpoint.nameSpace,generation,text));
                }
                else deserializeJson(out,wifiControlCommand(doc,context,now));
            }
            out["ack"]="wifi";out["op"]="wifi";out["request_id"]=doc["request_id"];
            if(wifiSetupBusy())radio::state.wifiUp=false;
            v2BleNotify(out);return;
        }
        if (operation == "wifi_authorize") {
            String result = wifiManagementAuthorize(doc, identity, now, power::state.bleRecovery.until,
                authenticated && allowed ? wifiWindowUntil : 0);
            JsonDocument out; deserializeJson(out, result); v2BleNotify(out); return;
        }
        if (operation == "wifi_owner") {
            JsonDocument out;
            out["ack"] = "wifi"; out["request_id"] = doc["request_id"];
            out["device_mac"] = radio::macText(); out["ok"] = true;
            OwnerRec current;
            if (ownerGet(current)) out["owner_id"] = current.id;
            v2BleNotify(out);
            return;
        }
        if (operation == "wifi_capabilities") {
            handleBleWifiCapabilities(doc, identity, generation);
            return;
        }
        if (operation == "wifi_migrate") {
            handleBleWifiMigration(doc, identity, now, allowed);
            return;
        }
        if(operation=="wifi_setup" || operation=="wifi_select" || operation=="wifi_scheme_delete") {
            JsonDocument out;out["ack"]="wifi";out["request_id"]=doc["request_id"];out["device_mac"]=radio::macText();out["ok"]=false;out["error"]="retired_operation";v2BleNotify(out);return;
        }
        String result = wifiSetupCommand(doc, identity, generation, now, power::state.bleRecovery.until,
            authenticated && allowed ? wifiWindowUntil : 0, allowed);
        if (wifiSetupBusy()) radio::state.wifiUp = false;
        if (operation == "wifi_scheme_delete") radio::loadPreferredWifiCredentials();
        JsonDocument reply; deserializeJson(reply, result);
        v2BleNotify(reply);
        return;
    }
    state.v2ReplyOverBle = true;
    if (!doc["protocol"].isNull() || !doc["rv"].isNull()) {
        v2Ack("command", "rejected", "unchanged", "ram",
              "retired_protocol", -1, 0, nullptr, UINT32_MAX);
        state.v2ReplyOverBle = false; state.v2RequestId = "";
        return;
    }
    if (!authenticated) {
        v2Ack("command", "rejected", "unchanged", "ram",
              "unauthorized", -1, 0, nullptr, UINT32_MAX);
    } else {
        diagnostics::traceSetTransport(1);
        diagnostics::traceSetRequest(doc["request_id"] | "");
        diagnostics::traceSetStage(TRACE_STAGE_COMMAND);
        // Plan C: the bridge stamps server_time/tz_offset_min into every
        // rendezvous command (status/plan/data); adopt them before any handler
        // renders or replies. Missing fields keep the local RTC fallback.
        if (!doc["server_time"].isNull()) clock::adoptServerTimeForce(doc);
        const char *op = doc["op"] | "";
        if (!strcmp(op, "status")) {
            // Status itself is authenticated; later commands validate their
            // own session before changing data freshness or device state.
            // Status is only observation; Data/Plan retain their own contact clock.
            if (!radio::state.v2InRendezvous && !radio::state.v2RvBleReleased) radio::v2RendezvousBegin();
            BleStatusSnapshot snapshot;
            snapshot.mac = radio::macText(); snapshot.nonce = v2Nonce();
            snapshot.rendezvousVersion = 1;
            snapshot.context = platform::state.v2Profile.contextId;
            snapshot.activeTemplate = platform::state.v2Profile.count ? platform::state.v2Profile.ids[platform::state.v2Profile.initial] : "";
            snapshot.job = platform::state.v2Profile.jobId; snapshot.appliedSeq = platform::state.v2DataSeq.appliedSeq();
            snapshot.commitSeq = bsCommitSeq();
            snapshot.light = power::state.v2Plan.lightActive(clock::v2NowMs()); snapshot.planId = power::state.v2Plan.acceptedId();
            snapshot.remainingS = power::state.v2Plan.remainingS(clock::v2NowMs());
            snapshot.provisionalS = power::state.v2Provisional ? V2PlanState::bootProvisionalRemaining(power::state.v2BootMs, clock::v2NowMs()) : 0;
            snapshot.syncAvailable = syncStoreAvailable();
            if (snapshot.syncAvailable) {
                OwnerRec current;
                snapshot.syncEnabled = sync::state.syncEnabled && ownerGet(current) && current.id == sync::state.syncOwner;
                snapshot.rounds = sync::retained.syncRtc.rounds; snapshot.due = (sync::retained.syncRtc.flags & 1) != 0;
                snapshot.retrySkip = sync::state.syncSkippedThisWake ? sync::state.syncVisibleSkip : sync::retained.syncRtc.retrySkip;
                SyncStoreBatch batch; snapshot.pending = syncStoreActive(batch);
            }
            v2Response(200, bleBuildStatus(snapshot));
        } else if (!strcmp(op, "claim")) {
            handleBleClaim(doc);
        } else if (!strcmp(op, "sync_config")) {
            OwnerRec current;
            V2CommandSessionDecision session = v2CheckCommandSession(doc, radio::macText(), &v2Nonce());
            if (!session.accepted || !doc["enabled"].is<bool>() || !ownerGet(current) ||
                current.id != (doc["bridge_id"] | "") || !syncStoreAvailable()) {
                v2Ack("sync_config", "rejected", "unchanged", "ram", "shape", -1, 0,
                      nullptr, UINT32_MAX);
            } else {
                bool changedOwner = sync::state.syncOwner.length() && sync::state.syncOwner != current.id;
                bool ready = !changedOwner || syncStoreChangeOwner();
                if (ready && changedOwner) {
                    syncAddReason(sync::retained.syncRtc, 1u);
                }
                Preferences p;
                bool saved = ready && p.begin("sync-v1", false);
                if (saved) {
                    saved = p.putBool("enabled", doc["enabled"].as<bool>()) == 1 &&
                            p.putString("owner", current.id) == current.id.length();
                    p.end();
                }
                if (saved) {
                    sync::state.syncEnabled = doc["enabled"].as<bool>();
                    sync::state.syncOwner = current.id;
                }
                v2Ack("sync_config", saved ? "applied" : "rejected", "unchanged",
                      saved ? "flash" : "ram", saved ? nullptr : "storage", -1, 0,
                      nullptr, UINT32_MAX);
            }
        } else if (!strcmp(op, "sync_open")) {
            OwnerRec current;
            V2CommandSessionDecision session = v2CheckCommandSession(doc, radio::macText(), &v2Nonce());
            String openId = doc["open_id"] | "";
            String reason = doc["reason"] | "";
            bool idValid = openId.length() == 32;
            for (size_t i = 0; i < openId.length(); ++i)
                if (!isxdigit((unsigned char)openId[i])) idValid = false;
            bool wakeValid = (doc["wake_generation"] | 0u) == diagnostics::retained.rtcWakeGeneration &&
                             (doc["wake_seq"] | 0u) == diagnostics::retained.rtcTraceCurrentSeq;
            SyncStoreBatch pending;
            bool retry = reason == "retry" &&
                         (syncStoreActive(pending) || (sync::retained.syncRtc.flags & 4) || sync::retained.syncRtc.reasonBits);
            bool periodic = reason == "periodic" && sync::retained.syncRtc.rounds == 15 && (sync::retained.syncRtc.flags & 1);
            const char *error = !session.accepted || !idValid || !wakeValid ? "session" :
                                !ownerGet(current) ? "claim_required" :
                                current.id != (doc["bridge_id"] | "") ? "occupied" :
                                !sync::state.syncEnabled || sync::state.syncOwner != current.id ? "disabled" :
                                v2BatteryPowerOff(power::state.plugged, power::batteryPercent()) ? "low_battery" :
                                sync::state.syncSkippedThisWake || sync::retained.syncRtc.retrySkip || (!periodic && !retry) ? "range" :
                                sync::state.syncOpenThisWake &&
                                (openId != sync::state.syncOpenId || reason != sync::state.syncOpenReason) ? "batch_conflict" : nullptr;
            if (error) {
                if (!strcmp(error, "low_battery")) sync::syncRecordOutcome(2, 7);
                v2Ack("sync_open", "rejected", "unchanged", "ram", error, -1, 0,
                      nullptr, UINT32_MAX);
            } else {
                if (!sync::state.syncOpenThisWake) {
                    sync::state.syncOpenThisWake = true;
                    sync::state.syncBlockedError = 0;
                    sync::state.syncOpenId = openId;
                    sync::state.syncOpenReason = reason;
                    sync::state.syncLastProgressMs = millis();
                    sync::state.syncOpenAnsweredAt = clock::v2NowMs();
                }
                v2Ack("sync_open", "applied", "unchanged", "ram", nullptr, -1, 0,
                      nullptr, UINT32_MAX);
            }
        } else if (!strcmp(op, "network_start")) {
            JsonDocument request;
            if (!v2Command(body, request)) { state.v2ReplyOverBle = false; return; }
            OwnerRec current;
            const uint64_t now = clock::v2NowMs();
            const bool owned = ownerGet(current) && current.id == String(request["bridge_id"] | "");
            const uint64_t legalUntil = power::state.v2Plan.lightActive(now) ? power::state.v2Plan.deadlineMs() :
                sync::state.syncOpenThisWake ? sync::state.syncOpenAnsweredAt + 90000 : 0;
            IPAddress bridge;
            const char *addr = request["bridge_addr"] | "";
            const uint32_t port = request["bridge_port"] | 0u;
            const uint64_t requestNo = request["origin_request_no"] | 0ULL;
            int8_t endpointIndex = -1;
            for (int i = 0; i < storeCount(); ++i) {
                EndpointRec endpoint;
                if (storeGet(i, endpoint) && endpoint.principal == current.id &&
                    endpoint.token == String(request["token"] | "")) { endpointIndex = i; break; }
            }
            const char *error = !owned ? "claim_required" :
                !radio::state.v2InRendezvous || !radio::state.v2Rv.hasPlan() || legalUntil <= now ||
                (request["plan_id"] | 0ULL) != power::state.v2Plan.acceptedId() ? "plan_window" :
                !bridge.fromString(addr) || bridge[0] == 0 || bridge[0] == 127 ||
                bridge[0] >= 224 || !port || port > UINT16_MAX || !requestNo ? "report_route" :
                endpointIndex < 0 ? "report_binding" : nullptr;
            if (!error && !radio::state.v2RvNetworkRequested) {
                radio::state.readyReport.bridge = bridge; radio::state.readyReport.port = (uint16_t)port;
                radio::state.readyReport.endpointIndex = endpointIndex;
                radio::state.readyReport.principal = current.id;
                radio::state.readyReport.originSession = radio::state.v2RvId;
                radio::state.readyReport.originRequestNo = requestNo;
                radio::state.readyReport.planId = power::state.v2Plan.acceptedId();
                radio::state.v2RvNetworkRequested = true;
                radio::state.v2RvBleReleaseAtMs = now + RV_ACK_GRACE_MS;
            }
            v2Ack("network_start", error ? "rejected" : "accepted", "unchanged", "ram",
                  error, -1, power::state.v2Plan.acceptedId(), platform::state.v2Profile.contextId, UINT32_MAX);
        } else if (!strcmp(op, "history")) {
            if (sync::state.v2HistorySyncMs) sync::state.v2HistorySyncUntilMs = clock::v2NowMs() + sync::state.v2HistorySyncMs;
            const uint32_t since = doc["since"] | 0u;
            uint8_t limit = doc["limit"] | 8u;
            if (!limit) limit = 1;
            if (limit > HIST_CAP) limit = HIST_CAP;
            bool more = false;
            // Keep room for the authenticated ACK envelope and request id;
            // never let diagnostics consume the normal status/data path.
            String out = diagnostics::traceHistoryJson(since, limit, 350, more);
            v2Response(200, out);
        } else if (!strcmp(op, "data")) {
            platform::applyV2Data(body);
        } else if (!strcmp(op, "plan")) {
            platform::applyV2Plan(body);
        } else {
            v2Ack("command", "rejected", "unchanged", "ram", "http_required", -1, 0, nullptr, UINT32_MAX);
        }
    }
    state.v2ReplyOverBle = false;
    state.v2RequestId = "";
}

void registerHttpRoutes() {
    static bool registered = false;
    if (registered) return;
    registered = true;
    const char *bundleHeaders[] = {"X-Request-Id", "X-Session-Nonce", "X-Offset", "Authorization", "Content-Type", "X-Device-Token"};
    state.server.collectHeaders(bundleHeaders, 6);
    state.server.on("/", HTTP_GET, diagnostics::handleStatus);
    state.server.on("/status.json", HTTP_GET, diagnostics::handleStatusJson);
    state.server.on("/log", HTTP_GET, diagnostics::handleLog);
    state.server.on("/history", HTTP_GET, diagnostics::handleHistory);
    state.server.on("/pmstats", HTTP_GET, diagnostics::handlePmStats);
    state.server.on("/frame", HTTP_GET, diagnostics::handleFrame);
    state.server.on("/diag", HTTP_POST, diagnostics::handleDiag);
    state.server.on("/claim", HTTP_POST, [] { v2HttpRoute(4, handleClaim); }, v2HttpRawHandler);
    // v2 platform protocol: authenticated business endpoints (token + owner);
    // none of them extend the light deadline implicitly.
    state.server.on("/api/status", HTTP_POST, [] { v2HttpRoute(1, diagnostics::handleV2Status); }, v2HttpRawHandler);
    state.server.on("/api/wifi/snapshot", HTTP_POST, [] { v2HttpRoute(14, [] { radio::handleWifiManagement("snapshot"); }); }, v2HttpRawHandler);
    state.server.on("/api/wifi/apply", HTTP_POST, [] { v2HttpRoute(15, [] { radio::handleWifiManagement("apply"); }); }, v2HttpRawHandler);
    state.server.on("/api/wifi/result", HTTP_POST, [] { v2HttpRoute(16, [] { radio::handleWifiManagement("result"); }); }, v2HttpRawHandler);
    state.server.on("/api/wifi/result_ack", HTTP_POST, [] { v2HttpRoute(17, [] { radio::handleWifiManagement("result_ack"); }); }, v2HttpRawHandler);
    state.server.on("/api/wifi/command", HTTP_POST, [] { v2HttpRoute(18, [] { radio::handleWifiManagement("command"); }); }, v2HttpRawHandler);
    state.server.on("/api/sync/begin", HTTP_POST, [] { v2HttpRoute(6, sync::handleSyncBegin); }, v2HttpRawHandler);
    state.server.on("/api/sync/page", HTTP_POST, [] { v2HttpRoute(7, sync::handleSyncPage); }, v2HttpRawHandler);
    state.server.on("/api/sync/ack", HTTP_POST, [] { v2HttpRoute(8, sync::handleSyncAck); }, v2HttpRawHandler);
    state.server.on("/api/sync/complete", HTTP_POST, [] { v2HttpRoute(9, sync::handleSyncComplete); }, v2HttpRawHandler);
    state.server.on("/api/ota/image", HTTP_POST, [] { v2HttpRoute(19, handleOtaImage); }, v2HttpRawHandler);
    state.server.on("/api/data", HTTP_POST, [] { v2HttpRoute(2, platform::handleV2Data); }, v2HttpRawHandler);
    state.server.on("/api/plan", HTTP_POST, [] { v2HttpRoute(3, platform::handleV2Plan); }, v2HttpRawHandler);
    state.server.on("/api/activate", HTTP_POST, [] { v2HttpRoute(13, platform::handleV2Activate); }, v2HttpRawHandler);

    state.server.on("/api/bundle/begin", HTTP_POST, [] { v2HttpRoute(10, platform::handleV2BundleBegin); }, v2HttpRawHandler);
    state.server.on("/api/bundle/chunk", HTTP_POST, [] { v2HttpRoute(11, platform::handleV2BundleChunk); }, v2HttpRawHandler);
    state.server.on("/api/bundle/commit", HTTP_POST, [] { v2HttpRoute(12, platform::handleV2BundleCommit); }, v2HttpRawHandler);
    state.server.on("/update", HTTP_GET, handleUpdatePage);
    state.server.on("/doUpdate", HTTP_POST,
        []() {
            state.server.sendHeader("Connection", "close");
            if (otaUploadDenied) {
                state.server.send(401, "text/plain", "unauthorized: negotiate a token over BLE first");
                return;
            }
            power::setOtaLock(false);
            power::setOtaWifiAwake(false);
            state.server.send(200, "text/plain", Update.hasError() ? "UPDATE FAILED" : "UPDATE OK");
        },
        []() {
            HTTPUpload &up = state.server.upload();
            if (up.status == UPLOAD_FILE_START) {
                if (!requestAuthorized()) {
                    otaUploadDenied = true;
                    DevLog.println("[ota] rejected: unauthorized");
                    return;
                }
                // Normal Bridge uploads include a target guard. Emergency
                // token-authenticated uploads may omit it to recover from a
                // broken Bridge or a ROM without our project marker.
                if (state.server.hasArg("target") && state.server.arg("target") != FW_TARGET_ID) {
                    otaUploadDenied = true;
                    DevLog.printf("[ota] rejected: target %s != %s\n",
                                  state.server.arg("target").c_str(), FW_TARGET_ID);
                    return;
                }
                otaUploadDenied = false;
                state.otaInProgress = true;
                power::setOtaLock(true);
                power::setOtaWifiAwake(true);
                state.otaLastDataMs = millis();
                DevLog.printf("[ota] upload start: %s\n", up.filename.c_str());
                std::vector<String> lines = {"OTA update", application::state.firmwareVersion, up.filename};
                display::screen(lines);
                if (!Update.begin(UPDATE_SIZE_UNKNOWN)) {
                    DevLog.printf("[ota] begin failed: %u\n", (unsigned)Update.getError());
                    otaUploadCleanup("begin-failed");
                }
            } else if (up.status == UPLOAD_FILE_WRITE) {
                if (otaUploadDenied || !state.otaInProgress) return;
                state.otaLastDataMs = millis();
                if (Update.write(up.buf, up.currentSize) != up.currentSize) {
                    DevLog.printf("[ota] write failed: %u\n", (unsigned)Update.getError());
                    otaUploadCleanup("write-failed");
                    otaUploadDenied = true;
                }
            } else if (up.status == UPLOAD_FILE_END) {
                state.otaInProgress = false;
                if (otaUploadDenied) { power::setOtaLock(false); return; }
                if (Update.end(true)) {
                    DevLog.printf("[ota] success %u bytes, rebooting shortly\n", (unsigned)up.totalSize);
                    display::screen({"OTA success", "Rebooting..."});
                    // 5 min light window for the bridge (NVS survives the OTA).
                    Preferences p;
                    p.begin("pm", false);
                    p.putUShort("post_ota_s", 300);
                    p.end();
                    state.otaRebootPending = true;
                    state.otaRebootAt = millis() + 1500;
                } else {
                    DevLog.printf("[ota] end failed: %u\n", (unsigned)Update.getError());
                    otaUploadCleanup("end-failed");
                }
            } else if (up.status == UPLOAD_FILE_ABORTED) {
                otaUploadCleanup("aborted");
            }
        });
}

} // namespace firmware::transport
