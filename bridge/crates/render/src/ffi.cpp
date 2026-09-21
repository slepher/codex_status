#include "GUI_Paint.h"
#include "template_engine.h"
#include "refresh_policy.h"

#include <cstring>
#include <cstdio>

SerialClass Serial;

namespace {
RgnSet g_rgn;
}

extern "C" {

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

// Display-safety policy host bridge (refresh_policy.cpp): derive the semantic
// regions once, then run decisions against caller-provided 1bpp framebuffers.
int codex_rgn_build(const char *tmpl) {
    if (!tmpl) return -1;
    bool ok = rgnBuild(String(tmpl), g_rgn);
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
