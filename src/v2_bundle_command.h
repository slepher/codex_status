#pragma once

#include <ArduinoJson.h>
#include "v2_state.h"

struct V2BundleFingerprint {
    const char *owner;
    const char *request;
    uint32_t crc;
    uint32_t length;
    const char *context;
};

enum V2BundleBeginAction : uint8_t {
    V2_BUNDLE_BEGIN_REJECT,
    V2_BUNDLE_BEGIN_REPLAY,
    V2_BUNDLE_BEGIN_RESUME,
    V2_BUNDLE_BEGIN_START,
};

struct V2BundleBeginDecision {
    V2BundleBeginAction action = V2_BUNDLE_BEGIN_REJECT;
    const char *error = nullptr;
    uint32_t nextOffset = 0;
    const char *replayContext = nullptr;
    V2BundleRx candidate{};
};

enum V2BundleCommitAction : uint8_t {
    V2_BUNDLE_COMMIT_REJECT,
    V2_BUNDLE_COMMIT_REPLAY,
    V2_BUNDLE_COMMIT_ALREADY_ACTIVE,
    V2_BUNDLE_COMMIT_INSTALL,
};

struct V2BundleCommitDecision {
    V2BundleCommitAction action = V2_BUNDLE_COMMIT_REJECT;
    const char *error = nullptr;
    const char *replayContext = nullptr;
    String owner;
    String request;
    uint32_t crc = 0;
    uint32_t length = 0;
};

V2BundleBeginDecision v2DecideBundleBegin(
    JsonDocument &doc, const V2BundleRx &current,
    const V2BundleFingerprint &committed, const char *sessionNonce,
    uint64_t nowMs);

// Validate a Bundle commit against the received payload on flash. The payload is
// streamed for CRC/owner/job checks, so a commit never holds the whole bundle in
// RAM; on V2_BUNDLE_COMMIT_INSTALL the caller installs it from the same file.
V2BundleCommitDecision v2DecideBundleCommit(
    JsonDocument &doc, const V2BundleRx &current,
    const V2BundleFingerprint &committed, const char *sessionNonce,
    uint64_t nowMs, const char *receivePath, const char *bridgeIdFallback);

struct V2BundleChunkStartDecision {
    bool allowed = false;
    bool replay = false;
    uint32_t offset = 0;
    const char *error = "session";
};

V2BundleChunkStartDecision v2DecideBundleChunkStart(
    const V2BundleRx &current, const char *request, const char *sessionNonce,
    const char *offsetText, uint64_t nowMs);

struct V2BundleChunkWriteDecision {
    bool allowed = false;
    const char *error = "offset_or_size";
};

V2BundleChunkWriteDecision v2DecideBundleChunkWrite(
    const V2BundleRx &current, uint32_t offset, bool replay,
    uint32_t processedBytes, uint32_t currentSize);

struct V2BundleChunkEndDecision {
    bool allowed = false;
    const char *error = "offset_or_size";
    uint32_t nextOffset = 0;
};

V2BundleChunkEndDecision v2DecideBundleChunkEnd(
    const V2BundleRx &current, uint32_t offset, bool replay,
    uint32_t processedBytes, uint64_t nowMs);
