#include "refresh_policy.h"

bool rgnDirtyWindow(const uint8_t *oldFrame, const uint8_t *newFrame,
                    uint16_t width, uint16_t height, DirtyWindow &out) {
    if (!oldFrame || !newFrame || !width || !height || width % 8) return false;
    int x0 = width, y0 = height, x1 = -1, y1 = -1;
    const int stride = width / 8;
    for (int y = 0; y < height; ++y) {
        for (int bx = 0; bx < stride; ++bx) {
            uint8_t diff = oldFrame[y * stride + bx] ^ newFrame[y * stride + bx];
            if (!diff) continue;
            for (int bit = 0; bit < 8; ++bit) {
                if (!(diff & (0x80 >> bit))) continue;
                int x = bx * 8 + bit;
                if (x < x0) x0 = x;
                if (x > x1) x1 = x;
            }
            if (y < y0) y0 = y;
            if (y > y1) y1 = y;
        }
    }
    if (x1 < 0) return false;
    out.x0 = (x0 > 0 ? x0 - 1 : 0) & ~7;
    out.x1 = ((x1 + 1 < width ? x1 + 1 : width - 1) | 7);
    out.y0 = y0 > 0 ? y0 - 1 : 0;
    out.y1 = y1 + 1 < height ? y1 + 1 : height - 1;
    return true;
}

#include <ArduinoJson.h>
#include <string.h>

#include "fonts.h"
#include "platform_target.h"

namespace {

// Panel geometry is a target property (v2 §5): set at boot / by the host
// harness so the same policy serves 200x200 and 400x300 panels.
static int sPanelW = TARGET_WIDTH;
static int sPanelH = TARGET_HEIGHT;
static inline int panelW() { return sPanelW; }
static inline int panelH() { return sPanelH; }
static inline int panelStride() { return sPanelW / 8; }
#define PANEL_W (panelW())
#define PANEL_H (panelH())
#define STRIDE  (panelStride())
void rgnSetPanel(int w, int h) {
    if (w > 0 && h > 0) {
        sPanelW = w;
        sPanelH = h;
    }
}

// The firmware toolchain is xtensa GCC; the host preview build may use MSVC.
inline int popcount8(uint8_t v) {
#if defined(__GNUC__)
    return __builtin_popcount((unsigned)v);
#else
    int c = 0;
    while (v) { v &= (uint8_t)(v - 1); c++; }
    return c;
#endif
}

sFONT *fontByName(const char *name) {
    if (!name) return nullptr;
    if (!strcmp(name, "f8"))  return &Font8;
    if (!strcmp(name, "f12")) return &Font12;
    if (!strcmp(name, "f16")) return &Font16;
    if (!strcmp(name, "f20")) return &Font20;
    if (!strcmp(name, "f24")) return &Font24;
    return nullptr;
}

// Mirror of the template engine's color mapping (B/W panel: accents collapse
// to black ink).
int colorVal(JsonVariant v, int def) {
    if (v.isNull()) return def;
    if (v.is<int>()) return v.as<int>();
    const char *s = v.as<const char *>();
    if (!s) return def;
    if (!strcmp(s, "black"))  return 0;
    if (!strcmp(s, "white"))  return 1;
    if (!strcmp(s, "yellow")) return 0;
    if (!strcmp(s, "red"))    return 0;
    if (!strcmp(s, "none"))   return 0xFF;
    return def;
}

// Conservative class precedence when regions merge: a merged region keeps the
// class of the most ink-heavy member (design §8.1).
int classRank(uint8_t cls) {
    switch (cls) {
    case RGN_SOLID:       return 8;
    case RGN_INVERTED:    return 7;
    case RGN_USAGE_DIGIT: return 6;
    case RGN_CLOCK:       return 5;
    case RGN_BAR:         return 4;
    case RGN_TEXT:        return 3;
    case RGN_ICON:        return 2;
    case RGN_LINE:        return 1;
    default:              return 0;
    }
}

int classDefaultBudget(uint8_t cls) {
    switch (cls) {
    case RGN_CLOCK: return 90;   // matches CLK_GHOST_LIMIT (docs §13.2)
    case RGN_SOLID:
    case RGN_INVERTED: return 4; // high-ink starting budget (design §8.2)
    default: return 10;
    }
}

bool clipRect(int &x, int &y, int &w, int &h) {
    if (w <= 0 || h <= 0) return false;
    if (x < 0) { w += x; x = 0; }
    if (y < 0) { h += y; y = 0; }
    if (x >= PANEL_W || y >= PANEL_H) return false;
    if (x + w > PANEL_W) w = PANEL_W - x;
    if (y + h > PANEL_H) h = PANEL_H - y;
    return w > 0 && h > 0;
}

bool addRegion(RgnSet &out, int x, int y, int w, int h, uint8_t cls, bool highInk) {
    if (!clipRect(x, y, w, h)) return false;
    if (out.n >= RGN_MAX) {
        out.wholeFrame = true;
        return false;
    }
    Rgn &r = out.r[out.n++];
    r = Rgn();
    r.cls = cls;
    r.highInk = highInk;
    r.px0 = (uint16_t)x;
    r.py0 = (uint16_t)y;
    r.px1 = (uint16_t)(x + w - 1);
    r.py1 = (uint16_t)(y + h - 1);
    r.x0b = (uint8_t)(x >> 3);
    r.x1b = (uint8_t)((x + w - 1) >> 3);
    r.area = (uint16_t)(w * h);
    r.budget = (uint8_t)classDefaultBudget(cls);
    return true;
}

bool rectFromElement(JsonObject e, int &x, int &y, int &w, int &h) {
    JsonArray r = e["rect"].as<JsonArray>();
    if (r.isNull() || r.size() < 4) return false;
    x = r[0] | 0;
    y = r[1] | 0;
    w = r[2] | 0;
    h = r[3] | 0;
    return true;
}

// Merge regions that overlap in real pixels, keeping the most conservative
// class. Byte-column expansion is deliberately not used here: it would glue
// adjacent icons/text into the black tile and turn every icon update into a
// full refresh (user-visible regression). Window writes expand rects to bytes
// at write time instead.
void mergeRegions(RgnSet &out) {
    bool merged = true;
    while (merged && out.n > 1) {
        merged = false;
        for (uint8_t i = 0; i < out.n && !merged; i++) {
            for (uint8_t j = (uint8_t)(i + 1); j < out.n; j++) {
                const Rgn &a = out.r[i];
                const Rgn &b = out.r[j];
                bool overlap = !(a.px1 < b.px0 || b.px1 < a.px0 ||
                                 a.py1 < b.py0 || b.py1 < a.py0);
                if (!overlap) continue;
                Rgn &m = out.r[i];
                m.px0 = a.px0 < b.px0 ? a.px0 : b.px0;
                m.py0 = a.py0 < b.py0 ? a.py0 : b.py0;
                m.px1 = a.px1 > b.px1 ? a.px1 : b.px1;
                m.py1 = a.py1 > b.py1 ? a.py1 : b.py1;
                m.x0b = (uint8_t)(m.px0 >> 3);
                m.x1b = (uint8_t)(m.px1 >> 3);
                m.highInk = a.highInk || b.highInk;
                if (classRank(b.cls) > classRank(a.cls)) m.cls = b.cls;
                m.area = (uint16_t)((m.px1 - m.px0 + 1) * (m.py1 - m.py0 + 1));
                m.budget = (uint8_t)classDefaultBudget(m.cls);
                out.r[j] = out.r[--out.n];
                merged = true;
                break;
            }
        }
    }
}

}  // namespace

void rgnReset(RgnSet &out) {
    out.n = 0;
    out.wholeFrame = false;
}

bool rgnBuild(const String &tmplJson, RgnSet &out) {
    rgnReset(out);
    if (!tmplJson.length()) {
        out.wholeFrame = true;
        return false;
    }

    JsonDocument doc;
    if (deserializeJson(doc, tmplJson)) {
        out.wholeFrame = true;
        return false;
    }
    JsonArray els = doc["elements"].as<JsonArray>();
    if (els.isNull() || els.size() == 0) {
        out.wholeFrame = true;
        return false;
    }

    for (JsonObject e : els) {
        const char *type = e["type"] | "";
        if (!strcmp(type, "text")) {
            sFONT *font = fontByName(e["font"] | "");
            if (!font) { out.wholeFrame = true; return false; }
            const char *bind = e["bind"] | "";
            const char *text = e["text"] | "";
            if (!strlen(bind) && !strlen(text)) { out.wholeFrame = true; return false; }
            int x, y, w, h;
            JsonArray region = e["region"].as<JsonArray>();
            if (!region.isNull() && region.size() == 4) {
                x = region[0] | 0; y = region[1] | 0;
                w = region[2] | 0; h = region[3] | 0;
            } else {
                int scale = e["scale"] | 1;
                if (scale < 1) scale = 1;
                if (scale > 3) scale = 3;
                int chars = 0;
                if (strlen(text)) {
                    chars = (int)strlen(text) + (int)strlen(e["prefix"] | "") +
                            (int)strlen(e["suffix"] | "");
                } else if (!strcmp(bind, "device.now")) {
                    chars = 5;   // "HH:MM"
                } else {
                    chars = 12;  // bounded estimate for bind-rendered text
                }
                x = e["x"] | 0;
                y = e["y"] | 0;
                w = chars * font->Width * scale;
                h = font->Height * scale;
            }
            int fg = colorVal(e["color"], 0);
            int bg = e["bg"] ? colorVal(e["bg"], 1) : 1;
            bool inverted = (bg == 0 && fg == 1);
            bool highInk = (bg == 0);
            uint8_t cls;
            if (!strcmp(bind, "device.now"))            cls = RGN_CLOCK;
            else if (!strncmp(bind, "buckets[", 8) ||
                     !strncmp(bind, "resetCredits.", 13)) cls = RGN_USAGE_DIGIT;
            else if (inverted)                          cls = RGN_INVERTED;
            else                                        cls = RGN_TEXT;
            addRegion(out, x, y, w, h, cls, highInk);
        } else if (!strcmp(type, "rect")) {
            int x, y, w, h;
            if (!rectFromElement(e, x, y, w, h)) { out.wholeFrame = true; return false; }
            bool fill = e["fill"] | false;
            int color = colorVal(e["color"], 0);
            bool highInk = fill && color == 0;
            addRegion(out, x, y, w, h, fill ? (highInk ? RGN_SOLID : RGN_TEXT) : RGN_LINE,
                      highInk);
        } else if (!strcmp(type, "icon")) {
            int x = e["x"] | 0, y = e["y"] | 0;
            int w = e["w"] | 0, h = e["h"] | 0;
            if (!strlen(e["bits"] | "")) { out.wholeFrame = true; return false; }
            addRegion(out, x, y, w, h, RGN_ICON, false);
        } else if (!strcmp(type, "bar")) {
            int x, y, w, h;
            if (!rectFromElement(e, x, y, w, h)) { out.wholeFrame = true; return false; }
            addRegion(out, x, y, w, h, RGN_BAR, false);
        } else if (!strcmp(type, "line")) {
            if (e["x1"].isNull() || e["y1"].isNull() ||
                e["x2"].isNull() || e["y2"].isNull()) {
                out.wholeFrame = true;
                return false;
            }
            int x1 = e["x1"] | 0, y1 = e["y1"] | 0;
            int x2 = e["x2"] | 0, y2 = e["y2"] | 0;
            int x = x1 < x2 ? x1 : x2;
            int y = y1 < y2 ? y1 : y2;
            int w = (x1 < x2 ? x2 - x1 : x1 - x2) + 1;
            int h = (y1 < y2 ? y2 - y1 : y1 - y2) + 1;
            addRegion(out, x, y, w, h, RGN_LINE, false);
        } else {
            out.wholeFrame = true;
            return false;
        }
    }
    if (out.n == 0) {
        out.wholeFrame = true;
        return false;
    }
    mergeRegions(out);
    // addRegion sets wholeFrame on overflow; keep that conservative outcome.
    return !out.wholeFrame;
}

// Compiled-template region derivation (v2 §8): identical classification to
// rgnBuild, but from the bounded compiled ops so no template JSON is parsed on
// an active switch.
bool rgnBuildCt(const CtTemplate &ct, RgnSet &out) {
    rgnReset(out);
    if (ct.opCount == 0) {
        out.wholeFrame = true;
        return false;
    }
    for (uint8_t i = 0; i < ct.opCount; i++) {
        const CtOp &op = ct.ops[i];
        const char *bind = op.bindIdx != CT_NONE_IDX ? ct.reqs[op.bindIdx].path : "";
        switch (op.type) {
        case CT_TEXT: {
            sFONT *font = nullptr;
            switch (op.font) {
            case 0: font = &Font8; break;
            case 1: font = &Font12; break;
            case 2: font = &Font16; break;
            case 3: font = &Font20; break;
            case 4: font = &Font24; break;
            default: break;
            }
            if (!font) { out.wholeFrame = true; return false; }
            int fw = font->Width;
            int fh = font->Height;
            int x, y, w, h;
            if (op.flags & 0x02) {
                x = op.x; y = op.y; w = op.w; h = op.h;
            } else {
                int scale = op.scale ? op.scale : 1;
                if (scale < 1) scale = 1;
                if (scale > 3) scale = 3;
                int chars;
                if (strlen(op.text)) {
                    chars = (int)strlen(op.text) + (int)strlen(op.prefix) +
                            (int)strlen(op.suffix);
                } else if (!strcmp(bind, "device.now")) {
                    chars = 5;
                } else {
                    chars = 12;
                }
                x = op.x;
                y = op.y;
                w = chars * fw * scale;
                h = fh * scale;
            }
            int fg = op.color;
            int bg = op.bg == 0xFF ? 1 : op.bg;
            bool inverted = (bg == 0 && fg == 1);
            bool highInk = (bg == 0);
            uint8_t cls;
            if (!strcmp(bind, "device.now")) cls = RGN_CLOCK;
            else if (!strncmp(bind, "buckets[", 8) || !strncmp(bind, "resetCredits.", 13))
                cls = RGN_USAGE_DIGIT;
            else if (inverted) cls = RGN_INVERTED;
            else cls = RGN_TEXT;
            addRegion(out, x, y, w, h, cls, highInk);
            break;
        }
        case CT_RECT: {
            bool fill = (op.flags & 0x01) != 0;
            bool highInk = fill && op.color == 0;
            addRegion(out, op.x, op.y, op.w, op.h,
                      fill ? (highInk ? RGN_SOLID : RGN_TEXT) : RGN_LINE, highInk);
            break;
        }
        case CT_ICON:
            addRegion(out, op.x, op.y, op.w, op.h, RGN_ICON, false);
            break;
        case CT_BAR:
            addRegion(out, op.x, op.y, op.w, op.h, RGN_BAR, false);
            break;
        case CT_LINE: {
            int x = op.x < op.x2 ? op.x : op.x2;
            int y = op.y < op.y2 ? op.y : op.y2;
            int w = (op.x < op.x2 ? op.x2 - op.x : op.x - op.x2) + 1;
            int h = (op.y < op.y2 ? op.y2 - op.y : op.y - op.y2) + 1;
            addRegion(out, x, y, w, h, RGN_LINE, false);
            break;
        }
        default:
            out.wholeFrame = true;
            return false;
        }
    }
    if (out.n == 0) {
        out.wholeFrame = true;
        return false;
    }
    mergeRegions(out);
    return !out.wholeFrame;
}

RfnDecision rgnDecide(RgnSet &set, const uint8_t *oldFb, const uint8_t *newFb,
                      bool trusted, bool forceFull, bool clean) {
    RfnDecision d;
    if (!oldFb || !newFb) {
        d.action = RFN_FULL;
        d.reason = RFNR_TRUST;
        return d;
    }

    // Full-frame changed pixels (1bpp byte compare, matching the legacy rule).
    uint32_t total = 0;
    for (int i = 0; i < STRIDE * PANEL_H; i++) {
        total += (uint32_t)popcount8((uint8_t)(oldFb[i] ^ newFb[i]));
    }
    d.changed = (uint16_t)(total > 0xFFFF ? 0xFFFF : total);

    // Per-region statistics over the original semantic rect. Edge bytes are
    // masked so neighboring pixels in the same byte column are never counted.
    uint32_t inside = 0;
    for (uint8_t i = 0; i < set.n; i++) {
        Rgn &r = set.r[i];
        uint32_t ch = 0, w2b = 0, b2w = 0, bo = 0, bn = 0;
        const uint8_t lead = (uint8_t)(0xFF >> (r.px0 & 7));
        const uint8_t trail = (uint8_t)(0xFF << (7 - (r.px1 & 7)));
        for (uint16_t y = r.py0; y <= r.py1; y++) {
            const uint8_t *o = oldFb + (uint32_t)y * STRIDE + r.x0b;
            const uint8_t *nw = newFb + (uint32_t)y * STRIDE + r.x0b;
            for (int b = 0; b <= (int)(r.x1b - r.x0b); b++) {
                uint8_t mask = 0xFF;
                if (b == 0 && (r.px0 & 7)) mask &= lead;
                if (b == (int)(r.x1b - r.x0b) && (r.px1 & 7) != 7) mask &= trail;
                uint8_t ov = o[b] & mask, nv = nw[b] & mask;
                ch += (uint32_t)popcount8((uint8_t)(ov ^ nv));
                w2b += (uint32_t)popcount8((uint8_t)(ov & ~nv));
                b2w += (uint32_t)popcount8((uint8_t)(~ov & nv));
                bo += (uint32_t)popcount8((uint8_t)~ov & mask);
                bn += (uint32_t)popcount8((uint8_t)~nv & mask);
            }
        }
        r.changed = (uint16_t)(ch > 0xFFFF ? 0xFFFF : ch);
        r.w2b = (uint16_t)(w2b > 0xFFFF ? 0xFFFF : w2b);
        r.b2w = (uint16_t)(b2w > 0xFFFF ? 0xFFFF : b2w);
        r.bOld = (uint16_t)(bo > 0xFFFF ? 0xFFFF : bo);
        r.bNew = (uint16_t)(bn > 0xFFFF ? 0xFFFF : bn);
        inside += ch;
    }
    d.dirty = (uint16_t)(inside > 0xFFFF ? 0xFFFF : inside);
    d.outside = (uint16_t)(total - inside > 0xFFFF ? 0xFFFF : total - inside);

    if (clean) {
        d.action = RFN_FULL;
        d.reason = RFNR_CLEAN;
        return d;
    }
    if (forceFull) {
        d.action = RFN_FULL;
        d.reason = RFNR_FORCE;
        return d;
    }
    if (!trusted) {
        d.action = RFN_FULL;
        d.reason = RFNR_TRUST;
        return d;
    }
    if (total == 0) {
        d.action = RFN_NONE;
        d.reason = RFNR_NONE;
        return d;
    }
    if (set.wholeFrame || set.n == 0) {
        d.action = RFN_FULL;
        d.reason = RFNR_DERIVE;
        return d;
    }
    if (total > (uint32_t)PANEL_W * PANEL_H / 8) {   // >12.5% full-frame rule
        d.action = RFN_FULL;
        d.reason = RFNR_AREA;
        return d;
    }

    bool anyLowInk = d.outside > 0;
    for (uint8_t i = 0; i < set.n; i++) {
        Rgn &r = set.r[i];
        if (!r.changed || !r.area) continue;
        uint32_t dRatio = (uint32_t)r.changed * 1000 / r.area;
        uint32_t absB = r.bNew > r.bOld ? r.bNew - r.bOld : r.bOld - r.bNew;
        if (r.highInk) {
            // Conservative gate: any black-tile/polarity movement takes the
            // full waveform until the photo acceptance in task-5 passes.
            if ((uint32_t)absB * 1000 / r.area >= 150 ||
                (uint32_t)r.b2w * 1000 / r.area >= 100 ||
                dRatio >= 250 || r.budget == 0 || r.cumS >= 500) {
                d.action = RFN_FULL;
                d.reason = RFNR_POLARITY;
            } else {
                d.action = RFN_FULL;
                d.reason = RFNR_HIGH_INK;
            }
            d.region = i;
            return d;
        }
        if (r.budget == 0 || (uint32_t)r.cumS + dRatio >= 1000) {
            d.action = RFN_FULL;
            d.reason = RFNR_BUDGET;
            d.region = i;
            return d;
        }
        anyLowInk = true;
    }
    d.action = anyLowInk ? RFN_PARTIAL : RFN_NONE;
    d.reason = anyLowInk ? RFNR_OK : RFNR_NONE;
    return d;
}

void rgnOnPartial(RgnSet &set) {
    for (uint8_t i = 0; i < set.n; i++) {
        Rgn &r = set.r[i];
        if (!r.changed || !r.area) continue;
        uint32_t dRatio = (uint32_t)r.changed * 1000 / r.area;
        if (r.cumS + dRatio > 0xFFFF) r.cumS = 0xFFFF;
        else r.cumS = (uint16_t)(r.cumS + dRatio);
        if (r.partials < 0xFFFF) r.partials++;
        if (r.budget > 0) r.budget--;
    }
}

void rgnOnFull(RgnSet &set) {
    for (uint8_t i = 0; i < set.n; i++) {
        Rgn &r = set.r[i];
        r.partials = 0;
        r.cumS = 0;
        r.budget = (uint8_t)classDefaultBudget(r.cls);
    }
}

const char *rgnClassName(uint8_t cls) {
    switch (cls) {
    case RGN_SOLID:       return "solid";
    case RGN_INVERTED:    return "inverted";
    case RGN_USAGE_DIGIT: return "usage";
    case RGN_CLOCK:       return "clock";
    case RGN_BAR:         return "bar";
    case RGN_TEXT:        return "text";
    case RGN_ICON:        return "icon";
    case RGN_LINE:        return "line";
    default:              return "?";
    }
}

const char *rfnActionName(uint8_t action) {
    switch (action) {
    case RFN_NONE:    return "none";
    case RFN_PARTIAL: return "partial";
    case RFN_FULL:    return "full";
    default:          return "?";
    }
}

const char *rfnReasonName(uint8_t reason) {
    switch (reason) {
    case RFNR_NONE:     return "none";
    case RFNR_OK:       return "ok";
    case RFNR_CLEAN:    return "clean";
    case RFNR_FORCE:    return "force";
    case RFNR_TRUST:    return "trust";
    case RFNR_DERIVE:   return "derive";
    case RFNR_AREA:     return "area";
    case RFNR_POLARITY: return "polarity";
    case RFNR_HIGH_INK: return "high_ink";
    case RFNR_BUDGET:   return "budget";
    default:            return "?";
    }
}
