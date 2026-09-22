// Display refresh safety policy (design §8): semantic regions derived from the
// active template, per-region ink statistics, and a conservative partial/full
// escalation decision. No controller or painting dependencies; the caller owns
// the framebuffers.
#pragma once
#include <Arduino.h>

struct DirtyWindow { uint16_t x0, y0, x1, y1; };
// Inclusive coordinates, one-pixel padding, X rounded out to whole bytes.
bool rgnDirtyWindow(const uint8_t *oldFrame, const uint8_t *newFrame,
                    uint16_t width, uint16_t height, DirtyWindow &out);

// Conservative first release: high-ink regions always take the full waveform
// until the fixed-rig photo gate passes (design §8.2). Low-ink regions may use
// the partial waveform while their own budget allows.
// quad v11 declares ~29 elements; regions are merged only after all elements
// are added, so the cap must cover the pre-merge count (overflow falls back to
// whole-frame full refreshes).
#define RGN_MAX            32
#define RGN_STR_MAX        24

enum RgnClass : uint8_t {
    RGN_LINE = 0,
    RGN_ICON = 1,
    RGN_TEXT = 2,
    RGN_BAR = 3,
    RGN_CLOCK = 4,
    RGN_USAGE_DIGIT = 5,
    RGN_INVERTED = 6,
    RGN_SOLID = 7,
};

struct Rgn {
    uint8_t  cls = RGN_TEXT;
    bool     highInk = false;   // black background / filled black rect
    // Original semantic pixel rect (inclusive), used for statistics and
    // merging. Byte expansion is a window-write concern, not a classification
    // one: merging on expanded bytes glued neighboring icons into a black
    // tile and forced unrelated full refreshes.
    uint16_t px0 = 0, py0 = 0, px1 = 0, py1 = 0;
    uint8_t  x0b = 0, x1b = 0;  // byte columns (inclusive) for window writes
    uint16_t area = 0;          // pixel area of the semantic rect
    // Last computed statistics (valid after rgnDecide).
    uint16_t changed = 0, w2b = 0, b2w = 0, bOld = 0, bNew = 0;
    // Ghost budget (partials allowed before a full refresh, and cumulative
    // black/white transition ratio x1000 per design §8.2).
    uint8_t  budget = 10;
    uint16_t partials = 0;
    uint16_t cumS = 0;
};

struct RgnSet {
    uint8_t n = 0;
    bool wholeFrame = true;   // derivation failed -> conservative whole frame
    Rgn r[RGN_MAX];
};

enum RfnAction : uint8_t { RFN_NONE = 0, RFN_PARTIAL = 1, RFN_FULL = 2 };
enum RfnReason : uint8_t {
    RFNR_NONE = 0, RFNR_OK, RFNR_CLEAN, RFNR_FORCE, RFNR_TRUST, RFNR_DERIVE,
    RFNR_AREA, RFNR_POLARITY, RFNR_HIGH_INK, RFNR_BUDGET,
};

struct RfnDecision {
    uint8_t  action = RFN_FULL;
    uint8_t  reason = RFNR_TRUST;
    uint8_t  region = 0xFF;
    uint16_t changed = 0;
    uint16_t dirty = 0;     // changed pixels inside semantic regions
    uint16_t outside = 0;   // changed pixels not covered by any region
};

#include "template_engine.h"

// Panel geometry (target property); called at boot / by the host harness.
void rgnSetPanel(int w, int h);

// Parse the active template and derive semantic regions. Always fills `out`;
// on parse/layout failure sets wholeFrame (decision then escalates to full).
bool rgnBuild(const String &tmplJson, RgnSet &out);

// Same derivation from the compiled template: the runtime path on activation,
// with no template JSON parsing (v2 §8).
bool rgnBuildCt(const CtTemplate &ct, RgnSet &out);
void rgnReset(RgnSet &out);

// Evaluate old/new framebuffers against the policy, filling per-region stats.
// Does not mutate budgets; call rgnOnPartial/rgnOnFull after the waveform.
RfnDecision rgnDecide(RgnSet &set, const uint8_t *oldFb, const uint8_t *newFb,
                      bool trusted, bool forceFull, bool clean);

// Budget accounting after the waveform actually succeeded.
void rgnOnPartial(RgnSet &set);
void rgnOnFull(RgnSet &set);

const char *rgnClassName(uint8_t cls);
const char *rfnActionName(uint8_t action);
const char *rfnReasonName(uint8_t reason);
