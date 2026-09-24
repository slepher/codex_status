#include "GUI_Paint.h"
#include "template_engine.h"
#include "refresh_policy.h"
#include "v2_state.h"
#include "v2_runtime.h"
#include "v2_data_command.h"
#include "v2_plan_command.h"
#include "v2_bundle_command.h"
#include "v2_activate_command.h"
#include "v2_claim_command.h"
#include "v2_command_envelope.h"
#include "v2_status_snapshot.h"
#include "bundle_store.h"
#include "font_asset.h"
#include "font_store.h"
#include "dev_log.h"
#include <LittleFS.h>

#include <ArduinoJson.h>

#include <cstring>
#include <cstdio>
#include <vector>

SerialClass Serial;
DevLogger DevLog;
// The host engine does not need the device log; the no-op definitions below keep
// every firmware source that logs (font store, bundle store, engine) linkable.
void DevLogger::printf(const char *, ...) {}
void DevLogger::println(const char *) {}
void DevLogger::println() {}
void DevLogger::print(const char *) {}
String DevLogger::dump() { return String(); }
void DevLogger::append(const char *, size_t) {}

namespace {
RgnSet g_rgn;
// Single active compiled template (the firmware keeps exactly one per device).
CtTemplate g_ct;
bool g_ct_valid = false;
int g_canvas_w = 200;
int g_canvas_h = 200;

struct BundleBeginHarness {
    V2BundleRx current;
    V2BundleRx candidate;
    bool hasCandidate = false;
    String committedOwner, committedRequest, committedContext;
    uint32_t committedCrc = 0, committedLength = 0;
};

struct ActivateHarness {
    bool configured = false;
    BsProfile profile{};
    String request, owner, templateId, expected, context;
};

void writeBundleRx(JsonObject out, const V2BundleRx &rx) {
    out["owner"] = rx.owner;
    out["request"] = rx.request;
    out["nonce"] = rx.nonce;
    out["length"] = rx.length;
    out["crc"] = rx.crc;
    out["offset"] = rx.offset;
    out["deadline"] = rx.deadline;
}
}

extern "C" {

void codex_set_canvas(int w, int h) {
    g_canvas_w = w;
    g_canvas_h = h;
    tplSetCanvas(w, h);
}

int codex_bundle_store_check(const char *json) {
    LittleFS.files.clear(); LittleFS.capacity = 1024 * 1024; LittleFS.writeBudget = -1;
    bsBegin();
    String err;
    if (!bsInstall(String(json), "codex-status-154g", "epd-ssd1681-200x200-1bpp", "ctx-a", err)) return 1;
    BsProfile profile;
    if (!bsHasSource(0) || !bsProfile(profile)) return 2;
    CtTemplate ct;
    if (!bsLoadCompiled(0, ct, err)) return 3;
    const auto original = LittleFS.files;
    const auto slot = LittleFS.files["/bundle/a.bin"];
    if (!bsSetActive(0, "ctx-b", err) || LittleFS.files["/bundle/a.bin"] != slot) return 4;
    if (!bsBegin() || !bsProfile(profile) || strcmp(profile.contextId, "ctx-b")) return 5;
    // A torn activation record must leave the previous activation usable.
    const auto activated = LittleFS.files;
    LittleFS.writeBudget = 20;
    if (bsSetActive(0, "ctx-c", err)) return 6;
    LittleFS.writeBudget = -1;
    if (!bsBegin() || !bsProfile(profile) || strcmp(profile.contextId, "ctx-b")) return 7;

    // Measure the write count, then cut power inside the slot, its patched
    // header, and the final commit record. Always recover the previous bundle.
    LittleFS.files = original; bsBegin(); LittleFS.writeBudget = 1000000;
    if (!bsInstall(String(json), "codex-status-154g", "epd-ssd1681-200x200-1bpp", "ctx-new", err)) return 8;
    long long totalWrites = 1000000 - LittleFS.writeBudget;
    for (long long cut : {0LL, 128LL, (long long)slot.size(), totalWrites - 1}) {
        LittleFS.files = original; LittleFS.writeBudget = -1; bsBegin();
        LittleFS.writeBudget = cut;
        if (bsInstall(String(json), "codex-status-154g", "epd-ssd1681-200x200-1bpp", "ctx-new", err)) return 9;
        LittleFS.writeBudget = -1;
        if (!bsBegin() || !bsProfile(profile) || strcmp(profile.contextId, "ctx-a")) return 10;
    }
    LittleFS.files = activated; bsBegin();
    LittleFS.capacity = LittleFS.usedBytes() + 10;
    if (bsInstall(String(json), "codex-status-154g", "epd-ssd1681-200x200-1bpp", "ctx-new", err)) return 11;
    if (LittleFS.files["/bundle/a.bin"] != slot) return 12;
    LittleFS.capacity = 1024 * 1024;

    // The current compiled cache is corrupt; rebuild from the independently
    // checksummed complete source payload retained in that same slot.
    auto &bytes = LittleFS.files["/bundle/a.bin"];
    bool found = false;
    for (size_t i = 0; i + 12 < bytes.size(); ++i) {
        if (!memcmp(bytes.data() + i, "CTP1", 4)) { bytes[i + 6] ^= 0x40; found = true; break; }
    }
    if (!found || !bsLoadCompiled(0, ct, err)) return 13;
    return 0;
}

int codex_render(const char *tmpl, const char *usage, const char *channel, const char *ip,
                 const char *sync_hhmm, int battery, const char *state, int offline_mins,
                 const char *mode, uint8_t *out, int out_len) {
    if (!tmpl || !out || out_len < ((g_canvas_w + 7) / 8) * g_canvas_h) return -2;
    Paint_NewImage(out, g_canvas_w, g_canvas_h, ROTATE_0, WHITE);
    Paint_Clear(WHITE);
    TplEnv env;
    env.channel = channel ? channel : "";
    env.ip = ip ? ip : "";
    env.syncHHMM = sync_hhmm ? sync_hhmm : "--:--";
    env.battery = battery;
    env.state = state ? state : "";
    env.offlineMins = offline_mins;
    env.mode = mode ? mode : "";
    return tplDraw(String(tmpl), String(usage ? usage : ""), env) ? 1 : 0;
}

int codex_validate(const char *tmpl, char *err, int err_len) {
    if (!tmpl) return 0;
    String message;
    bool ok = tplValidate(String(tmpl), message);
    if (!ok && err && err_len > 0) {
        std::strncpy(err, message.c_str(), (size_t)err_len - 1);
        err[err_len - 1] = '\0';
    }
    return ok ? 1 : 0;
}

// ---------------------------------------------------------------------------
// Compiled template path: compile once, render without parsing template JSON.
// ---------------------------------------------------------------------------

int codex_compile(const char *tmpl, char *err, int err_len) {
    if (!tmpl) return 0;
    String message;
    bool ok = tplCompile(String(tmpl), g_ct, message);
    g_ct_valid = ok;
    if (!ok && err && err_len > 0) {
        std::strncpy(err, message.c_str(), (size_t)err_len - 1);
        err[err_len - 1] = '\0';
    }
    return ok ? 1 : 0;
}

int codex_render_compiled(const char *usage, const char *channel, const char *ip,
                          const char *sync_hhmm, int battery, const char *state,
                          int offline_mins, const char *mode, uint8_t *out, int out_len) {
    if (!out || out_len < ((g_canvas_w + 7) / 8) * g_canvas_h) return -2;
    if (!g_ct_valid) return -4;
    Paint_NewImage(out, g_canvas_w, g_canvas_h, ROTATE_0, WHITE);
    Paint_Clear(WHITE);
    TplEnv env;
    env.channel = channel ? channel : "";
    env.ip = ip ? ip : "";
    env.syncHHMM = sync_hhmm ? sync_hhmm : "--:--";
    env.battery = battery;
    env.state = state ? state : "";
    env.offlineMins = offline_mins;
    env.mode = mode ? mode : "";
    return tplDrawCt(g_ct, String(usage ? usage : ""), env) ? 1 : 0;
}

int codex_ct_req_count(void) { return g_ct_valid ? g_ct.reqCount : -1; }

int codex_ct_req_path(int i, char *out, int cap) {
    if (!g_ct_valid || i < 0 || i >= g_ct.reqCount || !out || cap <= 0) return 0;
    std::strncpy(out, g_ct.reqs[i].path, (size_t)cap - 1);
    out[cap - 1] = '\0';
    return 1;
}

int codex_ct_op_count(void) { return g_ct_valid ? g_ct.opCount : -1; }

int codex_ct_source_crc(void) { return g_ct_valid ? (int)g_ct.sourceCrc : 0; }

int codex_ct_serialize(uint8_t *out, int cap) {
    if (!g_ct_valid || !out || cap <= 0) return -1;
    size_t written = 0;
    if (!tplCtSerialize(g_ct, out, (size_t)cap, written)) return -1;
    return (int)written;
}

int codex_ct_deserialize(const uint8_t *in, int len, char *err, int err_len) {
    if (!in || len <= 0) return 0;
    CtTemplate tmp;
    String message;
    if (!tplCtDeserialize(in, (size_t)len, tmp, message)) {
        if (err && err_len > 0) {
            std::strncpy(err, message.c_str(), (size_t)err_len - 1);
            err[err_len - 1] = '\0';
        }
        return 0;
    }
    g_ct = tmp;
    g_ct_valid = true;
    return 1;
}

// Atomic compile/inspect operation using a local record: callers never observe
// another request's global active template between compile and serialization.
int codex_ct_artifact(const char *source, const uint8_t *blob, int len,
                      uint8_t *out, int cap, char *metadata, int metaCap) {
    CtTemplate ct{};
    String error;
    bool ok = source ? tplCompile(String(source), ct, error)
                     : tplCtDeserialize(blob, len, ct, error);
    if (!ok) {
        std::snprintf(metadata, metaCap, "%s", error.c_str());
        return -1;
    }
    JsonDocument doc;
    doc["template_id"] = ct.id;
    doc["op_count"] = ct.opCount;
    doc["compiler_abi"] = ct.abi;
    doc["source_crc"] = ct.sourceCrc;
    JsonArray reqs = doc["requirements"].to<JsonArray>();
    for (uint8_t i = 0; i < ct.reqCount; ++i) {
        JsonObject req = reqs.add<JsonObject>();
        req["field"] = ct.reqs[i].path;
        req["local"] = ct.reqs[i].kind > 9;
    }
    JsonArray resources = doc["resources"].to<JsonArray>();
    for (uint8_t i = 0; i < ct.resCount; ++i) resources.add(ct.res[i].bits);
    if (measureJson(doc) >= (size_t)metaCap) return -1;
    serializeJson(doc, metadata, metaCap);
    size_t written = 0;
    if (!tplCtSerialize(ct, out, cap, written)) return -1;
    return (int)written;
}

// Display-safety policy host bridge (refresh_policy.cpp): derive the semantic
// regions once, then run decisions against caller-provided 1bpp framebuffers.
void codex_set_panel(int w, int h) {
    rgnSetPanel(w, h);
}

int codex_rgn_build(const char *tmpl) {
    if (!tmpl) return -1;
    bool ok = rgnBuild(String(tmpl), g_rgn);
    if (!ok || g_rgn.wholeFrame) return -1;
    return (int)g_rgn.n;
}

int codex_rgn_build_ct(const uint8_t *blob, int len, char *err, int errcap) {
    if (!blob || len <= 0) return -1;
    CtTemplate ct;
    String message;
    if (!tplCtDeserialize(blob, (size_t)len, ct, message)) {
        if (err && errcap > 0) {
            std::strncpy(err, message.c_str(), (size_t)errcap - 1);
            err[errcap - 1] = '\0';
        }
        return -1;
    }
    bool ok = rgnBuildCt(ct, g_rgn);
    if (!ok || g_rgn.wholeFrame) return -1;
    return (int)g_rgn.n;
}

int codex_rgn_decide(const uint8_t *old_fb, const uint8_t *new_fb, int trusted,
                     int force_full, int clean, char *out, int out_len) {
    RfnDecision d = rgnDecide(g_rgn, old_fb, new_fb, trusted != 0, force_full != 0,
                              clean != 0);
    if (out && out_len > 0) {
        std::snprintf(out, (size_t)out_len,
                      "{\"action\":\"%s\",\"reason\":\"%s\",\"region\":%d,"
                      "\"changed\":%u,\"dirty\":%u,\"outside\":%u}",
                      rfnActionName(d.action), rfnReasonName(d.reason),
                      d.region == 0xFF ? -1 : (int)d.region, d.changed, d.dirty,
                      d.outside);
    }
    return d.action;
}

int codex_rgn_on_partial(void) {
    rgnOnPartial(g_rgn);
    return 0;
}

int codex_rgn_on_full(void) {
    rgnOnFull(g_rgn);
    return 0;
}

// ---------------------------------------------------------------------------
// v2 device state machines (v2_state.h) exposed for host tests.
// ---------------------------------------------------------------------------

uint32_t codex_v2_fields_crc(const char *fieldsJson) {
    if (!fieldsJson) return 0;
    JsonDocument doc;
    if (deserializeJson(doc, fieldsJson)) return 0;
    JsonArrayConst fields = doc.as<JsonArrayConst>();
    if (fields.isNull()) return 0;
    return v2DataFieldsCrc(fields);
}

void *codex_v2_plan_new(void) { return new V2PlanState(); }
void codex_v2_plan_free(void *p) { delete (V2PlanState *)p; }

int codex_v2_plan_accept(void *p, uint64_t id, int mode, uint32_t duration, uint64_t now_ms,
                         int provisional, uint32_t max_light) {
    V2PowerPlan plan{id, (uint8_t)mode, duration, 60};
    return (int)((V2PlanState *)p)->accept(plan, now_ms, provisional != 0, max_light);
}
uint32_t codex_v2_plan_remaining(void *p, uint64_t now_ms) {
    return ((V2PlanState *)p)->remainingS(now_ms);
}
int codex_v2_plan_decide(void *p, const char *message, uint64_t now_ms,
                         int provisional, char *out, int cap) {
    if (!p || !message || !out || cap <= 0) return -99;
    JsonDocument doc;
    V2PlanDecision decision;
    if (deserializeJson(doc, message)) {
        decision.error = "plan_shape";
    } else {
        decision = v2DecidePlan(doc, *(V2PlanState *)p, now_ms, provisional != 0);
    }
    V2PlanState &state = *(V2PlanState *)p;
    JsonDocument result;
    result["accepted"] = decision.accepted;
    result["result"] = decision.result;
    result["display"] = decision.display;
    if (decision.error) result["error"] = decision.error;
    result["plan_id"] = decision.planId;
    result["granted_s"] = decision.grantedS;
    result["include_context"] = decision.includeContext;
    result["high_id"] = state.highId();
    result["accepted_id"] = state.acceptedId();
    result["state_accepted"] = state.accepted();
    result["state_granted_s"] = state.grantedS();
    result["deadline_ms"] = state.deadlineMs();
    result["remaining_s"] = state.remainingS(now_ms);
    result["light_active"] = state.lightActive(now_ms);
    result["from_boot"] = state.fromBoot();
    return (int)serializeJson(result, out, (size_t)cap);
}
void *codex_v2_bundle_begin_new(void) { return new BundleBeginHarness(); }
void codex_v2_bundle_begin_free(void *p) { delete (BundleBeginHarness *)p; }
void codex_v2_bundle_begin_committed(void *p, const char *owner,
                                     const char *request, uint32_t crc,
                                     uint32_t length, const char *context) {
    if (!p) return;
    BundleBeginHarness &h = *(BundleBeginHarness *)p;
    h.committedOwner = owner ? owner : "";
    h.committedRequest = request ? request : "";
    h.committedCrc = crc;
    h.committedLength = length;
    h.committedContext = context ? context : "";
}
int codex_v2_bundle_begin_seed_rx(void *p, const char *owner,
                                  const char *request, const char *nonce,
                                  uint32_t length, uint32_t crc,
                                  uint32_t offset, uint64_t deadline) {
    if (!p || offset > length) return 0;
    V2BundleRx rx;
    if (!rx.begin(owner, request, nonce, length, crc, 0)) return 0;
    rx.offset = offset;
    rx.deadline = deadline;
    ((BundleBeginHarness *)p)->current = rx;
    return 1;
}
int codex_v2_bundle_begin_decide(void *p, const char *message,
                                 const char *nonce, uint64_t now_ms,
                                 char *out, int cap) {
    if (!p || !message || !nonce || !out || cap <= 0) return -99;
    BundleBeginHarness &h = *(BundleBeginHarness *)p;
    JsonDocument doc;
    if (deserializeJson(doc, message)) return -2;
    const V2BundleFingerprint committed{
        h.committedOwner.c_str(), h.committedRequest.c_str(), h.committedCrc,
        h.committedLength, h.committedContext.c_str()
    };
    V2BundleBeginDecision decision = v2DecideBundleBegin(
        doc, h.current, committed, nonce, now_ms);
    h.hasCandidate = decision.action == V2_BUNDLE_BEGIN_START;
    if (h.hasCandidate) h.candidate = decision.candidate;

    JsonDocument result;
    const char *action = decision.action == V2_BUNDLE_BEGIN_REPLAY ? "replay"
                       : decision.action == V2_BUNDLE_BEGIN_RESUME ? "resume"
                       : decision.action == V2_BUNDLE_BEGIN_START ? "start" : "reject";
    result["action"] = action;
    if (decision.error) result["error"] = decision.error;
    if (decision.replayContext) result["replay_context"] = decision.replayContext;
    result["next_offset"] = decision.nextOffset;
    writeBundleRx(result["current"].to<JsonObject>(), h.current);
    if (h.hasCandidate) writeBundleRx(result["candidate"].to<JsonObject>(), h.candidate);
    return (int)serializeJson(result, out, (size_t)cap);
}
int codex_v2_bundle_begin_commit_candidate(void *p) {
    if (!p) return 0;
    BundleBeginHarness &h = *(BundleBeginHarness *)p;
    if (!h.hasCandidate) return 0;
    h.current = h.candidate;
    h.hasCandidate = false;
    return 1;
}
int codex_v2_bundle_chunk_start(void *p, const char *request, const char *nonce,
                                const char *offset, uint64_t now_ms,
                                char *out, int cap) {
    if (!p || !out || cap <= 0) return -99;
    BundleBeginHarness &h = *(BundleBeginHarness *)p;
    V2BundleChunkStartDecision decision = v2DecideBundleChunkStart(
        h.current, request, nonce, offset, now_ms);
    JsonDocument result;
    result["allowed"] = decision.allowed;
    result["replay"] = decision.replay;
    result["offset"] = decision.offset;
    if (decision.error) result["error"] = decision.error;
    writeBundleRx(result["current"].to<JsonObject>(), h.current);
    return (int)serializeJson(result, out, (size_t)cap);
}
int codex_v2_bundle_chunk_write(void *p, uint32_t offset, int replay,
                                uint32_t processed, uint32_t size,
                                char *out, int cap) {
    if (!p || !out || cap <= 0) return -99;
    BundleBeginHarness &h = *(BundleBeginHarness *)p;
    V2BundleChunkWriteDecision decision = v2DecideBundleChunkWrite(
        h.current, offset, replay != 0, processed, size);
    JsonDocument result;
    result["allowed"] = decision.allowed;
    if (decision.error) result["error"] = decision.error;
    writeBundleRx(result["current"].to<JsonObject>(), h.current);
    return (int)serializeJson(result, out, (size_t)cap);
}
int codex_v2_bundle_chunk_end(void *p, uint32_t offset, int replay,
                              uint32_t processed, uint64_t now_ms,
                              char *out, int cap) {
    if (!p || !out || cap <= 0) return -99;
    BundleBeginHarness &h = *(BundleBeginHarness *)p;
    V2BundleChunkEndDecision decision = v2DecideBundleChunkEnd(
        h.current, offset, replay != 0, processed, now_ms);
    JsonDocument result;
    result["allowed"] = decision.allowed;
    if (decision.error) result["error"] = decision.error;
    result["next_offset"] = decision.nextOffset;
    writeBundleRx(result["current"].to<JsonObject>(), h.current);
    return (int)serializeJson(result, out, (size_t)cap);
}
int codex_v2_bundle_commit_reset_store() {
    LittleFS.files.clear();
    LittleFS.dirs.clear();
    LittleFS.capacity = 1024 * 1024;
    LittleFS.writeBudget = -1;
    bsBegin();
    return 1;
}
int codex_v2_bundle_commit_install_active(const char *bundle, const char *context) {
    if (!bundle || !context || !codex_v2_bundle_commit_reset_store()) return 0;
    String error;
    return bsInstall(String(bundle), "codex-status-154g",
                     "epd-ssd1681-200x200-1bpp", context, error) ? 1 : 0;
}
int codex_v2_bundle_commit_seed_rx(void *p, const char *owner,
                                   const char *request, const char *nonce,
                                   uint32_t length, uint32_t crc, uint32_t offset,
                                   uint64_t deadline) {
    if (!p) return 0;
    V2BundleRx rx;
    if (!rx.begin(owner, request, nonce, length, crc, 0)) return 0;
    if (offset > length) return 0;
    rx.offset = offset;
    rx.deadline = deadline;
    ((BundleBeginHarness *)p)->current = rx;
    return 1;
}
int codex_v2_bundle_commit_write_file(const char *path,
                                      const uint8_t *body, int length) {
    if (!path || !body || length <= 0) return 0;
    File file = LittleFS.open(path, "w");
    if (!file) return 0;
    size_t written = file.write(body, (size_t)length);
    file.close();
    return written == (size_t)length ? 1 : 0;
}
int codex_v2_bundle_commit_decide(void *p, const char *message,
                                  const char *nonce, uint64_t now_ms,
                                  const char *path, const char *fallback,
                                  char *out, int cap,
                                  char *body_out, int body_cap) {
    if (!p || !message || !nonce || !path || !out || cap <= 0 ||
        !body_out || body_cap <= 0) return -99;
    BundleBeginHarness &h = *(BundleBeginHarness *)p;
    JsonDocument doc;
    if (deserializeJson(doc, message)) return -2;
    const V2BundleFingerprint committed{
        h.committedOwner.c_str(), h.committedRequest.c_str(), h.committedCrc,
        h.committedLength, h.committedContext.c_str()
    };
    String body;
    V2BundleCommitDecision decision = v2DecideBundleCommit(
        doc, h.current, committed, nonce, now_ms, path, fallback, body);
    JsonDocument result;
    const char *action = decision.action == V2_BUNDLE_COMMIT_REPLAY ? "replay"
                       : decision.action == V2_BUNDLE_COMMIT_ALREADY_ACTIVE ? "already_active"
                       : decision.action == V2_BUNDLE_COMMIT_INSTALL ? "install" : "reject";
    result["action"] = action;
    if (decision.error) result["error"] = decision.error;
    if (decision.replayContext) result["replay_context"] = decision.replayContext;
    result["owner"] = decision.owner;
    result["request"] = decision.request;
    result["crc"] = decision.crc;
    result["length"] = decision.length;
    writeBundleRx(result["current"].to<JsonObject>(), h.current);
    size_t written = serializeJson(result, out, (size_t)cap);
    if (!written) return -3;
    if (body.length() >= (size_t)body_cap) return -4;
    memcpy(body_out, body.c_str(), body.length() + 1);
    return (int)written;
}
void *codex_v2_activate_new(int configured, const char *profile_json) {
    if (!profile_json) return nullptr;
    auto *h = new ActivateHarness();
    JsonDocument profile;
    if (deserializeJson(profile, profile_json)) { delete h; return nullptr; }
    h->configured = configured != 0;
    const char *context = profile["context"] | "";
    strncpy(h->profile.contextId, context, sizeof(h->profile.contextId) - 1);
    JsonArrayConst ids = profile["ids"].as<JsonArrayConst>();
    if (ids.size() > BS_MAX_TEMPLATES) { delete h; return nullptr; }
    for (JsonVariantConst id : ids) {
        const char *value = id | "";
        strncpy(h->profile.ids[h->profile.count], value, BS_ID_LEN - 1);
        ++h->profile.count;
    }
    return h;
}
void codex_v2_activate_free(void *p) { delete (ActivateHarness *)p; }
void codex_v2_activate_seed_fingerprint(void *p, const char *request,
                                        const char *owner, const char *template_id,
                                        const char *expected, const char *context) {
    if (!p) return;
    ActivateHarness &h = *(ActivateHarness *)p;
    h.request = request ? request : "";
    h.owner = owner ? owner : "";
    h.templateId = template_id ? template_id : "";
    h.expected = expected ? expected : "";
    h.context = context ? context : "";
}
int codex_v2_activate_decide(void *p, const char *message, char *out, int cap) {
    if (!p || !message || !out || cap <= 0) return -99;
    ActivateHarness &h = *(ActivateHarness *)p;
    JsonDocument doc;
    if (deserializeJson(doc, message)) return -2;
    V2ActivateDecision decision = v2DecideActivate(
        doc, h.configured, h.profile, h.request.c_str(), h.owner.c_str(),
        h.templateId.c_str(), h.expected.c_str(), h.context.c_str());
    JsonDocument result;
    result["action"] = decision.action == V2_ACTIVATE_SWITCH ? "switch"
                      : decision.action == V2_ACTIVATE_REPLAY ? "replay" : "reject";
    result["result"] = decision.result;
    result["display"] = decision.display;
    if (decision.error) result["error"] = decision.error;
    if (decision.context) result["context"] = decision.context;
    result["index"] = decision.index;
    result["request"] = decision.request;
    result["owner"] = decision.owner;
    result["template_id"] = decision.templateId;
    result["expected"] = decision.expected;
    result["saved_request"] = h.request;
    result["saved_owner"] = h.owner;
    result["saved_template"] = h.templateId;
    result["saved_expected"] = h.expected;
    result["saved_context"] = h.context;
    return (int)serializeJson(result, out, (size_t)cap);
}
int codex_v2_claim_decide(const char *message, int have_owner,
                          const char *current_json, char *out, int cap) {
    if (!message || !current_json || !out || cap <= 0) return -99;
    JsonDocument doc, currentDoc;
    if (deserializeJson(doc, message) || deserializeJson(currentDoc, current_json)) return -2;
    V2ClaimArgs args = v2PrepareClaim(
        String(doc["id"] | ""), String(doc["name"] | ""),
        String(doc["host"] | ""), String(doc["port"] | ""),
        String(doc["lease"] | ""), doc["has_lease"] | false,
        doc["force"] | false, doc["release"] | false);
    OwnerRec current;
    current.id = currentDoc["id"] | "";
    current.name = currentDoc["name"] | "";
    current.host = currentDoc["host"] | "";
    current.port = currentDoc["port"] | 0;
    current.since = currentDoc["since"] | 0;
    current.lastSeen = currentDoc["last_seen"] | 0;
    current.lease = currentDoc["lease"] | 300;
    JsonDocument result;
    result["valid_id"] = args.validId;
    result["request_id"] = args.request.id;
    result["request_name"] = args.request.name;
    result["request_host"] = args.request.host;
    result["request_port"] = args.request.port;
    result["request_lease"] = args.request.lease;
    if (!args.validId) {
        result["action"] = "args";
    } else {
        V2ClaimDecision decision = v2DecideClaim(args, have_owner != 0, current);
        const char *action = decision.action == V2_CLAIM_RELEASE_EMPTY ? "release_empty"
                           : decision.action == V2_CLAIM_OCCUPIED ? "occupied"
                           : decision.action == V2_CLAIM_RELEASE ? "release" : "claim";
        result["action"] = action;
        result["keep_since"] = decision.keepSince;
        result["new_claim"] = decision.newClaim;
    }
    JsonObject unchanged = result["current"].to<JsonObject>();
    unchanged["id"] = current.id;
    unchanged["name"] = current.name;
    unchanged["host"] = current.host;
    unchanged["port"] = current.port;
    unchanged["since"] = current.since;
    unchanged["last_seen"] = current.lastSeen;
    unchanged["lease"] = current.lease;
    return (int)serializeJson(result, out, (size_t)cap);
}
int codex_v2_command_parse(const char *message, char *out, int cap) {
    if (!message || !out || cap <= 0) return -99;
    JsonDocument doc;
    const char *error = v2ParseCommand(String(message), doc);
    JsonDocument result;
    result["parsed"] = !error;
    if (error) result["error"] = error;
    else result["bridge_id"] = doc["bridge_id"] | "";
    return (int)serializeJson(result, out, (size_t)cap);
}
int codex_v2_command_check(const char *message, const char *current_mac,
                           const char *nonce, char *out, int cap) {
    if (!message || !current_mac || !nonce || !out || cap <= 0) return -99;
    JsonDocument doc;
    if (v2ParseCommand(String(message), doc)) return -2;
    String sessionNonce(nonce);
    V2CommandSessionDecision decision = v2CheckCommandSession(
        doc, String(current_mac), &sessionNonce);
    JsonDocument result;
    result["accepted"] = decision.accepted;
    if (decision.error) result["error"] = decision.error;
    result["bridge_id"] = doc["bridge_id"] | "";
    result["request_id"] = doc["request_id"] | "";
    return (int)serializeJson(result, out, (size_t)cap);
}
int codex_v2_build_ack(const char *op, const char *result, const char *display,
                       const char *retention, const char *error, int64_t seq,
                       uint64_t plan_id, const char *context,
                       uint32_t accepted_remaining_s, const char *fw_target,
                       char *out, int cap) {
    if (!out || cap <= 0) return -99;
    String body = v2BuildAck(op, result, display, retention, error, seq,
                             plan_id, context, accepted_remaining_s, fw_target);
    if (body.length() >= (size_t)cap) return -3;
    memcpy(out, body.c_str(), body.length() + 1);
    return (int)body.length();
}
int codex_v2_status_snapshot(const char *input, char *out, int cap) {
    if (!input || !out || cap <= 0) return -99;
    JsonDocument doc;
    if (deserializeJson(doc, input)) return -2;
    V2StatusSnapshot snapshot;
    snapshot.mac = doc["mac"] | "";
    snapshot.sessionNonce = doc["session_nonce"] | "";
    BsProfile profile{};
    const char *context = doc["context"] | "";
    const char *job = doc["job_id"] | "";
    strncpy(profile.contextId, context, sizeof(profile.contextId) - 1);
    strncpy(profile.jobId, job, sizeof(profile.jobId) - 1);
    JsonArrayConst ids = doc["template_ids"].as<JsonArrayConst>();
    if (ids.size() > BS_MAX_TEMPLATES) return -3;
    for (JsonVariantConst id : ids) {
        const char *value = id | "";
        strncpy(profile.ids[profile.count], value, BS_ID_LEN - 1);
        ++profile.count;
    }
    snapshot.profile = &profile;
    snapshot.configured = doc["configured"] | false;
    snapshot.activeTemplateId = doc["active_template_id"] | "";
    V2DataSeq dataSeq;
    uint64_t appliedSeq = doc["applied_seq"] | 0ULL;
    if (appliedSeq) dataSeq.noteApplied(appliedSeq, doc["data_crc"] | 0u);
    snapshot.dataSeq = &dataSeq;
    snapshot.displayState = doc["display_state_code"] | 0;
    snapshot.commitSeq = doc["commit_seq"] | 0u;
    snapshot.deepSleep = doc["deep_sleep"] | false;
    V2PlanState plan;
    if (doc["plan_accepted"] | false) {
        V2PowerPlan active{};
        active.planId = doc["plan_id"] | 0ULL;
        active.mode = !strcmp(doc["plan_mode"] | "sleep", "light")
            ? V2_PLAN_LIGHT : V2_PLAN_SLEEP;
        active.lightDurationS = doc["granted_s"] | 0u;
        active.rendezvousPeriodS = V2_RENDEZVOUS_S;
        plan.accept(active, doc["plan_accepted_at_ms"] | 0ULL,
                    false, V2_MAX_LIGHT_S);
    }
    snapshot.plan = &plan;
    snapshot.provisional = doc["provisional"] | false;
    snapshot.bootMs = doc["boot_ms"] | 0ULL;
    snapshot.nowMs = doc["now_ms"] | 0ULL;
    snapshot.battery = doc["battery"] | 0;
    String status = v2BuildStatusSnapshot(snapshot);
    JsonDocument statusDoc, result;
    if (deserializeJson(statusDoc, status)) return -4;
    result["status"] = statusDoc.as<JsonVariantConst>();
    result["state_after"]["applied_seq"] = dataSeq.appliedSeq();
    result["state_after"]["next_seq"] = dataSeq.nextSeq();
    result["state_after"]["plan_id"] = plan.acceptedId();
    result["state_after"]["plan_granted_s"] = plan.grantedS();
    return (int)serializeJson(result, out, (size_t)cap);
}
uint64_t codex_v2_plan_high(void *p) { return ((V2PlanState *)p)->highId(); }
int codex_v2_plan_light_active(void *p, uint64_t now_ms) {
    return ((V2PlanState *)p)->lightActive(now_ms) ? 1 : 0;
}
uint32_t codex_v2_boot_remaining(uint64_t t_boot_ms, uint64_t now_ms) {
    return V2PlanState::bootProvisionalRemaining(t_boot_ms, now_ms);
}

void *codex_v2_seq_new(void) { return new V2DataSeq(); }
int codex_v2_accept_data(void *p, const char *message) {
    CtTemplate ct{};
    ct.reqCount = 1;
    ct.reqs[0].kind = 1;
    std::strcpy(ct.reqs[0].path, "bridge.label");
    String usage, error;
    return v2AcceptData(ct, String(message), "ctx", *(V2DataSeq *)p, usage, error);
}

// Host-test hook: validate a Data message against a freshly compiled template,
// so tests cover local (device.*) requirements the bridge does not transmit.
int codex_v2_accept_data_template(const char *source, const char *message) {
    if (!source || !message) return -99;
    CtTemplate ct{};
    String error;
    if (!tplCompile(String(source), ct, error)) return -98;
    V2DataSeq seq;
    String usage;
    return v2AcceptData(ct, String(message), "ctx", seq, usage, error);
}
int codex_v2_data_decide(void *p, int configured, const char *source,
                        const char *message, const char *context,
                        char *out, int cap) {
    if (!p || !message || !out || cap <= 0) return -99;
    CtTemplate ct{};
    String compileError;
    const bool validTemplate = source && tplCompile(String(source), ct, compileError);
    V2DataDecision decision;
    if (configured && !validTemplate) {
        decision.result = "rejected";
        decision.display = "failed";
        decision.error = compileError.length() ? compileError : String("template");
    } else {
        JsonDocument input;
        uint64_t seq = 0;
        if (!deserializeJson(input, message)) seq = input["seq"] | 0ULL;
        decision = v2DecideData(configured != 0, validTemplate ? &ct : nullptr,
                                String(message), seq, context, *(V2DataSeq *)p);
    }
    JsonDocument result;
    result["first_applied"] = decision.firstApplied;
    result["seq"] = decision.seq;
    result["result"] = decision.result;
    if (decision.display) result["display"] = decision.display;
    if (decision.error.length()) result["error"] = decision.error;
    result["usage"] = decision.usage;
    result["include_context"] = decision.includeContext;
    size_t written = serializeJson(result, out, (size_t)cap);
    return written ? (int)written : -1;
}
void codex_v2_seq_free(void *p) { delete (V2DataSeq *)p; }
void codex_v2_seq_begin(void *p, uint64_t now_ms, uint32_t keep_next) {
    ((V2DataSeq *)p)->beginContext(now_ms, keep_next);
}
int codex_v2_seq_observe(void *p, uint64_t seq, uint32_t crc) {
    return (int)((V2DataSeq *)p)->observe(seq, crc);
}
void codex_v2_seq_note(void *p, uint64_t seq, uint32_t crc) {
    ((V2DataSeq *)p)->noteApplied(seq, crc);
}
uint64_t codex_v2_seq_next(void *p) { return ((V2DataSeq *)p)->nextSeq(); }
uint64_t codex_v2_seq_applied(void *p) { return ((V2DataSeq *)p)->appliedSeq(); }
uint32_t codex_v2_crc(const uint8_t *data, int len) {
    return v2Crc32(data, (size_t)len);
}

int codex_v2_checkpoint_check() {
    V2DataSeq before, after;
    before.noteApplied(9, 0x12345678);
    V2DataCheckpoint checkpoint;
    checkpoint.save("ctx", before);
    if (!checkpoint.restore("ctx", after)) return 1;
    if (after.observe(9, 0x12345678) != V2_DATA_UNCHANGED ||
        after.observe(9, 0x87654321) != V2_DATA_CONFLICT ||
        after.observe(8, 0) != V2_DATA_STALE) return 2;
    if (checkpoint.restore("other", after)) return 3;
    checkpoint.seq++;
    if (checkpoint.restore("ctx", after)) return 4;
    return 0;
}

int codex_v2_bundle_rx_check() {
    V2BundleRx rx;
    if (rx.begin("owner", "req", "nonce", 262145, 1, 0)) return 1;
    if (!rx.begin("owner", "req", "nonce", 100, 1, 10)) return 2;
    if (rx.matches("other", "req", "nonce") || rx.matches("owner", "old", "nonce") ||
        rx.matches("owner", "req", "old")) return 3;
    if (rx.append(1, 10, 20) || rx.append(0, 101, 20) || rx.append(0, 0, 20)) return 4;
    if (!rx.append(0, 100, 20) || rx.complete(100, 1, 20)) return 5;
    rx.offset = 100;
    if (!rx.complete(100, 1, 20) || rx.complete(99, 1, 20) || rx.complete(100, 2, 20)) return 6;
    if (rx.complete(100, 1, 120010) || rx.append(100, 1, 120010)) return 7;
    return 0;
}

int codex_dirty_window(const uint8_t *oldFrame, const uint8_t *newFrame,
                       int width, int height, uint16_t *rect) {
    DirtyWindow window;
    if (!rgnDirtyWindow(oldFrame, newFrame, width, height, window)) return 0;
    rect[0] = window.x0; rect[1] = window.y0; rect[2] = window.x1; rect[3] = window.y1;
    return 1;
}

int codex_font_asset_check(const uint8_t *bytes, int len, char *out, int cap) {
    if (!bytes || len <= 0 || !out || cap <= 0) return -1;
    FontAssetInfo info;
    String err;
    if (!fontAssetValidate(bytes, (size_t)len, info, err)) {
        std::snprintf(out, (size_t)cap, "err=%s", err.c_str());
        return 0;
    }
    if (!fontAssetMatchesTarget(info, err)) {
        std::snprintf(out, (size_t)cap, "err=%s", err.c_str());
        return 0;
    }
    Note4PropFont view;
    bool viewOk = fontAssetView(bytes, (size_t)len, info, view);
    std::snprintf(out, (size_t)cap,
                  "id=%s name=%s family=%s coverage=%s size=%u weight=%u bpp=%u pf=%s "
                  "line=%u base=%u maxadv=%u blob=%u glyphs=%u filled=%u hint=%u bytes=%u view=%d",
                  info.id, info.name, info.family, info.coverage, (unsigned)info.sizePx,
                  (unsigned)info.weight, (unsigned)info.bpp,
                  fontAssetPixelFormatName(info.pixelFormat), (unsigned)info.lineHeight,
                  (unsigned)info.baseLine, (unsigned)info.maxAdv, (unsigned)info.blobBytes,
                  (unsigned)info.glyphCount, (unsigned)info.filledGlyphs,
                  (unsigned)info.hint, (unsigned)info.bytes, viewOk ? 1 : 0);
    return 1;
}

// Font store (font_store.cpp): host tests drive the real device store, including
// its atomic write path, its inventory and its Profile pruning.
void codex_font_store_reset() {
    LittleFS.files.clear();
    LittleFS.dirs.clear();
    LittleFS.capacity = 1024 * 1024;
    LittleFS.writeBudget = -1;
    fontStoreBegin("");
}

void codex_font_store_budget(long long budget) {
    LittleFS.writeBudget = budget;
}

int codex_font_store_begin(const char *profile) {
    return fontStoreBegin(profile) ? 1 : 0;
}

// 1 = stored, 0 = already present (no-op), -1 = rejected (`out` carries the reason).
int codex_font_store_write(const uint8_t *bytes, int len, char *out, int cap) {
    if (!bytes || len <= 0 || !out || cap <= 0) return -1;
    FontAssetInfo info;
    bool stored = false;
    String err;
    if (!fontStoreWrite(bytes, (size_t)len, info, stored, err)) {
        std::snprintf(out, (size_t)cap, "err=%s", err.c_str());
        return -1;
    }
    std::snprintf(out, (size_t)cap, "id=%s name=%s stored=%d", info.id, info.name,
                  stored ? 1 : 0);
    return stored ? 1 : 0;
}

int codex_font_store_inventory(char *out, int cap) {
    if (!out || cap <= 0) return -1;
    FontAssetInfo items[FONT_STORE_MAX_PER_PROFILE];
    int bad = 0;
    const int n = fontStoreInventory(items, FONT_STORE_MAX_PER_PROFILE, &bad);
    int used = std::snprintf(out, (size_t)cap, "n=%d bad=%d", n, bad);
    for (int i = 0; i < n && used < cap - 48; i++) {
        used += std::snprintf(out + used, (size_t)(cap - used), " %s:%s:%u",
                              items[i].id, items[i].name, (unsigned)items[i].bytes);
    }
    return n;
}

int codex_font_store_load(const char *id, char *out, int cap) {
    if (!id || !out || cap <= 0) return -1;
    std::vector<uint8_t> buf(FONT_ASSET_MAX_BYTES);
    size_t len = 0;
    FontAssetInfo info;
    String err;
    if (!fontStoreLoad(id, buf.data(), buf.size(), len, info, err)) {
        std::snprintf(out, (size_t)cap, "err=%s", err.c_str());
        return 0;
    }
    // Report a content digest as well: the test compares it against the file it
    // pushed, which proves the round trip is byte-exact.
    std::snprintf(out, (size_t)cap, "id=%s name=%s bytes=%u crc=%08x", info.id,
                  info.name, (unsigned)len, v2Crc32(buf.data(), len));
    return 1;
}

// `ids` is a comma-separated keep list (empty = keep nothing).
int codex_font_store_prune(const char *ids, char *out, int cap) {
    std::vector<String> keep;
    if (ids && *ids) {
        String list(ids);
        int start = 0;
        while (start <= (int)list.length()) {
            int comma = list.indexOf(',', start);
            if (comma < 0) comma = list.length();
            keep.push_back(list.substring(start, comma));
            start = comma + 1;
        }
    }
    std::vector<const char *> pointers;
    for (const String &k : keep) pointers.push_back(k.c_str());
    const int removed = fontStorePrune(pointers.data(), (int)pointers.size());
    int left = 0;
    uint32_t bytes = 0;
    fontStoreUsage(bytes, left);
    std::snprintf(out, (size_t)cap, "removed=%d left=%d bytes=%u", removed, left,
                  (unsigned)bytes);
    return removed;
}

int codex_font_store_clear() {
    return fontStoreClearProfile() ? 1 : 0;
}

int codex_font_store_usage(char *out, int cap) {
    uint32_t bytes = 0;
    int count = 0;
    fontStoreUsage(bytes, count);
    std::snprintf(out, (size_t)cap, "bytes=%u count=%d profile=%s", (unsigned)bytes,
                  count, fontStoreProfile());
    return count;
}

// Place raw bytes at the store path the device would use, bypassing validation.
// Used to model a file that was damaged or mis-named outside the write path.
int codex_font_store_put_raw(const char *id, const uint8_t *bytes, int len) {
    if (!id || !bytes || len <= 0) return 0;
    String path = String("/fonts/") + fontStoreProfile() + "/" + id + ".bin";
    File f = LittleFS.open(path.c_str(), FILE_WRITE);
    if (!f) return 0;
    const size_t wrote = f.write(bytes, (size_t)len);
    f.close();
    return wrote == (size_t)len ? 1 : 0;
}

int codex_rgn_dump(char *out, int out_len) {    if (!out || out_len <= 0) return -1;
    int used = 0;
    used += std::snprintf(out + used, (size_t)(out_len - used), "[");
    for (uint8_t i = 0; i < g_rgn.n && used < out_len - 80; i++) {
        const Rgn &r = g_rgn.r[i];
        used += std::snprintf(out + used, (size_t)(out_len - used),
                              "%s{\"cls\":\"%s\",\"hi\":%d,\"x0\":%u,\"x1\":%u,"
                              "\"y0\":%u,\"y1\":%u,\"area\":%u,\"budget\":%u}",
                              i ? "," : "", rgnClassName(r.cls), r.highInk ? 1 : 0,
                              r.px0, r.px1, r.py0, r.py1, r.area, r.budget);
    }
    std::snprintf(out + used, (size_t)(out_len - used), "]");
    return (int)g_rgn.n;
}
}
