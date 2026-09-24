#include "v2_bundle_command.h"

#include <string.h>

V2BundleBeginDecision v2DecideBundleBegin(
    JsonDocument &doc, const V2BundleRx &current,
    const V2BundleFingerprint &committed, const char *sessionNonce,
    uint64_t nowMs) {
    V2BundleBeginDecision decision;
    const char *owner = doc["bridge_id"] | "";
    const char *request = doc["request_id"] | "";
    const uint32_t length = doc["length"] | 0u;
    const char *committedOwner = committed.owner ? committed.owner : "";
    const char *committedRequest = committed.request ? committed.request : "";

    if (!strcmp(owner, committedOwner) && !strcmp(request, committedRequest)) {
        uint32_t crc = 0;
        if (!v2ParseCrc(doc["content_crc"] | "", crc) ||
            crc != committed.crc || length != committed.length) {
            decision.error = "request_conflict";
        } else {
            decision.action = V2_BUNDLE_BEGIN_REPLAY;
            decision.replayContext = committed.context;
        }
        return decision;
    }

    uint32_t crc = 0;
    if (!v2ParseCrc(doc["content_crc"] | "", crc)) {
        decision.error = "crc";
        return decision;
    }
    if (current.live(nowMs)) {
        if (!current.matches(owner, request, sessionNonce)) {
            decision.error = "busy";
        } else if (current.length != length || current.crc != crc) {
            decision.error = "request_conflict";
        } else {
            decision.action = V2_BUNDLE_BEGIN_RESUME;
            decision.nextOffset = current.offset;
        }
        return decision;
    }

    if (!decision.candidate.begin(owner, request, sessionNonce, length, crc, nowMs)) {
        decision.error = "size";
        return decision;
    }
    decision.action = V2_BUNDLE_BEGIN_START;
    return decision;
}
