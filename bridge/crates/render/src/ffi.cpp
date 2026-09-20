#include "GUI_Paint.h"
#include "template_engine.h"

#include <cstring>

SerialClass Serial;

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
}
