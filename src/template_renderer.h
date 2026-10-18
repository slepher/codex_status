#pragma once
#include "template_engine.h"
// Native XIP code carries no writable singleton. Each UI owner supplies its
// PAINT and owns one renderer; calls on the same renderer must be serialized.
// ESP/host legacy free functions above retain their existing default instance.
#include <ArduinoJson.h>
#include "GUI_Paint.h"
#include "font_noto.h"

#if defined(CODEX_TARGET_NOTE4) || defined(CODEX_RENDER_NOTE4_FONTS)
static constexpr unsigned TPL_FONT_SLOT_COUNT = 10;
#else
static constexpr unsigned TPL_FONT_SLOT_COUNT = 9;
#endif

class TemplateRenderer : public PaintCanvas {
    int sCanvasW, sCanvasH;
    Note4PropFont sAssetFonts[TPL_FONT_SLOT_COUNT] = {};
    bool sAssetBound[TPL_FONT_SLOT_COUNT] = {};
    int sClockClipped = 0;
    int tplCanvasW() const { return sCanvasW; }
    int tplCanvasH() const { return sCanvasH; }
    enum BindKind {
        B_PLAN, B_LABEL, B_HOSTID, B_SERVER_TIME,
        B_RESET_COUNT, B_RESET_EXPIRES,
        B_BUCKET_USED, B_BUCKET_REMAIN, B_BUCKET_RESET, B_BUCKET_WINMINS,
        B_DEV_CHANNEL, B_DEV_IP, B_DEV_SYNC, B_DEV_BATTERY,
        // Serialized tag 14 is retired; never reuse it for another field.
        B_DEV_OFFLINE = 15, B_DEV_NOW, B_DEV_MODE, B_DEV_DATE,
        B_DEV_WIFI_STATE, B_DEV_WIFI_BLINK, B_DEV_BLE_MANUAL
    };
    enum TextTimeFormat { TTF_DATE, TTF_HHMM };
    struct BindSpec {
        BindKind kind;
        String   bucket;
        int      winMode  = 0;
        int      winIndex = 0;
    };
    struct DrawCondition {
        bool active = false;
        BindSpec bind;
        bool exists = false;
        bool useEquals = false;
        String equals;
    };
    String asciiOnly(const String &in);
    int colorVal(JsonVariant v, int def);
    bool parseBind(const String &path, BindSpec &s);
    bool isEpochBind(const BindSpec &s);
    bool parseTimeFormat(JsonVariant v, const BindSpec &s, TextTimeFormat &fmt);
    bool findWindow(JsonDocument &usage, const BindSpec &s, JsonObject &w);
    String fmtEpoch(long long epoch, TextTimeFormat format = TTF_DATE);
    bool evalBind(const BindSpec &s, TextTimeFormat format, JsonDocument &usage,
                     const TplEnv &env, String &out);
    bool evalBindNum(const BindSpec &s, JsonDocument &usage,
                        const TplEnv &env, double &v);
    bool bindExists(const BindSpec &s, JsonDocument &usage, const TplEnv &env,
                       bool haveUsage);
    bool parseCondition(JsonObject e, DrawCondition &condition);
    bool textScale(JsonVariant value, int &scale);
    bool textRegion(JsonVariant value, int &x, int &y, int &w, int &h);
    bool parseTextLayout(JsonObject e, BindSpec *spec, int &scale,
                            bool &hasRegion, int &rx, int &ry, int &rw, int &rh,
                            TextTimeFormat &format);
    bool decodeBase64(const char *in, uint8_t *out, size_t outLen);
    void drawScaledText(const String &value, sFONT *font, int x, int y,
                           int scale, int fg, int bg, bool clipped,
                           int clipX, int clipY, int clipW, int clipH);
    uint32_t fontNextChar(const Note4PropFont &f, const char *text, size_t len, size_t &at);
    const Note4Glyph & fontGlyph(const Note4PropFont &f, uint32_t cp);
    int propTextWidth(const Note4PropFont &f, const String &value, int scale);
    void drawPropText(const Note4PropFont &f, const String &value, int x, int y,
                         int scale, int fg, int bg, bool clipped,
                         int clipX, int clipY, int clipW, int clipH);
    bool drawIcon(const uint8_t *bits, int x, int y, int w, int h, int fg);
    bool clampRect(JsonArray r, int &x, int &y, int &w, int &h);
    const Note4PropFont * propFontByIndex(int idx);
    void ctCopy(char *dst, size_t cap, const char *src);
    int ctFindReq(const CtTemplate &ct, const String &path);
    int ctAddReq(CtTemplate &ct, const BindSpec &spec, const String &path);
    int ctFindRes(const CtTemplate &ct, const char *bits);
    bool ctBind(const CtReq &r, BindSpec &s);
    bool ctEvalText(const CtTemplate &ct, uint8_t idx, TextTimeFormat fmt,
                       JsonDocument &usage, const TplEnv &env, String &out);
    bool ctExists(const CtTemplate &ct, uint8_t idx, JsonDocument &usage,
                     const TplEnv &env, bool haveUsage);
    bool ctCondMatches(const CtTemplate &ct, const CtOp &op, JsonDocument &usage,
                          const TplEnv &env, bool haveUsage);
    bool parseElementCompiled(JsonObject e, CtTemplate &ct, CtOp &op);
    bool drawCtOp(const CtTemplate &ct, const CtOp &op, JsonDocument &usage,
                     const TplEnv &env, bool haveUsage, bool dry);
    bool parseTemplate(const String &tmplJson, JsonDocument &td, JsonArray &els, String &err);
public:
    TemplateRenderer(const TemplateRenderer &) = delete;
    TemplateRenderer &operator=(const TemplateRenderer &) = delete;
    TemplateRenderer(PAINT &paint, int w, int h) : PaintCanvas(paint), sCanvasW(w), sCanvasH(h) {}
    void tplSetCanvas(int w, int h) { if (w > 0 && h > 0) { sCanvasW = w; sCanvasH = h; } }
    void tplFontClearAssets();
    bool tplFontBindAsset(const uint8_t *bytes, size_t len);
    int tplFontCount();
    int tplFontFixedCount();
    int tplFontIndexByName(const char *name);
    const char * tplFontNameByIndex(int idx);
    bool tplFontCellByName(const char *name, int &cellW, int &cellH);
    bool tplFontCellByIndex(int idx, int &cellW, int &cellH);
    bool tplFontClockBox(int idx, int scale, int &w, int &h);
    bool tplFontDrawClock(uint8_t *win, int bw, int rows, int xOff, int idx,
                      const char *text, int scale);
    int tplFontClockClipped();
    int tplFontPropWidth(int idx, int scale, const char *text);
    bool tplValidateCt(const CtTemplate &ct, String &err);
    bool tplDrawCt(const CtTemplate &ct, const String &usageJson, const TplEnv &env);
    bool tplCompile(const String &tmplJson, CtTemplate &out, String &err);
    bool tplValidate(const String &tmplJson, String &err);
    bool tplDraw(const String &tmplJson, const String &usageJson, const TplEnv &env);
    size_t tplCtSize();
    bool tplCtSerialize(const CtTemplate &ct, uint8_t *out, size_t cap, size_t &written);
    bool tplCtDeserialize(const uint8_t *in, size_t len, CtTemplate &out, String &err);
};

#if !defined(CODEX_AP01_NATIVE)
TemplateRenderer &tplDefaultRenderer();
#endif
