#include "v2_status_snapshot.h"

void v2WriteDataContract(JsonObject out, const CtTemplate &compiled) {
    out["template_id"] = compiled.id;
    out["compiler_abi"] = compiled.abi;
    char crc[9];
    snprintf(crc, sizeof(crc), "%08x", (unsigned)compiled.sourceCrc);
    out["source_crc"] = crc;
    JsonArray requirements = out["requirements"].to<JsonArray>();
    for (uint8_t i = 0; i < compiled.reqCount; ++i) {
        const CtReq &req = compiled.reqs[i];
        JsonObject field = requirements.add<JsonObject>();
        field["index"] = i;
        field["field"] = req.path;
        const bool text = req.kind <= 2 || req.kind == 10 || req.kind == 11 ||
            req.kind == 12 || req.kind == 14 || req.kind == 16 || req.kind == 17 || req.kind == 18;
        field["kind"] = text ? "text" : "number";
        field["missing"] = "hide";
        field["local"] = req.kind > 9;
    }
}

String v2BuildStatusSnapshot(const V2StatusSnapshot &snapshot) {
    JsonDocument doc;
    const BsProfile &profile = *snapshot.profile;
    const V2DataSeq &dataSeq = *snapshot.dataSeq;
    const V2PlanState &plan = *snapshot.plan;
    doc["result"] = "applied";
    doc["device_mac"] = snapshot.mac;
    if (snapshot.firmwareVersion.length()) doc["fw"] = snapshot.firmwareVersion;
    // Firmware-only wake diagnostics, emitted here so the field order matches the
    // device's original document. Unbound on the host/simulator: absent keys.
    if (snapshot.wake) {
        doc["wake_generation"] = snapshot.wake->generation;
        doc["wake_seq"] = snapshot.wake->seq;
        doc["wake_stage"] = snapshot.wake->stage ? snapshot.wake->stage : "none";
        doc["wake_cause"] = snapshot.wake->cause ? snapshot.wake->cause : "";
    }
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
    if (snapshot.configured && snapshot.compiled)
        v2WriteDataContract(doc["data_contract"].to<JsonObject>(), *snapshot.compiled);
    else doc["data_contract"] = nullptr;
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
