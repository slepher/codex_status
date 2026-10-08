#include "display_internal.h"
#include "application_internal.h"
#include "clock_internal.h"
#include "diagnostics_internal.h"
#include "platform_internal.h"
#include "power_internal.h"
#include "radio_internal.h"
#include "transport_internal.h"

namespace firmware::display {

State state;
RTC_DATA_ATTR Retained retained;

#if defined(CODEX_TARGET_NOTE4)
RTC_DATA_ATTR static uint32_t rtcNote4FrameHash = 0;
#endif

static bool     panelThinReady = false;    // epdThinBegin() ran on this boot

static bool     epdFullLut = false;        // full-refresh LUT currently loaded

struct WinInfo {
    bool found = false;
    int used = -1;
    long long resets = 0;
};

RTC_DATA_ATTR static uint8_t  clkPixels[CLK_MAX_BYTES] = {};

// ---- Quad built-in screen (B/W design, partial refresh) ----
// Block size matches the factory UI proportions (~52% x 34% of the panel).
static const int Q_BLK_W = 104;

static const int Q_BLK_H = 90;

static const int Q_TL_X  = 4;

static const int Q_TL_Y  = 4;

static const int Q_BR_X  = EPD_W - 4 - Q_BLK_W;

static const int Q_BR_Y  = EPD_H - 4 - Q_BLK_H;

static const int Q_TR_RIGHT = EPD_W - 4;

static const int Q_TR_Y0 = 8;

static const int Q_LINE_H = 14;

static const int Q_BL_X = 4;

static const int Q_BL_SLOTS[4] = {140, 154, 168, 182};

static String fitText(const String &s, size_t maxChars = 17);
static std::vector<String> wrapText(const String &s, size_t maxChars = 17);
static WinInfo findWindow(JsonDocument &doc, bool fiveHour);
static void screenStatus();
static void epdFlush(bool forceFull = false);
static int clkFontId(const char *name);
static void clkBaselineAfterPanelWrite(bool full, const DirtyWindow *win);
#if defined(CODEX_TARGET_NOTE4)
static uint32_t note4FrameHash(const uint8_t *pixels);
#endif
static bool clkBlitString(uint8_t *buf, const char *s);
static int bigTextWidth(const String &s, int scale, int gap);
static void drawBigChar(char c, int x, int y, int scale);
static void drawBigCentered(const String &s, int bx, int by, int bw, int bh);
static void drawHeroBlock(int x, int y, const String &value);
static void drawSmallRight(const String &s, int right, int y);
static void drawSmallLeft(const String &s, int x, int y);
static void renderUsage(const String &json, const char *channel);
static void renderSleepGlyph();
static void v2RendezvousClockRender();

#if defined(CODEX_TARGET_NOTE4)
bool setNote4PanelPower(bool keep) {
    Preferences p;
    if (!p.begin("pm", false)) return false;
    const bool saved = p.putUChar("panel_pwr", keep ? 1 : 0) == 1;
    p.end();
    if (!saved) return false;
    state.note4KeepPanelPower = keep;
    EPD_SSD2683_SetKeepPower(keep);
    return true;
}
#endif

static String fitText(const String &s, size_t maxChars) {
    String t;
    t.reserve(s.length());
    for (size_t i = 0; i < s.length(); i++) {
        char c = s[i];
        t += (c >= 32 && c <= 126) ? c : '?';
    }
    if (t.length() <= maxChars) return t;
    return t.substring(0, maxChars - 1) + "~";
}

// Status screens wrap long text (OTA file names) instead of truncating it;
// glyph tables are ASCII-only, so non-printable bytes become '?'.
static std::vector<String> wrapText(const String &s, size_t maxChars) {
    String t;
    t.reserve(s.length());
    for (size_t i = 0; i < s.length(); i++) {
        char c = s[i];
        t += (c >= 32 && c <= 126) ? c : '?';
    }
    std::vector<String> out;
    if (t.length() == 0) {
        out.push_back(String(""));
        return out;
    }
    for (size_t i = 0; i < t.length(); i += maxChars) {
        out.push_back(t.substring(i, i + maxChars));
    }
    return out;
}

static WinInfo findWindow(JsonDocument &doc, bool fiveHour) {
    WinInfo r;
    JsonArray buckets = doc["buckets"].as<JsonArray>();
    JsonObject codex;
    for (JsonObject b : buckets) {
        if (String((const char *)(b["id"] | "")) == "codex") {
            codex = b;
            break;
        }
    }
    if (codex.isNull()) return r;
    JsonArray wins = codex["windows"].as<JsonArray>();
    for (JsonObject w : wins) {
        int wm = w["windowMins"] | 0;
        bool match = fiveHour ? (wm == 300) : (wm >= 10080);
        if (!match) continue;
        r.found = true;
        r.used = w["usedPercent"] | -1;
        r.resets = w["resetsAt"] | 0LL;
        return r;
    }
    return r;
}

static void screenStatus() {
    std::vector<String> lines;
    lines.push_back("CODEX STATUS");
    lines.push_back(radio::deviceStateText());
    lines.push_back(String("BATT ") + power::batteryPercent() + "%");
    lines.push_back(String("IP ") + radio::ipText());
    lines.push_back(String("SYNC ") + clock::fmtEpoch((long long)radio::retained.rtcLastSyncEpoch, "%H:%M"));
    lines.push_back(String("FW ") + application::state.firmwareVersion);
    screen(lines);
}

void screen(const std::vector<String> &lines, UBYTE color) {
    if (!state.frame) return;
    Paint_SelectImage(state.frame);
    Paint_Clear(WHITE);
    Paint_DrawRectangle(0, 0, EPD_W - 1, EPD_H - 1, BLACK, DOT_PIXEL_1X1, DRAW_FILL_EMPTY);
    int y = 10;
    bool full = false;
    for (const auto &line : lines) {
        for (const auto &part : wrapText(line)) {
            if (y > EPD_H - 24) { full = true; break; }
            Paint_DrawString_EN(8, y, part.c_str(), &Font16, WHITE, color);
            y += 20;
        }
        if (full) break;
    }
    epdFlush();
}

void epdBegin(bool clearPanel) {
#if defined(CODEX_TARGET_NOTE4)
    // Note4's EPD rail is active HIGH; the 1.54-inch board is active LOW.
    pinMode(EPD_PWR_PIN, OUTPUT);
    digitalWrite(EPD_PWR_PIN, HIGH);
    delay(20);
#else
    pinMode(EPD_PWR_PIN, OUTPUT);
    digitalWrite(EPD_PWR_PIN, HIGH);
    delay(500);
    digitalWrite(EPD_PWR_PIN, LOW);
    delay(200);
#endif
    pinMode(42, OUTPUT);
    digitalWrite(42, LOW);
    pinMode(17, OUTPUT);
    digitalWrite(17, HIGH);
    power::retainSleepCriticalGpio();
    delay(20);
    DEV_Module_Init();
    epdFullLut = EPD_TGT_Init();
    state.epdBaselineTrusted = false;
    if (!epdFullLut) {
        retained.rtcEpdBusyFails++;
        DevLog.println("[epd] init busy timeout; baseline untrusted");
    }
    if (clearPanel && EPD_TGT_Clear(EPD_TGT_WHITE)) {
        state.epdBaselineTrusted = true;   // panel is white, lastDisplayedFrame is white
    } else if (clearPanel) {
        retained.rtcEpdBusyFails++;
        DevLog.println("[epd] clear busy timeout; baseline untrusted");
    }
    panelThinReady = true;
    // Allocate once per boot: epdBegin() may be re-entered (OOM retry / future
    // callers) and overwriting the pointers would leak both 5000-byte buffers.
    if (!state.frame) {
        state.frame = (UBYTE *)malloc(EPD_FB_BYTES);
        if (!state.frame) {
            DevLog.println("[epd] frame buffer malloc failed");
            return;
        }
    }
    if (!state.lastDisplayedFrame) {
        state.lastDisplayedFrame = (UBYTE *)malloc(EPD_FB_BYTES);
        if (state.lastDisplayedFrame) {
            memset(state.lastDisplayedFrame, 0xFF, EPD_FB_BYTES);
        } else {
            DevLog.println("[epd] last-display buffer malloc failed; writes will not be skipped");
        }
    }
    Paint_NewImage(state.frame, EPD_W, EPD_H, 0, WHITE);
    Paint_SetScale(2);
    Paint_SelectImage(state.frame);
    state.epdPartialReady = false;
    state.epdPartialCount = 0;
}

// Thin deep-sleep wake init: power the panel and bring up SPI only. The
// window calls (WakePartialWindow) reset the controller and load the partial
// LUT themselves, so the full init/clear and the framebuffer stay untouched.
void epdThinBegin() {
    if (panelThinReady) return;
#if defined(CODEX_TARGET_NOTE4)
    pinMode(EPD_PWR_PIN, OUTPUT);
    digitalWrite(EPD_PWR_PIN, HIGH);
    delay(20);
#else
    pinMode(EPD_PWR_PIN, OUTPUT);
    digitalWrite(EPD_PWR_PIN, HIGH);
    delay(10);
    digitalWrite(EPD_PWR_PIN, LOW);
    delay(100);
#endif
    pinMode(42, OUTPUT);
    digitalWrite(42, LOW);
    pinMode(17, OUTPUT);
    digitalWrite(17, HIGH);
    power::retainSleepCriticalGpio();
    DEV_Module_Init();
    panelThinReady = true;
    state.epdAsleep = false;
}

// Data screens default to partial refresh (~300ms, no flash). A full refresh
// (~1.5s, flashes) runs when the panel is not partial-ready, when the changed
// area exceeds 12.5% of the panel (layout/value jumps), or after 30 partials
// to clear ghosting. forceFull requests a full refresh explicitly.
// The panel sleeps (SSD1681 deep-sleep mode 1, RAM retained) after every
// refresh and is woken/re-initialized before the next draw.
void epdPanelSleep() {
    if (state.epdAsleep) return;
    EPD_TGT_Sleep();
    state.epdAsleep = true;
}

static void epdFlush(bool forceFull) {
    if (!state.frame) return;
    const bool blink = state.rfnBlink;
    state.rfnBlink = false;
    const uint32_t flushT0 = millis();

    RfnDecision d;
    if (state.rgnPolicyOn) {
        d = rgnDecide(state.rgnSet, state.lastDisplayedFrame, state.frame,
                      state.epdBaselineTrusted, forceFull, state.forceCleanRefresh);
        state.forceCleanRefresh = false;
        // Blink ticks must never flash a full waveform (task-10 B): the icon
        // toggles are transient, so an exhausted icon ghost budget is the only
        // full decision downgraded here. Any other full reason stays full.
        if (blink && d.action == RFN_FULL && d.reason == RFNR_BUDGET && state.epdPartialReady) {
            d.action = RFN_PARTIAL;
            d.reason = RFNR_OK;
        }
        state.rfnKind = rfnActionName(d.action);
        state.rfnReason = (blink && d.action == RFN_PARTIAL) ? "blink" : rfnReasonName(d.reason);
        state.rfnDirty = d.dirty;
        state.rfnDecisions++;
        if (d.action == RFN_NONE) return;
    } else {
        state.forceCleanRefresh = false;
        if (state.lastDisplayedFrame && memcmp(state.frame, state.lastDisplayedFrame, EPD_FB_BYTES) == 0) return;
        bool full = forceFull || !state.epdPartialReady || !state.epdBaselineTrusted;
        if (!full && state.lastDisplayedFrame) {
            int changed = 0;
            for (int i = 0; i < EPD_FB_BYTES; i++)
                changed += __builtin_popcount((unsigned char)(state.frame[i] ^ state.lastDisplayedFrame[i]));
            if (changed > EPD_W * EPD_H / 8) full = true;
        }
        d.action = full ? RFN_FULL : RFN_PARTIAL;
        state.rfnKind = rfnActionName(d.action);
        state.rfnReason = blink ? "blink" : "legacy";
        state.rfnDirty = 0;
    }

    if (!TARGET_PARTIAL && d.action == RFN_PARTIAL) {
        d.action = RFN_FULL;
        state.rfnKind = rfnActionName(RFN_FULL);
        state.rfnReason = "target_full_only";
    }
    bool partial = (d.action == RFN_PARTIAL) && state.epdPartialReady;
    if (partial && !blink && ++state.epdPartialCount > 30) partial = false;
    if (partial) {
        DirtyWindow window;
        bool ok = state.epdBaselineTrusted &&
                  rgnDirtyWindow(state.lastDisplayedFrame, state.frame, EPD_W, EPD_H, window);
        const size_t stride = ok ? (window.x1 - window.x0 + 1) / 8 : 0;
        const size_t bytes = ok ? stride * (window.y1 - window.y0 + 1) : 0;
        uint8_t *pixels = bytes ? (uint8_t *)malloc(bytes) : nullptr;
        ok = ok && pixels;
        if (ok) {
            for (int y = window.y0; y <= window.y1; ++y)
                memcpy(pixels + (y - window.y0) * stride,
                       state.lastDisplayedFrame + y * (EPD_W / 8) + window.x0 / 8, stride);
            ok = EPD_TGT_WakePartialWindow(window.x0, window.y0, window.x1, window.y1, pixels);
            if (ok) {
                for (int y = window.y0; y <= window.y1; ++y)
                    memcpy(pixels + (y - window.y0) * stride,
                           state.frame + y * (EPD_W / 8) + window.x0 / 8, stride);
                ok = EPD_TGT_DisplayPartWindow(window.x0, window.y0, window.x1, window.y1, pixels);
            }
        }
        free(pixels);
        if (ok) {
            state.epdWriteCount++;
            diagnostics::state.bootRenderCount++;
            if (blink) radio::state.blinkTicks++;
            if (state.rgnPolicyOn && !blink) rgnOnPartial(state.rgnSet);
            if (state.lastDisplayedFrame) memcpy(state.lastDisplayedFrame, state.frame, EPD_FB_BYTES);
            // Keep the RTC clock baseline in step with the panel: the window path
            // refuses to write while `clkPixels` disagrees with the driver shadow,
            // and only this re-capture re-arms it after a frame render.
            clkBaselineAfterPanelWrite(false, &window);
            epdPanelSleep();
            uint32_t elapsed = millis() - flushT0;
            state.rfnLastMs = (uint16_t)(elapsed > 0xFFFF ? 0xFFFF : elapsed);
            diagnostics::state.wakeRenderMs += elapsed;
            return;
        }
        // Unknown waveform state: never keep the stale software baseline, and
        // escalate to a full refresh so the panel converges (design §8.4).
        retained.rtcEpdBusyFails++;
        state.epdBaselineTrusted = false;
        if (state.rgnPolicyOn) rgnOnFull(state.rgnSet);
        DevLog.println("[epd] partial failed; escalating to full refresh");
    }
    if (state.rgnPolicyOn && d.action == RFN_FULL) rgnOnFull(state.rgnSet);

    state.epdPartialCount = 0;
    if (!epdFullLut || state.epdAsleep) {
        if (!EPD_TGT_Init()) {
            retained.rtcEpdBusyFails++;
            state.epdBaselineTrusted = false;
            DevLog.println("[epd] full init busy timeout");
        } else {
            epdFullLut = true;
        }
    }
    state.epdAsleep = false;
    bool ok = EPD_TGT_Display(state.frame);
    if (ok) {
        state.epdWriteCount++;
        diagnostics::state.bootRenderCount++;
        retained.rtcClkPartials = 0;   // full waveform clears the clock-window ghosting
        if (state.lastDisplayedFrame) memcpy(state.lastDisplayedFrame, state.frame, EPD_FB_BYTES);
        // The whole panel now matches `frame`, so the clock window baseline is
        // trustworthy again (this is what re-arms the partial clock path).
        clkBaselineAfterPanelWrite(true, nullptr);
        state.epdBaselineTrusted = true;
        state.epdPartialReady = TARGET_PARTIAL && EPD_TGT_Init_Partial();
        epdFullLut = false;
        if (TARGET_PARTIAL && !state.epdPartialReady) {
            retained.rtcEpdBusyFails++;
            DevLog.println("[epd] partial-mode init failed; next flush is full");
        }
    } else {
        retained.rtcEpdBusyFails++;
        state.epdBaselineTrusted = false;
        DevLog.println("[epd] full refresh busy timeout; baseline untrusted");
    }
    epdPanelSleep();
    uint32_t elapsed = millis() - flushT0;
    state.rfnLastMs = (uint16_t)(elapsed > 0xFFFF ? 0xFFFF : elapsed);
    diagnostics::state.wakeRenderMs += elapsed;
}

// The clock fast path reads the shared font registry (template_engine) instead
// of keeping its own copy of the font list; both the bitmap family (f8..f24)
// and the proportional family (nt16/nt30) are recognised.
static int clkFontId(const char *name) {
    return tplFontIndexByName(name);
}

// Reserve the clock cell from the active template. The bind prints exactly
// "HH:MM" (5 glyphs), so the max-width box is 5 x font cell at the element's
// scale/x/y. No `device.now` element -> no reservation (clkR.valid stays false).
void clkComputeRect() {
    retained.clkR.valid = false;
    if (!TARGET_PARTIAL) return;
    if (!platform::state.activeTplJson.length()) { DevLog.println("[clk] no active template"); return; }
    JsonDocument doc;
    if (deserializeJson(doc, platform::state.activeTplJson)) { DevLog.println("[clk] template parse failed"); return; }
    for (JsonObject e : doc["elements"].as<JsonArray>()) {
        if (strcmp(e["type"] | "", "text")) continue;
        if (strcmp(e["bind"] | "", "device.now")) continue;
        // The direct write blits exactly "HH:MM"; anything else (prefix,
        // suffix, extra text) would be erased, so those templates opt out.
        if (strlen(e["prefix"] | "") || strlen(e["suffix"] | "")) {
            DevLog.println("[clk] device.now has prefix/suffix; no reservation");
            return;
        }
        const int fid = clkFontId(e["font"] | "");
        if (fid < 0) { DevLog.println("[clk] clock font unknown"); return; }
        int scale = e["scale"] | 1;
        if (scale < 1) scale = 1;
        int x = e["x"] | 0, y = e["y"] | 0;
        int textW = 0, textH = 0;
        if (!tplFontClockBox(fid, scale, textW, textH)) {
            DevLog.println("[clk] clock font unknown");
            return;
        }
        JsonArray rect = e["rect"].as<JsonArray>();
        if (!rect.isNull() && rect.size() == 4) {
            int rx = rect[0] | 0, ry = rect[1] | 0, rw = rect[2] | 0;
            const char *align = e["align"] | "left";
            x = rx;
            if (!strcmp(align, "center")) x = rx + (rw - textW) / 2;
            else if (!strcmp(align, "right")) x = rx + rw - textW;
            y = ry + (rect[3].as<int>() - textH) / 2;
        }
        int w = textW;
        int h = textH;
        if (x < 0) x = 0;
        if (y < 0) y = 0;
        if (x + w > EPD_W) w = EPD_W - x;
        if (y + h > EPD_H) h = EPD_H - y;
        if (w <= 0 || h <= 0) { DevLog.println("[clk] clock outside panel"); return; }
        const int x0 = (x >> 3) << 3;
        int x1 = ((x + w - 1) >> 3) * 8 + 7;
        if (x1 > EPD_W - 1) x1 = EPD_W - 1;
        const int bw = (x1 >> 3) - (x0 >> 3) + 1;
        const int bytes = bw * h;
        if (bytes > CLK_MAX_BYTES) {
            DevLog.printf("[clk] region %d bytes over cap\n", bytes);
            return;
        }
        retained.clkR.valid = true;
        retained.clkR.fontId = (uint8_t)fid;
        retained.clkR.x0b = x0 >> 3;
        retained.clkR.x1b = x1 >> 3;
        retained.clkR.y0 = (uint16_t)y;
        retained.clkR.y1 = (uint16_t)(y + h - 1);
        retained.clkR.bw = (uint8_t)bw;
        retained.clkR.rows = (uint8_t)h;
        retained.clkR.scale = (uint8_t)scale;
        retained.clkR.xOff = (uint8_t)(x - x0);
        retained.clkR.yOff = 0;
        retained.clkPixelsValid = false;
        DevLog.printf("[clk] reserved x=%d..%d y=%d..%d box=%dx%d win=%dx%dB\n",
                      x, x + w - 1, y, y + h - 1, w, h, bw, bytes);
        return;
    }
    DevLog.println("[clk] template has no device.now; no reservation");
}

// Compiled-template variant: identical reservation rules without parsing the
// template JSON at activation time (v2 §8).
void clkComputeRectCt() {
    const ClkRegion previous = retained.clkR;
    const bool previousPixelsValid = retained.clkPixelsValid;
    retained.clkR.valid = false;
    retained.clkPixelsValid = false;
    if (!TARGET_PARTIAL) return;
    if (!platform::state.v2CtValid) return;
    for (uint8_t i = 0; i < platform::state.v2Ct.opCount; i++) {
        const CtOp &op = platform::state.v2Ct.ops[i];
        if (op.type != CT_TEXT) continue;
        if (op.bindIdx == CT_NONE_IDX) continue;
        if (strcmp(platform::state.v2Ct.reqs[op.bindIdx].path, "device.now") != 0) continue;
        if (strlen(op.prefix) || strlen(op.suffix)) {
            DevLog.println("[clk] device.now has prefix/suffix; no reservation");
            return;
        }
        int scale = op.scale ? op.scale : 1;
        int x = op.x, y = op.y;
        int textW = 0, textH = 0;
        if (!tplFontClockBox(op.font, scale, textW, textH)) {
            DevLog.println("[clk] clock font unknown");
            return;
        }
        if (op.flags & 0x02) {
            x = op.x;
            if (op.align == 1) x = op.x + (op.w - textW) / 2;
            else if (op.align == 2) x = op.x + op.w - textW;
            y = op.y + (op.h - textH) / 2;
        }
        int w = textW;
        int h = textH;
        if (x < 0) x = 0;
        if (y < 0) y = 0;
        if (x + w > EPD_W) w = EPD_W - x;
        if (y + h > EPD_H) h = EPD_H - y;
        if (w <= 0 || h <= 0) { DevLog.println("[clk] clock outside panel"); return; }
        const int x0 = (x >> 3) << 3;
        int x1 = ((x + w - 1) >> 3) * 8 + 7;
        if (x1 > EPD_W - 1) x1 = EPD_W - 1;
        const int bw = (x1 >> 3) - (x0 >> 3) + 1;
        const int bytes = bw * h;
        if (bytes > CLK_MAX_BYTES) {
            DevLog.printf("[clk] region %d bytes over cap\n", bytes);
            return;
        }
        retained.clkR.valid = true;
        retained.clkR.fontId = op.font;
        retained.clkR.x0b = x0 >> 3;
        retained.clkR.x1b = x1 >> 3;
        retained.clkR.y0 = (uint16_t)y;
        retained.clkR.y1 = (uint16_t)(y + h - 1);
        retained.clkR.bw = (uint8_t)bw;
        retained.clkR.rows = (uint8_t)h;
        retained.clkR.scale = (uint8_t)scale;
        retained.clkR.xOff = (uint8_t)(x - x0);
        retained.clkR.yOff = 0;
        // A rendezvous boot reloads the same compiled template. Its RTC
        // window pixels still describe the panel unless the context or the
        // clock region changed; only then must the first wake redraw fully.
        retained.clkPixelsValid = previousPixelsValid && previous.valid &&
            platform::state.v2Profile.contextId[0] != '\0' &&
            strcmp(retained.rtcClkContextId, platform::state.v2Profile.contextId) == 0 &&
            previous.fontId == retained.clkR.fontId &&
            previous.x0b == retained.clkR.x0b && previous.x1b == retained.clkR.x1b &&
            previous.y0 == retained.clkR.y0 && previous.y1 == retained.clkR.y1 &&
            previous.bw == retained.clkR.bw && previous.rows == retained.clkR.rows &&
            previous.scale == retained.clkR.scale &&
            previous.xOff == retained.clkR.xOff && previous.yOff == retained.clkR.yOff;
        DevLog.printf("[clk] reserved x=%d..%d y=%d..%d box=%dx%d win=%dx%dB\n",
                      x, x + w - 1, y, y + h - 1, w, h, bw, bytes);
        return;
    }
    DevLog.println("[clk] template has no device.now; no reservation");
}

// Copy the clock window bytes off the framebuffer that is currently on screen.
void clkCaptureFromFramebuffer() {
    if (!retained.clkR.valid || !state.lastDisplayedFrame) return;
    const int stride = EPD_W / 8;
    for (int row = 0; row < retained.clkR.rows; row++) {
        memcpy(&clkPixels[row * retained.clkR.bw],
               &state.lastDisplayedFrame[(retained.clkR.y0 + row) * stride + retained.clkR.x0b],
               retained.clkR.bw);
    }
    retained.clkPixelsValid = true;
    if (platform::state.v2BundleReady) {
        strncpy(retained.rtcClkContextId, platform::state.v2Profile.contextId, sizeof(retained.rtcClkContextId) - 1);
        retained.rtcClkContextId[sizeof(retained.rtcClkContextId) - 1] = '\0';
    } else {
        retained.rtcClkContextId[0] = '\0';
    }
}

// Re-arm the RTC clock-window baseline after a successful panel write. A full
// refresh makes the whole frame trustworthy; a region partial only matters when
// it actually covered the clock window. Defined here (and forward-declared above
// epdFlush) because the clock state lives further down this file.
static void clkBaselineAfterPanelWrite(bool full, const DirtyWindow *win) {
    if (!retained.clkR.valid || !state.lastDisplayedFrame) return;
    if (!full) {
        if (!win) return;
        if (win->y1 < retained.clkR.y0 || win->y0 > retained.clkR.y1) return;
        if (win->x1 < retained.clkR.x0b * 8 || win->x0 > retained.clkR.x1b * 8 + 7) return;
    }
    clkCaptureFromFramebuffer();
}

#if defined(CODEX_TARGET_NOTE4)
static uint32_t note4FrameHash(const uint8_t *pixels) {
    uint32_t hash = 2166136261u;
    for (int i = 0; i < EPD_FB_BYTES; ++i)
        hash = (hash ^ pixels[i]) * 16777619u;
    return hash ? hash : 1;
}
#endif

// Persist the actual panel image once when entering deep sleep. Thin minute
// wakes update only the RTC clock window, avoiding a flash write every minute.
#if defined(CODEX_TARGET_NOTE4)
void note4SaveFrameBaseline() {
    if (!state.lastDisplayedFrame) return;  // thin clock wake: cached base is unchanged
    if (!state.epdBaselineTrusted) {
        rtcNote4FrameHash = 0;
        return;
    }
    const uint32_t hash = note4FrameHash(state.lastDisplayedFrame);
    if (hash == rtcNote4FrameHash) return;
    rtcNote4FrameHash = 0;
    if (!LittleFS.begin(false, "/littlefs", 10, "storage")) return;
    File file = LittleFS.open("/panel-base.tmp", "w");
    if (!file) return;
    const uint32_t header[2] = {0x344E5045u, hash};
    const bool written = file.write((const uint8_t *)header, sizeof(header)) == sizeof(header) &&
                         file.write(state.lastDisplayedFrame, EPD_FB_BYTES) == EPD_FB_BYTES;
    file.close();
    if (!written) { LittleFS.remove("/panel-base.tmp"); return; }
    LittleFS.remove("/panel-base.bin");
    if (LittleFS.rename("/panel-base.tmp", "/panel-base.bin"))
        rtcNote4FrameHash = header[1];
}
#endif

#if defined(CODEX_TARGET_NOTE4)
bool note4RestoreFrameBaseline(bool thin) {
    const uint32_t expectedHash = rtcNote4FrameHash;
    rtcNote4FrameHash = 0;
    if (!expectedHash || !LittleFS.begin(false, "/littlefs", 10, "storage"))
        return false;
    File file = LittleFS.open("/panel-base.bin", "r");
    if (!file || file.size() != EPD_FB_BYTES + 8) return false;
    uint32_t header[2] = {};
    uint8_t *pixels = thin ? (uint8_t *)malloc(EPD_FB_BYTES) : state.lastDisplayedFrame;
    if (!pixels) return false;
    const bool read = file.read((uint8_t *)header, sizeof(header)) == sizeof(header) &&
                      file.read(pixels, EPD_FB_BYTES) == EPD_FB_BYTES;
    file.close();
    bool valid = read && header[0] == 0x344E5045u &&
                 header[1] == expectedHash &&
                 note4FrameHash(pixels) == header[1];
    if (valid && retained.clkPixelsValid && retained.clkR.valid) {
        valid = retained.clkR.x0b <= retained.clkR.x1b && retained.clkR.x1b < EPD_W / 8 &&
                retained.clkR.y0 <= retained.clkR.y1 && retained.clkR.y1 < EPD_H &&
                retained.clkR.bw == retained.clkR.x1b - retained.clkR.x0b + 1 &&
                retained.clkR.rows == retained.clkR.y1 - retained.clkR.y0 + 1 &&
                (size_t)retained.clkR.bw * retained.clkR.rows <= sizeof(clkPixels);
    }
    if (valid && retained.clkPixelsValid && retained.clkR.valid) {
        const int stride = EPD_W / 8;
        for (int row = 0; row < retained.clkR.rows; ++row)
            memcpy(pixels + (retained.clkR.y0 + row) * stride + retained.clkR.x0b,
                   clkPixels + row * retained.clkR.bw, retained.clkR.bw);
    }
    if (valid) {
        rtcNote4FrameHash = expectedHash;
        EPD_SSD2683_RestoreShadow(pixels);
        if (!thin) {
            state.epdBaselineTrusted = true;
            state.epdPartialReady = EPD_TGT_Init_Partial();
            epdFullLut = false;
        }
    }
    if (thin) free(pixels);
    DevLog.printf("[epd] baseline cache %s\n", valid ? "restored" : "invalid");
    return valid;
}
#endif

static bool clkBlitString(uint8_t *buf, const char *s) {
    if (!tplFontDrawClock(buf, retained.clkR.bw, retained.clkR.rows, retained.clkR.xOff, retained.clkR.fontId, s, retained.clkR.scale)) {
        DevLog.println("[clk] clock font unavailable at blit");
        return false;
    }
    return true;
}

// Clock tick: rebuild only the reserved window and push it to the panel.
// Used by the light-mode minute tick and by thin deep-sleep wakes; needs the
// panel to be powered/SPI-initialized (epdBegin or epdThinBegin).
bool clockTickWake() {
    if (!retained.clkR.valid || !panelThinReady) return false;
    if (!clock::timeKnown()) return false;
    const uint64_t t0 = esp_timer_get_time();
    const size_t n = (size_t)retained.clkR.bw * retained.clkR.rows;
    static uint8_t buf[CLK_MAX_BYTES];
    memset(buf, 0xFF, n);
    time_t nowSec = time(nullptr);
    struct tm *lt = localtime(&nowSec);
    char s[8] = "--:--";
    if (lt) strftime(s, sizeof(s), "%H:%M", lt);
    if (!clkBlitString(buf, s)) return false;
    // The reserved window must actually hold the string. A blit that had to drop
    // ink (or a string wider than the window) would leave the previous frame's
    // pixels in the clipped tail, so escalate to a full frame instead of writing
    // a half-updated clock window.
    const int clipped = tplFontClockClipped();
    const int avail = retained.clkR.bw * 8 - retained.clkR.xOff;
    const int need = tplFontPropWidth(retained.clkR.fontId, retained.clkR.scale, s);
    if (clipped > 0 || (need > 0 && need > avail)) {
        retained.rtcEpdBusyFails++;
        retained.rtcClkPartials = CLK_GHOST_LIMIT;
        retained.clkPixelsValid = false;
        state.epdPartialReady = false;
        epdFullLut = false;
        state.epdAsleep = false;
        DevLog.printf("[clk] window %dx%d too narrow for \"%s\" (%d>%d clipped=%d); full refresh required\n",
                      retained.clkR.bw * 8, retained.clkR.rows, s, need, avail, clipped);
        return false;
    }
    const int x0 = retained.clkR.x0b * 8, x1 = retained.clkR.x1b * 8 + 7;
    const uint64_t t1 = esp_timer_get_time();
    bool ok = EPD_TGT_WakePartialWindow(x0, retained.clkR.y0, x1, retained.clkR.y1,
                                            retained.clkPixelsValid ? clkPixels : nullptr);
    const uint64_t t2 = esp_timer_get_time();
    if (ok) ok = EPD_TGT_DisplayPartWindow(x0, retained.clkR.y0, x1, retained.clkR.y1, buf);
    const uint64_t t3 = esp_timer_get_time();
    if (!ok) {
        // The window waveform state is unknown: keep the previous clock pixels
        // and schedule a full rebuild at the next opportunity.
        retained.rtcEpdBusyFails++;
        retained.rtcClkPartials = CLK_GHOST_LIMIT;
        retained.clkPixelsValid = false;
        state.epdPartialReady = false;
        epdFullLut = false;
        state.epdAsleep = false;
        DevLog.println("[clk] window write failed; full refresh required");
        return false;
    }
    memcpy(clkPixels, buf, n);
    retained.clkPixelsValid = true;
    state.epdAsleep = false;
    epdPanelSleep();
    diagnostics::state.bootRenderCount++;
    retained.rtcClockTicks++;
    retained.rtcClkPartials++;
    if (retained.rtcClkPartials > CLK_GHOST_LIMIT) retained.rtcClkPartials = CLK_GHOST_LIMIT;
    state.epdPartialCount++;   // window writes count toward the light-mode ghost reset
    const uint64_t t4 = esp_timer_get_time();
    diagnostics::state.wakeRenderMs += (uint32_t)((t4 - t0) / 1000);
    DevLog.printf("[clk] tick build=%uus wake=%uus write=%uus zzz=%uus total=%uus\n",
                  (unsigned)(t1 - t0), (unsigned)(t2 - t1), (unsigned)(t3 - t2),
                  (unsigned)(t4 - t3), (unsigned)(t4 - t0));
    return true;
}

#ifdef CODEX_CLK_WINDOW_TEST
void clkTestTick() {
    static uint32_t ticks = 0;   // survives light sleep (RAM kept)
    if (!retained.clkR.valid) return;
    ticks++;
    if (ticks > CLK_TEST_TICKS) {
        if (ticks == CLK_TEST_TICKS + 1) {
            DevLog.printf("[clk] A/B test done (%u ticks)\n", (unsigned)CLK_TEST_TICKS);
        }
        renderCurrent();
        return;
    }
    const uint64_t t0 = esp_timer_get_time();
    if (!retained.clkPixelsValid) clkCaptureFromFramebuffer();
    if (ticks % 2 == 0) {
        clockTickWake();
    } else {
        renderCurrent();
        clkCaptureFromFramebuffer();
        DevLog.printf("[clk] A total=%uus\n", (unsigned)(esp_timer_get_time() - t0));
    }
}
#endif

static int bigTextWidth(const String &s, int scale, int gap) {
    int n = (int)s.length();
    if (!n) return 0;
    return n * Font24.Width * scale + (n - 1) * gap;
}

static void drawBigChar(char c, int x, int y, int scale) {
    int bytesPerRow = (Font24.Width + 7) / 8;
    const UBYTE *p = &Font24.table[(c - ' ') * Font24.Height * bytesPerRow];
    for (int row = 0; row < Font24.Height; row++) {
        for (int col = 0; col < Font24.Width; col++) {
            if (p[row * bytesPerRow + (col >> 3)] & (0x80 >> (col & 7))) {
                Paint_DrawRectangle(x + col * scale, y + row * scale,
                                    x + col * scale + scale - 1, y + row * scale + scale - 1,
                                    WHITE, DOT_PIXEL_1X1, DRAW_FILL_FULL);
            }
        }
    }
}

static void drawBigCentered(const String &s, int bx, int by, int bw, int bh) {
    int scale = 2, gap = 4;
    if (bigTextWidth(s, scale, gap) > bw - 8 || Font24.Height * scale > bh - 6) {
        scale = 1;
        gap = 3;
    }
    int w = bigTextWidth(s, scale, gap);
    int x = bx + (bw - w) / 2;
    int y = by + (bh - Font24.Height * scale) / 2;
    for (int i = 0; i < (int)s.length(); i++) {
        drawBigChar(s[i], x, y, scale);
        x += Font24.Width * scale + gap;
    }
}

static void drawHeroBlock(int x, int y, const String &value) {
    Paint_DrawRectangle(x, y, x + Q_BLK_W - 1, y + Q_BLK_H - 1, BLACK, DOT_PIXEL_1X1, DRAW_FILL_FULL);
    drawBigCentered(value, x, y, Q_BLK_W, Q_BLK_H);
}

static void drawSmallRight(const String &s, int right, int y) {
    int w = (int)s.length() * Font12.Width;
    Paint_DrawString_EN(right - w, y, s.c_str(), &Font12, WHITE, BLACK);
}

static void drawSmallLeft(const String &s, int x, int y) {
    Paint_DrawString_EN(x, y, s.c_str(), &Font12, WHITE, BLACK);
}

static void renderUsage(const String &json, const char *channel) {
    if (!state.frame) return;
    JsonDocument doc;
    if (deserializeJson(doc, json)) {
        DevLog.println("[usage] parse failed");
        return;
    }
    const char *plan  = doc["account"]["plan"] | "?";
    const char *label = doc["bridge"]["label"] | "?";
    int rc = doc["resetCredits"]["availableCount"] | -1;
    WinInfo fh = findWindow(doc, true);
    WinInfo wk = findWindow(doc, false);

    Paint_SelectImage(state.frame);
    Paint_Clear(WHITE);

    drawHeroBlock(Q_TL_X, Q_TL_Y, (wk.found && wk.used >= 0) ? String(100 - wk.used) : String("--"));
    if (fh.found) drawHeroBlock(Q_BR_X, Q_BR_Y, (fh.used >= 0) ? String(100 - fh.used) : String("--"));
    else          drawHeroBlock(Q_BR_X, Q_BR_Y, String("100"));

    String rcLine = (rc >= 0) ? String("RC ") + rc : String("RC --");
    std::vector<String> tr, bl;
    int battery = power::batteryPercent();

    tr.push_back(fitText(String(plan), 12));
    tr.push_back(fitText(String(label), 12));
    tr.push_back(clock::fmtEpoch(wk.resets, "%m-%d %H:%M"));
    tr.push_back(rcLine);
    if (fh.found) bl.push_back(String("5H ") + clock::fmtEpoch(fh.resets, "%H:%M"));
    bl.push_back(String("BATT ") + battery + "%");
    bl.push_back(String("SYNC ") + clock::nowHHMM());

    for (int i = 0; i < (int)tr.size(); i++)
        drawSmallRight(tr[i], Q_TR_RIGHT, Q_TR_Y0 + i * Q_LINE_H);
    int first = 4 - (int)bl.size();
    for (int i = 0; i < (int)bl.size(); i++)
        drawSmallLeft(bl[i], Q_BL_X, Q_BL_SLOTS[first + i]);

    epdFlush(false);
    DevLog.printf("[ui] quad rendered (wk=%d fh=%d ch=%s)\n",
                  wk.used, fh.used, channel ? channel : "");
}

void renderActiveUsage(const String &json, const char *channel) {
    if (!state.frame) return;
    bool ready = platform::state.v2BundleReady ? platform::v2ActiveLoad() : platform::tplCacheLoad();
    if (!ready) { screenStatus(); return; }
    TplEnv env;
    env.channel  = channel ? channel : "";
    env.ip       = radio::ipText();
    env.syncHHMM = clock::nowHHMM();
    env.battery  = power::batteryPercent();
    env.state    = (power::retained.rtcMode == MODE_DEEP) ? "DEEP" : radio::templateStateText();
    env.mode     = (power::retained.rtcMode == MODE_DEEP) ? "deep" : "light";
    // A wake has no Wi-Fi verdict yet. Keep the Bridge glyph from the sleep
    // frame until the connection attempt finishes; deep clock wakes also keep it.
    if (radio::state.wifiConnActive || (power::retained.rtcMode == MODE_DEEP && !radio::state.wifiUp && !radio::state.wifiLostHandled)) {
        env.offlineMins = (diagnostics::retained.rtcDeepGlyph & 8) ? 0 : -1;
    } else {
        env.offlineMins = radio::state.wifiUp ? -1 : 0;
        if (radio::state.wifiUp && radio::retained.rtcLastSyncEpoch > 1600000000 && clock::timeKnown()) {
            long mins = ((long)time(nullptr) - (long)radio::retained.rtcLastSyncEpoch) / 60;
            // The row means "bridge unreachable": the bridge pushes at least every
            // 5 minutes, so only expose the value once contact is clearly lost.
            if (mins > BRIDGE_LOST_MIN) env.offlineMins = (int)mins;
        }
        if (env.offlineMins >= 0) diagnostics::retained.rtcDeepGlyph |= 8;
        else diagnostics::retained.rtcDeepGlyph &= ~8;
    }
    Paint_SelectImage(state.frame);
    Paint_Clear(WHITE);
    bool drawn = platform::state.v2CtValid ? tplDrawCt(platform::state.v2Ct, json, env)
                           : tplDraw(platform::state.activeTplJson, json, env);
    if (drawn) {
        epdFlush(false);
        if (state.epdBaselineTrusted) clkCaptureFromFramebuffer();
        else retained.clkPixelsValid = false;
        DevLog.printf("[tpl] rendered %s (%s)\n", platform::state.activeTplId.c_str(), channel ? channel : "");
        return;
    }
    DevLog.printf("[tpl] %s invalid/incomplete, built-in fallback\n", platform::state.activeTplId.c_str());
    if (json.length()) renderUsage(json, channel);
    else screenStatus();
}

void renderCurrent() {
    if (platform::state.lastUsage.length()) renderActiveUsage(platform::state.lastUsage, platform::state.lastChannel.c_str());
    else screenStatus();
    state.networkGlyph.observe(radio::state.wifiUp);
    const bool confirmed = state.epdBaselineTrusted && state.lastDisplayedFrame && state.frame &&
        memcmp(state.lastDisplayedFrame, state.frame, EPD_FB_BYTES) == 0;
    state.networkGlyph.rendered(confirmed, clock::v2NowMs());
}

void serviceNetworkGlyph() {
    state.networkGlyph.observe(radio::state.wifiUp);
    if (!state.networkGlyph.due(clock::v2NowMs()) || radio::state.wifiConnActive || transport::state.otaInProgress) return;
    if (state.networkGlyph.attempts) state.forceCleanRefresh = true;
    renderCurrent();
    DevLog.printf("[ui] network glyph connected=%d confirmed=%d attempts=%u\n",
                  radio::state.wifiUp, !state.networkGlyph.pending, state.networkGlyph.attempts);
}

// Put the sleep frame back without entering deep: used when a wake render
// already removed Zzz but the link never came up, so the panel does not sit on
// a stale light frame. Full baseline for the sleep glyph (see enterDeep/0.14.13).
static void renderSleepGlyph() {
    uint8_t saved = power::retained.rtcMode;
    power::retained.rtcMode = MODE_DEEP;
    state.epdPartialReady = false;
    renderCurrent();
    power::retained.rtcMode = saved;
}

// Plan C: the single wake render, after the BLE window is closed. The reserved
// clock window is written directly (same partial path as thin deep wakes) so
// the sleep frame stays untouched; a full frame is only the failure fallback.
// No clock in the template (or unknown time) -> nothing to draw.
static void v2RendezvousClockRender() {
    if (!retained.clkR.valid || !platform::state.activeTplHasNow || !clock::timeKnown()) {
        DevLog.printf("[platform] rendezvous render skipped (clk=%d now=%d time=%d)\n",
                      retained.clkR.valid ? 1 : 0, platform::state.activeTplHasNow ? 1 : 0, clock::timeKnown() ? 1 : 0);
        return;
    }
    if (!panelThinReady) epdThinBegin();
    const uint32_t t0 = millis();
    // First wake after a cold boot/OTA: the software old-window is unknown, so
    // seed it from the framebuffer (full baseline) instead of a window write.
    if (!retained.clkPixelsValid) {
        state.forceCleanRefresh = true;
        renderCurrent();
        clkCaptureFromFramebuffer();
        DevLog.printf("[platform] rendezvous full frame (no clock baseline) in %ums\n",
                      (unsigned)(millis() - t0));
        return;
    }
    if (!clockTickWake()) {
        DevLog.println("[platform] rendezvous clock window failed; full frame");
        state.forceCleanRefresh = true;
        renderCurrent();
        clkCaptureFromFramebuffer();
        return;
    }
    DevLog.printf("[platform] rendezvous clock rendered in %ums (src=%s)\n",
                  (unsigned)(millis() - t0), clock::timeSourceName(clock::state.timeSource));
}

// Plan C: the one render after the window. A BLE snapshot applied during the
// window is drawn as a full frame (data + clock together); a light plan keeps
// its clock for the light first frame; a sleep plan otherwise writes only the
// reserved clock window.
void v2RendezvousRender(bool light) {
    if (platform::state.v2WakeRenderPending) {
        const uint32_t t0 = millis();
        const uint32_t busyBefore = retained.rtcEpdBusyFails;
        const uint32_t writesBefore = state.epdWriteCount;
        // Light: this frame is the Zzz-removing wake baseline, so
        // startNormalMode must not force a second full refresh (task-10).
        if (light) state.forceCleanRefresh = true;
        renderActiveUsage(platform::state.lastUsage, platform::state.lastChannel.c_str());
        if (light) state.wakeBaselineDrawn = true;
        platform::state.v2WakeRenderPending = false;
        clkCaptureFromFramebuffer();
        platform::state.v2DisplayState = displayStateAfterRender(writesBefore, state.epdWriteCount, busyBefore, retained.rtcEpdBusyFails);
        DevLog.printf("[platform] rendezvous %s frame (data+clock) in %ums\n",
                      light ? "light" : "sleep", (unsigned)(millis() - t0));
        return;
    }
    if (light) {
        // The clock rides in the light first frame (startNormalMode).
        DevLog.println("[platform] rendezvous light plan; clock in the first light frame");
        return;
    }
    v2RendezvousClockRender();
}

} // namespace firmware::display
