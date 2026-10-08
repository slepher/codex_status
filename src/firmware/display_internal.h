#pragma once

// Firmware integration only: state is owned here; never copy it as a snapshot.
#include "config.h"
#include "dependencies.h"
#include "network_display_state.h"

namespace firmware::display {

struct ClkRegion {
    bool     valid;
    uint8_t  fontId;
    uint8_t  x0b, x1b;    // byte columns of the window
    uint16_t y0, y1;      // pixel rows of the window
    uint8_t  bw, rows;    // bytes per row, rows
    uint8_t  scale;
    uint8_t  xOff, yOff;  // element x/y inside the window
};

struct State {
    NetworkDisplayState networkGlyph;

    // NVS `pm/panel_pwr`: 1 keeps the Note4 logic rail on in deep sleep; 0
    // powers it off. Both modes turn the controller's internal HV supply off.
#if defined(CODEX_TARGET_NOTE4)
    bool note4KeepPanelPower = true;
#endif
    UBYTE      *frame = nullptr;
    UBYTE      *lastDisplayedFrame = nullptr;
    uint32_t    epdWriteCount = 0;
    bool     rfnBlink = false;
    int      epdPartialCount = 0;
    bool     epdPartialReady = false;
    bool     epdAsleep = false;
    bool     epdBaselineTrusted = true; // software old-frame matches the panel

    // Display safety layer (design §8): semantic regions from the active template,
    // conservative escalation, ghost budgets. `rgnPolicyOn=false` restores the
    // legacy changed-pixels rule at runtime (token-gated /diag?policy=off).
    RgnSet   rgnSet;
    bool     rgnPolicyOn = true;
    uint32_t rfnDecisions = 0;
    String   rfnKind = "init";
    String   rfnReason = "init";
    uint16_t rfnDirty = 0;
    bool     forceCleanRefresh = false;
    uint16_t rfnLastMs = 0;       // duration of the last epdFlush waveform

    // Avoid drawing the wake baseline twice after a rendezvous frame.
    bool     wakeBaselineDrawn = false;
};
extern State state;

struct Retained {
    uint32_t rtcClockTicks = 0;    // clock window writes
    uint16_t rtcClkPartials = 0;   // clock window partials

    // Panel BUSY timeouts (RTC: survives deep sleep, cleared on power-on). A
    // non-zero count means at least one waveform had an unknown state; the
    // baseline is then untrusted and the next required display is a full refresh.
    uint32_t rtcEpdBusyFails = 0;
    ClkRegion clkR = {};
    bool     clkPixelsValid = false;
    char     rtcClkContextId[BS_CTX_LEN] = {};
};
extern Retained retained;

#if defined(CODEX_TARGET_NOTE4)
bool setNote4PanelPower(bool keep);
#endif
void screen(const std::vector<String> &lines, UBYTE color = BLACK);
void epdBegin(bool clearPanel = true);
void epdThinBegin();
void epdPanelSleep();
void clkComputeRect();
void clkComputeRectCt();
void clkCaptureFromFramebuffer();
#if defined(CODEX_TARGET_NOTE4)
void note4SaveFrameBaseline();
#endif
#if defined(CODEX_TARGET_NOTE4)
bool note4RestoreFrameBaseline(bool thin);
#endif
bool clockTickWake();
#ifdef CODEX_CLK_WINDOW_TEST
void clkTestTick();
#endif
void renderActiveUsage(const String &json, const char *channel);
void renderCurrent();
void serviceNetworkGlyph();
void v2RendezvousRender(bool light);

} // namespace firmware::display
