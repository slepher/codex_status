#include "platform_internal.h"
#include "clock_internal.h"
#include "diagnostics_internal.h"
#include "display_internal.h"
#include "power_internal.h"
#include "radio_internal.h"
#include "sync_internal.h"
#include "transport_internal.h"

namespace firmware::platform {

State state;
RTC_DATA_ATTR Retained retained;

static uint64_t       v2LastAckSeq = 0;

static String         v2AppliedFields;           // bounded last applied Data fields

static V2BundleRx     v2Rx;

static String         v2RxPath = bsReceivePath();

bool bundleUpdating() { return v2Rx.live(clock::v2NowMs()); }

// One activation result per boot session. Expected context prevents delayed
// commands from activating after a local switch or a subsequent publication.
static String v2ActivateRequest, v2ActivateOwner, v2ActivateExpected;

static String v2ActivateTemplate, v2ActivateContext;

// Owner/session-bound transfer; reading and writing never renews power.
static String v2CommittedRequest, v2CommittedOwner, v2CommittedContext;

static uint32_t v2CommittedCrc = 0, v2CommittedLength = 0;

static void usageCacheSave(const String &json);
static V2DataDecision applyV2DataAuthorized(const String &body, JsonDocument &peek);
static void v2BundleError(const char *error, const char *stage = "receive");

bool usageCacheLoad(String &out) {
    Preferences p;
    p.begin("ucache", true);
    out = p.getString("json", "");
    p.end();
    return out.length() > 0;
}

static void usageCacheSave(const String &json) {
    uint32_t h = diagnostics::fnv1a(json);
    if (h == retained.rtcUsageHash) return;
    if (json.length() >= 4000) {
        DevLog.println("[pm] usage cache skipped: too large");
        retained.rtcUsageHash = h;
        return;
    }
    Preferences p;
    p.begin("ucache", false);
    p.putString("json", json);
    p.end();
    retained.rtcUsageHash = h;
}

bool tplCacheLoad() {
    String id = tplStoreActive();
    if (!id.length()) {
        state.activeTplJson = ""; state.activeTplId = ""; state.activeTplHasNow = false;
        rgnReset(display::state.rgnSet); display::state.rgnSet.wholeFrame = true;
        return false;
    }
    if (id == state.activeTplId && state.activeTplJson.length()) return true;
    String json;
    if (!tplStoreLoad(id, json)) {
        state.activeTplJson = ""; state.activeTplId = ""; state.activeTplHasNow = false;
        rgnReset(display::state.rgnSet); display::state.rgnSet.wholeFrame = true;
        return false;
    }
    state.activeTplJson = json;
    state.activeTplId = id;
    // Scan the (multi-KB, possibly PSRAM-backed) template once per load, not
    // once per loop: the per-loop scan thrashs the wake-path cache and light
    // sleep never recovers on this 40 MHz-flash board.
    state.activeTplHasNow = state.activeTplJson.indexOf("device.now") >= 0;
    state.activeTplHasMode = state.activeTplJson.indexOf("device.mode") >= 0;
    display::clkComputeRect();
    // Semantic refresh regions for the display-safety layer (design §8.1).
    // Failure to derive them leaves wholeFrame=true (conservative full).
    bool rgnOk = rgnBuild(state.activeTplJson, display::state.rgnSet);
    DevLog.printf("[rgn] %s n=%u whole=%d\n", rgnOk ? "derived" : "fallback",
                  (unsigned)display::state.rgnSet.n, display::state.rgnSet.wholeFrame ? 1 : 0);
    return true;
}

// Load the committed Bundle's initial compiled template (or the current one)
// and prime refresh regions + clock reservation from the compiled record.
bool v2ActiveLoad() {
    if (!state.v2BundleReady) { state.v2CtValid = false; return false; }
    String err;
    if (!bsLoadCompiled(state.v2Profile.initial, state.v2Ct, err)) {
        DevLog.printf("[platform] active load failed: %s\n", err.c_str());
        state.v2CtValid = false;
        return false;
    }
    state.v2CtValid = true;
    state.activeTplId = state.v2Profile.ids[state.v2Profile.initial];
    state.activeTplJson = "";
    state.activeTplHasNow = false;
    state.activeTplHasMode = false;
    for (int i = 0; i < state.v2Ct.reqCount; i++) {
        const char *p = state.v2Ct.reqs[i].path;
        if (!strcmp(p, "device.now")) state.activeTplHasNow = true;
        if (!strcmp(p, "device.mode")) state.activeTplHasMode = true;
    }
    bool rgnOk = rgnBuildCt(state.v2Ct, display::state.rgnSet);
    DevLog.printf("[rgn] ct %s n=%u whole=%d\n", rgnOk ? "derived" : "fallback",
                  (unsigned)display::state.rgnSet.n, display::state.rgnSet.wholeFrame ? 1 : 0);
    display::clkComputeRectCt();
    return true;
}

// Active-template switch (BOOT key cycle): new context, re-prime regions.
bool v2SwitchActive(uint8_t index) {
    char ctx[BS_CTX_LEN];
    snprintf(ctx, sizeof(ctx), "%08x%08x", state.v2CtxGen.next(), (unsigned)esp_random());
    String err;
    if (!bsSetActive(index, ctx, err)) {
        DevLog.printf("[platform] activate failed: %s\n", err.c_str());
        return false;
    }
    bsProfile(state.v2Profile);
    state.v2DataSeq.beginContext(clock::v2NowMs(), 1);
    v2AppliedFields = "";
    if (!v2ActiveLoad()) {
        return false;
    }
    DevLog.printf("[platform] active=%s ctx=%s\n", state.activeTplId.c_str(), state.v2Profile.contextId);
    return true;
}

void nextTemplate() {
    if (state.bundleMaintenance) return;
    // v2 Bundle: every installed template participates in the key cycle, and a
    // switch generates a new unreusable context (v2 §4/§10).
    if (state.v2BundleReady && state.v2Profile.count > 0) {
        uint8_t next = (uint8_t)((state.v2Profile.initial + 1) % state.v2Profile.count);
        v2SwitchActive(next);
        state.pendingTplChanged = true;
        power::noteActivity("template-switch");
        DevLog.printf("[platform] local switch -> %s\n", state.v2Profile.ids[state.v2Profile.initial]);
        return;
    }
    int n = tplStoreCount();
    if (n <= 0) return;
    String active = tplStoreActive();
    int cur = -1;
    for (int i = 0; i < n; i++) {
        TplMeta m;
        if (tplStoreGet(i, m) && m.id == active) { cur = i; break; }
    }
    int next = (cur + 1) % n;
    TplMeta m;
    if (!tplStoreGet(next, m)) return;
    tplStoreSetActive(m.id);
    state.activeTplId = "";
    state.pendingTplChanged = true;
    power::noteActivity("template-switch");
    DevLog.printf("[tpl] local switch -> %s\n", m.id.c_str());
}

void factoryReset() {
    display::screen({"FACTORY RESET", "", "clearing..."});
    if (!power::state.planStore.clear()) {
        display::screen({"RESET FAILED", "Plan storage", "Retry factory reset"}); return;
    }
    bleClearBonds();
    storeClear();
    tplStoreClear();
    ownerClear(true);
    radio::state.prefs.begin("wifi", false);
    radio::state.prefs.clear();
    radio::state.prefs.end();
    radio::state.prefs.begin("auth", false);
    radio::state.prefs.clear();
    radio::state.prefs.end();
    delay(1000);
    ESP.restart();
}

// Both standalone Data and Plan's optional Data use this exact state transition.
static V2DataDecision applyV2DataAuthorized(const String &body, JsonDocument &peek) {
    uint64_t seq = peek["seq"] | 0ULL;
    V2DataDecision decision = v2DecideData(state.v2BundleReady && state.v2CtValid,
                                            state.v2CtValid ? &state.v2Ct : nullptr, body, seq,
                                            state.v2Profile.contextId, state.v2DataSeq);
    if (!decision.firstApplied) return decision;
    const String &usage = decision.usage;
    v2AppliedFields = "";
    serializeJson(peek["fields"], v2AppliedFields);
    v2LastAckSeq = seq;
    state.v2LastAckAtMs = clock::v2NowMs();
    // Update on every accepted snapshot, before replying. No NVS wear and no
    // truncated field JSON; timer/physical deep wakes retain the exact baseline.
    retained.v2DataCheckpoint.save(state.v2Profile.contextId, state.v2DataSeq);
    usageCacheSave(usage);
    state.lastUsage = usage;
    state.lastChannel = transport::state.v2ReplyOverBle ? "BLE" : "PULL";
    const char *display = "unchanged";
    if (transport::state.v2ReplyOverBle) {
        // Plan C: do not render inside the rendezvous window. The snapshot is
        // drawn once after the radio is off, together with the clock
        // (v2RendezvousRender); the ACK reports it as pending.
        state.v2WakeRenderPending = true;
        state.v2DisplayState = 2;
        display = "pending";
    } else {
        uint32_t before = display::state.epdWriteCount;
        uint32_t busyBefore = display::retained.rtcEpdBusyFails;
        display::renderActiveUsage(usage, state.lastChannel.c_str());
        state.v2DisplayState = displayStateAfterRender(before, display::state.epdWriteCount, busyBefore, display::retained.rtcEpdBusyFails);
        if (display::state.epdWriteCount != before) {
            display = "displayed";
        } else if (display::retained.rtcEpdBusyFails != busyBefore) {
            display = "failed";
        }
    }
    decision.display = display;
    return decision;
}

// POST /api/data: atomic complete snapshot inside the current context.
void applyV2Data(const String &body) {
    JsonDocument peek;
    if (!transport::v2Command(body, peek)) return;
    radio::markSynced();
    const V2DataDecision decision = applyV2DataAuthorized(body, peek);
    transport::v2Ack("data", decision.result, decision.display, "ram",
          decision.error.length() ? decision.error.c_str() : nullptr, decision.seq, 0,
          decision.includeContext ? state.v2Profile.contextId : nullptr, UINT32_MAX,
          decision.includeContext);
}

void handleV2Data() {
    DevLog.printf("[platform] req data heap=%u", (unsigned)ESP.getFreeHeap());
    String mac;
    if (!transport::endpointTokenAuthorized(mac)) {
        transport::v2Response(401, "{\"result\":\"unauthorized\"}");
        return;
    }
    applyV2Data(transport::state.v2HttpBody);
}

// POST /api/plan: the only way to change the light deadline.
void applyV2Plan(const String &body) {
    JsonDocument doc;
    if (!transport::v2Command(body, doc)) return;
    radio::markSynced();
    V2PlanDecision decision = power::state.planStore.apply(doc, power::state.v2Plan,
        clock::v2NowMs(), clock::planUtc(), power::state.v2Provisional);
    // Local: the requested history window is recorded as soon as the plan shape is
    // valid -- before the accept result is known -- mirroring the pre-refactor
    // path, which set it right after its own shape check and before v2Plan.accept().
    if (!decision.error || strcmp(decision.error, "plan_shape")) {
        uint32_t historyMs = doc["history_sync_ms"] | 0u;
        if (historyMs > 2500) historyMs = 2500;
        sync::state.v2HistorySyncMs = historyMs;
    }
    if (!decision.accepted) {
        transport::v2Ack("plan", decision.result, decision.display, "ram", decision.error,
              -1, decision.planId, decision.includeContext ? state.v2Profile.contextId : nullptr,
              UINT32_MAX);
        return;
    }
    if (radio::state.v2InRendezvous) {
        radio::state.v2Rv.planReceived();
        radio::state.v2RvNetworkExpected = doc["need_http"] == true;
        if (radio::state.v2RvNetworkExpected)
            radio::state.v2Rv.awaitNetworkStart(clock::v2NowMs(), power::state.v2Plan.lightActive(clock::v2NowMs()) ?
                power::state.v2Plan.deadlineMs() : clock::v2NowMs() + RV_NETWORK_MS);
    }
    const V2PowerPlan &plan = decision.plan;
    power::state.v2PlanReason = "bridge";
    if (plan.mode == V2_PLAN_LIGHT) {
        power::state.v2LightDeadlineMs = power::state.v2Plan.deadlineMs();
        power::state.v2Provisional = false;
        if (power::retained.rtcMode != MODE_LIGHT) {
            if (sync::state.syncEnabled) syncAddReason(sync::retained.syncRtc, 1u << 1);
            power::retained.rtcMode = MODE_LIGHT;
            power::persistMode();
        }
        DevLog.printf("[platform] plan %lu light %us\n", (unsigned long)plan.planId,
                      (unsigned)power::state.v2Plan.grantedS());
    } else {
        power::state.v2LightDeadlineMs = 0;
        power::state.v2Provisional = false;
        DevLog.printf("[platform] plan %lu sleep\n", (unsigned long)plan.planId);
    }
    if (transport::state.v2ReplyOverBle && sync::state.v2HistorySyncMs) {
        sync::state.v2HistorySyncUntilMs = clock::v2NowMs() + sync::state.v2HistorySyncMs;
    }
    JsonDocument ack;
    deserializeJson(ack, v2BuildAck("plan", decision.result, decision.display, "nvs",
        decision.error, -1, decision.planId, state.v2Profile.contextId, decision.grantedS, FW_TARGET_ID));
    if (!doc["data"].isNull()) {
        JsonDocument data;
        data.set(doc["data"]);
        String dataBody;
        serializeJson(data, dataBody);
        V2DataDecision applied = applyV2DataAuthorized(dataBody, data);
        JsonObject dataAck = ack["data"].to<JsonObject>();
        dataAck["result"] = applied.result;
        dataAck["display_state"] = applied.display;
        if (applied.error.length()) dataAck["error"] = applied.error;
        if (applied.seq >= 0) dataAck["data_seq"] = (uint64_t)applied.seq;
        if (applied.includeContext) dataAck["active_context_id"] = state.v2Profile.contextId;
        radio::state.readyReport.dataResult = applied.result;
    }
    String response; serializeJson(ack, response);
    transport::v2Response(200, response);
}

void handleV2Plan() {
    DevLog.printf("[platform] req plan heap=%u", (unsigned)ESP.getFreeHeap());
    String mac;
    if (!transport::endpointTokenAuthorized(mac)) {
        transport::v2Response(401, "{\"result\":\"unauthorized\"}");
        return;
    }
    applyV2Plan(transport::state.v2HttpBody);
}

void handleV2Activate() {
    String mac;
    if (!transport::endpointTokenAuthorized(mac)) {
        transport::v2Response(401, "{\"result\":\"unauthorized\"}");
        return;
    }
    JsonDocument doc;
    if (!transport::v2Command(transport::state.v2HttpBody, doc)) return;
    V2ActivateDecision decision = v2DecideActivate(
        doc, state.v2BundleReady, state.v2Profile, v2ActivateRequest.c_str(),
        v2ActivateOwner.c_str(), v2ActivateTemplate.c_str(),
        v2ActivateExpected.c_str(), v2ActivateContext.c_str());
    if (decision.action != V2_ACTIVATE_SWITCH) {
        transport::v2Ack("activate", decision.result, decision.display, "flash",
              decision.error, -1, 0, decision.context, UINT32_MAX);
        return;
    }
    if (!v2SwitchActive((uint8_t)decision.index)) {
        transport::v2Ack("activate", "rejected", "unchanged", "flash",
              "activation_failed", -1, 0,
              state.v2Profile.contextId, UINT32_MAX);
        return;
    }
    v2ActivateRequest = decision.request; v2ActivateOwner = decision.owner;
    v2ActivateExpected = decision.expected; v2ActivateTemplate = decision.templateId;
    v2ActivateContext = state.v2Profile.contextId;
    display::renderCurrent();
    transport::v2Ack("activate", "applied", "displayed", "flash", nullptr, -1, 0,
          state.v2Profile.contextId, UINT32_MAX);
}

static void v2BundleError(const char *error, const char *stage) {
    state.bundleError = error ? error : "update_failed";
    if (state.bundleMaintenance) display::renderCurrent();
    JsonDocument ack;
    deserializeJson(ack, v2BuildAck("bundle", "rejected", state.bundleMaintenance ? "failed" : "unchanged", "flash", error, -1, 0,
          state.v2Profile.contextId, UINT32_MAX, FW_TARGET_ID));
    ack["stage"] = stage;
    String body; serializeJson(ack, body);
    transport::v2Response(200, body);
}

static void beginBundleReplacement() {
    // Called after download/CRC/target checks on the loop task, immediately
    // before replacement. No content renderer may keep old font pointers.
    state.bundleMaintenance = true;
    state.v2BundleReady = state.v2CtValid = false;
    state.v2Profile = BsProfile{};
    state.activeTplId = state.activeTplJson = "";
    state.activeTplHasNow = state.activeTplHasMode = false;
    state.lastUsage = v2AppliedFields = "";
    state.v2WakeRenderPending = false;
    state.v2DataSeq.beginContext(clock::v2NowMs(), 1);
    display::retained.clkPixelsValid = false;
    rgnReset(display::state.rgnSet);
    v2ActivateRequest = v2CommittedRequest = "";
    display::screen({"CONTENT UPDATE", "", "Installing..."});
}

void handleV2BundleBegin() {
    String mac;
    if (!transport::endpointTokenAuthorized(mac)) {
        transport::v2Response(401, "{\"result\":\"unauthorized\"}"); return;
    }
    JsonDocument doc;
    if (!transport::v2Command(transport::state.v2HttpBody, doc)) return;
    const String nonce = transport::v2Nonce();
    const uint64_t nowMs = clock::v2NowMs();
    const V2BundleFingerprint committed{
        v2CommittedOwner.c_str(), v2CommittedRequest.c_str(), v2CommittedCrc,
        v2CommittedLength, v2CommittedContext.c_str()
    };
    V2BundleBeginDecision decision = v2DecideBundleBegin(
        doc, v2Rx, committed, nonce.c_str(), nowMs);
    if (decision.action == V2_BUNDLE_BEGIN_REPLAY) {
        transport::v2Ack("bundle", "applied", "unchanged", "flash", nullptr, -1, 0,
              decision.replayContext, UINT32_MAX);
        return;
    }
    if (decision.action == V2_BUNDLE_BEGIN_REJECT) {
        v2BundleError(decision.error);
        return;
    }
    if (decision.action == V2_BUNDLE_BEGIN_START) {
        if (decision.candidate.length > bsMaxBundleBytes()) { v2BundleError("size"); return; }
        v2Rx = decision.candidate;
        state.bundleError = "";
        String error;
        if (!bsStartReceive(decision.candidate.length, error, v2Rx.owner, v2Rx.request)) {
            v2Rx.deadline = 0;
            v2BundleError(error.c_str()); return;
        }
    }
    transport::v2Response(200, String("{\"result\":\"applied\",\"next_offset\":") +
                String(decision.nextOffset) + "}");
}

void handleV2BundleChunk() {
    String mac;
    if (!transport::endpointTokenAuthorized(mac)) {
        transport::v2Response(401, "{\"result\":\"unauthorized\"}"); return;
    }
    if (!ownerAllows(v2Rx.owner)) { v2BundleError("occupied"); return; }
    const String request = transport::state.server.header("X-Request-Id");
    const String nonce = transport::state.server.header("X-Session-Nonce");
    const String offsetText = transport::state.server.header("X-Offset");
    const V2BundleChunkStartDecision start = v2DecideBundleChunkStart(
        v2Rx, request.c_str(), nonce.c_str(), offsetText.c_str(), clock::v2NowMs());
    if (!start.allowed) { v2BundleError(start.error); return; }
    const V2BundleChunkWriteDecision write = v2DecideBundleChunkWrite(
        v2Rx, start.offset, start.replay, 0, transport::state.v2HttpFrame.payloadLen);
    if (!write.allowed) { v2BundleError(write.error); return; }
    File file = LittleFS.open(v2RxPath, start.replay ? "r" : "a");
    if (!file || (start.replay && !file.seek(start.offset))) {
        if (file) file.close();
        v2BundleError("open"); return;
    }
    const uint8_t *payload = reinterpret_cast<const uint8_t *>(transport::state.v2HttpRaw.c_str()) + COMPACT_FRAME_HEADER;
    bool okay = true;
    if (start.replay) {
        for (size_t i = 0; i < transport::state.v2HttpFrame.payloadLen; ++i)
            if (file.read() != payload[i]) { okay = false; break; }
    } else {
        okay = file.write(payload, transport::state.v2HttpFrame.payloadLen) == transport::state.v2HttpFrame.payloadLen;
    }
    file.close();
    if (!okay) {
        if (!start.replay) v2Rx.deadline = 0;
        v2BundleError(start.replay ? "chunk_conflict" : "write"); return;
    }
    const V2BundleChunkEndDecision end = v2DecideBundleChunkEnd(
        v2Rx, start.offset, start.replay, transport::state.v2HttpFrame.payloadLen, clock::v2NowMs());
    if (!end.allowed) {
        if (!start.replay) v2Rx.deadline = 0;
        v2BundleError(end.error); return;
    }
    v2Rx.offset = end.nextOffset;
    transport::v2Response(200, String("{\"result\":\"applied\",\"next_offset\":") +
               String(v2Rx.offset) + "}");
}

void handleV2BundleCommit() {
    String mac;
    if (!transport::endpointTokenAuthorized(mac)) {
        transport::v2Response(401, "{\"result\":\"unauthorized\"}"); return;
    }
    JsonDocument doc;
    if (!transport::v2Command(transport::state.v2HttpBody, doc)) return;
    const String nonce = transport::v2Nonce();
    const uint64_t nowMs = clock::v2NowMs();
    const String bridgeIdFallback = transport::state.server.arg("bridge_id");
    const V2BundleFingerprint committed{
        v2CommittedOwner.c_str(), v2CommittedRequest.c_str(), v2CommittedCrc,
        v2CommittedLength, v2CommittedContext.c_str()
    };
    V2BundleCommitDecision decision = v2DecideBundleCommit(
        doc, v2Rx, committed, nonce.c_str(), nowMs, v2RxPath.c_str(),
        bridgeIdFallback.c_str());
    if (decision.action == V2_BUNDLE_COMMIT_REJECT) {
        v2BundleError(decision.error, "commit_validate");
        return;
    }
    if (decision.action == V2_BUNDLE_COMMIT_REPLAY) {
        transport::v2Ack("bundle", "applied", "unchanged", "flash", nullptr, -1, 0,
              decision.replayContext, UINT32_MAX);
        return;
    }
    if (decision.action == V2_BUNDLE_COMMIT_ALREADY_ACTIVE) {
        bsProfile(state.v2Profile);
        v2CommittedOwner = decision.owner; v2CommittedRequest = decision.request;
        v2CommittedCrc = decision.crc; v2CommittedLength = decision.length;
        v2CommittedContext = state.v2Profile.contextId;
        v2Rx.deadline = 0;
        LittleFS.remove(v2RxPath);
        transport::v2Ack("bundle", "applied", "unchanged", "flash", nullptr, -1, 0,
              state.v2Profile.contextId, UINT32_MAX);
        return;
    }
    char ctx[BS_CTX_LEN];
    snprintf(ctx, sizeof(ctx), "%08x%08x", state.v2CtxGen.next(), (unsigned)esp_random());
    String err;
    // Local: install by streaming the already-validated payload straight from
    // flash. v2DecideBundleCommit settled length/CRC/owner/job without holding the
    // bundle in RAM, so this re-opens the same file instead of rebuilding it.
    File f = LittleFS.open(v2RxPath, "r");
    if (!f || f.size() != decision.length) {
        if (f) f.close();
        v2BundleError("length", "commit_validate");
        return;
    }
    if (!bsInstall(f, decision.length, decision.crc, FW_TARGET_ID, RENDER_TARGET_ID, ctx, err, beginBundleReplacement)) {
        f.close();
        v2Rx.deadline = 0;
        // A failed preflight or failed invalidation can still leave old content
        // intact. Only restore it when the store confirms its current commit.
        if (bsProfile(state.v2Profile)) {
            state.v2BundleReady = true;
            state.bundleMaintenance = !v2ActiveLoad();
        }
        v2BundleError(err.c_str(), "install"); return;
    }
    f.close();
    v2CommittedOwner = decision.owner; v2CommittedRequest = decision.request;
    v2CommittedCrc = decision.crc; v2CommittedLength = decision.length;
    v2CommittedContext = ctx;
    v2Rx.deadline = 0;
    bsProfile(state.v2Profile);
    state.v2BundleReady = true;
    state.bundleError = "";
    state.v2DataSeq.beginContext(clock::v2NowMs(), 1);
    v2AppliedFields = "";
    state.bundleMaintenance = !v2ActiveLoad();
    uint32_t busyBefore = display::retained.rtcEpdBusyFails;
    uint32_t writesBefore = display::state.epdWriteCount;
    display::renderCurrent();
    state.v2DisplayState = displayStateAfterRender(writesBefore, display::state.epdWriteCount, busyBefore, display::retained.rtcEpdBusyFails);
    if (sync::state.syncEnabled) {
        syncAddReason(sync::retained.syncRtc, 1u << 4);
        int32_t args[] = {0, (int32_t)bsCommitSeq()};
        syncAppendEvent(sync::retained.syncRtc, sync::state.syncLogWakeSeq, millis(), 7, args, 2);
    }
    transport::v2Ack("bundle", "applied", state.v2DisplayState == 3 ? "failed" : "displayed",
          "flash", nullptr, -1, 0, state.v2Profile.contextId,
          UINT32_MAX);
}

} // namespace firmware::platform
