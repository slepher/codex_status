#pragma once
#include <Arduino.h>

struct TplEnv {
    String channel;   // "WIFI" / "BLE"
    String ip;
    String syncHHMM;  // "--:--" when unknown
    int battery = -1; // percentage, or -1 when the device value is unknown
    String state;                // AP / BLE ON / BLE OFF / WIFI OFF
    int offlineMins = -1;        // minutes since the last successful sync
    String mode;                 // "deep" / "light" (v0.14 device.mode)
    bool hasNowEpoch = false;    // host simulation supplies its experiment clock
    long long nowEpochSecs = 0;
};

// Draw a template onto the current Paint image (caller has cleared the frame).
// Returns false if the template is invalid/unsupported (nothing is drawn in
// that case: validation runs before the draw pass).
// NOTE: this path compiles first and is only used by legacy/one-shot callers;
// the runtime uses tplDrawCt with the persisted compiled template.
bool tplDraw(const String &tmplJson, const String &usageJson, const TplEnv &env);

// Structural validation only (no usage needed): used at BLE receive time.
bool tplValidate(const String &tmplJson, String &err);

// ---------------------------------------------------------------------------
// Font registry: the single source of truth for every font the engine renders.
//
// `CtOp.font` is an index into this one table, which holds both families:
//   [0, tplFontFixedCount())  bitmap family compiled into the firmware (f8..f24)
//   [tplFontFixedCount(), …)  proportional large-display family (nt16/nt30)
// Template validation, the refresh policy's region derivation, the clock fast
// path and the font-slot resolver all read this table; no other file keeps its
// own copy of the font list or its index mapping.
// ---------------------------------------------------------------------------
int tplFontCount();
int tplFontFixedCount();
int tplFontIndexByName(const char *name);
const char *tplFontNameByIndex(int idx);

// Conservative text cell for one font: the bitmap family reports its uniform
// cell, the proportional family the line height and the widest advance.
// Used by the refresh policy (region derivation) so every font family gets a
// bounded box instead of falling back to a whole-frame refresh. False = unknown
// font.
bool tplFontCellByName(const char *name, int &cellW, int &cellH);
bool tplFontCellByIndex(int idx, int &cellW, int &cellH);

// Clock fast path (main.cpp): box of the clock string "HH:MM" for a registry
// font. `w` is the widest possible box (widest digit advance) so the reserved
// window always fits whatever the clock shows, `h` the scaled line height.
// False = unknown font.
bool tplFontClockBox(int idx, int scale, int &w, int &h);

// Blit the clock string into a byte-aligned 1bpp window buffer (ink only, the
// window already holds the background) at `xOff` inside the window, using the
// same pixel writes as the full render path for both font families.
// False = unknown font / bad arguments.
bool tplFontDrawClock(uint8_t *win, int winBytesPerRow, int winRows, int xOff,
                      int idx, const char *text, int scale);

// Advance width (px) of `text` in the proportional family; 0 for bitmap fonts or
// unknown indexes. Used to check that a reserved clock window really fits the
// string before a partial write clips it.
int tplFontPropWidth(int idx, int scale, const char *text);

// Ink pixels the LAST tplFontDrawClock() blit had to drop because they fell
// outside the window. Non-zero means the reserved window is too narrow for the
// string, so that partial write must not be trusted. Reading it clears it.
int tplFontClockClipped();

// Render-target canvas (v2 §5): the engine is target-parameterized so one
// binary family serves 200x200 and 400x300 panels. Called at boot and by the
// host harness; defaults to TARGET_WIDTH x TARGET_HEIGHT.
void tplSetCanvas(int w, int h);

// ---------------------------------------------------------------------------
// CompiledTemplate (v2 §8): bounded, pointer-free render plan + field
// requirements. Template JSON is parsed exactly once (save/install); all
// runtime wake/data/switch/render paths address ops and requirement indices.
// ---------------------------------------------------------------------------

#define CT_ABI          2
#define CT_MAX_OPS      64
#define CT_MAX_REQS     32
#define CT_MAX_RES      16
#define CT_BIND_MAX     64
#define CT_TEXT_MAX     48
#define CT_STR_MAX      12
#define CT_B64_MAX      64
#define CT_NONE_IDX     0xFF

enum CtOpType { CT_TEXT = 0, CT_BAR = 1, CT_RECT = 2, CT_LINE = 3, CT_ICON = 4 };

// Condition modes (bit-stable for persistence).
enum CtWhenMode { CTW_NONE = 0, CTW_EXISTS_TRUE = 1, CTW_EXISTS_FALSE = 2, CTW_EQ_NUM = 3, CTW_EQ_STR = 4 };

#pragma pack(push, 1)

// One field requirement: enough to resolve a bind without parsing its path.
struct CtReq {
    uint8_t kind;          // BindKind from template_engine.cpp
    uint8_t winMode;       // 0 weekly, 1 5h, 2 index, 3 monthly
    int16_t winIndex;
    char    bucket[16];
    char    path[CT_BIND_MAX];
};

// One render operation: fixed width, no pointers.
struct CtOp {
    uint8_t  type;
    uint8_t  flags;        // bit0 fill, bit1 region, bit2 align, bit3 timefmt, bit4 digit_x
    uint8_t  font;         // index into CT_FONTS (f8,f12,f16,f20,f24,nt16,nt30)
    uint8_t  scale;
    int16_t  x, y, w, h;   // primary rect / text origin
    int16_t  x2, y2;       // line end; for digit_x: two/three-digit origins
    int16_t  whenEqNum;    // numeric `equals`
    int16_t  maxVal;       // bar max (default 100)
    uint8_t  bindIdx;      // CT_NONE_IDX or requirement index
    uint8_t  whenIdx;      // CT_NONE_IDX or requirement index
    uint8_t  whenMode;     // CtWhenMode
    uint8_t  align;        // 0 left, 1 center, 2 right
    uint8_t  timeFormat;   // 0 date, 1 hhmm
    uint8_t  resourceIdx;  // icon resource; for digit_x text: numeric bind index
    uint8_t  color;        // 0 black, 1 white
    uint8_t  bg;           // 0xFF none, else 0/1
    uint8_t  fg;           // bar fg
    uint8_t  border;       // 1 when the bar draws a border
    char     text[CT_TEXT_MAX];
    char     prefix[CT_STR_MAX];
    char     suffix[CT_STR_MAX];
    char     whenEqStr[16];
};

struct CtResource {
    uint8_t w, h;
    char    bits[CT_B64_MAX];
};

struct CtTemplate {
    uint8_t  abi;
    uint8_t  opCount;
    uint8_t  reqCount;
    uint8_t  resCount;
    uint32_t sourceCrc;
    char     id[17];
    CtOp       ops[CT_MAX_OPS];
    CtReq      reqs[CT_MAX_REQS];
    CtResource res[CT_MAX_RES];
};

#pragma pack(pop)

// Parse + validate template JSON into a compiled template (one-time, cold path).
bool tplCompile(const String &tmplJson, CtTemplate &out, String &err);

// Structural revalidation of a compiled record (persisted or received).
bool tplValidateCt(const CtTemplate &ct, String &err);

// Render a compiled template (no template JSON parsing).
bool tplDrawCt(const CtTemplate &ct, const String &usageJson, const TplEnv &env);

// Fixed-layout serialization for littlefs (no pointers, ABI checked on load).
size_t tplCtSize();
bool tplCtSerialize(const CtTemplate &ct, uint8_t *out, size_t cap, size_t &written);
bool tplCtDeserialize(const uint8_t *in, size_t len, CtTemplate &out, String &err);
