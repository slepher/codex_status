/*
 * Codex Status - 0.2.0 BLE
 * Wi-Fi 主通道（HTTP /usage）+ BLE 备选通道（GATT），BLE 配对时下发 endpoint+token
 */

#include <Arduino.h>
#include <WiFi.h>
#include <WebServer.h>
#include <Update.h>
#include <ArduinoOTA.h>
#include <Preferences.h>
#include <ESPmDNS.h>
#include <ArduinoJson.h>
#include <time.h>
#include <sys/time.h>
#include <string.h>
#include <vector>
#include <esp_sleep.h>
#include <esp_ota_ops.h>
#include <esp_system.h>
#include <driver/rtc_io.h>

#include "DEV_Config.h"
#include "dev_log.h"
#include "EPD_SSD1681.h"
#include "GUI_Paint.h"
#include "fonts.h"
#include "ble_bridge.h"
#include "bridge_store.h"
#include "usage_client.h"
#include "template_store.h"
#include "template_engine.h"
#include "template_xfer.h"

#define FW_VERSION    "0.10.3-bw"
#define AP_PASSWORD   "codex1234"
#define MAX_SLOTS     3

static const int EPD_W = EPD_SSD1681_WIDTH;
static const int EPD_H = EPD_SSD1681_HEIGHT;
static const int EPD_FB_BYTES = (EPD_W / 8) * EPD_H;

static Preferences prefs;
static WebServer   server(80);
static UBYTE      *frame = nullptr;
static UBYTE      *lastDisplayedFrame = nullptr;
static uint32_t    epdWriteCount = 0;
static bool        otaRebootPending = false;
static uint32_t    otaRebootAt = 0;
static String      authToken;
static char        otaPasswordBuf[40] = {0};
static bool        otaUploadDenied = false;
static bool        configMode = false;
static String      hostname;
static String      apSsid;

// Runtime power mode: 0 auto (DEEP windows, M2), 1 deep, 2 live.
// Until M3 delivers the LIVE path (custom core + PM), `live` behaves as deep.
static const char *MODE_NAMES[3] = {"auto", "deep", "live"};

static uint8_t runtimeMode() {
    Preferences p;
    p.begin("cfg", true);
    uint8_t m = p.getUChar("mode", 0);
    p.end();
    return m > 2 ? 0 : m;
}

static void setRuntimeMode(uint8_t m) {
    Preferences p;
    p.begin("cfg", false);
    p.putUChar("mode", m > 2 ? 0 : m);
    p.end();
}

// ---------------- M2 DEEP window state (sleep.md §4.1/§4.2) ----------------
#define WINDOW_MS        15000UL
#define WINDOW_HOLD_MS   30000UL
#define WINDOW_MAX_MS    (10UL * 60UL * 1000UL)
#define ACTIVE_HOLD_S    600
#define CONFIG_IDLE_MS   (5UL * 60UL * 1000UL)
#define STORE_MAX_LOCAL  8

enum IdleReason { IDLE_BOOT = 0, IDLE_WIFI_LOST, IDLE_BRIDGE_LOST, IDLE_ENV_SWITCH };
static const char *IDLE_REASON_NAMES[] = {"boot", "wifi_lost", "bridge_lost", "env_switch"};

RTC_DATA_ATTR static uint32_t rtcMagic = 0;
RTC_DATA_ATTR static uint32_t rtcActiveAt = 0;
RTC_DATA_ATTR static char     rtcActiveMac[20] = {0};
RTC_DATA_ATTR static uint8_t  rtcFailCount = 0;
RTC_DATA_ATTR static uint8_t  rtcFastLeft = 3;
RTC_DATA_ATTR static uint8_t  rtcIdleReason = IDLE_BOOT;
RTC_DATA_ATTR static bool     rtcNeverSynced = true;
RTC_DATA_ATTR static uint32_t rtcBssidHash = 0;
RTC_DATA_ATTR static uint32_t rtcUsageHash = 0;

static bool     windowMode = false;      // DEEP window flow (M2 default)
static uint32_t windowDeadline = 0;
static uint32_t windowHardStop = 0;
static bool     windowSynced = false;
static bool     windowHadWifi = false;
static bool     windowEnvSwitch = false;
static uint32_t activeHoldSec = ACTIVE_HOLD_S;
static bool     otaInProgress = false;
static uint32_t configStartedAt = 0;

// LIVE (M3 Plan B: modem sleep on the stock core; PM auto-light-sleep later).
static bool     liveMode = false;
static uint32_t liveEnteredAtMs = 0;
static uint32_t liveLastSyncMs = 0;
static uint32_t liveLastPollMs = 0;
static uint32_t wifiLostSinceMs = 0;

static uint32_t fnv1a(const String &s) {
    uint32_t h = 2166136261u;
    for (size_t i = 0; i < s.length(); i++) {
        h ^= (uint8_t)s[i];
        h *= 16777619u;
    }
    return h;
}

static bool timeKnown() { return time(nullptr) > 1600000000; }

static const char *idleReasonText() {
    return IDLE_REASON_NAMES[rtcIdleReason <= IDLE_ENV_SWITCH ? rtcIdleReason : 0];
}

static void setActiveMac(const String &mac) {
    strncpy(rtcActiveMac, mac.c_str(), sizeof(rtcActiveMac) - 1);
    rtcActiveMac[sizeof(rtcActiveMac) - 1] = '\0';
}

static void adoptServerTime(JsonDocument &doc) {
    long long st = doc["server_time"] | 0LL;
    if (!timeKnown() && st > 1600000000) {
        struct timeval tv = {(time_t)st, 0};
        settimeofday(&tv, nullptr);
        DevLog.printf("[pm] clock set from bridge: %lld\n", st);
    }
}

static bool usageCacheLoad(String &out) {
    Preferences p;
    p.begin("ucache", true);
    out = p.getString("json", "");
    p.end();
    return out.length() > 0;
}

static void usageCacheSave(const String &json) {
    uint32_t h = fnv1a(json);
    if (h == rtcUsageHash) return;
    if (json.length() >= 4000) {
        DevLog.println("[pm] usage cache skipped: too large");
        rtcUsageHash = h;
        return;
    }
    Preferences p;
    p.begin("ucache", false);
    p.putString("json", json);
    p.end();
    rtcUsageHash = h;
}

// Keep the current window open for an explicit user action (BLE pairing/token
// issuance, OTA upload), up to a bounded hard stop.
static void holdWindow(uint32_t ms) {
    if (!windowMode) return;
    windowHardStop = millis() + ms;
    if ((int32_t)(windowHardStop - windowDeadline) > 0) windowDeadline = windowHardStop;
}

// A sync is accepted from any bridge while there is no active bridge, from the
// active bridge itself, after the active hold window, when the active
// endpoint's BSSID no longer matches the current one, or on an explicit
// activate flag (sleep.md §2).
static bool usageAccepted(const String &mac, bool explicitActivate) {
    if (explicitActivate) return true;
    if (!rtcActiveMac[0] || rtcActiveAt == 0) return true;
    if (mac.length() && mac == rtcActiveMac) return true;
    if (timeKnown() && rtcActiveAt > 1600000000 &&
        (time_t)time(nullptr) - (time_t)rtcActiveAt >= (time_t)activeHoldSec) {
        return true;
    }
    String bssid = WiFi.BSSIDstr();
    for (int i = 0; i < storeCount(); i++) {
        EndpointRec r;
        if (!storeGet(i, r)) continue;
        if (r.mac == String(rtcActiveMac)) {
            if (r.bssid.length() && bssid.length() && r.bssid != bssid) return true;
            break;
        }
    }
    return false;
}

static volatile bool pendingUsageReady = false;
static String        pendingUsage;
static String        pendingChannel;
static volatile bool pendingEndpoint = false;
static volatile bool pendingTplChanged = false;
static bool pairingOverlay = false;
static bool drawingPairingOverlay = false;

static String activeTplJson;
static String activeTplId;

static String lastUsage;
static String lastChannel = "-";
static String renderedIp;
static uint32_t lastSyncMs = 0;
static time_t   lastSyncEpoch = 0;
static uint32_t lastOkMs = 0;
static bool     usageOnScreen = false;
static String   screenSig;
static String   renderedMinute;
static int      renderedBattery = -1;
static int      epdPartialCount = 0;
static bool     epdPartialReady = false;
static bool     epdAsleep = false;

static void screen(const std::vector<String> &lines, UBYTE color = BLACK);
static void epdFlush(bool forceFull = false);
static int  batteryPercent();
static bool requestAuthorized();
static void deepSleepFor(uint32_t sec, bool renderIdle);

static esp_sleep_wakeup_cause_t bootWakeCause = ESP_SLEEP_WAKEUP_UNDEFINED;

static bool pairingOverlayActive() {
    bool paired = bleIsConnected() && blePeerIsBonded() && blePeerIsEncrypted();
    return pairingOverlay && blePairingWindowOpen() && !paired;
}

static String macSuffix() {
    String mac = WiFi.macAddress();
    mac.replace(":", "");
    return mac.substring(6);
}

static String ipText() {
    if (configMode) return "192.168.4.1";
    if (WiFi.status() == WL_CONNECTED) return WiFi.localIP().toString();
    return "-";
}

static bool bridgeOk() {
    return lastOkMs > 0 && (millis() - lastOkMs) < 120000;
}

static String nowHHMM() {
    char b[8] = "--:--";
    time_t n = time(nullptr);
    struct tm *t = localtime(&n);
    if (t && n > 1600000000) strftime(b, sizeof(b), "%H:%M", t);
    return String(b);
}

static String fitText(const String &s, size_t maxChars = 17) {
    String t;
    t.reserve(s.length());
    for (size_t i = 0; i < s.length(); i++) {
        char c = s[i];
        t += (c >= 32 && c <= 126) ? c : '?';
    }
    if (t.length() <= maxChars) return t;
    return t.substring(0, maxChars - 1) + "~";
}

struct WinInfo {
    bool found = false;
    int used = -1;
    long long resets = 0;
};

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

static void screenIdle() {
    std::vector<String> lines;
    lines.push_back("CODEX STATUS");
    lines.push_back("IDLE - NO LINK");
    lines.push_back(String("BATT ") + batteryPercent() + "%");
    lines.push_back(String("IP ") + ipText());
    lines.push_back("SYNC --:--");
    lines.push_back(String("FW ") + FW_VERSION);
    screen(lines);
    usageOnScreen = false;
    renderedIp = ipText();
    screenSig = String("idle|") + ipText() + "|" + String(bridgeOk()) + "|" + String(bleIsConnected());
}

static void screen(const std::vector<String> &lines, UBYTE color) {
    if (pairingOverlayActive() && !drawingPairingOverlay) return;
    if (!frame) return;
    Paint_SelectImage(frame);
    Paint_Clear(WHITE);
    Paint_DrawRectangle(0, 0, EPD_W - 1, EPD_H - 1, BLACK, DOT_PIXEL_1X1, DRAW_FILL_EMPTY);
    int y = 10;
    for (const auto &line : lines) {
        Paint_DrawString_EN(8, y, fitText(line).c_str(), &Font16, WHITE, color);
        y += 20;
        if (y > EPD_H - 24) break;
    }
    epdFlush();
}

static void screenPairingOverlay() {
    drawingPairingOverlay = true;
    screen({"RELEASE BOOT", "BLE PAIRING", "WINDOWS ADD DEVICE"});
    drawingPairingOverlay = false;
}

static void epdBegin(bool clearPanel = true) {
    pinMode(EPD_PWR_PIN, OUTPUT);
    digitalWrite(EPD_PWR_PIN, HIGH);
    delay(500);
    digitalWrite(EPD_PWR_PIN, LOW);
    delay(200);
    pinMode(42, OUTPUT);
    digitalWrite(42, LOW);
    pinMode(17, OUTPUT);
    digitalWrite(17, HIGH);
    delay(20);
    DEV_Module_Init();
    EPD_SSD1681_Init();
    if (clearPanel) EPD_SSD1681_Clear(EPD_SSD1681_WHITE);
    frame = (UBYTE *)malloc(EPD_FB_BYTES);
    if (!frame) {
        DevLog.println("[epd] frame buffer malloc failed");
        return;
    }
    lastDisplayedFrame = (UBYTE *)malloc(EPD_FB_BYTES);
    if (lastDisplayedFrame) {
        memset(lastDisplayedFrame, 0xFF, EPD_FB_BYTES);
    } else {
        DevLog.println("[epd] last-display buffer malloc failed; writes will not be skipped");
    }
    Paint_NewImage(frame, EPD_W, EPD_H, 0, WHITE);
    Paint_SetScale(2);
    Paint_SelectImage(frame);
    epdPartialReady = false;
    epdPartialCount = 0;
}

// Data screens default to partial refresh (~300ms, no flash). A full refresh
// (~1.5s, flashes) runs when the panel is not partial-ready, when the changed
// area exceeds 12.5% of the panel (layout/value jumps), or after 30 partials
// to clear ghosting. forceFull requests a full refresh explicitly.
// The panel sleeps (SSD1681 deep-sleep mode 1, RAM retained) after every
// refresh and is woken/re-initialized before the next draw.
static void epdPanelSleep() {
    if (epdAsleep) return;
    EPD_SSD1681_Sleep();
    epdAsleep = true;
}

static void epdFlush(bool forceFull) {
    if (!frame) return;
    if (lastDisplayedFrame && memcmp(frame, lastDisplayedFrame, EPD_FB_BYTES) == 0) return;

    bool partial = !forceFull && epdPartialReady;
    if (partial && lastDisplayedFrame) {
        int changed = 0;
        for (int i = 0; i < EPD_FB_BYTES; i++)
            changed += __builtin_popcount((unsigned char)(frame[i] ^ lastDisplayedFrame[i]));
        if (changed > EPD_W * EPD_H / 8) partial = false;
    }
    if (partial && ++epdPartialCount <= 30) {
        if (epdAsleep) {
            EPD_SSD1681_WakePartial(lastDisplayedFrame);
            epdAsleep = false;
        }
        EPD_SSD1681_DisplayPart(frame);
        epdWriteCount++;
        if (lastDisplayedFrame) memcpy(lastDisplayedFrame, frame, EPD_FB_BYTES);
        epdPanelSleep();
        return;
    }

    epdPartialCount = 0;
    if (epdPartialReady || epdAsleep) EPD_SSD1681_Init();   // full LUT + wake
    epdAsleep = false;
    EPD_SSD1681_Display(frame);
    epdWriteCount++;
    if (lastDisplayedFrame) memcpy(lastDisplayedFrame, frame, EPD_FB_BYTES);
    EPD_SSD1681_Init_Partial();
    epdPartialReady = true;
    epdPanelSleep();
}

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

static uint32_t batteryMilliVolts() {
    uint32_t mv = 0;
    for (int i = 0; i < 8; i++) mv += analogReadMilliVolts(4);
    return (mv / 8) * 2;   // board divider: VBAT = VADC x 2
}

static int batteryPercent() {
    uint32_t vbat = batteryMilliVolts();
    if (vbat >= 4200) return 100;
    if (vbat <= 3300) return 0;
    return (int)((vbat - 3300) * 100 / 900);
}

static String fmtEpoch(long long ts, const char *fmt) {
    if (ts <= 0) return String("--");
    time_t t = (time_t)ts;
    struct tm *lt = localtime(&t);
    char b[24];
    if (!lt || !strftime(b, sizeof(b), fmt, lt)) return String("--");
    return String(b);
}

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
    if (pairingOverlayActive()) return;
    if (!frame) return;
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

    Paint_SelectImage(frame);
    Paint_Clear(WHITE);

    drawHeroBlock(Q_TL_X, Q_TL_Y, (wk.found && wk.used >= 0) ? String(100 - wk.used) : String("--"));
    if (fh.found) drawHeroBlock(Q_BR_X, Q_BR_Y, (fh.used >= 0) ? String(100 - fh.used) : String("--"));
    else          drawHeroBlock(Q_BR_X, Q_BR_Y, String("100"));

    String rcLine = (rc >= 0) ? String("RC ") + rc : String("RC --");
    std::vector<String> tr, bl;
    int battery = batteryPercent();

    // Pre-confirmed fixed layout: "MM-DD HH:MM" is 11 chars x Font12.Width(7)
    // = 77px, which fits the 98px available right of the week block, so the
    // date+time stays on one line and the plan name stays top-right. (The
    // two-line date/time + plan-to-bottom-left variant was rejected at
    // design time; do not switch dynamically.)
    tr.push_back(fitText(String(plan), 12));
    tr.push_back(fitText(String(label), 12));
    tr.push_back(fmtEpoch(wk.resets, "%m-%d %H:%M"));
    tr.push_back(rcLine);
    if (fh.found) bl.push_back(String("5H ") + fmtEpoch(fh.resets, "%H:%M"));
    bl.push_back(String("BATT ") + battery + "%");
    bl.push_back(String("SYNC ") + nowHHMM());

    for (int i = 0; i < (int)tr.size(); i++)
        drawSmallRight(tr[i], Q_TR_RIGHT, Q_TR_Y0 + i * Q_LINE_H);
    int first = 4 - (int)bl.size();
    for (int i = 0; i < (int)bl.size(); i++)
        drawSmallLeft(bl[i], Q_BL_X, Q_BL_SLOTS[first + i]);

    epdFlush(false);
    usageOnScreen = true;
    renderedIp = ipText();
    renderedMinute = nowHHMM();
    renderedBattery = battery;
    screenSig = String("usage");
    DevLog.printf("[ui] quad rendered (wk=%d fh=%d ch=%s)\n",
                  wk.used, fh.used, channel ? channel : "");
}

static void updateInfoExtra() {
    String items = "\"ip\":\"" + ipText() + "\",\"http_port\":80,\"templates\":[";
    String active = tplStoreActive();
    for (int i = 0; i < tplStoreCount(); i++) {
        TplMeta m;
        if (!tplStoreGet(i, m)) continue;
        if (i) items += ",";
        items += String("{\"id\":\"") + m.id + "\",\"hash\":\"" + m.hash +
                 "\",\"active\":" + ((m.id == active) ? "true" : "false") + "}";
    }
    items += "]";
    bleSetInfoExtra(items);
}

static bool tplCacheLoad() {
    String id = tplStoreActive();
    if (!id.length()) { activeTplJson = ""; activeTplId = ""; return false; }
    if (id == activeTplId && activeTplJson.length()) return true;
    String json;
    if (!tplStoreLoad(id, json)) { activeTplJson = ""; activeTplId = ""; return false; }
    activeTplJson = json;
    activeTplId = id;
    return true;
}

static void renderActiveUsage(const String &json, const char *channel, bool idle = false) {
    if (pairingOverlayActive()) return;
    if (!frame) return;
    String tplJson, tplId;
    if (idle) {
        String iid = tplStoreIdle();
        if (iid.length() && tplStoreLoad(iid, tplJson)) tplId = iid;
    }
    if (!tplJson.length()) {
        if (!tplCacheLoad()) {
            if (idle) screenIdle();
            return;
        }
        tplJson = activeTplJson;
        tplId = activeTplId;
    }
    TplEnv env;
    env.channel   = channel ? channel : "";
    env.ip        = ipText();
    env.syncHHMM  = nowHHMM();
    env.battery   = batteryPercent();
    env.idle      = idle;
    if (idle) {
        env.idleReason = idleReasonText();
        if (timeKnown() && rtcActiveAt > 1600000000) {
            env.offlineMins = (int)(((time_t)time(nullptr) - (time_t)rtcActiveAt) / 60);
        }
    }
    Paint_SelectImage(frame);
    Paint_Clear(WHITE);
    if (tplDraw(tplJson, json, env)) {
        epdFlush(false);
        usageOnScreen = true;
        renderedIp = ipText();
        renderedMinute = env.syncHHMM;
        renderedBattery = env.battery;
        screenSig = String("usage");
        DevLog.printf("[tpl] rendered %s %s (%s)\n", tplId.c_str(),
                      idle ? "idle" : "live", channel ? channel : "");
        return;
    }
    DevLog.printf("[tpl] %s invalid, fallback built-in\n", tplId.c_str());
    if (!idle) renderUsage(json, channel);
    else screenIdle();
}

static void maybeFetchTemplate(const EndpointRec &rec, const String &usageJson) {
    JsonDocument doc;
    if (deserializeJson(doc, usageJson)) return;
    JsonObject tpls = doc["templates"].as<JsonObject>();
    if (tpls.isNull()) return;
    for (JsonPair kv : tpls) {
        String id = String(kv.key().c_str());
        JsonObject meta = kv.value().as<JsonObject>();
        String hash = String((const char *)(meta["hash"] | ""));
        uint32_t ver = meta["version"] | 0;
        if (!hash.length()) continue;
        TplMeta local;
        String localHash;
        bool localValid = false;
        if (tplStoreFind(id, local) && local.hash == hash) {
            String localJson, localErr;
            localValid = tplStoreLoad(id, localJson) &&
                         tplValidateForStorage(localJson, hash, FW_VERSION, localErr);
            if (localValid) localHash = local.hash;
            if (!localValid) {
                DevLog.printf("[tpl] local %s rejected: %s\n", id.c_str(), localErr.c_str());
            }
        } else if (tplStoreFind(id, local)) {
            localHash = local.hash;
        }
        if (localValid) continue;
        String out, err;
        if (usageTemplateGet(rec, id, localHash, out, err)) {
            String acceptErr;
            if (!tplValidateForStorage(out, hash, FW_VERSION, acceptErr)) {
                DevLog.printf("[tpl] HTTP %s rejected: %s\n", id.c_str(), acceptErr.c_str());
                continue;
            }
            if (tplStoreSave(id, ver, hash, (const uint8_t *)out.c_str(), out.length())) {
                if (tplStoreActive().length() == 0 || tplStoreActive() == id) {
                    tplStoreSetActive(id);
                    activeTplId = "";
                }
                updateInfoExtra();
                DevLog.printf("[tpl] fetched %s hash=%s\n", id.c_str(), hash.c_str());
            }
        } else if (err != "http 304") {
            DevLog.printf("[tpl] fetch %s failed: %s\n", id.c_str(), err.c_str());
        }
    }
}

static void nextTemplate() {
    int n = tplStoreCount();
    if (n <= 0) return;
    String active = tplStoreActive();
    int cur = -1;
    for (int i = 0; i < n; i++) {
        TplMeta m;
        if (tplStoreGet(i, m) && m.id == active) { cur = i; break; }
    }
    int next = (cur + 1) % n;
    TplMeta m;
    if (!tplStoreGet(next, m)) return;
    tplStoreSetActive(m.id);
    activeTplId = "";
    pendingTplChanged = true;
    DevLog.printf("[tpl] local switch -> %s\n", m.id.c_str());
}

static void factoryReset() {
    screen({"FACTORY RESET", "", "clearing..."});
    bleClearBonds();
    storeClear();
    tplStoreClear();
    prefs.begin("wifi", false);
    prefs.clear();
    prefs.end();
    prefs.begin("auth", false);
    prefs.clear();
    prefs.end();
    delay(1000);
    ESP.restart();
}

static void applyEnvelopeMeta(JsonDocument &doc) {
    long long hold = doc["active_hold_seconds"] | 0LL;
    if (hold >= 60 && hold <= 86400) activeHoldSec = (uint32_t)hold;
    const char *idle = doc["idle_template"] | "";
    if (strlen(idle)) tplStoreSetIdle(String(idle));
}

// Endpoint selection (sleep.md §4.2): when the active bridge synced recently,
// only try its endpoint; otherwise try same-BSSID endpoints first (MRU), then
// the rest. Per-endpoint timeout is 2 s.
static bool tryWifiUsage() {
    if (WiFi.status() != WL_CONNECTED) return false;
    int n = storeCount();
    if (n <= 0) return false;

    String bssid = WiFi.BSSIDstr();
    bool activeFresh = rtcActiveMac[0] && rtcActiveAt > 0 && timeKnown() &&
                       (time_t)time(nullptr) - (time_t)rtcActiveAt < (time_t)activeHoldSec;
    int order[STORE_MAX_LOCAL];
    int count = 0;
    bool used[STORE_MAX_LOCAL] = {false};

    if (activeFresh) {
        for (int i = 0; i < n; i++) {
            EndpointRec r;
            if (storeGet(i, r) && r.mac == String(rtcActiveMac)) { order[count++] = i; used[i] = true; break; }
        }
    }
    for (int pass = 0; pass < 2; pass++) {
        while (count < n) {
            int idx = -1;
            uint32_t mx = 0;
            for (int i = 0; i < n; i++) {
                if (used[i]) continue;
                EndpointRec r;
                if (!storeGet(i, r)) { used[i] = true; continue; }
                bool same = bssid.length() && r.bssid.length() && r.bssid == bssid;
                if ((pass == 0) != same) continue;
                if (idx < 0 || r.mru > mx) { idx = i; mx = r.mru; }
            }
            if (idx < 0) break;
            used[idx] = true;
            order[count++] = idx;
        }
        if (!bssid.length()) break;   // no BSSID known: single MRU pass
    }

    for (int k = 0; k < count; k++) {
        EndpointRec rec;
        if (!storeGet(order[k], rec)) continue;
        String out, err;
        if (!usageHttpGet(rec, out, err, 2000)) {
            DevLog.printf("[wifi] %s:%u failed: %s\n", rec.host.c_str(), rec.port, err.c_str());
            continue;
        }
        JsonDocument parsed;
        if (deserializeJson(parsed, out) || parsed.as<JsonObject>().isNull()) {
            DevLog.println("[wifi] usage rejected: invalid JSON");
            continue;
        }
        adoptServerTime(parsed);
        applyEnvelopeMeta(parsed);
        bool explicitActivate = parsed["activate"] | false;
        bool accepted = usageAccepted(rec.mac, explicitActivate);
        storeTouch(rec.mac);
        if (bssid.length()) storeSetBssid(rec.mac, bssid);
        lastSyncMs = millis();
        lastOkMs = millis();
        lastSyncEpoch = time(nullptr);
        windowSynced = true;
        DevLog.printf("[wifi] usage from %s:%u accepted=%d\n",
                      rec.host.c_str(), rec.port, accepted ? 1 : 0);
        bleNotifyStatus("{\"ack\":\"wifi-usage\",\"ok\":true}");
        maybeFetchTemplate(rec, out);
        lastUsage = out;
        lastChannel = "WIFI";
        usageCacheSave(out);
        if (accepted) {
            setActiveMac(rec.mac);
            rtcActiveAt = timeKnown() ? (uint32_t)time(nullptr) : 0;
            renderActiveUsage(lastUsage, "WIFI", false);
        }
        return true;
    }
    return false;
}

// POST /usage from the bridge (sleep.md §4.3). Auth uses the endpoint token
// that the bridge itself wrote over BLE; the matching record identifies the
// bridge MAC for the §2 active rules.
static bool endpointTokenAuthorized(String &mac) {
    String header = server.header("Authorization");
    if (!header.startsWith("Bearer ")) return false;
    String token = header.substring(7);
    token.trim();
    if (!token.length()) return false;
    for (int i = 0; i < storeCount(); i++) {
        EndpointRec rec;
        if (!storeGet(i, rec)) continue;
        if (rec.token.length() && rec.token == token) {
            mac = rec.mac;
            return true;
        }
    }
    return false;
}

static void handleUsagePost() {
    String mac;
    if (!endpointTokenAuthorized(mac)) {
        server.send(401, "application/json", "{\"accepted\":false}");
        return;
    }
    String body = server.arg("plain");
    JsonDocument parsed;
    if (deserializeJson(parsed, body) || parsed.as<JsonObject>().isNull()) {
        server.send(400, "application/json", "{\"accepted\":false}");
        return;
    }
    adoptServerTime(parsed);
    applyEnvelopeMeta(parsed);
    bool explicitActivate = parsed["activate"] | false;
    bool accepted = usageAccepted(mac, explicitActivate);
    lastSyncMs = millis();
    lastOkMs = millis();
    lastSyncEpoch = time(nullptr);
    liveLastSyncMs = millis();
    windowSynced = true;
    usageCacheSave(body);
    if (accepted) {
        setActiveMac(mac);
        rtcActiveAt = timeKnown() ? (uint32_t)time(nullptr) : 0;
        lastUsage = body;
        lastChannel = "PUSH";
        renderActiveUsage(lastUsage, "PUSH");
    } else {
        DevLog.printf("[live] push ignored (active=%s)\n", rtcActiveMac);
    }
    server.send(200, "application/json",
                accepted ? "{\"accepted\":true}" : "{\"accepted\":false}");
}

// Deep-sleep wake sources: RTC timer plus BOOT (GPIO0) and PWR (GPIO18),
// active-low (sleep.md §4.2). The RTC pull-ups are armed explicitly so the
// buttons stay readable once the RTC domain is the only powered island.
static void armWakeSources(uint64_t timerUs) {
    rtc_gpio_pullup_en(GPIO_NUM_0);
    rtc_gpio_pulldown_dis(GPIO_NUM_0);
    rtc_gpio_pullup_en(GPIO_NUM_18);
    rtc_gpio_pulldown_dis(GPIO_NUM_18);
    esp_sleep_enable_ext1_wakeup((1ULL << 0) | (1ULL << 18), ESP_EXT1_WAKEUP_ANY_LOW);
    if (timerUs) esp_sleep_enable_timer_wakeup(timerUs);
}

// Common DEEP entry: optional IDLE render, panel/BLE/Wi-Fi teardown, wake
// sources armed, then deep sleep.
static void deepSleepFor(uint32_t sec, bool renderIdle) {
    if (renderIdle) {
        String cached;
        if (usageCacheLoad(cached)) renderActiveUsage(cached, "DEEP", true);
        else screenIdle();
    }
    epdPanelSleep();
    bleAdvertiseStop();
    WiFi.disconnect(true);
    armWakeSources((uint64_t)sec * 1000000ULL);
    esp_deep_sleep_start();
}

// Token-gated debug route: put the device straight into DEEP with a short
// timer so hardware tests (ext1 wake, current) can run without waiting for the
// LIVE exit watchdog. POST /sleep?sec=60
static void handleSleepPost() {
    if (!requestAuthorized()) {
        server.send(401, "text/plain", "unauthorized");
        return;
    }
    uint32_t sec = server.hasArg("sec") ? (uint32_t)server.arg("sec").toInt() : 60;
    if (sec < 30) sec = 30;
    if (sec > 900) sec = 900;
    DevLog.printf("[pm] test sleep %us\n", (unsigned)sec);
    server.send(200, "application/json", String("{\"sleeping\":") + String(sec) + "}");
    delay(200);
    deepSleepFor(sec, true);
}

static void handleBleUsage(const String &json) {
    JsonDocument parsed;
    if (deserializeJson(parsed, json) || parsed.as<JsonObject>().isNull()) {
        bleNotifyStatus("{\"ack\":\"usage\",\"ok\":false}");
        return;
    }
    pendingUsage = json;
    pendingChannel = "BLE";
    pendingUsageReady = true;
    bleNotifyStatus("{\"ack\":\"usage\",\"ok\":true}");
}

static void handleBleEndpoint(const String &json) {
    JsonDocument doc;
    if (deserializeJson(doc, json)) {
        bleNotifyStatus("{\"ack\":\"endpoint\",\"ok\":false}");
        return;
    }
    const char *host = doc["host"] | "";
    uint16_t port = doc["port"] | 0;
    const char *token = doc["token"] | "";
    if (!strlen(host) || !port) {
        bleNotifyStatus("{\"ack\":\"endpoint\",\"ok\":false}");
        return;
    }
    String mac = blePeerAddress();
    if (!mac.length()) mac = "unknown";
    storeUpsert(mac, host, port, token);
    bleNotifyStatus("{\"ack\":\"endpoint\",\"ok\":true}");
    pendingEndpoint = true;
}

static bool hasWifiSlots() {
    prefs.begin("wifi", true);
    bool any = false;
    for (int i = 0; i < MAX_SLOTS; i++) {
        if (prefs.getString(("s" + String(i)).c_str(), "").length()) { any = true; break; }
    }
    prefs.end();
    return any;
}

// Scan once and connect to the saved slot with the best signal; the last-used
// slot wins near-ties. Single 9 s connect attempt (sleep.md §4.2/§4.9).
static bool connectBest() {
    WiFi.mode(WIFI_STA);
    WiFi.setHostname(hostname.c_str());
    int n = WiFi.scanNetworks();
    if (n <= 0) {
        DevLog.println("[wifi] scan: no networks");
        return false;
    }
    prefs.begin("wifi", true);
    uint8_t last = prefs.getUChar("last", 0xFF);
    prefs.end();

    int bestSlot = -1;
    int bestRssi = -1000;
    for (int i = 0; i < MAX_SLOTS; i++) {
        prefs.begin("wifi", true);
        String ssid = prefs.getString(("s" + String(i)).c_str(), "");
        prefs.end();
        if (!ssid.length()) continue;
        for (int k = 0; k < n; k++) {
            if (WiFi.SSID(k) != ssid) continue;
            int rssi = WiFi.RSSI(k) + (i == last ? 10 : 0);
            if (rssi > bestRssi) { bestRssi = rssi; bestSlot = i; }
            break;
        }
    }
    WiFi.scanDelete();
    if (bestSlot < 0) {
        DevLog.println("[wifi] scan: no saved network visible");
        return false;
    }
    prefs.begin("wifi", true);
    String ssid = prefs.getString(("s" + String(bestSlot)).c_str(), "");
    String pass = prefs.getString(("p" + String(bestSlot)).c_str(), "");
    prefs.end();
    screen({"CODEX STATUS", FW_VERSION, "", "Connecting:", ssid});
    DevLog.printf("[wifi] slot %d (%s) rssi=%d\n", bestSlot, ssid.c_str(), bestRssi);
    WiFi.begin(ssid.c_str(), pass.c_str());
    uint32_t t0 = millis();
    while (WiFi.status() != WL_CONNECTED && millis() - t0 < 9000) {
        delay(200);
        DevLog.print(".");
    }
    DevLog.println();
    if (WiFi.status() != WL_CONNECTED) {
        DevLog.println("[wifi] connect timeout");
        WiFi.disconnect(true);
        return false;
    }
    prefs.begin("wifi", false);
    prefs.putUChar("last", (uint8_t)bestSlot);
    prefs.end();
    DevLog.printf("[wifi] connected: %s ip=%s bssid=%s\n", ssid.c_str(),
                  WiFi.localIP().toString().c_str(), WiFi.BSSIDstr().c_str());
    return true;
}

// ---------------- 配网模式 ----------------
static void handleConfigRoot() {
    int16_t n = WiFi.scanNetworks();
    String html = F("<!DOCTYPE html><html><head><meta charset='utf-8'>"
                    "<meta name='viewport' content='width=device-width,initial-scale=1'>"
                    "<title>Codex Status Setup</title></head><body>");
    html += F("<h2>Codex Status - Wi-Fi Setup</h2>");
    html += F("<form action='/save' method='post'>");
    html += F("<label>Wi-Fi:<br><select name='ssid'>");
    for (int i = 0; i < n; i++) {
        html += "<option value='" + WiFi.SSID(i) + "'>" + WiFi.SSID(i) + " (" + String(WiFi.RSSI(i)) + " dBm)</option>";
    }
    html += F("</select></label><br><br>");
    html += F("Or manual SSID:<br><input name='manual' size='32'><br><br>");
    html += F("Password:<br><input type='password' name='pass' size='32'><br><br>");
    html += F("<button type='submit'>Save &amp; Reboot</button></form>");
    html += F("<p><a href='/clear'>Clear all saved Wi-Fi</a></p></body></html>");
    server.send(200, "text/html", html);
    WiFi.scanDelete();
}

static void handleConfigSave() {
    String ssid = (server.hasArg("manual") && server.arg("manual").length()) ? server.arg("manual") : server.arg("ssid");
    String pass = server.arg("pass");
    if (!ssid.length()) {
        server.send(400, "text/plain", "ssid required");
        return;
    }
    prefs.begin("wifi", false);
    int slot = -1;
    for (int i = 0; i < MAX_SLOTS; i++) {
        if (!prefs.getString(("s" + String(i)).c_str(), "").length()) { slot = i; break; }
    }
    if (slot < 0) slot = 0;
    prefs.putString(("s" + String(slot)).c_str(), ssid);
    prefs.putString(("p" + String(slot)).c_str(), pass);
    prefs.putUChar("last", (uint8_t)slot);
    prefs.end();
    server.send(200, "text/html", "<h3>Saved. Rebooting...</h3>");
    screen({"Wi-Fi saved:", ssid, "", "Rebooting..."});
    delay(800);
    ESP.restart();
}

static void startConfigMode() {
    configMode = true;
    configStartedAt = millis();
    WiFi.mode(WIFI_AP);
    apSsid = "CodexStatus-" + macSuffix();
    WiFi.softAP(apSsid.c_str(), AP_PASSWORD);
    DevLog.printf("[config] AP=%s pass=%s url=http://192.168.4.1\n", apSsid.c_str(), AP_PASSWORD);
    screen({"WIFI SETUP", "", "AP:   " + apSsid, "PASS: " AP_PASSWORD, "", "Open http://", "192.168.4.1"});
    server.on("/", HTTP_GET, handleConfigRoot);
    server.on("/save", HTTP_POST, handleConfigSave);
    server.on("/clear", HTTP_GET, []() {
        prefs.begin("wifi", false);
        prefs.clear();
        prefs.end();
        server.send(200, "text/plain", "cleared");
        screen({"Wi-Fi cleared", "Rebooting..."});
        delay(600);
        ESP.restart();
    });
    server.begin();
}

// ---------------- 正常模式 ----------------
static bool requestAuthorized();

static const char *resetReasonName() {
    switch (esp_reset_reason()) {
    case ESP_RST_POWERON:   return "power-on";
    case ESP_RST_EXT:       return "external";
    case ESP_RST_SW:        return "software";
    case ESP_RST_PANIC:     return "panic";
    case ESP_RST_INT_WDT:   return "int-wdt";
    case ESP_RST_TASK_WDT:  return "task-wdt";
    case ESP_RST_WDT:       return "wdt";
    case ESP_RST_DEEPSLEEP: return "deep-sleep";
    case ESP_RST_BROWNOUT:  return "brownout";
    case ESP_RST_SDIO:      return "sdio";
    default:                return "unknown";
    }
}

static const char *wakeCauseName(esp_sleep_wakeup_cause_t cause) {
    switch (cause) {
    case ESP_SLEEP_WAKEUP_UNDEFINED: return "power-on";
    case ESP_SLEEP_WAKEUP_EXT0:      return "ext0";
    case ESP_SLEEP_WAKEUP_EXT1:      return "ext1";
    case ESP_SLEEP_WAKEUP_TIMER:     return "timer";
    default:                         return "other";
    }
}

static void handleStatus() {
    String html = F("<!DOCTYPE html><html><head><meta charset='utf-8'><title>Codex Status</title></head><body>");
    html += F("<h2>Codex Status</h2><ul>");
    html += "<li>Version: " FW_VERSION "</li>";
    const esp_partition_t *running = esp_ota_get_running_partition();
    const esp_partition_t *next = esp_ota_get_next_update_partition(nullptr);
    html += "<li>Running: " + String(running ? running->label : "?") +
            " (next OTA slot: " + String(next ? next->label : "?") + ")</li>";
    html += "<li>Reset reason: " + String(resetReasonName()) +
            " (uptime " + String(millis() / 1000) + "s)</li>";
    html += "<li>SSID: " + WiFi.SSID() + "</li>";
    html += "<li>IP: " + WiFi.localIP().toString() + "</li>";
    html += "<li>RSSI: " + String(WiFi.RSSI()) + " dBm</li>";
    html += "<li>Battery: " + String(batteryPercent()) + "% (" +
            String(batteryMilliVolts()) + " mV)</li>";
    html += "<li>BLE connected: " + String(bleIsConnected() ? "yes" : "no") + "</li>";
    html += "<li>Endpoints stored: " + String(storeCount()) + "</li>";
    html += "<li>Last channel: " + lastChannel + "</li>";
    String activeId = tplStoreActive();
    TplMeta activeMeta;
    String activeHash = tplStoreFind(activeId, activeMeta) ? activeMeta.hash : "";
    html += "<li>Templates: " + String(tplStoreCount()) + " (active: " + activeId +
            (activeHash.length() ? String(" hash ") + activeHash : String("")) + ")</li>";
    html += "<li>EPD writes: " + String(epdWriteCount) + " (partial " +
            String(epdPartialReady ? "ready" : "off") + ", streak " + String(epdPartialCount) + ")</li>";
    html += "<li>Free heap: " + String(ESP.getFreeHeap()) + "</li>";
    html += F("</ul><p>");
    if (requestAuthorized()) {
        html += "<a href='/update?token=" + authToken + "'>Firmware OTA update</a>";
    } else {
        html += "Firmware OTA: negotiate a token over BLE first (tools/device-auth)";
    }
    html += F("</p></body></html>");
    server.send(200, "text/html", html);
}

static void handleStatusJson() {
    JsonDocument doc;
    doc["fw"] = FW_VERSION;
    const esp_partition_t *running = esp_ota_get_running_partition();
    const esp_partition_t *next = esp_ota_get_next_update_partition(nullptr);
    doc["slot"] = running ? running->label : "?";
    doc["next_slot"] = next ? next->label : "?";
    doc["reset"] = resetReasonName();
    doc["wake"] = wakeCauseName(bootWakeCause);
    doc["pwr"] = digitalRead(18);
    doc["uptime_s"] = millis() / 1000;
    doc["ssid"] = WiFi.SSID();
    doc["ip"] = WiFi.localIP().toString();
    doc["rssi"] = WiFi.RSSI();
    doc["ble"] = bleIsConnected();
    doc["endpoints"] = storeCount();
    doc["channel"] = lastChannel;
    doc["mode"] = MODE_NAMES[runtimeMode()];
    doc["idle_reason"] = idleReasonText();
    doc["active_mac"] = rtcActiveMac;
    doc["active_at"] = rtcActiveAt;
    doc["fail_count"] = rtcFailCount;
    doc["window_synced"] = windowSynced;
    doc["live"] = liveMode;
    doc["battery"] = batteryPercent();
    doc["battery_mv"] = batteryMilliVolts();
    doc["heap"] = ESP.getFreeHeap();
    doc["epd_writes"] = epdWriteCount;
    doc["epd_partial"] = epdPartialReady;
    doc["epd_streak"] = epdPartialCount;
    JsonArray templates = doc["templates"].to<JsonArray>();
    String activeId = tplStoreActive();
    for (int i = 0; i < tplStoreCount(); i++) {
        TplMeta meta;
        if (!tplStoreGet(i, meta)) continue;
        JsonObject item = templates.add<JsonObject>();
        item["id"] = meta.id;
        item["hash"] = meta.hash;
        item["active"] = meta.id == activeId;
    }
    String out;
    serializeJson(doc, out);
    server.send(200, "application/json", out);
}

static void handleLog() {
    server.send(200, "text/plain; charset=utf-8", DevLog.dump());
}

static void randomHex(char *out, size_t bytes) {
    static const char *hex = "0123456789abcdef";
    for (size_t i = 0; i < bytes; i++) {
        uint32_t r = esp_random();
        out[i * 2]     = hex[(r >> 4) & 0x0F];
        out[i * 2 + 1] = hex[r & 0x0F];
    }
    out[bytes * 2] = '\0';
}

static void setOtaPassword(const char *value) {
    strncpy(otaPasswordBuf, value, sizeof(otaPasswordBuf) - 1);
    otaPasswordBuf[sizeof(otaPasswordBuf) - 1] = '\0';
    ArduinoOTA.setPassword(otaPasswordBuf);
}

// OTA/Wi-Fi operation token. Issued once on the device and persisted in NVS so
// reboots and DEEP sleep no longer invalidate it; it is disclosed only over the
// bonded BLE link (never over HTTP or serial).
static void persistAuthToken() {
    Preferences p;
    p.begin("auth", false);
    p.putString("token", authToken);
    p.end();
}

static void loadOrIssueAuthToken() {
    Preferences p;
    p.begin("auth", true);
    authToken = p.getString("token", "");
    p.end();
    if (authToken.length() != 32) {
        char buf[33];
        randomHex(buf, 16);
        authToken = buf;
        persistAuthToken();
        DevLog.println("[auth] token initialized in NVS");
    } else {
        DevLog.println("[auth] token loaded from NVS");
    }
    setOtaPassword(authToken.c_str());
}

static void rotateAuthToken() {
    char buf[33];
    randomHex(buf, 16);
    authToken = buf;
    persistAuthToken();
    setOtaPassword(authToken.c_str());
    DevLog.println("[auth] token rotated");
}

static bool authValid() {
    // No expiry: the token lives in NVS across reboots and deep sleep.
    return authToken.length() > 0;
}

// Wi-Fi operations require a token that was negotiated over the bonded BLE
// link; accepted as "Authorization: Bearer <token>" or ?token=<token>.
static bool requestAuthorized() {
    if (!authValid()) return false;
    if (server.hasHeader("Authorization")) {
        if (server.header("Authorization") == String("Bearer ") + authToken) return true;
    }
    if (server.hasArg("token") && server.arg("token") == authToken) return true;
    return false;
}

static void handleBleAuth(const String &json) {
    JsonDocument doc;
    if (deserializeJson(doc, json) || doc["cmd"].isNull() || strcmp(doc["cmd"] | "", "token")) {
        bleNotifyStatusQuiet("{\"ack\":\"auth\",\"ok\":false}");
        return;
    }
    if (doc["rotate"] | false) rotateAuthToken();
    else if (!authValid()) loadOrIssueAuthToken();
    holdWindow(WINDOW_MAX_MS);
    bleNotifyStatusQuiet(String("{\"ack\":\"auth\",\"ok\":true,\"token\":\"") + authToken + "\"}");
}

static void handleUpdatePage() {
    if (!requestAuthorized()) {
        server.send(401, "text/plain", "unauthorized: negotiate a token over BLE first");
        return;
    }
    String page = F("<!DOCTYPE html><html><head><meta charset='utf-8'><title>OTA</title></head><body>"
                    "<h2>Firmware OTA</h2>"
                    "<form method='POST' action='/doUpdate?token=");
    page += authToken;
    page += F("' enctype='multipart/form-data'>"
              "<input type='file' name='firmware' accept='.bin'>"
              "<button type='submit'>Upload</button></form></body></html>");
    server.send(200, "text/html", page);
}

static void startNormalMode() {
    configMode = false;
    hostname = "codex-status-" + macSuffix();
    configTzTime("CST-8", "pool.ntp.org");
    pinMode(0, INPUT_PULLUP);
    pinMode(18, INPUT_PULLUP);

    windowMode = true;
    bool haveWifi = connectBest();
    windowHadWifi = haveWifi;
    if (haveWifi) {
        uint32_t h = fnv1a(WiFi.BSSIDstr());
        if (h && h != rtcBssidHash) {
            rtcBssidHash = h;
            rtcFastLeft = 3;   // accelerated windows after a network change
            windowEnvSwitch = true;
        }
    } else {
        DevLog.println("[wifi] window without Wi-Fi");
    }

    bleBegin("CodexStatus-" + macSuffix(), FW_VERSION);
    bleSetHandlers(handleBleUsage, handleBleEndpoint);
    bleSetTemplateHandlers(tplXferHandleCtrl, tplXferHandleChunk, tplXferReset);
    bleSetAuthHandler(handleBleAuth);
    updateInfoExtra();

    server.on("/", HTTP_GET, handleStatus);
    server.on("/status.json", HTTP_GET, handleStatusJson);
    server.on("/log", HTTP_GET, handleLog);
    server.on("/usage", HTTP_POST, handleUsagePost);
    server.on("/sleep", HTTP_POST, handleSleepPost);
    server.on("/update", HTTP_GET, handleUpdatePage);
    server.on("/doUpdate", HTTP_POST,
        []() {
            server.sendHeader("Connection", "close");
            if (otaUploadDenied) {
                server.send(401, "text/plain", "unauthorized: negotiate a token over BLE first");
                return;
            }
            server.send(200, "text/plain", Update.hasError() ? "UPDATE FAILED" : "UPDATE OK");
        },
        []() {
            HTTPUpload &up = server.upload();
            if (up.status == UPLOAD_FILE_START) {
                if (!requestAuthorized()) {
                    otaUploadDenied = true;
                    DevLog.println("[ota] rejected: unauthorized");
                    return;
                }
                otaUploadDenied = false;
                otaInProgress = true;
                windowDeadline = millis() + WINDOW_HOLD_MS;
                DevLog.printf("[ota] upload start: %s\n", up.filename.c_str());
                std::vector<String> lines = {"OTA update", up.filename};
                screen(lines);
                if (!Update.begin(UPDATE_SIZE_UNKNOWN)) Update.printError(Serial);
            } else if (up.status == UPLOAD_FILE_WRITE) {
                if (otaUploadDenied) return;
                if (Update.write(up.buf, up.currentSize) != up.currentSize) Update.printError(Serial);
            } else if (up.status == UPLOAD_FILE_END) {
                otaInProgress = false;
                if (otaUploadDenied) return;
                if (Update.end(true)) {
                    DevLog.printf("[ota] success %u bytes, rebooting shortly\n", (unsigned)up.totalSize);
                    screen({"OTA success", "Rebooting..."});
                    otaRebootPending = true;
                    otaRebootAt = millis() + 1500;
                } else {
                    Update.printError(Serial);
                }
            }
        });
    server.begin();

    if (haveWifi) {
        ArduinoOTA.setHostname(hostname.c_str());
        loadOrIssueAuthToken();
        ArduinoOTA.onStart([]() { screen({"ArduinoOTA", "updating..."}); });
        ArduinoOTA.onProgress([](unsigned int p, unsigned int t) {
            DevLog.printf("[ota] %u%%\r", t ? p * 100 / t : 0);
        });
        ArduinoOTA.onEnd([]() { screen({"OTA OK", "rebooting..."}); delay(800); });
        ArduinoOTA.onError([](ota_error_t e) { DevLog.printf("[ota] error %u\n", e); });
        ArduinoOTA.begin();
        MDNS.addService("http", "tcp", 80);
    }

    DevLog.printf("[net] ready: http://%s/  host=%s.local  endpoints=%d mode=%s wifi=%d\n",
                  WiFi.localIP().toString().c_str(), hostname.c_str(), storeCount(),
                  MODE_NAMES[runtimeMode()], haveWifi ? 1 : 0);

    windowDeadline = millis() + WINDOW_MS;
    windowHardStop = millis() + WINDOW_MAX_MS;
    windowSynced = false;
    if (haveWifi && storeCount() > 0) tryWifiUsage();
}

void setup() {
    esp_sleep_wakeup_cause_t cause = esp_sleep_get_wakeup_cause();
    bootWakeCause = cause;
    bool woke = (cause == ESP_SLEEP_WAKEUP_TIMER || cause == ESP_SLEEP_WAKEUP_EXT1);
    hostname = "codex-status-" + macSuffix();
    epdBegin(!woke);
    if (!woke) screen({"CODEX STATUS", FW_VERSION, "booting..."});
    if (rtcMagic != 0xC0DE0001) {
        rtcMagic = 0xC0DE0001;
        rtcActiveAt = 0;
        rtcActiveMac[0] = 0;
        rtcFailCount = 0;
        rtcFastLeft = 3;
        rtcIdleReason = IDLE_BOOT;
        rtcNeverSynced = true;
        rtcBssidHash = 0;
        rtcUsageHash = 0;
        DevLog.println("[pm] RTC state initialized");
    }
    const esp_partition_t *running = esp_ota_get_running_partition();
    DevLog.printf("\n[codex-status] v%s mac=%s reset=%s slot=%s mode=%s wake=%d(%s)\n", FW_VERSION,
                  WiFi.macAddress().c_str(), resetReasonName(),
                  running ? running->label : "?", MODE_NAMES[runtimeMode()], (int)cause,
                  wakeCauseName(cause));
    { Preferences p; p.begin("brg", false); p.end(); }

    tplStoreBegin();
    tplXferBegin(FW_VERSION, []() { pendingTplChanged = true; });

    if (hasWifiSlots()) {
        startNormalMode();
    } else {
        DevLog.println("[config] no saved Wi-Fi slots; entering AP mode (D10)");
        startConfigMode();
    }
}

// LIVE entry/exit (sleep.md §4.1/§4.3). Plan B keeps the Wi-Fi association
// with modem sleep on the stock core; custom-core PM light sleep is a later
// optimization gated by the T10 current measurement.
static void enterLive() {
    liveMode = true;
    windowMode = false;
    liveEnteredAtMs = millis();
    liveLastSyncMs = millis();
    liveLastPollMs = millis();
    wifiLostSinceMs = 0;
    WiFi.setSleep(true);
    bleAdvertiseStop();
    DevLog.println("[live] enter (modem sleep, BLE advertising off)");
}

static void exitLive(uint8_t reason) {
    rtcIdleReason = reason;
    liveMode = false;
    String cached;
    if (usageCacheLoad(cached)) renderActiveUsage(cached, "DEEP", true);
    else screenIdle();
    DevLog.printf("[live] exit (%s)\n", idleReasonText());
    deepSleepFor(300, false);
}

// Window finished: render IDLE when required, update the failure/backoff state
// and deep-sleep until the next window (sleep.md §4.1/§4.2).
static void finishWindowAndSleep(bool haveWifi) {
    if (windowSynced) {
        rtcFailCount = 0;
        rtcNeverSynced = false;
        if (haveWifi) {
            enterLive();
            return;
        }
    } else {
        if (rtcFailCount < 255) rtcFailCount++;
        if (windowEnvSwitch) rtcIdleReason = IDLE_ENV_SWITCH;
        else if (rtcNeverSynced) rtcIdleReason = IDLE_BOOT;
        else if (!haveWifi) rtcIdleReason = IDLE_WIFI_LOST;
        else rtcIdleReason = IDLE_BRIDGE_LOST;
    }
    bool showIdle = !windowSynced && (rtcFailCount >= 2 || rtcNeverSynced);
    if (showIdle) {
        String cached;
        if (usageCacheLoad(cached)) renderActiveUsage(cached, "DEEP", true);
        else screenIdle();
    }
    uint32_t next;
    if (rtcFailCount >= 3) {
        next = 900;
    } else if (rtcFastLeft > 0) {
        next = 60;
        rtcFastLeft--;
    } else {
        next = 300;
    }
    DevLog.printf("[pm] window done synced=%d wifi=%d fail=%u idle=%d reason=%s next=%us\n",
                  windowSynced ? 1 : 0, haveWifi ? 1 : 0, (unsigned)rtcFailCount,
                  showIdle ? 1 : 0, idleReasonText(), (unsigned)next);
    deepSleepFor(next, false);
}

// USB serial provisioning: `wifi <ssid> <pass>` saves to NVS and reboots;
// `status` prints IP/RSSI/heap; `batt` prints battery percent.
static void handleSerialCli() {
    static String rx;
    while (Serial.available()) {
        char c = (char)Serial.read();
        if (c == '\r') continue;
        if (c != '\n') {
            if (rx.length() < 200) rx += c;
            continue;
        }
        String line = rx;
        rx = "";
        line.trim();
        if (line.startsWith("wifi ")) {
            int sp = line.indexOf(' ', 5);
            if (sp > 5) {
                String ssid = line.substring(5, sp);
                String pass = line.substring(sp + 1);
                prefs.begin("wifi", false);
                int slot = -1;
                for (int i = 0; i < MAX_SLOTS; i++) {
                    if (!prefs.getString(("s" + String(i)).c_str(), "").length()) { slot = i; break; }
                }
                if (slot < 0) slot = 0;
                prefs.putString(("s" + String(slot)).c_str(), ssid);
                prefs.putString(("p" + String(slot)).c_str(), pass);
                prefs.putUChar("last", (uint8_t)slot);
                prefs.end();
                DevLog.printf("[cli] wifi saved slot %d ssid=%s, rebooting\n", slot, ssid.c_str());
                screen({"Wi-Fi saved via USB:", ssid, "", "Rebooting..."});
                delay(800);
                ESP.restart();
            } else {
                DevLog.println("[cli] usage: wifi <ssid> <pass>");
            }
        } else if (line == "status") {
            DevLog.printf("[cli] fw=%s ip=%s rssi=%d heap=%u\n",
                          FW_VERSION, ipText().c_str(), WiFi.RSSI(), ESP.getFreeHeap());
        } else if (line == "batt") {
            DevLog.printf("[cli] battery=%d%%\n", batteryPercent());
        } else if (line == "mode") {
            DevLog.printf("[cli] mode=%s\n", MODE_NAMES[runtimeMode()]);
        } else if (line.startsWith("mode ")) {
            String v = line.substring(5);
            v.trim();
            int m = -1;
            for (int i = 0; i < 3; i++)
                if (v == MODE_NAMES[i]) m = i;
            if (m < 0) {
                DevLog.println("[cli] usage: mode auto|deep|live");
            } else {
                setRuntimeMode((uint8_t)m);
                DevLog.printf("[cli] mode=%s%s\n", MODE_NAMES[m],
                              m == 2 ? " (live == deep until M3)" : "");
            }
        } else if (line == "pair") {
            bleOpenPairingWindow(120000);
            bleAdvertiseStart();
            DevLog.println("[cli] pairing window open 120s");
        } else if (line == "sleep" || line.startsWith("sleep ")) {
            uint32_t sec = line.length() > 6 ? (uint32_t)line.substring(6).toInt() : 60;
            if (sec < 15) sec = 15;
            if (sec > 900) sec = 900;
            DevLog.printf("[cli] sleep %us\n", (unsigned)sec);
            delay(100);
            deepSleepFor(sec, true);
        } else if (line.length()) {
            DevLog.println("[cli] commands: wifi <ssid> <pass> | status | batt | mode [auto|deep|live] | sleep [sec] | pair");
        }
    }
}

// PWR (GPIO18) held for 3 s: software power-off by dropping the VBAT latch
// (GPIO17). On battery the MCU dies here; if USB/charger power keeps the board
// alive the hold degrades to a clean restart so the device never sits inert.
// (HWCDC's isPlugged() is not reliable after a cable unplug, so it is not used
// as a gate.)
static void powerOff() {
    DevLog.println("[pm] PWR held 3s: power off");
    epdPanelSleep();
    bleAdvertiseStop();
    WiFi.disconnect(true);
    delay(200);
    digitalWrite(17, LOW);
    delay(3000);
    DevLog.println("[pm] latch dropped but still powered (USB); restarting");
    ESP.restart();
}

void loop() {
    if (otaRebootPending && (int32_t)(millis() - otaRebootAt) >= 0) ESP.restart();
    handleSerialCli();
    server.handleClient();
    blePoll();

    static uint32_t pwrDownAt = 0;
    static bool pwrHandled = false;
    static uint32_t pwrEnableAt = 0;
    if (!pwrEnableAt) pwrEnableAt = millis() + 5000;   // boot grace: PWR just powered the board on
    if (digitalRead(18) == LOW) {
        if (!pwrDownAt) pwrDownAt = millis();
        else if (!pwrHandled && millis() - pwrDownAt > 3000 &&
                 (int32_t)(millis() - pwrEnableAt) >= 0) {
            pwrHandled = true;
            powerOff();
        }
    } else {
        pwrDownAt = 0;
        pwrHandled = false;
    }

    if (configMode) {
        // D10: AP provisioning sleeps again after 5 idle minutes.
        if (millis() - configStartedAt > CONFIG_IDLE_MS) {
            DevLog.println("[config] idle timeout, sleeping");
            deepSleepFor(300, false);
        }
        delay(5);
        return;
    }

    ArduinoOTA.handle();

    static uint32_t bootDownAt = 0;
    static int      bootStage = 0;
    if (digitalRead(0) == LOW) {
        if (!bootDownAt) bootDownAt = millis();
        uint32_t held = millis() - bootDownAt;
        if (bootStage < 1 && held > 2000) {
            bootStage = 1;
            bleOpenPairingWindow(120000);
            bleAdvertiseStart();   // LIVE keeps BLE off until pairing is asked for
            pairingOverlay = true;
            screenPairingOverlay();
        }
        if (bootStage < 2 && held > 5000) {
            bootStage = 2;
        }
        if (bootStage < 3 && held > 10000) {
            bootStage = 3;
            pairingOverlay = false;
            factoryReset();
        }
    } else {
        if (bootDownAt) {
            uint32_t held = millis() - bootDownAt;
            if (bootStage == 0 && held > 50 && held < 1500) nextTemplate();
            else if (bootStage == 2 && held >= 5000 && held < 10000) {
                DevLog.println("[config] BOOT held 5s; entering AP mode (D10)");
                startConfigMode();
                return;
            }
            bootDownAt = 0;
            bootStage = 0;
        }
    }

    if (pairingOverlay) {
        bool paired = bleIsConnected() && blePeerIsBonded() && blePeerIsEncrypted();
        if (!blePairingWindowOpen() || paired) {
            pairingOverlay = false;
            activeTplId = "";
            if (lastUsage.length()) renderActiveUsage(lastUsage, lastChannel.c_str());
            else screenIdle();
        }
    }

    if (pendingTplChanged) {
        pendingTplChanged = false;
        activeTplId = "";
        updateInfoExtra();
        if (lastUsage.length()) renderActiveUsage(lastUsage, lastChannel.c_str());
        else screenIdle();
    }
    if (pendingEndpoint) {
        pendingEndpoint = false;
        if (WiFi.status() == WL_CONNECTED) tryWifiUsage();
    }
    if (pendingUsageReady) {
        pendingUsageReady = false;
        JsonDocument parsed;
        if (!deserializeJson(parsed, pendingUsage) && !parsed.as<JsonObject>().isNull()) {
            adoptServerTime(parsed);
            applyEnvelopeMeta(parsed);
            bool explicitActivate = parsed["activate"] | false;
            String mac = blePeerAddress();
            bool accepted = usageAccepted(mac, explicitActivate);
            lastSyncMs = millis();
            lastOkMs = millis();
            lastSyncEpoch = time(nullptr);
            windowSynced = true;
            usageCacheSave(pendingUsage);
            if (accepted) {
                setActiveMac(mac);
                rtcActiveAt = timeKnown() ? (uint32_t)time(nullptr) : 0;
                lastUsage = pendingUsage;
                lastChannel = pendingChannel;
                renderActiveUsage(lastUsage, lastChannel.c_str());
            } else {
                DevLog.printf("[usage] BLE usage ignored (active=%s)\n", rtcActiveMac);
            }
        } else {
            DevLog.println("[usage] BLE usage rejected: invalid JSON");
        }
    }

    // LIVE: the bridge pushes; watchdog falls back to DEEP (sleep.md §4.1).
    if (liveMode) {
        if (WiFi.status() == WL_CONNECTED) {
            wifiLostSinceMs = 0;
        } else if (!wifiLostSinceMs) {
            wifiLostSinceMs = millis();
        } else if (millis() - wifiLostSinceMs > 180000UL) {
            exitLive(IDLE_WIFI_LOST);
        }
        if (liveLastSyncMs && millis() - liveLastSyncMs > 600000UL) {
            exitLive(IDLE_BRIDGE_LOST);
        }
        if (millis() - liveLastPollMs > 900000UL) {
            liveLastPollMs = millis();
            if (storeCount() > 0) tryWifiUsage();
        }
        delay(5);
        return;
    }

    // An OTA upload holds the window open (bounded recovery safeguard).
    if (otaInProgress) holdWindow(WINDOW_HOLD_MS);
    // Pairing/token operations and live BLE peers also keep the window open.
    if (windowMode && !otaInProgress && !bleIsConnected() && !blePairingWindowOpen() &&
        !pairingOverlay && (int32_t)(millis() - windowDeadline) >= 0) {
        finishWindowAndSleep(windowHadWifi || WiFi.status() == WL_CONNECTED);
    }

    delay(5);
}
