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

V2BundleBeginDecision v2DecideBundleBegin(
    JsonDocument &doc, const V2BundleRx &current,
    const V2BundleFingerprint &committed, const char *sessionNonce,
    uint64_t nowMs);
