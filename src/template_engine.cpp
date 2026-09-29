#include "template_engine.h"
#include "platform_target.h"
#include "v2_state.h"
#include <ArduinoJson.h>
#include <math.h>
#include <string.h>
#include <time.h>
#include <mbedtls/base64.h>
#include "GUI_Paint.h"
#include "fonts.h"
#include "font_noto.h"

// Canvas size is a target property (v2 §5): set once at boot (or by the host
// harness) instead of being compiled in, so one engine serves 200x200 and
// 400x300 render targets.
static int sCanvasW = TARGET_WIDTH;
static int sCanvasH = TARGET_HEIGHT;
static inline int tplCanvasW() { return sCanvasW; }
static inline int tplCanvasH() { return sCanvasH; }
#define TPL_W (tplCanvasW())
#define TPL_H (tplCanvasH())
void tplSetCanvas(int w, int h) {
    if (w > 0 && h > 0) {
        sCanvasW = w;
        sCanvasH = h;
    }
}
static const UWORD COLOR_NONE = 0xFF;

enum BindKind {
    B_PLAN, B_LABEL, B_HOSTID, B_SERVER_TIME,
    B_RESET_COUNT, B_RESET_EXPIRES,
    B_BUCKET_USED, B_BUCKET_REMAIN, B_BUCKET_RESET, B_BUCKET_WINMINS,
    B_DEV_CHANNEL, B_DEV_IP, B_DEV_SYNC, B_DEV_BATTERY,
    B_DEV_STATE, B_DEV_OFFLINE, B_DEV_NOW, B_DEV_MODE, B_DEV_DATE
};

// The optional JSON "time_format" accepts exactly "date" (the compact local
// MM-DD HH:MM default) or "hhmm" (reserved for epoch bindings).
enum TextTimeFormat { TTF_DATE, TTF_HHMM };

// winMode: 0 = weekly, 1 = 5h, 2 = index (winIndex)
struct BindSpec {
    BindKind kind;
    String   bucket;
    int      winMode  = 0;
    int      winIndex = 0;
};

// `when` accepts either {"bind":…,"exists":bool} or {"bind":…,"equals":…}
// (string or integer compared against the bind's rendered text).
struct DrawCondition {
    bool active = false;
    BindSpec bind;
    bool exists = false;
    bool useEquals = false;
    String equals;
};

// Font glyph tables are ASCII-only; keep multi-byte/UTF-8 text from walking
// out of bounds in Paint_DrawChar.
static String asciiOnly(const String &in) {
    String out;
    out.reserve(in.length());
    for (size_t i = 0; i < in.length(); i++) {
        char c = in[i];
        out += (c >= 32 && c <= 126) ? c : '?';
    }
    return out;
}

// B/W panel: the 4-color accents (yellow/red) collapse to black ink so
// templates written for the old panel stay readable.
static int colorVal(JsonVariant v, int def) {
    int c = def;
    if (v.is<int>()) {
        c = v.as<int>();
    } else {
        const char *s = v.as<const char *>();
        if (s) {
            if (!strcmp(s, "black"))       c = 0;
            else if (!strcmp(s, "white"))  c = 1;
            else if (!strcmp(s, "yellow")) c = 2;
            else if (!strcmp(s, "red"))    c = 3;
            else if (!strcmp(s, "none"))   c = COLOR_NONE;
        }
    }
    if (c == 2 || c == 3) c = 0;
    return c;
}

static bool parseBind(const String &path, BindSpec &s) {
    if (path == "account.plan")               { s.kind = B_PLAN; return true; }
    if (path == "bridge.label")               { s.kind = B_LABEL; return true; }
    if (path == "bridge.hostId")              { s.kind = B_HOSTID; return true; }
    if (path == "server_time")                { s.kind = B_SERVER_TIME; return true; }
    if (path == "resetCredits.availableCount"){ s.kind = B_RESET_COUNT; return true; }
    if (path == "resetCredits.nextExpiresAt") { s.kind = B_RESET_EXPIRES; return true; }
    if (path == "device.channel")             { s.kind = B_DEV_CHANNEL; return true; }
    if (path == "device.ip")                  { s.kind = B_DEV_IP; return true; }
    if (path == "device.sync_hhmm")           { s.kind = B_DEV_SYNC; return true; }
    if (path == "device.battery")              { s.kind = B_DEV_BATTERY; return true; }
    if (path == "device.state")                { s.kind = B_DEV_STATE; return true; }
    if (path == "device.offline_mins")         { s.kind = B_DEV_OFFLINE; return true; }
    if (path == "device.now")                  { s.kind = B_DEV_NOW; return true; }
    if (path == "device.date")                 { s.kind = B_DEV_DATE; return true; }
    if (path == "device.mode")                 { s.kind = B_DEV_MODE; return true; }
    if (path.startsWith("buckets[")) {
        int close = path.indexOf(']', 8);
        if (close < 0) return false;
        s.bucket = path.substring(8, close);
        if (s.bucket.length() == 0) return false;
        String rest = path.substring(close + 1);
        if (!rest.startsWith(".")) return false;
        int dot = rest.indexOf('.', 1);
        if (dot < 0) return false;
        String win   = rest.substring(1, dot);
        String field = rest.substring(dot + 1);
        if (win == "weekly")        { s.winMode = 0; }
        else if (win == "5h")       { s.winMode = 1; }
        else if (win == "monthly")  { s.winMode = 3; }
        else if (win == "primary")  { s.winMode = 2; s.winIndex = 0; }
        else if (win == "secondary"){ s.winMode = 2; s.winIndex = 1; }
        else if (win.startsWith("windows[")) {
            int e = win.indexOf(']', 8);
            if (e < 0) return false;
            s.winMode = 2;
            s.winIndex = win.substring(8, e).toInt();
        } else return false;
        if (field == "usedPercent")      s.kind = B_BUCKET_USED;
        else if (field == "remaining")   s.kind = B_BUCKET_REMAIN;
        else if (field == "resetsAt")    s.kind = B_BUCKET_RESET;
        else if (field == "windowMins")  s.kind = B_BUCKET_WINMINS;
        else return false;
        return true;
    }
    return false;
}

static bool isEpochBind(const BindSpec &s) {
    return s.kind == B_SERVER_TIME || s.kind == B_RESET_EXPIRES ||
           s.kind == B_BUCKET_RESET;
}

static bool parseTimeFormat(JsonVariant v, const BindSpec &s, TextTimeFormat &fmt) {
    if (v.isNull()) return false;
    const char *value = v.as<const char *>();
    if (!value || !isEpochBind(s)) return false;
    if (!strcmp(value, "date")) {
        fmt = TTF_DATE;
        return true;
    }
    if (!strcmp(value, "hhmm")) {
        fmt = TTF_HHMM;
        return true;
    }
    return false;
}

static bool findWindow(JsonDocument &usage, const BindSpec &s, JsonObject &w) {
    JsonArray buckets = usage["buckets"].as<JsonArray>();
    if (buckets.isNull()) return false;
    JsonObject bucket;
    for (JsonObject b : buckets) {
        const char *bid = b["id"] | "";
        if (s.bucket == bid) { bucket = b; break; }
    }
    if (bucket.isNull()) return false;
    JsonArray wins = bucket["windows"].as<JsonArray>();
    if (wins.isNull()) return false;
    if (s.winMode == 2) {
        if (s.winIndex < 0 || s.winIndex >= (int)wins.size()) return false;
        w = wins[s.winIndex].as<JsonObject>();
        return !w.isNull();
    }
    for (JsonObject it : wins) {
        int mins = it["windowMins"] | 0;
        // Duration decides the window class: 5h / weekly (7d, and any other
        // long window) / monthly (>= 30d, free & go plans).
        bool match = s.winMode == 0 ? (mins >= 10080)
                   : s.winMode == 3 ? (mins >= 43200)
                   : (mins == 300);
        if (match) { w = it; return true; }
    }
    return false;
}

static String fmtEpoch(long long epoch, TextTimeFormat format = TTF_DATE) {
    char b[24] = "--";
    if (epoch > 0) {
        time_t t = (time_t)epoch;
        struct tm *lt = localtime(&t);
        if (lt && t > 1600000000) {
            strftime(b, sizeof(b), format == TTF_HHMM ? "%H:%M" : "%m-%d %H:%M", lt);
        }
    }
    return String(b);
}

static bool evalBind(const BindSpec &s, TextTimeFormat format, JsonDocument &usage,
                     const TplEnv &env, String &out) {
    switch (s.kind) {
    case B_PLAN:    out = String((const char *)(usage["account"]["plan"] | "--")); return true;
    case B_LABEL:   out = String((const char *)(usage["bridge"]["label"] | "--")); return true;
    case B_HOSTID:  out = String((const char *)(usage["bridge"]["hostId"] | "--")); return true;
    case B_DEV_CHANNEL: out = env.channel.length() ? env.channel : "--"; return true;
    case B_DEV_IP:      out = env.ip; return true;
    case B_DEV_SYNC:    out = env.syncHHMM; return true;
    case B_DEV_BATTERY: out = env.battery >= 0 ? String(env.battery) : "--"; return true;
    case B_DEV_STATE:
        if (env.state.length() == 0) return false;
        out = env.state;
        return true;
    case B_DEV_OFFLINE:
        if (env.offlineMins < 0) return false;
        out = String(env.offlineMins);
        return true;
    case B_DEV_NOW: {
        // Device local clock (server_time synced + RTC): the loop redraws once
        // per minute while the active template references this bind.
        time_t n = (time_t)(env.hasNowEpoch ? env.nowEpochSecs : time(nullptr));
        if (n < 1600000000) return false;
        struct tm *lt = localtime(&n);
        if (!lt) return false;
        char b[8];
        strftime(b, sizeof(b), "%H:%M", lt);
        out = b;
        return true;
    }
    case B_DEV_DATE: {
        time_t n = (time_t)(env.hasNowEpoch ? env.nowEpochSecs : time(nullptr));
        if (n < 1600000000) return false;
        struct tm *lt = localtime(&n);
        if (!lt) return false;
        char b[6];
        strftime(b, sizeof(b), "%m/%d", lt);
        out = b;
        return true;
    }
    case B_DEV_MODE:
        if (env.mode.length() == 0) return false;
        out = env.mode;
        return true;
    case B_SERVER_TIME:
        if (usage["server_time"].isNull()) return false;
        out = fmtEpoch(usage["server_time"] | 0LL, format);
        return true;
    case B_RESET_COUNT: out = String(usage["resetCredits"]["availableCount"] | 0); return true;
    case B_RESET_EXPIRES:
        if (usage["resetCredits"]["nextExpiresAt"].isNull()) return false;
        out = fmtEpoch(usage["resetCredits"]["nextExpiresAt"] | 0LL, format);
        return true;
    default: break;
    }
    JsonObject w;
    if (!findWindow(usage, s, w)) return false;
    switch (s.kind) {
    case B_BUCKET_USED:
        if (w["usedPercent"].isNull()) return false;
        out = String(w["usedPercent"] | 0);
        return true;
    case B_BUCKET_REMAIN:
        if (w["usedPercent"].isNull()) return false;
        out = String(100 - (w["usedPercent"] | 0));
        return true;
    case B_BUCKET_RESET:
        if (w["resetsAt"].isNull()) return false;
        out = fmtEpoch(w["resetsAt"] | 0LL, format);
        return true;
    case B_BUCKET_WINMINS:
        if (w["windowMins"].isNull()) return false;
        out = String(w["windowMins"] | 0);
        return true;
    default: return false;
    }
}

static bool evalBindNum(const BindSpec &s, JsonDocument &usage,
                        const TplEnv &env, double &v) {
    if (s.kind == B_DEV_BATTERY) {
        if (env.battery < 0) return false;
        v = env.battery;
        return true;
    }
    if (s.kind != B_BUCKET_USED && s.kind != B_BUCKET_REMAIN &&
        s.kind != B_BUCKET_RESET && s.kind != B_BUCKET_WINMINS) {
        return false;
    }
    JsonObject w;
    if (!findWindow(usage, s, w)) return false;
    switch (s.kind) {
    case B_BUCKET_USED:
        if (w["usedPercent"].isNull()) return false;
        v = w["usedPercent"] | 0;
        return true;
    case B_BUCKET_REMAIN:
        if (w["usedPercent"].isNull()) return false;
        v = 100 - (w["usedPercent"] | 0);
        return true;
    case B_BUCKET_RESET:
        if (w["resetsAt"].isNull()) return false;
        v = w["resetsAt"] | 0LL;
        return true;
    case B_BUCKET_WINMINS:
        if (w["windowMins"].isNull()) return false;
        v = w["windowMins"] | 0;
        return true;
    default: return false;
    }
}

static bool bindExists(const BindSpec &s, JsonDocument &usage, const TplEnv &env,
                       bool haveUsage) {
    switch (s.kind) {
    case B_DEV_CHANNEL: return env.channel.length() > 0;
    case B_DEV_IP:      return env.ip.length() > 0;
    case B_DEV_SYNC:    return env.syncHHMM.length() > 0 && env.syncHHMM != "--:--";
    case B_DEV_BATTERY: return env.battery >= 0;
    case B_DEV_STATE:   return env.state.length() > 0;
    case B_DEV_OFFLINE: return env.offlineMins >= 0;
    case B_DEV_NOW:
    case B_DEV_DATE:    return (env.hasNowEpoch ? env.nowEpochSecs : time(nullptr)) > 1600000000;
    case B_DEV_MODE:    return env.mode.length() > 0;
    case B_PLAN:        return haveUsage && !usage["account"]["plan"].isNull();
    case B_LABEL:       return haveUsage && !usage["bridge"]["label"].isNull();
    case B_HOSTID:      return haveUsage && !usage["bridge"]["hostId"].isNull();
    case B_SERVER_TIME: return haveUsage && !usage["server_time"].isNull();
    case B_RESET_COUNT: return haveUsage && (usage["resetCredits"]["availableCount"] | 0) > 0;
    case B_RESET_EXPIRES: return haveUsage && !usage["resetCredits"]["nextExpiresAt"].isNull();
    default: break;
    }
    if (!haveUsage) return false;
    JsonObject w;
    if (!findWindow(usage, s, w)) return false;
    switch (s.kind) {
    case B_BUCKET_USED:
    case B_BUCKET_REMAIN: return !w["usedPercent"].isNull();
    case B_BUCKET_RESET:   return !w["resetsAt"].isNull();
    case B_BUCKET_WINMINS: return !w["windowMins"].isNull();
    default: return false;
    }
}

static bool parseCondition(JsonObject e, DrawCondition &condition) {
    JsonVariant raw = e["when"];
    if (!e.containsKey("when")) return true;
    if (raw.isNull()) return false;
    if (!raw.is<JsonObject>()) return false;
    JsonObject obj = raw.as<JsonObject>();
    bool hasExists = !obj["exists"].isNull();
    bool hasEquals = !obj["equals"].isNull();
    if (obj.size() != 2 || obj["bind"].isNull() || hasExists == hasEquals) return false;
    for (JsonPair kv : obj) {
        const char *key = kv.key().c_str();
        if (strcmp(key, "bind") && strcmp(key, "exists") && strcmp(key, "equals")) return false;
    }
    const char *bind = obj["bind"].as<const char *>();
    if (!bind || !parseBind(String(bind), condition.bind)) return false;
    if (hasExists) {
        if (!obj["exists"].is<bool>()) return false;
        condition.exists = obj["exists"].as<bool>();
    } else {
        JsonVariant eq = obj["equals"];
        if (eq.is<const char *>()) {
            condition.equals = eq.as<const char *>();
        } else if (!eq.is<bool>() && eq.is<long long>()) {
            condition.equals = String((long long)eq.as<long long>());
        } else {
            return false;
        }
        condition.useEquals = true;
    }
    condition.active = true;
    return true;
}

static bool textScale(JsonVariant value, int &scale) {
    if (value.isNull()) return false;
    if (!value.is<int>()) return false;
    scale = value.as<int>();
    return scale >= 1 && scale <= 3;
}

static bool textRegion(JsonVariant value, int &x, int &y, int &w, int &h) {
    if (value.isNull()) return false;
    JsonArray r = value.as<JsonArray>();
    if (r.isNull() || r.size() != 4) return false;
    if (!r[0].is<int>() || !r[1].is<int>() || !r[2].is<int>() || !r[3].is<int>()) return false;
    x = r[0].as<int>(); y = r[1].as<int>(); w = r[2].as<int>(); h = r[3].as<int>();
    return x >= 0 && y >= 0 && x < TPL_W && y < TPL_H &&
           w > 0 && h > 0 && w <= TPL_W && h <= TPL_H &&
           x <= TPL_W - w && y <= TPL_H - h;
}

static bool parseTextLayout(JsonObject e, BindSpec *spec, int &scale,
                            bool &hasRegion, int &rx, int &ry, int &rw, int &rh,
                            TextTimeFormat &format) {
    if (e.containsKey("scale")) {
        if (!textScale(e["scale"], scale)) return false;
    } else {
        scale = 1;
    }
    hasRegion = e.containsKey("region");
    if (hasRegion && !textRegion(e["region"], rx, ry, rw, rh)) return false;
    JsonVariant align = e["align"];
    if (e.containsKey("align")) {
        if (!hasRegion || !align.is<const char *>()) return false;
        const char *a = align.as<const char *>();
        if (strcmp(a, "left") && strcmp(a, "center") && strcmp(a, "right")) return false;
    }
    if (e.containsKey("time_format")) {
        if (!spec || !parseTimeFormat(e["time_format"], *spec, format)) return false;
    } else {
        format = TTF_DATE;
    }
    return true;
}

static bool decodeBase64(const char *in, uint8_t *out, size_t outLen) {
    size_t olen = 0;
    int rc = mbedtls_base64_decode(out, outLen, &olen,
                                   (const unsigned char *)in, strlen(in));
    return rc == 0 && olen == outLen;
}

static void drawScaledText(const String &value, sFONT *font, int x, int y,
                           int scale, int fg, int bg, bool clipped,
                           int clipX, int clipY, int clipW, int clipH) {
    int bytesPerRow = (font->Width + 7) / 8;
    int cursor = x;
    for (size_t i = 0; i < value.length(); i++) {
        char c = value[i];
        if (c < ' ' || c > '~') c = '?';
        size_t offset = (size_t)(c - ' ') * font->Height * bytesPerRow;
        const uint8_t *glyph = font->table + offset;
        for (int row = 0; row < font->Height; row++) {
            for (int col = 0; col < font->Width; col++) {
                bool ink = (glyph[row * bytesPerRow + col / 8] & (0x80 >> (col % 8))) != 0;
                if (!ink && bg == COLOR_NONE) continue;
                int px0 = cursor + col * scale;
                int py0 = y + row * scale;
                for (int sy = 0; sy < scale; sy++) {
                    for (int sx = 0; sx < scale; sx++) {
                        int px = px0 + sx;
                        int py = py0 + sy;
                        if (px < 0 || py < 0 || px >= TPL_W || py >= TPL_H) continue;
                        if (clipped && (px < clipX || py < clipY ||
                                        px >= clipX + clipW || py >= clipY + clipH)) continue;
                        Paint_SetPixel((UWORD)px, (UWORD)py, (UWORD)(ink ? fg : bg));
                    }
                }
            }
        }
        cursor += font->Width * scale;
    }
}

// --- Proportional family (cropped Noto Sans) --------------------------------
// The pen advances by each glyph's own 1/16 px advance and every glyph is
// placed from its box offset, so spacing matches the upstream font instead of
// a fixed cell. Same pixel writes as the fixed path, so host and device agree.

static int propTextWidth(const Note4PropFont &f, const String &value, int scale) {
    long adv = 0;
    for (size_t i = 0; i < value.length(); i++) {
        char c = value[i];
        if (c < ' ' || c > '~') c = '?';
        adv += f.glyphs[c - ' '].adv;
    }
    return (int)((adv * scale + 8) / 16);
}

static void drawPropText(const Note4PropFont &f, const String &value, int x, int y,
                         int scale, int fg, int bg, bool clipped,
                         int clipX, int clipY, int clipW, int clipH) {
    if (scale < 1) scale = 1;
    int baseline = y + (int)f.lineHeight - (int)f.baseLine;
    long pen = (long)x << 4;   // 1/16 px fixed point
    for (size_t i = 0; i < value.length(); i++) {
        char c = value[i];
        if (c < ' ' || c > '~') c = '?';
        const Note4Glyph &g = f.glyphs[c - ' '];
        int gx = (int)(pen >> 4) + g.ox;
        int gy = baseline - g.oy - g.h;
        int stride = (g.w + 7) / 8;
        for (int row = 0; row < g.h; row++) {
            for (int col = 0; col < g.w; col++) {
                bool ink = (f.blob[g.off + row * stride + col / 8] &
                            (0x80 >> (col % 8))) != 0;
                if (!ink && bg == COLOR_NONE) continue;
                for (int sy = 0; sy < scale; sy++) {
                    for (int sx = 0; sx < scale; sx++) {
                        int px = gx + col * scale + sx;
                        int py = gy + row * scale + sy;
                        if (px < 0 || py < 0 || px >= TPL_W || py >= TPL_H) continue;
                        if (clipped && (px < clipX || py < clipY ||
                                        px >= clipX + clipW || py >= clipY + clipH)) continue;
                        Paint_SetPixel((UWORD)px, (UWORD)py, (UWORD)(ink ? fg : bg));
                    }
                }
            }
        }
        pen += (long)g.adv * scale;
    }
}

static bool drawIcon(const uint8_t *bits, int x, int y, int w, int h, int fg) {
    int stride = (w + 7) / 8;
    for (int row = 0; row < h; row++) {
        for (int col = 0; col < w; col++) {
            uint8_t b = bits[row * stride + col / 8];
            if (b & (0x80 >> (col % 8))) {
                Paint_SetPixel(x + col, y + row, fg);
            }
        }
    }
    return true;
}

static bool clampRect(JsonArray r, int &x, int &y, int &w, int &h) {
    if (r.isNull() || r.size() < 4) return false;
    x = r[0] | 0; y = r[1] | 0; w = r[2] | 0; h = r[3] | 0;
    if (w <= 0 || h <= 0) return false;
    if (x < 0) { w += x; x = 0; }
    if (y < 0) { h += y; y = 0; }
    if (x >= TPL_W || y >= TPL_H) return false;
    if (x + w > TPL_W) w = TPL_W - x;
    if (y + h > TPL_H) h = TPL_H - y;
    return w > 0 && h > 0;
}

// ===========================================================================
// Compiled template: parse once, draw from ops forever.
// ===========================================================================

// ---------------------------------------------------------------------------
// Font registry (single source of truth, see template_engine.h).
//
// Two families share one index space because `CtOp.font` is a uint8 index:
// the bitmap family compiled into the firmware and the proportional
// large-display family cropped from Noto Sans. Adding a font means adding one
// row here; template validation, the region derivation, the clock fast path and
// the font-slot resolver all follow automatically.
// ---------------------------------------------------------------------------
enum FontKind { FONT_KIND_BITMAP = 0, FONT_KIND_PROP = 1 };

struct TplFontEntry {
    const char          *name;
    uint8_t              kind;
    const sFONT         *bitmap;   // FONT_KIND_BITMAP
    const Note4PropFont *prop;     // FONT_KIND_PROP
};

static const TplFontEntry TPL_FONTS[] = {
    {"f8",   FONT_KIND_BITMAP, &Font8,  nullptr},
    {"f12",  FONT_KIND_BITMAP, &Font12, nullptr},
    {"f16",  FONT_KIND_BITMAP, &Font16, nullptr},
    {"f20",  FONT_KIND_BITMAP, &Font20, nullptr},
    {"f24",  FONT_KIND_BITMAP, &Font24, nullptr},
    {"nt16", FONT_KIND_PROP,   nullptr, &note4_nt16},
    {"nt30", FONT_KIND_PROP,   nullptr, &note4_nt30},
    // Appended, never reordered: `CtOp.font` is a persisted index, so existing
    // compiled templates must keep resolving to the same glyph tables.
    {"ntthin18", FONT_KIND_PROP, nullptr, &note4_ntthin18},  // normal text, Thin 100 @18
    {"ntreg64",  FONT_KIND_PROP, nullptr, &note4_ntreg64},   // large text, Regular 400 @64
#if defined(CODEX_TARGET_NOTE4) || defined(CODEX_RENDER_NOTE4_FONTS)
    {"ntreg96",  FONT_KIND_PROP, nullptr, &note4_ntreg96},   // large text, Regular 400 @96
#endif
};

int tplFontCount() {
    return (int)(sizeof(TPL_FONTS) / sizeof(TPL_FONTS[0]));
}

int tplFontFixedCount() {
    int n = 0;
    for (int i = 0; i < tplFontCount(); i++) {
        if (TPL_FONTS[i].kind == FONT_KIND_BITMAP) n++;
    }
    return n;
}

static const TplFontEntry *fontEntry(int idx) {
    if (idx < 0 || idx >= tplFontCount()) return nullptr;
    return &TPL_FONTS[idx];
}

int tplFontIndexByName(const char *name) {
    if (!name) return -1;
    for (int i = 0; i < tplFontCount(); i++) {
        if (!strcmp(name, TPL_FONTS[i].name)) return i;
    }
    return -1;
}

const char *tplFontNameByIndex(int idx) {
    const TplFontEntry *e = fontEntry(idx);
    return e ? e->name : nullptr;
}

static sFONT *fontByIndex(int idx) {
    const TplFontEntry *e = fontEntry(idx);
    return (e && e->kind == FONT_KIND_BITMAP) ? (sFONT *)e->bitmap : nullptr;
}

// The proportional large-display family; index >= tplFontFixedCount().
static const Note4PropFont *propFontByIndex(int idx) {
    const TplFontEntry *e = fontEntry(idx);
    return (e && e->kind == FONT_KIND_PROP) ? e->prop : nullptr;
}

bool tplFontCellByName(const char *name, int &cellW, int &cellH) {
    int idx = tplFontIndexByName(name);
    if (idx < 0) return false;
    return tplFontCellByIndex(idx, cellW, cellH);
}

bool tplFontCellByIndex(int idx, int &cellW, int &cellH) {
    sFONT *f = fontByIndex(idx);
    if (f) {
        cellW = f->Width;
        cellH = f->Height;
        return true;
    }
    const Note4PropFont *p = propFontByIndex(idx);
    if (!p) return false;
    cellW = (int)((p->maxAdv + 15) / 16);   // widest glyph, rounded up
    cellH = p->lineHeight;
    return true;
}

bool tplFontClockBox(int idx, int scale, int &w, int &h) {
    if (scale < 1) scale = 1;
    sFONT *f = fontByIndex(idx);
    if (f) {
        w = 5 * (int)f->Width * scale;
        h = (int)f->Height * scale;
        return true;
    }
    const Note4PropFont *p = propFontByIndex(idx);
    if (!p) return false;
    // Widest possible "HH:MM": four digits (all share the widest digit advance)
    // plus the colon, so the reserved window always fits whatever the clock
    // shows. The +2 px covers glyph boxes that overhang their advance; the window
    // is byte-aligned anyway. `2 * digitAdv` here used to under-count by two
    // digits (26 px window for a 44 px string), which made the partial window
    // write clip the trailing glyphs and leave the previous frame's ink there.
    int digitAdv = 0;
    for (char c = '0'; c <= '9'; c++) {
        int a = p->glyphs[c - ' '].adv;
        if (a > digitAdv) digitAdv = a;
    }
    long adv = 4L * digitAdv + p->glyphs[':' - ' '].adv;
    w = (int)((adv * scale + 8) / 16) + 2;
    h = (int)p->lineHeight * scale;
    return true;
}

// Ink pixels dropped by the last tplFontDrawClock() call (see the accessor).
static int sClockClipped = 0;

bool tplFontDrawClock(uint8_t *win, int bw, int rows, int xOff, int idx,
                      const char *text, int scale) {
    if (!win || !text || bw <= 0 || rows <= 0 || xOff < 0 || scale < 1) return false;
    const int bufW = bw * 8;
    // Ink only: the window buffer already holds the background (the caller
    // restores it from the captured window before the write).
    // `clipped` counts ink pixels the window could not hold: that is never
    // normal, and it used to hide a wrong reserved width behind a "successful"
    // partial write (the dropped tail kept the previous frame's ink).
    sClockClipped = 0;
    auto put = [&](int px, int py) {
        if (px < 0 || py < 0 || px >= bufW || py >= rows) { sClockClipped++; return; }
        win[py * bw + (px >> 3)] &= (uint8_t)~(0x80 >> (px & 7));
    };
    sFONT *f = fontByIndex(idx);
    if (f) {
        const int stride = (f->Width + 7) / 8;
        for (int i = 0; text[i]; i++) {
            char c = text[i];
            if (c < ' ' || c > '~') c = '?';
            const uint8_t *p = &f->table[((int)c - ' ') * f->Height * stride];
            for (int row = 0; row < (int)f->Height; row++) {
                for (int col = 0; col < (int)f->Width; col++) {
                    if (!(p[row * stride + col / 8] & (0x80 >> (col % 8)))) continue;
                    for (int dy = 0; dy < scale; dy++) {
                        for (int dx = 0; dx < scale; dx++) {
                            put(xOff + (i * (int)f->Width + col) * scale + dx,
                                row * scale + dy);
                        }
                    }
                }
            }
        }
        return true;
    }
    const Note4PropFont *p = propFontByIndex(idx);
    if (!p) return false;
    const int baseline = (int)p->lineHeight - (int)p->baseLine;
    long pen = (long)xOff << 4;   // 1/16 px fixed point, same as drawPropText
    for (int i = 0; text[i]; i++) {
        char c = text[i];
        if (c < ' ' || c > '~') c = '?';
        const Note4Glyph &g = p->glyphs[c - ' '];
        const int gx = (int)(pen >> 4) + g.ox;
        const int gy = baseline - g.oy - g.h;
        const int stride = (g.w + 7) / 8;
        for (int row = 0; row < g.h; row++) {
            for (int col = 0; col < g.w; col++) {
                if (!(p->blob[g.off + row * stride + col / 8] & (0x80 >> (col % 8)))) continue;
                for (int sy = 0; sy < scale; sy++) {
                    for (int sx = 0; sx < scale; sx++) {
                        put(gx + col * scale + sx, gy + row * scale + sy);
                    }
                }
            }
        }
        pen += (long)g.adv * scale;
    }
    return true;
}

// Ink pixels the last tplFontDrawClock() blit had to drop because they fell
// outside the window. Non-zero means the reserved window is too small for the
// string; the caller must not trust that partial write. Reading it clears it.
int tplFontClockClipped() {
    const int v = sClockClipped;
    sClockClipped = 0;
    return v;
}

// Advance width (px) of `text` in the proportional family, same metrics the
// renderer uses, so a caller can verify a reserved window really fits before
// writing it.
int tplFontPropWidth(int idx, int scale, const char *text) {
    if (!text || scale < 1) return 0;
    const Note4PropFont *p = propFontByIndex(idx);
    if (!p) return 0;
    long adv = 0;
    for (const char *c = text; *c; c++) {
        const char ch = (*c < ' ' || *c > '~') ? '?' : *c;
        adv += p->glyphs[ch - ' '].adv;
    }
    return (int)((adv * scale + 8) / 16);
}

static void ctCopy(char *dst, size_t cap, const char *src) {
    if (!src) { dst[0] = 0; return; }
    size_t n = strlen(src);
    if (n >= cap) n = cap - 1;
    memcpy(dst, src, n);
    dst[n] = 0;
}

static int ctFindReq(const CtTemplate &ct, const String &path) {
    for (int i = 0; i < ct.reqCount; i++) {
        if (path == ct.reqs[i].path) return i;
    }
    return -1;
}

static int ctAddReq(CtTemplate &ct, const BindSpec &spec, const String &path) {
    int found = ctFindReq(ct, path);
    if (found >= 0) return found;
    if (ct.reqCount >= CT_MAX_REQS) return -1;
    CtReq &r = ct.reqs[ct.reqCount];
    memset(&r, 0, sizeof(r));
    r.kind = (uint8_t)spec.kind;
    r.winMode = (uint8_t)spec.winMode;
    r.winIndex = (int16_t)spec.winIndex;
    ctCopy(r.bucket, sizeof(r.bucket), spec.bucket.c_str());
    ctCopy(r.path, sizeof(r.path), path.c_str());
    return ct.reqCount++;
}

static int ctFindRes(const CtTemplate &ct, const char *bits) {
    for (int i = 0; i < ct.resCount; i++) {
        if (!strcmp(ct.res[i].bits, bits)) return i;
    }
    return -1;
}

static bool ctBind(const CtReq &r, BindSpec &s) {
    s.kind = (BindKind)r.kind;
    s.bucket = r.bucket;
    s.winMode = r.winMode;
    s.winIndex = r.winIndex;
    return true;
}

static bool ctEvalText(const CtTemplate &ct, uint8_t idx, TextTimeFormat fmt,
                       JsonDocument &usage, const TplEnv &env, String &out) {
    if (idx == CT_NONE_IDX || idx >= ct.reqCount) return false;
    BindSpec s;
    ctBind(ct.reqs[idx], s);
    return evalBind(s, fmt, usage, env, out);
}

static bool ctExists(const CtTemplate &ct, uint8_t idx, JsonDocument &usage,
                     const TplEnv &env, bool haveUsage) {
    if (idx == CT_NONE_IDX || idx >= ct.reqCount) return false;
    BindSpec s;
    ctBind(ct.reqs[idx], s);
    return bindExists(s, usage, env, haveUsage);
}

static bool ctCondMatches(const CtTemplate &ct, const CtOp &op, JsonDocument &usage,
                          const TplEnv &env, bool haveUsage) {
    if (op.whenMode == CTW_NONE) return true;
    if (op.whenMode == CTW_EQ_NUM || op.whenMode == CTW_EQ_STR) {
        String v;
        if (!ctEvalText(ct, op.whenIdx, TTF_DATE, usage, env, v)) return false;
        if (op.whenMode == CTW_EQ_NUM) return v == String((int)op.whenEqNum);
        return v == String(op.whenEqStr);
    }
    bool exists = ctExists(ct, op.whenIdx, usage, env, haveUsage);
    return exists == (op.whenMode == CTW_EXISTS_TRUE);
}

// Parse one element into a compiled op (also the validator for the JSON path).
static bool parseElementCompiled(JsonObject e, CtTemplate &ct, CtOp &op) {
    memset(&op, 0, sizeof(op));
    op.bindIdx = CT_NONE_IDX;
    op.whenIdx = CT_NONE_IDX;
    op.resourceIdx = CT_NONE_IDX;
    op.bg = 0xFF;
    op.border = 1;
    op.maxVal = 100;

    // `when` (validated for every element; drawing applies it to all types).
    if (e.containsKey("when")) {
        DrawCondition condition;
        if (!parseCondition(e, condition)) return false;
        const char *bind = e["when"]["bind"] | "";
        int whenIdx = ctAddReq(ct, condition.bind, String(bind));
        if (whenIdx < 0) return false;
        op.whenIdx = (uint8_t)whenIdx;
        if (condition.useEquals) {
            if (e["when"]["equals"].is<const char *>()) {
                op.whenMode = CTW_EQ_STR;
                ctCopy(op.whenEqStr, sizeof(op.whenEqStr),
                       e["when"]["equals"].as<const char *>());
            } else {
                op.whenMode = CTW_EQ_NUM;
                op.whenEqNum = (int16_t)(e["when"]["equals"].as<long long>());
            }
        } else {
            op.whenMode = condition.exists ? CTW_EXISTS_TRUE : CTW_EXISTS_FALSE;
        }
    }

    const char *type = e["type"] | "";
    if (!strcmp(type, "text")) {
        int fi = tplFontIndexByName(e["font"] | "");
        if (fi < 0) return false;
        const char *bind = e["bind"] | "";
        const char *text = e["text"] | "";
        if (!strlen(bind) && !strlen(text)) return false;
        BindSpec spec;
        if (strlen(bind) && !parseBind(String(bind), spec)) return false;
        int scale = 1, rx = 0, ry = 0, rw = 0, rh = 0;
        bool hasRegion = false;
        TextTimeFormat format = TTF_DATE;
        if (!parseTextLayout(e, strlen(bind) ? &spec : nullptr, scale, hasRegion,
                             rx, ry, rw, rh, format)) return false;
        op.type = CT_TEXT;
        op.font = (uint8_t)fi;
        op.scale = (uint8_t)scale;
        op.color = (uint8_t)colorVal(e["color"], 0);
        if (e["bg"]) op.bg = (uint8_t)colorVal(e["bg"], 1);
        if (!strlen(bind) || strcmp(bind, "device.now") == 0) {
            // device.now is rendered by the runtime clock path; still compiled.
        }
        if (strlen(bind)) {
            int idx = ctAddReq(ct, spec, String(bind));
            if (idx < 0) return false;
            op.bindIdx = (uint8_t)idx;
        }
        op.x = (int16_t)(e["x"] | 0);
        op.y = (int16_t)(e["y"] | 0);
        if (e.containsKey("digit_bind") || e.containsKey("digit_x")) {
            const char *digitBind = e["digit_bind"].as<const char *>();
            JsonArray positions = e["digit_x"].as<JsonArray>();
            BindSpec digitSpec;
            if (strlen(bind) || !strlen(text) || !digitBind ||
                !parseBind(String(digitBind), digitSpec) ||
                (digitSpec.kind != B_BUCKET_REMAIN && digitSpec.kind != B_BUCKET_USED &&
                 digitSpec.kind != B_DEV_BATTERY) ||
                positions.isNull() || positions.size() != 3 || hasRegion) return false;
            int idx = ctAddReq(ct, digitSpec, String(digitBind));
            if (idx < 0) return false;
            op.flags |= 0x10;
            op.resourceIdx = (uint8_t)idx;
            for (int i = 0; i < 3; i++) {
                if (!positions[i].is<int>()) return false;
                int x = positions[i].as<int>();
                if (x < 0 || x >= TPL_W) return false;
                if (i == 0) op.x = (int16_t)x;
                else if (i == 1) op.x2 = (int16_t)x;
                else op.y2 = (int16_t)x;
            }
        }
        if (hasRegion) {
            op.flags |= 0x02;
            op.x = (int16_t)rx;
            op.y = (int16_t)ry;
            op.w = (int16_t)rw;
            op.h = (int16_t)rh;
            const char *align = e["align"] | "left";
            if (!strcmp(align, "center")) op.align = 1;
            else if (!strcmp(align, "right")) op.align = 2;
            if (e.containsKey("align")) op.flags |= 0x04;
        }
        if (e.containsKey("time_format")) {
            op.flags |= 0x08;
            op.timeFormat = (format == TTF_HHMM) ? 1 : 0;
        }
        ctCopy(op.text, sizeof(op.text), text);
        ctCopy(op.prefix, sizeof(op.prefix), e["prefix"] | "");
        ctCopy(op.suffix, sizeof(op.suffix), e["suffix"] | "");
        return true;
    }
    if (!strcmp(type, "bar")) {
        const char *bind = e["bind"] | "";
        BindSpec spec;
        if (!strlen(bind) || !parseBind(String(bind), spec)) return false;
        int x, y, w, h;
        if (!clampRect(e["rect"].as<JsonArray>(), x, y, w, h)) return false;
        int idx = ctAddReq(ct, spec, String(bind));
        if (idx < 0) return false;
        op.type = CT_BAR;
        op.bindIdx = (uint8_t)idx;
        op.x = (int16_t)x; op.y = (int16_t)y; op.w = (int16_t)w; op.h = (int16_t)h;
        op.fg = (uint8_t)(e["fg"] ? colorVal(e["fg"], 0) : 0);
        op.bg = (uint8_t)(e["bg"] ? colorVal(e["bg"], 1) : 1);
        op.border = (e["border"] | true) ? 1 : 0;
        double maxV = e["max"] | 100.0;
        if (maxV <= 0) maxV = 100;
        op.maxVal = (int16_t)maxV;
        return true;
    }
    if (!strcmp(type, "rect")) {
        int x, y, w, h;
        if (!clampRect(e["rect"].as<JsonArray>(), x, y, w, h)) return false;
        op.type = CT_RECT;
        op.x = (int16_t)x; op.y = (int16_t)y; op.w = (int16_t)w; op.h = (int16_t)h;
        op.color = (uint8_t)colorVal(e["color"], 0);
        if (e["fill"] | false) op.flags |= 0x01;
        return true;
    }
    if (!strcmp(type, "line")) {
        if (e["x1"].isNull() || e["y1"].isNull() || e["x2"].isNull() || e["y2"].isNull())
            return false;
        op.type = CT_LINE;
        op.x = (int16_t)(e["x1"] | 0); op.y = (int16_t)(e["y1"] | 0);
        op.x2 = (int16_t)(e["x2"] | 0); op.y2 = (int16_t)(e["y2"] | 0);
        op.color = (uint8_t)colorVal(e["color"], 0);
        return true;
    }
    if (!strcmp(type, "icon")) {
        int x = e["x"] | 0, y = e["y"] | 0;
        int w = e["w"] | 0, h = e["h"] | 0;
        const char *b64 = e["bits"] | "";
        if (w <= 0 || h <= 0 || !strlen(b64)) return false;
        int existing = ctFindRes(ct, b64);
        if (existing < 0) {
            if (ct.resCount >= CT_MAX_RES) return false;
            existing = ct.resCount;
            ctCopy(ct.res[existing].bits, sizeof(ct.res[existing].bits), b64);
            ct.res[existing].w = (uint8_t)w;
            ct.res[existing].h = (uint8_t)h;
            ct.resCount++;
        }
        op.type = CT_ICON;
        op.x = (int16_t)x; op.y = (int16_t)y; op.w = (int16_t)w; op.h = (int16_t)h;
        op.color = (uint8_t)colorVal(e["color"], 0);
        op.resourceIdx = (uint8_t)existing;
        return true;
    }
    return false;
}

static bool drawCtOp(const CtTemplate &ct, const CtOp &op, JsonDocument &usage,
                     const TplEnv &env, bool haveUsage, bool dry) {
    if (!dry && !ctCondMatches(ct, op, usage, env, haveUsage)) return true;
    sFONT *font = nullptr;
    switch (op.type) {
    case CT_TEXT: {
        if (dry) return true;
        const Note4PropFont *prop = propFontByIndex(op.font);
        font = prop ? nullptr : fontByIndex(op.font);
        if (!prop && !font) return false;
        String val;
        if (op.bindIdx != CT_NONE_IDX) {
            TextTimeFormat fmt = op.flags & 0x08 ? (op.timeFormat ? TTF_HHMM : TTF_DATE) : TTF_DATE;
            if (!haveUsage) val = "--";
            else if (!ctEvalText(ct, op.bindIdx, fmt, usage, env, val)) val = "--";
        } else {
            val = op.text;
        }
        val = asciiOnly(String(op.prefix) + val + String(op.suffix));
        bool hasRegion = (op.flags & 0x02) != 0;
        int scale = op.scale ? op.scale : 1;
        int fg = op.color, bg = op.bg == 0xFF ? 1 : op.bg;
        int x = op.x, y = op.y;
        if (op.flags & 0x10) {
            String digits;
            if (!ctEvalText(ct, op.resourceIdx, TTF_DATE, usage, env, digits) ||
                digits.length() < 1 || digits.length() > 3) return true;
            for (size_t i = 0; i < digits.length(); i++)
                if (digits[i] < '0' || digits[i] > '9') return true;
            x = digits.length() == 1 ? op.x : digits.length() == 2 ? op.x2 : op.y2;
        }
        int cellH = prop ? prop->lineHeight : font->Height;
        if (!hasRegion && scale == 1) {
            if (prop) drawPropText(*prop, val, x, y, 1, fg, bg, false, 0, 0, 0, 0);
            else Paint_DrawString_EN(x, y, val.c_str(), font, (UWORD)bg, (UWORD)fg);
        } else {
            int useScale = scale;
            int textW = prop ? propTextWidth(*prop, val, useScale)
                             : (int)val.length() * font->Width * useScale;
            while (hasRegion && useScale > 1 &&
                   (textW > op.w || cellH * useScale > op.h)) {
                useScale--;
                textW = prop ? propTextWidth(*prop, val, useScale)
                             : (int)val.length() * font->Width * useScale;
            }
            if (hasRegion) {
                x = op.x;
                if (op.align == 1) x = op.x + (op.w - textW) / 2;
                else if (op.align == 2) x = op.x + op.w - textW;
                y = op.y + (op.h - cellH * useScale) / 2;
            }
            if (prop) {
                drawPropText(*prop, val, x, y, useScale, fg, bg, hasRegion,
                             op.x, op.y, op.w, op.h);
            } else {
                drawScaledText(val, font, x, y, useScale, fg, bg, hasRegion,
                               op.x, op.y, op.w, op.h);
            }
        }
        return true;
    }
    case CT_BAR: {
        if (dry) return true;
        int x = op.x, y = op.y, w = op.w, h = op.h;
        int fg = op.fg, bg = op.bg;
        double maxV = op.maxVal > 0 ? op.maxVal : 100;
        double v = 0;
        if (op.bindIdx != CT_NONE_IDX) {
            BindSpec spec;
            ctBind(ct.reqs[op.bindIdx], spec);
            evalBindNum(spec, usage, env, v);
        }
        if (bg != COLOR_NONE) {
            Paint_DrawRectangle(x, y, x + w - 1, y + h - 1, (UWORD)bg,
                                DOT_PIXEL_1X1, DRAW_FILL_FULL);
        }
        int fw = (int)lround(w * (v / maxV));
        if (fw < 0) fw = 0;
        if (fw > w) fw = w;
        if (fw > 0) {
            Paint_DrawRectangle(x, y, x + fw - 1, y + h - 1, (UWORD)fg,
                                DOT_PIXEL_1X1, DRAW_FILL_FULL);
        }
        if (op.border) {
            Paint_DrawRectangle(x, y, x + w - 1, y + h - 1, (UWORD)fg,
                                DOT_PIXEL_1X1, DRAW_FILL_EMPTY);
        }
        return true;
    }
    case CT_RECT:
        if (dry) return true;
        Paint_DrawRectangle(op.x, op.y, op.x + op.w - 1, op.y + op.h - 1,
                            (UWORD)op.color, DOT_PIXEL_1X1,
                            (op.flags & 0x01) ? DRAW_FILL_FULL : DRAW_FILL_EMPTY);
        return true;
    case CT_LINE:
        if (dry) return true;
        Paint_DrawLine(op.x, op.y, op.x2, op.y2, (UWORD)op.color,
                       DOT_PIXEL_1X1, LINE_STYLE_SOLID);
        return true;
    case CT_ICON: {
        if (dry) return true;
        if (op.resourceIdx == CT_NONE_IDX || op.resourceIdx >= ct.resCount) return false;
        const CtResource &res = ct.res[op.resourceIdx];
        size_t need = (size_t)((res.w + 7) / 8) * res.h;
        uint8_t *buf = (uint8_t *)malloc(need);
        if (!buf) return false;
        bool ok = decodeBase64(res.bits, buf, need);
        if (ok) drawIcon(buf, op.x, op.y, res.w, res.h, op.color);
        free(buf);
        return ok;
    }
    default:
        return false;
    }
}

bool tplValidateCt(const CtTemplate &ct, String &err) {
    if (ct.abi != CT_ABI) { err = "ct_abi"; return false; }
    if (ct.opCount == 0 || ct.opCount > CT_MAX_OPS) { err = "ct_ops"; return false; }
    if (ct.reqCount > CT_MAX_REQS || ct.resCount > CT_MAX_RES) { err = "ct_bounds"; return false; }
    if (ct.id[0] == 0) { err = "ct_id"; return false; }
    for (int i = 0; i < ct.opCount; i++) {
        const CtOp &op = ct.ops[i];
        if (op.type > CT_ICON) { err = "ct_op_type"; return false; }
        if (op.bindIdx != CT_NONE_IDX && op.bindIdx >= ct.reqCount) { err = "ct_bind"; return false; }
        if (op.whenIdx != CT_NONE_IDX && op.whenIdx >= ct.reqCount) { err = "ct_when"; return false; }
        if ((op.flags & 0x10) && (op.type != CT_TEXT || op.resourceIdx >= ct.reqCount)) {
            err = "ct_digit_bind"; return false;
        }
        if (!(op.flags & 0x10) && op.resourceIdx != CT_NONE_IDX && op.resourceIdx >= ct.resCount) {
            err = "ct_res"; return false;
        }
        if (op.type == CT_TEXT && op.font >= tplFontCount()) { err = "ct_font"; return false; }
        if (op.type == CT_TEXT && op.bindIdx == CT_NONE_IDX && op.text[0] == 0) {
            err = "ct_text"; return false;
        }
    }
    for (int i = 0; i < ct.reqCount; i++) {
        if (ct.reqs[i].path[0] == 0) { err = "ct_req"; return false; }
    }
    return true;
}

bool tplDrawCt(const CtTemplate &ct, const String &usageJson, const TplEnv &env) {
    String err;
    if (!tplValidateCt(ct, err)) {
        Serial.printf("[tpl] ct reject: %s\n", err.c_str());
        return false;
    }
    JsonDocument ud;
    bool haveUsage = usageJson.length() > 0 && !deserializeJson(ud, usageJson);
    for (int i = 0; i < ct.opCount; i++) {
        if (!drawCtOp(ct, ct.ops[i], ud, env, haveUsage, false)) return false;
    }
    return true;
}

static bool parseTemplate(const String &tmplJson, JsonDocument &td, JsonArray &els, String &err) {
    if (deserializeJson(td, tmplJson)) { err = "json"; return false; }
    if ((td["schema"] | 0) != 1) { err = "schema"; return false; }
    JsonObject canvas = td["canvas"].as<JsonObject>();
    if (canvas.isNull() || (canvas["w"] | 0) != TPL_W || (canvas["h"] | 0) != TPL_H) {
        err = "canvas";
        return false;
    }
    els = td["elements"].as<JsonArray>();
    if (els.isNull() || els.size() == 0) { err = "elements"; return false; }
    return true;
}

bool tplCompile(const String &tmplJson, CtTemplate &out, String &err) {
    JsonDocument td;
    JsonArray els;
    if (!parseTemplate(tmplJson, td, els, err)) return false;
    memset(&out, 0, sizeof(out));
    out.abi = CT_ABI;
    out.sourceCrc = v2Crc32((const uint8_t *)tmplJson.c_str(), tmplJson.length());
    ctCopy(out.id, sizeof(out.id), td["id"] | "");
    if (els.size() > CT_MAX_OPS) { err = "elements"; return false; }
    for (JsonObject e : els) {
        if (out.opCount >= CT_MAX_OPS) { err = "elements"; return false; }
        CtOp op;
        if (!parseElementCompiled(e, out, op)) { err = "element"; return false; }
        out.ops[out.opCount++] = op;
    }
    return tplValidateCt(out, err);
}

bool tplValidate(const String &tmplJson, String &err) {
    static CtTemplate scratch;
    return tplCompile(tmplJson, scratch, err);
}

bool tplDraw(const String &tmplJson, const String &usageJson, const TplEnv &env) {
    static CtTemplate scratch;
    String err;
    if (!tplCompile(tmplJson, scratch, err)) {
        Serial.printf("[tpl] reject: %s\n", err.c_str());
        return false;
    }
    return tplDrawCt(scratch, usageJson, env);
}

size_t tplCtSize() { return sizeof(CtTemplate); }

// Fixed-layout record: magic | size | abi | crc32 | CtTemplate bytes.
bool tplCtSerialize(const CtTemplate &ct, uint8_t *out, size_t cap, size_t &written) {
    const size_t payload = sizeof(CtTemplate);
    const size_t total = 4 + 2 + 2 + 4 + payload;
    written = total;
    if (cap < total) return false;
    uint32_t magic = 0x31505443u;  // "CTP1" little-endian
    memcpy(out, &magic, 4);
    uint16_t size = (uint16_t)payload;
    memcpy(out + 4, &size, 2);
    uint16_t abi = CT_ABI;
    memcpy(out + 6, &abi, 2);
    uint32_t crc = v2Crc32((const uint8_t *)&ct, payload);
    memcpy(out + 8, &crc, 4);
    memcpy(out + 12, &ct, payload);
    return true;
}

bool tplCtDeserialize(const uint8_t *in, size_t len, CtTemplate &out, String &err) {
    if (len < 12) { err = "ct_short"; return false; }
    uint32_t magic = 0;
    memcpy(&magic, in, 4);
    if (magic != 0x31505443u) { err = "ct_magic"; return false; }
    uint16_t size = 0, abi = 0;
    memcpy(&size, in + 4, 2);
    memcpy(&abi, in + 6, 2);
    if (size != sizeof(CtTemplate) || abi != CT_ABI) { err = "ct_abi"; return false; }
    if (len < 12 + size) { err = "ct_short"; return false; }
    uint32_t crc = 0;
    memcpy(&crc, in + 8, 4);
    if (crc != v2Crc32(in + 12, size)) { err = "ct_crc"; return false; }
    memcpy(&out, in + 12, size);
    return tplValidateCt(out, err);
}
