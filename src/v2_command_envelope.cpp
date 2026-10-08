#include "v2_command_envelope.h"

#include <string.h>

static bool compactContextValid(const uint8_t context[16]) {
    bool empty = true;
    for (size_t i = 0; i < 16; ++i) empty &= context[i] == 0;
    if (empty) return true;
    for (size_t i = 0; i < 16; ++i)
        if (!((context[i] >= '0' && context[i] <= '9') ||
              (context[i] >= 'a' && context[i] <= 'f') ||
              (context[i] >= 'A' && context[i] <= 'F'))) return false;
    return true;
}

static bool compactHeaderValid(const CompactFrameHeader &h) {
    return h.kind <= 2 && h.op > 0 && h.op <= COMPACT_FRAME_MAX_OP &&
           h.result <= 3 && (h.kind == 1 || h.result == 0) &&
           h.requestNo != 0 && compactContextValid(h.context);
}

bool compactFrameEncode(const CompactFrameHeader &h, uint8_t out[COMPACT_FRAME_HEADER]) {
    if (!out || !compactHeaderValid(h)) return false;
    out[0] = COMPACT_FRAME_MARKER; out[1] = h.kind;
    out[2] = h.op >> 8; out[3] = h.op;
    out[4] = h.result >> 8; out[5] = h.result;
    out[6] = h.payloadLen >> 8; out[7] = h.payloadLen;
    memcpy(out + 8, h.session, 16);
    for (size_t i = 0; i < 8; ++i) out[24 + i] = h.requestNo >> (56 - i * 8);
    memcpy(out + 32, h.context, 16);
    return true;
}

bool compactFrameDecode(const uint8_t *frame, size_t length, CompactFrameHeader &h,
                        size_t frameLimit) {
    if (!frame || length < COMPACT_FRAME_HEADER || length > frameLimit ||
        frame[0] != COMPACT_FRAME_MARKER) return false;
    CompactFrameHeader parsed;
    parsed.kind = frame[1];
    parsed.op = (uint16_t(frame[2]) << 8) | frame[3];
    parsed.result = (uint16_t(frame[4]) << 8) | frame[5];
    parsed.payloadLen = (uint16_t(frame[6]) << 8) | frame[7];
    if (length != COMPACT_FRAME_HEADER + parsed.payloadLen) return false;
    memcpy(parsed.session, frame + 8, 16);
    for (size_t i = 0; i < 8; ++i) parsed.requestNo = (parsed.requestNo << 8) | frame[24 + i];
    memcpy(parsed.context, frame + 32, 16);
    if (!compactHeaderValid(parsed)) return false;
    h = parsed;
    return true;
}

uint16_t compactOperation(const char *name) {
    if (!name) return 0;
    static const char *names[] = {"", "status", "data", "plan", "claim", "renew",
        "sync_begin", "sync_page", "sync_ack", "sync_complete", "bundle_begin",
        "bundle_chunk", "bundle_commit", "activate", "wifi_snapshot", "wifi_apply",
        "wifi_result", "wifi_result_ack", "wifi_command", "ota_image",
        "ready_report", "business_query", "network_start", "sync_config",
        "sync_open", "history", "wifi_owner", "wifi_info"};
    for (uint16_t i = 1; i <= COMPACT_FRAME_MAX_OP; ++i)
        if (!strcmp(name, names[i])) return i;
    return !strncmp(name, "wifi_", 5) ? 18 : 0;
}

const char *v2ParseCommand(const String &body, JsonDocument &doc) {
    return deserializeJson(doc, body) ? "json" : nullptr;
}

V2CommandSessionDecision v2CheckCommandSession(
    JsonDocument &doc, const String &currentMac, const String *sessionNonce) {
    V2CommandSessionDecision decision;
    JsonString id = doc["request_id"].as<JsonString>();
    const char *request = doc["request_id"] | "";
    if (!doc["protocol"].isNull() || !doc["rv"].isNull() ||
        String(doc["device_mac"] | "") != currentMac ||
        !id || id.size() != strlen(request) || !*request || strlen(request) > 64) return decision;
    if (!sessionNonce) {
        decision.needsNonce = true;
        decision.error = nullptr;
        return decision;
    }
    if (*sessionNonce != (doc["session_nonce"] | "")) return decision;
    decision.accepted = true;
    decision.error = nullptr;
    return decision;
}

String v2BuildAck(const char *op, const char *result, const char *display,
                  const char *retention, const char *error, int64_t seq,
                  uint64_t planId, const char *context,
                  uint32_t acceptedRemainingS, const char *fwTarget, bool fullSeq) {
    JsonDocument doc;
    doc["op"] = op;
    doc["result"] = result;
    doc["display_state"] = display;
    doc["retention"] = retention;
    if (error && *error) doc["error"] = error;
    if (seq >= 0 || fullSeq) doc["data_seq"] = (uint64_t)seq;
    if (planId) doc["plan_id"] = planId;
    if (context && *context) doc["active_context_id"] = context;
    if (acceptedRemainingS != UINT32_MAX)
        doc["accepted_remaining_s"] = acceptedRemainingS;
    doc["fw_target"] = fwTarget;
    String output;
    serializeJson(doc, output);
    return output;
}
