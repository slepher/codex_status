#pragma once

#include "v2_runtime.h"

#include <stdint.h>

struct V2DataDecision {
    bool firstApplied = false;
    int64_t seq = -1;
    const char *result = "rejected";
    const char *display = nullptr;
    String error;
    String usage;
    bool includeContext = false;
};

V2DataDecision v2DecideData(bool configured, const CtTemplate *ct,
                            const String &body, uint64_t seq,
                            const char *currentContext,
                            V2DataSeq &state);
