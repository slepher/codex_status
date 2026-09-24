#include "v2_plan_command.h"

#include <string.h>

V2PlanDecision v2DecidePlan(JsonDocument &doc, V2PlanState &state,
                            uint64_t nowMs, bool provisional) {
    V2PlanDecision decision;
    decision.plan.planId = doc["plan_id"] | 0ULL;
    const char *mode = doc["mode"] | "sleep";
    if (!doc["plan_id"].is<uint64_t>() ||
        (!strcmp(mode, "light") && !doc["light_duration_s"].is<uint32_t>()) ||
        (strcmp(mode, "light") && strcmp(mode, "sleep"))) {
        decision.error = "plan_shape";
        return decision;
    }

    decision.plan.mode = !strcmp(mode, "light") ? V2_PLAN_LIGHT : V2_PLAN_SLEEP;
    decision.plan.lightDurationS = doc["light_duration_s"] | 0u;
    decision.plan.rendezvousPeriodS = doc["rendezvous_period_s"] | V2_RENDEZVOUS_S;
    decision.planId = decision.plan.planId;
    decision.includeContext = true;

    switch (state.accept(decision.plan, nowMs, false,
                         provisional ? V2_BOOT_PROVISIONAL_S : V2_MAX_LIGHT_S)) {
    case V2_PLAN_STALE_ID:
        decision.error = "stale_plan";
        break;
    case V2_PLAN_CONFLICT:
        decision.error = "plan_conflict";
        break;
    case V2_PLAN_REJECTED_LIMIT:
        decision.error = "plan_limit";
        break;
    case V2_PLAN_ACCEPTED:
        decision.accepted = true;
        decision.result = "applied";
        decision.grantedS = state.grantedS();
        break;
    }
    return decision;
}
