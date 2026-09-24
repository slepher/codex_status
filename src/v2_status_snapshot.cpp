#include "v2_status_snapshot.h"

String v2BuildStatusSnapshot(const V2StatusSnapshot &snapshot) {
    JsonDocument doc;
    const BsProfile &profile = *snapshot.profile;
    const V2DataSeq &dataSeq = *snapshot.dataSeq;
    const V2PlanState &plan = *snapshot.plan;
    doc["result"] = "applied";
    doc["protocol"] = 2;
    doc["device_mac"] = snapshot.mac;
    doc["session_nonce"] = snapshot.sessionNonce;
    doc["active_context_id"] = profile.contextId;
    doc["active_template_id"] = snapshot.activeTemplateId;
    doc["committed_job_id"] = profile.jobId;
    doc["data_seq"] = dataSeq.appliedSeq();
    doc["applied_seq"] = dataSeq.appliedSeq();
    doc["display_state"] = snapshot.displayState == 1 ? "displayed"
                          : snapshot.displayState == 2 ? "pending"
                          : snapshot.displayState == 3 ? "failed"
                                                      : "unchanged";
    doc["commit_seq"] = (unsigned)snapshot.commitSeq;
    doc["configured"] = snapshot.configured;
    JsonArray ids = doc["template_ids"].to<JsonArray>();
    for (uint8_t i = 0; i < profile.count; ++i) ids.add(profile.ids[i]);
    JsonObject power = doc["power"].to<JsonObject>();
    power["mode"] = snapshot.deepSleep ? "sleep" : "light";
    power["plan_id"] = plan.acceptedId();
    power["remaining_s"] = plan.remainingS(snapshot.nowMs);
    power["granted_s"] = plan.grantedS();
    power["provisional"] = snapshot.provisional && !plan.accepted();
    power["provisional_remaining_s"] = snapshot.provisional
        ? V2PlanState::bootProvisionalRemaining(snapshot.bootMs, snapshot.nowMs) : 0;
    power["rendezvous_period_s"] = V2_RENDEZVOUS_S;
    power["battery"] = snapshot.battery;
    String output;
    serializeJson(doc, output);
    return output;
}
