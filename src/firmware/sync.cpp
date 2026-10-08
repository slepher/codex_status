#include "sync_internal.h"
#include "log_sink.h"
#include "application_internal.h"
#include "clock_internal.h"
#include "platform_internal.h"
#include "power_internal.h"
#include "radio_internal.h"
#include "transport_internal.h"

namespace firmware::sync {

State state;
RTC_DATA_ATTR Retained retained;

void appendLog(const char *text, size_t count, uint8_t flags) {
    if (state.syncLogReady && syncDiagnosticText(text))
        syncAppend(retained.syncRtc, SYNC_TEXT, flags, state.syncLogWakeSeq, millis(),
                   reinterpret_cast<const uint8_t *>(text), count);
}

String dumpLog() {
    String out;
    out.reserve(retained.syncRtc.used);
    for (uint16_t offset = 0, next = 0; offset < retained.syncRtc.used; offset = next) {
        SyncRecord record;
        if (!syncReadAt(retained.syncRtc, offset, record, next)) break;
        if (record.kind != SYNC_TEXT) continue;
        for (uint8_t i = 0; i < record.payloadLen; ++i) out += (char)record.payload[i];
        out += '\n';
    }
    return out;
}

static bool syncParseNumber(const char *text, uint64_t &value);
static bool syncCommand(const char *op, JsonDocument &doc);
static JsonDocument syncFrozenSnapshot();
static void handleSyncTransfer(const char *op);

void syncRecordOutcome(uint8_t result, uint16_t error, uint64_t serial) {
    syncAppendResult(retained.syncRtc, state.syncLogWakeSeq, millis(), result,
                     (uint8_t)((retained.syncRtc.flags & 1 ? 1 : 0) | retained.syncRtc.reasonBits),
                     error, serial);
    int32_t args[] = {(int32_t)result, (int32_t)error};
    syncAppendEvent(retained.syncRtc, state.syncLogWakeSeq, millis(), 10, args, 2);
}

String syncNumber(uint64_t value) {
    char text[24];
    snprintf(text, sizeof(text), "%llu", (unsigned long long)value);
    return String(text);
}

static bool syncParseNumber(const char *text, uint64_t &value) {
    if (!text || !*text) return false;
    value = 0;
    for (const char *p = text; *p; ++p) {
        if (*p < '0' || *p > '9' || value > (UINT64_MAX - (*p - '0')) / 10) return false;
        value = value * 10 + (*p - '0');
    }
    return true;
}

void syncReply(const char *op, const char *result, const char *error,
                      int httpStatus, JsonDocument *extra) {
    if (state.syncOpenThisWake && !state.syncBlockedError && error) {
        uint16_t blocked = !strcmp(error, "unauthorized") ? 4 :
                           !strcmp(error, "occupied") ? 5 :
                           !strcmp(error, "owner_changed") ? 10 :
                           !strcmp(error, "disabled") ? 11 : 0;
        if (blocked) {
            state.syncBlockedError = blocked;
            syncRecordOutcome(2, blocked);
            state.syncExitAtMs = millis() + 200;
        }
    }
    JsonDocument doc;
    doc["op"] = op;
    doc["result"] = result;
    if (error) doc["error"] = error;
    doc["request_id"] = transport::state.v2RequestId;
    if (httpStatus != 401) {
        doc["device_mac"] = radio::macText();
        doc["session_nonce"] = transport::v2Nonce();
    }
    doc["sync_version"] = 1;
    if (extra) {
        for (JsonPair field : extra->as<JsonObject>()) doc[field.key()] = field.value();
    }
    String out;
    serializeJson(doc, out);
    transport::v2Response(httpStatus, out);
}

static bool syncCommand(const char *op, JsonDocument &doc) {
    String endpoint;
    if (!transport::endpointTokenAuthorized(endpoint)) {
        syncReply(op, "rejected", "unauthorized", 401); return false;
    }
    String body = transport::state.v2HttpBody;
    if (body.length() > 4096) {
        syncReply(op, "rejected", "body_limit", 413); return false;
    }
    if (deserializeJson(doc, body)) {
        syncReply(op, "rejected", "json", 400); return false;
    }
    transport::state.v2RequestId = doc["request_id"] | "";
    const char *bridge = doc["bridge_id"] | "";
    if (!doc["protocol"].isNull() || !doc["rv"].isNull() ||
        (doc["sync_version"] | 0) != 1 ||
        !*bridge || !transport::state.v2RequestId.length() || transport::state.v2RequestId.length() > 64 ||
        String(doc["device_mac"] | "") != radio::macText()) {
        syncReply(op, "rejected", "shape", 400); return false;
    }
    if (String(doc["session_nonce"] | "") != transport::v2Nonce()) {
        syncReply(op, "rejected", "session", 400); return false;
    }
    OwnerRec owner;
    if (!ownerGet(owner)) {
        syncReply(op, "rejected", "claim_required", 409); return false;
    }
    if (owner.id != bridge) {
        syncReply(op, "rejected", "occupied", 409); return false;
    }
    if (!syncStoreAvailable()) {
        syncReply(op, "rejected", "storage", 503); return false;
    }
    if (!state.syncEnabled || state.syncOwner != bridge) {
        syncReply(op, "rejected", "disabled", 409); return false;
    }
    return true;
}

void syncBatchFields(JsonDocument &doc, const SyncStoreBatch &batch) {
    syncProtocolBatchFields(doc.to<JsonObject>(), batch);
}

static JsonDocument syncFrozenSnapshot() {
    JsonDocument doc;
    JsonObject snapshot = doc["snapshot"].to<JsonObject>();
    snapshot["target"] = FW_TARGET_ID;
    snapshot["fw"] = application::state.firmwareVersion;
    const esp_partition_t *running = esp_ota_get_running_partition();
    snapshot["slot"] = running ? running->label : nullptr;
    snapshot["reset_reason"] = (int)esp_reset_reason();
    snapshot["active_context_id"] = platform::state.v2Profile.contextId;
    snapshot["active_template_id"] = platform::state.v2Profile.count ? platform::state.v2Profile.ids[platform::state.v2Profile.initial] : "";
    snapshot["committed_job_id"] = platform::state.v2Profile.jobId;
    snapshot["commit_seq"] = bsCommitSeq();
    snapshot["display_state"] = platform::state.v2DisplayState;
    snapshot["applied_seq"] = platform::state.v2DataSeq.appliedSeq();
    snapshot["heap_free"] = ESP.getFreeHeap();
    snapshot["battery"] = power::batteryPercent();
    snapshot["plan_id"] = power::state.v2Plan.acceptedId();
    snapshot["plan_remaining_s"] = power::state.v2Plan.remainingS(clock::v2NowMs());
    snapshot["rounds"] = retained.syncRtc.rounds;
    OwnerRec owner;
    snapshot["owner_id"] = ownerGet(owner) ? owner.id : "";
    return doc;
}

static void handleSyncTransfer(const char *op) {
    JsonDocument request;
    if (!syncCommand(op, request)) return;
    JsonDocument snapshot = syncFrozenSnapshot();
    uint8_t randomBytes[16];
    for (uint8_t i = 0; i < 16; i += 4) {
        uint32_t word = esp_random();
        memcpy(randomBytes + i, &word, 4);
    }
    char suffix[33];
    syncHex(randomBytes, 16, suffix);
    String mac = radio::macText();
    SyncProtocolInput input{retained.syncRtc, mac.c_str(),
                            request["bridge_id"] | "", suffix,
                            state.syncLogWakeSeq, millis(), false,
                            snapshot["snapshot"].as<JsonVariantConst>()};
    JsonDocument fields;
    uint32_t before = syncStoreAckedOffset();
    SyncProtocolResult result = syncProtocolRun(input, op, request, fields);
    if (result.status == 200 && String(fields["result"] | "") == "applied") {
        if (!strcmp(op, "sync_begin") ||
            (!strcmp(op, "sync_ack") && syncStoreAckedOffset() > before))
            state.syncLastProgressMs = millis();
    }
    if (result.completed && state.syncOpenThisWake && !(retained.syncRtc.flags & 1))
        state.syncExitAtMs = millis() + 200;
    syncReply(op, fields["result"] | "rejected",
              fields["error"] | nullptr, result.status, &fields);
}

void handleSyncBegin() { handleSyncTransfer("sync_begin"); }

void handleSyncPage() { handleSyncTransfer("sync_page"); }

void handleSyncAck() { handleSyncTransfer("sync_ack"); }

void handleSyncComplete() { handleSyncTransfer("sync_complete"); }

} // namespace firmware::sync
