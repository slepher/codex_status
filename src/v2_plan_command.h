#pragma once

#include <ArduinoJson.h>
#include "v2_state.h"

struct V2PlanDecision {
    bool accepted = false;
    V2PowerPlan plan{};
    const char *result = "rejected";
    const char *display = "unchanged";
    const char *error = nullptr;
    uint64_t planId = 0;
    uint32_t grantedS = 0;
    bool includeContext = false;
};

V2PlanDecision v2DecidePlan(JsonDocument &doc, V2PlanState &state,
                            uint64_t nowMs, bool provisional);
