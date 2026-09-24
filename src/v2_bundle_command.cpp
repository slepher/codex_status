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

V2BundleChunkStartDecision v2DecideBundleChunkStart(
    const V2BundleRx &current, const char *request, const char *sessionNonce,
    const char *offsetText, uint64_t nowMs) {
    V2BundleChunkStartDecision decision;
    if (!request || strcmp(current.request, request) || !sessionNonce ||
        strcmp(current.nonce, sessionNonce) || !current.live(nowMs) ||
        !offsetText || !*offsetText) return decision;

    uint64_t parsedOffset = 0;
    for (const char *p = offsetText; *p; ++p) {
        if (*p < '0' || *p > '9') return decision;
        const uint8_t digit = (uint8_t)(*p - '0');
        if (parsedOffset > (UINT64_MAX - digit) / 10) {
            parsedOffset = UINT64_MAX;
        } else {
            parsedOffset = parsedOffset * 10 + digit;
        }
    }
    if (parsedOffset > current.offset) {
        decision.error = "offset_or_size";
        return decision;
    }
    decision.allowed = true;
    decision.replay = parsedOffset < current.offset;
    decision.offset = (uint32_t)parsedOffset;
    decision.error = nullptr;
    return decision;
}

V2BundleChunkWriteDecision v2DecideBundleChunkWrite(
    const V2BundleRx &current, uint32_t offset, bool replay,
    uint32_t processedBytes, uint32_t currentSize) {
    V2BundleChunkWriteDecision decision;
    const uint64_t end = (uint64_t)offset + processedBytes + currentSize;
    if (end <= (replay ? current.offset : current.length)) {
        decision.allowed = true;
        decision.error = nullptr;
    }
    return decision;
}

V2BundleChunkEndDecision v2DecideBundleChunkEnd(
    const V2BundleRx &current, uint32_t offset, bool replay,
    uint32_t processedBytes, uint64_t nowMs) {
    V2BundleChunkEndDecision decision;
    if (!processedBytes || (!replay && !current.append(offset, processedBytes, nowMs)))
        return decision;
    decision.allowed = true;
    decision.error = nullptr;
    decision.nextOffset = replay ? current.offset : offset + processedBytes;
    return decision;
}
