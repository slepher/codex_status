#include "template_engine.h"
#include <ArduinoJson.h>
#include <math.h>
#include <string.h>
#include <time.h>
#include <mbedtls/base64.h>
#include "GUI_Paint.h"
#include "fonts.h"

static const int TPL_W = 200;
static const int TPL_H = 200;
static const UWORD COLOR_NONE = 0xFF;

enum BindKind {
    B_PLAN, B_LABEL, B_HOSTID, B_SERVER_TIME,
    B_RESET_COUNT, B_RESET_EXPIRES,
    B_BUCKET_USED, B_BUCKET_REMAIN, B_BUCKET_RESET, B_BUCKET_WINMINS,
    B_DEV_CHANNEL, B_DEV_IP, B_DEV_SYNC, B_DEV_BATTERY,
    B_DEV_STATE, B_DEV_OFFLINE, B_DEV_NOW, B_DEV_MODE
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

static sFONT *fontByName(const char *name) {
    if (!name) return nullptr;
    if (!strcmp(name, "f8"))  return &Font8;
    if (!strcmp(name, "f12")) return &Font12;
    if (!strcmp(name, "f16")) return &Font16;
    if (!strcmp(name, "f20")) return &Font20;
    if (!strcmp(name, "f24")) return &Font24;
    return nullptr;
}

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
        time_t n = time(nullptr);
        if (n < 1600000000) return false;
        struct tm *lt = localtime(&n);
        if (!lt) return false;
        char b[8];
        strftime(b, sizeof(b), "%H:%M", lt);
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

static bool evalBindNum(const BindSpec &s, JsonDocument &usage, double &v) {
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
    case B_DEV_NOW:     return time(nullptr) > 1600000000;
    case B_DEV_MODE:    return env.mode.length() > 0;
    case B_PLAN:        return haveUsage && !usage["account"]["plan"].isNull();
    case B_LABEL:       return haveUsage && !usage["bridge"]["label"].isNull();
    case B_HOSTID:      return haveUsage && !usage["bridge"]["hostId"].isNull();
    case B_SERVER_TIME: return haveUsage && !usage["server_time"].isNull();
    case B_RESET_COUNT: return haveUsage && !usage["resetCredits"]["availableCount"].isNull();
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

static bool conditionMatches(const DrawCondition &condition, JsonDocument &usage,
                             const TplEnv &env, bool haveUsage) {
    if (!condition.active) return true;
    if (condition.useEquals) {
        String value;
        if (!evalBind(condition.bind, TTF_DATE, usage, env, value)) return false;
        return value == condition.equals;
    }
    return bindExists(condition.bind, usage, env, haveUsage) == condition.exists;
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

static bool drawElements(JsonArray els, JsonDocument &usage, const TplEnv &env,
                         bool haveUsage, bool dry) {
    for (JsonObject e : els) {
        DrawCondition condition;
        if (!parseCondition(e, condition)) return false;
        if (!dry) {
            if (!conditionMatches(condition, usage, env, haveUsage)) continue;
        }
        const char *type = e["type"] | "";
        if (!strcmp(type, "text")) {
            sFONT *font = fontByName(e["font"] | "");
            if (!font) return false;
            const char *bind = e["bind"] | "";
            const char *text = e["text"] | "";
            if (!strlen(bind) && !strlen(text)) return false;
            BindSpec spec;
            if (strlen(bind) && !parseBind(String(bind), spec)) return false;
            int scale = 1, rx = 0, ry = 0, rw = 0, rh = 0;
            bool hasRegion = false;
            TextTimeFormat format = TTF_DATE;
            if (!parseTextLayout(e, strlen(bind) ? &spec : nullptr, scale,
                                 hasRegion, rx, ry, rw, rh, format)) return false;
            if (!dry) {
                String val;
                if (strlen(bind)) {
                    if (!haveUsage) val = "--";
                    else if (!evalBind(spec, format, usage, env, val)) val = "--";
                } else {
                    val = text;
                }
                val = asciiOnly(String((const char *)(e["prefix"] | "")) + val +
                                String((const char *)(e["suffix"] | "")));
                int fg = colorVal(e["color"], 0);
                int bg = e["bg"] ? colorVal(e["bg"], 1) : 1;
                int x = e["x"] | 0, y = e["y"] | 0;
                if (!hasRegion && scale == 1) {
                    Paint_DrawString_EN(x, y, val.c_str(), font, (UWORD)bg, (UWORD)fg);
                } else {
                    int useScale = scale;
                    int textW = (int)val.length() * font->Width * useScale;
                    while (hasRegion && useScale > 1 &&
                           (textW > rw || font->Height * useScale > rh)) {
                        useScale--;
                        textW = (int)val.length() * font->Width * useScale;
                    }
                    if (hasRegion) {
                        const char *align = e["align"] | "left";
                        x = rx;
                        if (!strcmp(align, "center")) x = rx + (rw - textW) / 2;
                        else if (!strcmp(align, "right")) x = rx + rw - textW;
                        y = ry + (rh - font->Height * useScale) / 2;
                    }
                    drawScaledText(val, font, x, y, useScale, fg, bg,
                                   hasRegion, rx, ry, rw, rh);
                }
            }
        } else if (!strcmp(type, "bar")) {
            const char *bind = e["bind"] | "";
            BindSpec spec;
            if (!strlen(bind) || !parseBind(String(bind), spec)) return false;
            int x, y, w, h;
            if (!clampRect(e["rect"].as<JsonArray>(), x, y, w, h)) return false;
            if (!dry) {
                int fg = e["fg"] ? colorVal(e["fg"], 0) : 0;
                int bg = e["bg"] ? colorVal(e["bg"], 1) : 1;
                bool border = e["border"] | true;
                double maxV = e["max"] | 100.0;
                if (maxV <= 0) maxV = 100;
                double v = 0;
                if (haveUsage) evalBindNum(spec, usage, v);
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
                if (border) {
                    Paint_DrawRectangle(x, y, x + w - 1, y + h - 1, (UWORD)fg,
                                        DOT_PIXEL_1X1, DRAW_FILL_EMPTY);
                }
            }
        } else if (!strcmp(type, "rect")) {
            int x, y, w, h;
            if (!clampRect(e["rect"].as<JsonArray>(), x, y, w, h)) return false;
            if (!dry) {
                int color = colorVal(e["color"], 0);
                bool fill = e["fill"] | false;
                Paint_DrawRectangle(x, y, x + w - 1, y + h - 1, (UWORD)color,
                                    DOT_PIXEL_1X1,
                                    fill ? DRAW_FILL_FULL : DRAW_FILL_EMPTY);
            }
        } else if (!strcmp(type, "line")) {
            int x1 = e["x1"] | 0, y1 = e["y1"] | 0, x2 = e["x2"] | 0, y2 = e["y2"] | 0;
            if (e["x1"].isNull() || e["y1"].isNull() || e["x2"].isNull() || e["y2"].isNull())
                return false;
            if (!dry) {
                int color = colorVal(e["color"], 0);
                Paint_DrawLine(x1, y1, x2, y2, (UWORD)color,
                               DOT_PIXEL_1X1, LINE_STYLE_SOLID);
            }
        } else if (!strcmp(type, "icon")) {
            int x = e["x"] | 0, y = e["y"] | 0;
            int w = e["w"] | 0, h = e["h"] | 0;
            const char *b64 = e["bits"] | "";
            if (w <= 0 || h <= 0 || !strlen(b64)) return false;
            if (!dry) {
                int fg = colorVal(e["color"], 0);
                size_t need = (size_t)((w + 7) / 8) * h;
                uint8_t *buf = (uint8_t *)malloc(need);
                if (!buf) return false;
                bool ok = decodeBase64(b64, buf, need);
                if (ok) drawIcon(buf, x, y, w, h, fg);
                free(buf);
                if (!ok) return false;
            }
        } else {
            return false;
        }
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

bool tplValidate(const String &tmplJson, String &err) {
    JsonDocument td;
    JsonArray els;
    if (!parseTemplate(tmplJson, td, els, err)) return false;
    JsonDocument empty;
    TplEnv env;
    env.channel = "--";
    env.ip = "--";
    env.syncHHMM = "--:--";
    env.battery = -1;
    if (!drawElements(els, empty, env, false, true)) { err = "element"; return false; }
    return true;
}

bool tplDraw(const String &tmplJson, const String &usageJson, const TplEnv &env) {
    JsonDocument td;
    JsonArray els;
    String err;
    if (!parseTemplate(tmplJson, td, els, err)) {
        Serial.printf("[tpl] reject: %s\n", err.c_str());
        return false;
    }
    JsonDocument ud;
    bool haveUsage = usageJson.length() > 0 && !deserializeJson(ud, usageJson);
    if (!drawElements(els, ud, env, haveUsage, true)) {
        Serial.println("[tpl] reject: element");
        return false;
    }
    drawElements(els, ud, env, haveUsage, false);
    return true;
}
