#include "GUI_Paint.h"
#include "template_engine.h"
#include "refresh_policy.h"
#include "v2_state.h"
#include "v2_runtime.h"
#include "bundle_store.h"
#include "dev_log.h"
#include <LittleFS.h>

#include <ArduinoJson.h>

#include <cstring>
#include <cstdio>

SerialClass Serial;
DevLogger DevLog;
void DevLogger::printf(const char *, ...) {}

namespace {
RgnSet g_rgn;
// Single active compiled template (the firmware keeps exactly one per device).
CtTemplate g_ct;
bool g_ct_valid = false;
}

extern "C" {

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
    if (!tmpl || !out || out_len < 200 * 200 / 8) return -2;
    Paint_NewImage(out, 200, 200, ROTATE_0, WHITE);
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
    if (!out || out_len < 200 * 200 / 8) return -2;
    if (!g_ct_valid) return -4;
    Paint_NewImage(out, 200, 200, ROTATE_0, WHITE);
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

int codex_rgn_dump(char *out, int out_len) {
    if (!out || out_len <= 0) return -1;
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
