#include "v2_data_command.h"

#include <ArduinoJson.h>

V2DataDecision v2DecideData(bool configured, const CtTemplate *ct,
                            const String &body, uint64_t seq,
                            const char *currentContext,
                            V2DataSeq &state) {
    V2DataDecision decision;
    decision.seq = (int64_t)seq;
    if (!configured || !ct) {
        decision.result = "rejected";
        decision.display = "failed";
        decision.error = "unconfigured";
        decision.seq = -1;
        return decision;
    }

    V2DataAck rc = v2AcceptData(*ct, body, currentContext, state,
                                decision.usage, decision.error);
    decision.includeContext = true;
    switch (rc) {
    case V2_DATA_CONTEXT_MISMATCH:
        decision.result = "rejected";
        decision.display = "pending";
        decision.error = "context";
        break;
    case V2_DATA_REJECTED:
        decision.result = "rejected";
        decision.display = "failed";
        break;
    case V2_DATA_UNCHANGED:
        decision.result = "applied";
        decision.display = "unchanged";
        decision.error = "";
        break;
    case V2_DATA_CONFLICT:
        decision.result = "rejected";
        decision.display = "unchanged";
        decision.error = "seq_conflict";
        break;
    case V2_DATA_STALE:
        decision.result = "rejected";
        decision.display = "unchanged";
        decision.error = "stale_seq";
        break;
    case V2_DATA_APPLIED:
        decision.firstApplied = true;
        decision.result = "applied";
        decision.error = "";
        break;
    }
    return decision;
}
