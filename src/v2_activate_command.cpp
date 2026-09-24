#include "v2_activate_command.h"

#include <string.h>

V2ActivateDecision v2DecideActivate(
    JsonDocument &doc, bool bundleReady, const BsProfile &profile,
    const char *previousRequest, const char *previousOwner,
    const char *previousTemplate, const char *previousExpected,
    const char *previousContext) {
    V2ActivateDecision decision;
    decision.request = doc["request_id"] | "";
    decision.owner = doc["bridge_id"] | "";
    decision.templateId = doc["template_id"] | "";
    decision.expected = doc["expected_active_context_id"] | "";

    if (decision.request == (previousRequest ? previousRequest : "") &&
        decision.owner == (previousOwner ? previousOwner : "")) {
        const bool same = decision.templateId == (previousTemplate ? previousTemplate : "") &&
                          decision.expected == (previousExpected ? previousExpected : "");
        decision.action = same ? V2_ACTIVATE_REPLAY : V2_ACTIVATE_REJECT;
        decision.result = same ? "applied" : "rejected";
        decision.error = same ? nullptr : "request_conflict";
        decision.context = previousContext ? previousContext : "";
        return decision;
    }

    decision.context = profile.contextId;
    if (!bundleReady || decision.expected != profile.contextId) {
        decision.error = "context";
        return decision;
    }
    for (uint8_t i = 0; i < profile.count; ++i) {
        if (decision.templateId == profile.ids[i]) {
            decision.action = V2_ACTIVATE_SWITCH;
            decision.index = i;
            decision.error = nullptr;
            return decision;
        }
    }
    decision.error = "unknown_template";
    return decision;
}
