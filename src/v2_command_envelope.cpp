#include "v2_command_envelope.h"

#include <string.h>

const char *v2ParseCommand(const String &body, JsonDocument &doc) {
    return deserializeJson(doc, body) ? "json" : nullptr;
}

V2CommandSessionDecision v2CheckCommandSession(
    JsonDocument &doc, const String &currentMac, const String *sessionNonce) {
    V2CommandSessionDecision decision;
    const char *request = doc["request_id"] | "";
    if (!doc["protocol"].isNull() || !doc["rv"].isNull() ||
        String(doc["device_mac"] | "") != currentMac ||
        !*request || strlen(request) > 64) return decision;
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
                  uint32_t acceptedRemainingS, const char *fwTarget) {
    JsonDocument doc;
    doc["op"] = op;
    doc["result"] = result;
    doc["display_state"] = display;
    doc["retention"] = retention;
    if (error && *error) doc["error"] = error;
    if (seq >= 0) doc["data_seq"] = seq;
    if (planId) doc["plan_id"] = planId;
    if (context && *context) doc["active_context_id"] = context;
    if (acceptedRemainingS != UINT32_MAX)
        doc["accepted_remaining_s"] = acceptedRemainingS;
    doc["fw_target"] = fwTarget;
    String output;
    serializeJson(doc, output);
    return output;
}
