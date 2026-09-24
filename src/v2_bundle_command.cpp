#include "v2_bundle_command.h"
#include "bundle_store.h"
#include "v2_runtime.h"

#include <LittleFS.h>
#include <string.h>

namespace {
struct CommittedReplay {
    bool matched = false;
    bool replay = false;
    uint32_t crc = 0;
    uint32_t length = 0;
    const char *context = nullptr;
    const char *error = nullptr;
};

CommittedReplay checkCommittedReplay(JsonDocument &doc,
                                     const V2BundleFingerprint &committed) {
    CommittedReplay result;
    const char *owner = doc["bridge_id"] | "";
    const char *request = doc["request_id"] | "";
    const char *committedOwner = committed.owner ? committed.owner : "";
    const char *committedRequest = committed.request ? committed.request : "";
    if (strcmp(owner, committedOwner) || strcmp(request, committedRequest)) return result;

    result.matched = true;
    result.length = doc["length"] | 0u;
    if (!v2ParseCrc(doc["content_crc"] | "", result.crc) ||
        result.crc != committed.crc || result.length != committed.length) {
        result.error = "request_conflict";
    } else {
        result.replay = true;
        result.context = committed.context;
    }
    return result;
}
}

V2BundleBeginDecision v2DecideBundleBegin(
    JsonDocument &doc, const V2BundleRx &current,
    const V2BundleFingerprint &committed, const char *sessionNonce,
    uint64_t nowMs) {
    V2BundleBeginDecision decision;
    const char *owner = doc["bridge_id"] | "";
    const char *request = doc["request_id"] | "";
    const uint32_t length = doc["length"] | 0u;
    CommittedReplay replay = checkCommittedReplay(doc, committed);
    if (replay.matched) {
        if (replay.replay) {
            decision.action = V2_BUNDLE_BEGIN_REPLAY;
            decision.replayContext = replay.context;
        } else decision.error = replay.error;
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

V2BundleCommitDecision v2DecideBundleCommit(
    JsonDocument &doc, const V2BundleRx &current,
    const V2BundleFingerprint &committed, const char *sessionNonce,
    uint64_t nowMs, const char *receivePath, const char *bridgeIdFallback) {
    V2BundleCommitDecision decision;
    const char *owner = doc["bridge_id"] | "";
    const char *request = doc["request_id"] | "";
    decision.owner = owner;
    decision.request = request;
    decision.length = doc["length"] | 0u;

    CommittedReplay replay = checkCommittedReplay(doc, committed);
    if (replay.matched) {
        if (replay.replay) {
            decision.action = V2_BUNDLE_COMMIT_REPLAY;
            decision.crc = replay.crc;
            decision.length = replay.length;
            decision.replayContext = replay.context;
        } else {
            decision.error = replay.error;
        }
        return decision;
    }

    if (!v2ParseCrc(doc["content_crc"] | "", decision.crc)) {
        decision.error = "crc";
        return decision;
    }
    if (!current.matches(owner, request, sessionNonce) ||
        !current.complete(decision.length, decision.crc, nowMs)) {
        decision.error = "session_or_length";
        return decision;
    }

    File file = LittleFS.open(receivePath, "r");
    if (!file || file.size() != current.length) {
        if (file) file.close();
        decision.error = "length";
        return decision;
    }
    // Hash the payload from flash in bounded chunks: the device must never hold a
    // whole bundle in RAM just to validate it.
    uint32_t state = v2Crc32Start();
    uint8_t chunk[256];
    uint32_t remaining = current.length;
    while (remaining) {
        size_t want = remaining > sizeof(chunk) ? sizeof(chunk) : (size_t)remaining;
        size_t got = file.read(chunk, want);
        if (!got) {
            file.close();
            decision.error = "crc";
            return decision;
        }
        state = v2Crc32Update(state, chunk, got);
        remaining -= (uint32_t)got;
    }
    if (v2Crc32Finish(state) != decision.crc) {
        file.close();
        decision.error = "crc";
        return decision;
    }

    // The bundle's own `bridge_id` must match the command; fall back to the
    // query/form value when the payload is not readable as JSON. Filtered parses
    // keep only the two keys we need, so the cost stays bounded.
    JsonDocument ownerFilter, ownerDoc;
    ownerFilter["bridge_id"] = true;
    const char *bodyOwner = bridgeIdFallback ? bridgeIdFallback : "";
    if (file.seek(0) && !deserializeJson(ownerDoc, file, DeserializationOption::Filter(ownerFilter))) {
        const char *id = ownerDoc["bridge_id"] | "";
        if (*id) bodyOwner = id;
    }
    if (strcmp(bodyOwner, owner)) {
        file.close();
        decision.error = "owner";
        return decision;
    }

    JsonDocument idFilter, idDoc;
    idFilter["job_id"] = true;
    if (!file.seek(0) || deserializeJson(idDoc, file, DeserializationOption::Filter(idFilter))) {
        file.close();
        decision.error = "json";
        return decision;
    }
    const char *jobId = idDoc["job_id"] | "";
    uint32_t activeCrc = 0;
    if (bsActiveJobPayload(jobId, activeCrc)) {
        file.close();
        if (activeCrc != decision.crc) {
            decision.error = "request_conflict";
            return decision;
        }
        decision.action = V2_BUNDLE_COMMIT_ALREADY_ACTIVE;
        return decision;
    }
    file.close();
    decision.action = V2_BUNDLE_COMMIT_INSTALL;
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
