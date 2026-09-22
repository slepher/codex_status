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
};

// Draw a template onto the current Paint image (caller has cleared the frame).
// Returns false if the template is invalid/unsupported (nothing is drawn in
// that case: validation runs before the draw pass).
// NOTE: this path compiles first and is only used by legacy/one-shot callers;
// the runtime uses tplDrawCt with the persisted compiled template.
bool tplDraw(const String &tmplJson, const String &usageJson, const TplEnv &env);

// Structural validation only (no usage needed): used at BLE receive time.
bool tplValidate(const String &tmplJson, String &err);

// Render-target canvas (v2 §5): the engine is target-parameterized so one
// binary family serves 200x200 and 400x300 panels. Called at boot and by the
// host harness; defaults to TARGET_WIDTH x TARGET_HEIGHT.
void tplSetCanvas(int w, int h);

// ---------------------------------------------------------------------------
// CompiledTemplate (v2 §8): bounded, pointer-free render plan + field
// requirements. Template JSON is parsed exactly once (save/install); all
// runtime wake/data/switch/render paths address ops and requirement indices.
// ---------------------------------------------------------------------------

#define CT_ABI          1
#define CT_MAX_OPS      48
#define CT_MAX_REQS     32
#define CT_MAX_RES      8
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
    uint8_t  flags;        // bit0 fill, bit1 has_region, bit2 has_align, bit3 has_timefmt
    uint8_t  font;         // 0..4 (f8,f12,f16,f20,f24)
    uint8_t  scale;
    int16_t  x, y, w, h;   // primary rect / text origin
    int16_t  x2, y2;       // line end
    int16_t  whenEqNum;    // numeric `equals`
    int16_t  maxVal;       // bar max (default 100)
    uint8_t  bindIdx;      // CT_NONE_IDX or requirement index
    uint8_t  whenIdx;      // CT_NONE_IDX or requirement index
    uint8_t  whenMode;     // CtWhenMode
    uint8_t  align;        // 0 left, 1 center, 2 right
    uint8_t  timeFormat;   // 0 date, 1 hhmm
    uint8_t  resourceIdx;  // CT_NONE_IDX or resource index
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
