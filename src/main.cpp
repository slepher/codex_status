/*
 * Codex Status - 0.12.6 single-mode runtime
 * Wi-Fi 主通道（HTTP 推送/轮询/模板）+ 按需 BLE 会话（身份/token）；空闲 light sleep。
 * 状态机权威文档：docs/power-state.md
 */

#include <Arduino.h>
#include <WiFi.h>
#include <WiFiUdp.h>
#include <WebServer.h>
#include <Update.h>
#include <ArduinoOTA.h>
#include <Preferences.h>
#include <ESPmDNS.h>
#include <ArduinoJson.h>
#include <time.h>
#include <sys/time.h>
#include <string.h>
#include <stdio.h>
#include <vector>
#include <esp_sleep.h>
#include <esp_ota_ops.h>
#include <esp_system.h>
#include <esp_pm.h>
#include <esp_wifi.h>
#include <esp_mac.h>
#include <esp_private/pm_impl.h>
#include <driver/rtc_io.h>
#include <driver/gpio.h>
#include <driver/usb_serial_jtag.h>

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

#define FW_VERSION    "0.13.0-bw"
#define AP_PASSWORD   "codex1234"
#define MAX_SLOTS     3

static const int EPD_W = EPD_SSD1681_WIDTH;
static const int EPD_H = EPD_SSD1681_HEIGHT;
static const int EPD_FB_BYTES = (EPD_W / 8) * EPD_H;

#define ACTIVE_HOLD_S    600
#define CONFIG_IDLE_MS   (5UL * 60UL * 1000UL)   // AP idle sleep (battery)
#define WIFI_CONNECT_MS  30000UL                 // boot connect attempt
#define WIFI_LOST_MS     30000UL                 // link-loss declaration
#define WIFI_RETRY_MS    60000UL                 // plugged retry cadence
#define BLE_GRACE_MS     120000UL                // BLE keep-alive after last use
#define BATT_CHECK_MS    (5UL * 60UL * 1000UL)
#define ANNOUNCE_MS      (5UL * 60UL * 1000UL)   // UDP announce heartbeat
#define BRIDGE_LOST_MIN  6                       // bridge heartbeat 5 min + margin
#define LOW_BATT_PCT     5
#define BLE_AUTO_PCT     20
#define STORE_MAX_LOCAL  8

static Preferences prefs;
static WebServer   server(80);
static WiFiUDP     announceUdp;
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
static esp_pm_lock_handle_t otaPmLock = nullptr;
static bool        otaLockHeld = false;

RTC_DATA_ATTR static uint32_t rtcMagic = 0;
RTC_DATA_ATTR static uint32_t rtcActiveAt = 0;
RTC_DATA_ATTR static char     rtcActiveMac[20] = {0};
RTC_DATA_ATTR static uint32_t rtcLastSyncEpoch = 0;
RTC_DATA_ATTR static uint8_t  rtcRetryStage = 0;
RTC_DATA_ATTR static uint32_t rtcUsageHash = 0;
// Why AP config mode was entered last (0 none, 1 no saved slots, 2 BOOT held
// 15 s). Kept in RTC memory for post-mortem /status.json reads.
RTC_DATA_ATTR static uint8_t  rtcApReason = 0;
static const char *AP_REASON_NAMES[] = {"none", "no_slots", "boot_hold"};

static bool     otaInProgress = false;
static uint32_t configStartedAt = 0;
static uint32_t activeHoldSec = ACTIVE_HOLD_S;
static bool     pmLightSleep = false;

// v0.12 runtime state (docs/power-state.md §3-§6)
static bool     plugged = false;          // PC USB host present (SOF)
static bool     wifiUp = false;           // STA associated
static bool     wifiLostHandled = false;  // WIFI OFF policy already applied
static bool     wifiReconfiguring = false; // deliberate power-save reassociation
static uint32_t wifiReconfigDeadline = 0;
static uint32_t wifiLostSince = 0;
static uint32_t nextWifiRetry = 0;
static String   wifiSsid, wifiPass;
static bool     bleOn = false;
static bool     bleUserOff = false;       // user switched BLE off while docked
static bool     lastBleAuto = false;      // keep-alive came from plug+charge
static uint32_t bleOffDeadline = 0;
static int      batteryPct = -1;
static uint32_t lastBattCheck = 0;
static uint32_t announcedIp = 0;
static uint32_t lastAnnounce = 0;
static uint32_t lastBootAction = 0;
static uint32_t ledPulseUntil = 0;
static bool     ledState = false;         // logical LED: BLE ON

// LIVE power management: PM dynamic frequency scaling (240/40 MHz) plus
// automatic light sleep. Requires CONFIG_PM_ENABLE and
// CONFIG_FREERTOS_USE_TICKLESS_IDLE from the custom sdkconfig.
static void configurePowerManagement() {
#ifdef CODEX_PM
    esp_pm_config_t cfg = {};
    cfg.max_freq_mhz = 240;
    cfg.min_freq_mhz = 40;
    cfg.light_sleep_enable = true;
    esp_err_t err = esp_pm_configure(&cfg);
    pmLightSleep = (err == ESP_OK);
    DevLog.printf("[pm] esp_pm_configure(light_sleep=1, 240/40MHz): %s\n", esp_err_to_name(err));
#else
    DevLog.println("[pm] stock core: PM light sleep not compiled in");
#endif
}

static void setupOtaPmLock() {
#ifdef CODEX_PM
    if (!otaPmLock) {
        esp_pm_lock_create(ESP_PM_NO_LIGHT_SLEEP, 0, "ota", &otaPmLock);
    }
#endif
}

static void setOtaLock(bool held) {
    if (!otaPmLock || held == otaLockHeld) return;
    otaLockHeld = held;
    if (held) esp_pm_lock_acquire(otaPmLock);
    else      esp_pm_lock_release(otaPmLock);
    DevLog.printf("[pm] OTA NO_LIGHT_SLEEP %s\n", held ? "acquired" : "released");
}

// CONFIG_PM_SLP_DISABLE_GPIO floats every pad during automatic light sleep.
// The VBAT latch (GPIO17 high), the panel power enable (GPIO6 low = panel
// powered) and the audio amp power (GPIO42 low = off) must keep their levels,
// so they opt out of the sleep switch.
static void retainSleepCriticalGpio() {
    gpio_sleep_sel_dis(GPIO_NUM_17);
    gpio_sleep_sel_dis(GPIO_NUM_6);
    gpio_sleep_sel_dis(GPIO_NUM_42);
    DevLog.println("[pm] GPIO sleep retention: 17/6/42");
}

static uint32_t fnv1a(const String &s) {
    uint32_t h = 2166136261u;
    for (size_t i = 0; i < s.length(); i++) {
        h ^= (uint8_t)s[i];
        h *= 16777619u;
    }
    return h;
}

static bool timeKnown() { return time(nullptr) > 1600000000; }

static byte hostMac[6] = {0};
static void loadHostMac() {
    esp_read_mac(hostMac, ESP_MAC_WIFI_STA);
}

static String macSuffix() {
    // Read the base MAC straight from eFuse: WiFi.macAddress() needs the Wi-Fi
    // driver (and NVS) up, which is not true yet at BLE init / AP startup.
    char buf[8];
    snprintf(buf, sizeof(buf), "%02X%02X%02X", hostMac[3], hostMac[4], hostMac[5]);
    return String(buf);
}

static String macText() {
    char buf[18];
    snprintf(buf, sizeof(buf), "%02X:%02X:%02X:%02X:%02X:%02X",
             hostMac[0], hostMac[1], hostMac[2], hostMac[3], hostMac[4], hostMac[5]);
    return String(buf);
}

static const char *deviceStateText() {
    if (configMode) return "AP";
    if (!wifiUp)    return "WIFI OFF";
    if (bleOn)      return "BLE ON";
    return "BLE OFF";
}

static const char *wifiStateText() {
    if (configMode) return "ap";
    if (wifiUp)     return "connected";
    return wifiLostHandled ? "lost" : "connecting";
}

static void setActiveMac(const String &mac) {
    strncpy(rtcActiveMac, mac.c_str(), sizeof(rtcActiveMac) - 1);
    rtcActiveMac[sizeof(rtcActiveMac) - 1] = '\0';
}

static uint32_t lastPersistedSync = 0;

static void markSynced() {
    rtcLastSyncEpoch = timeKnown() ? (uint32_t)time(nullptr) : 0;
    rtcRetryStage = 0;
    if (rtcLastSyncEpoch == 0) return;
    // RTC data does not survive the OTA software reset on this board, so keep
    // the last-sync epoch in NVS too (throttled: at most one write per minute).
    if (lastPersistedSync && rtcLastSyncEpoch - lastPersistedSync < 60) return;
    Preferences p;
    p.begin("sync", false);
    p.putUInt("last", rtcLastSyncEpoch);
    p.end();
    lastPersistedSync = rtcLastSyncEpoch;
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

// A sync is accepted from any bridge while there is no active bridge, from the
// active bridge itself, after the active hold window, or when the active
// endpoint's BSSID no longer matches the current one.
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

static String activeTplJson;
static String activeTplId;

static String lastUsage;
static String lastChannel = "-";
static int      epdPartialCount = 0;
static bool     epdPartialReady = false;
static bool     epdAsleep = false;

static void screen(const std::vector<String> &lines, UBYTE color = BLACK);
static void epdFlush(bool forceFull = false);
static int  batteryPercent();
static bool requestAuthorized();
static void deepSleepFor(uint32_t sec);
static void renderCurrent();
static void enterBleOn(bool userInitiated);
static void bleOff(const char *reason);
static void requestAnnounce(bool bleFlag);
static void handleBleUsage(const String &json);
static void handleBleEndpoint(const String &json);
static void handleBleAuth(const String &json);
static String fmtEpoch(long long ts, const char *fmt);
static void powerOff();
static bool configureWifiPowerSave();

static esp_sleep_wakeup_cause_t bootWakeCause = ESP_SLEEP_WAKEUP_UNDEFINED;

static String ipText() {
    if (configMode) return "192.168.4.1";
    if (WiFi.status() == WL_CONNECTED) return WiFi.localIP().toString();
    return "-";
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

// Status screens wrap long text (OTA file names) instead of truncating it;
// glyph tables are ASCII-only, so non-printable bytes become '?'.
static std::vector<String> wrapText(const String &s, size_t maxChars = 17) {
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

static void screenStatus() {
    std::vector<String> lines;
    lines.push_back("CODEX STATUS");
    lines.push_back(deviceStateText());
    lines.push_back(String("BATT ") + batteryPercent() + "%");
    lines.push_back(String("IP ") + ipText());
    lines.push_back(String("SYNC ") + fmtEpoch((long long)rtcLastSyncEpoch, "%H:%M"));
    lines.push_back(String("FW ") + FW_VERSION);
    screen(lines);
}

static void screen(const std::vector<String> &lines, UBYTE color) {
    if (!frame) return;
    Paint_SelectImage(frame);
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
    retainSleepCriticalGpio();
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

static void renderActiveUsage(const String &json, const char *channel) {
    if (!frame) return;
    if (!tplCacheLoad()) { screenStatus(); return; }
    TplEnv env;
    env.channel  = channel ? channel : "";
    env.ip       = ipText();
    env.syncHHMM = nowHHMM();
    env.battery  = batteryPercent();
    env.state    = deviceStateText();
    env.offlineMins = -1;
    if (rtcLastSyncEpoch > 1600000000 && timeKnown()) {
        long mins = ((long)time(nullptr) - (long)rtcLastSyncEpoch) / 60;
        // The row means "bridge unreachable": the bridge pushes at least every
        // 5 minutes, so only expose the value once contact is clearly lost.
        if (mins > BRIDGE_LOST_MIN) env.offlineMins = (int)mins;
    }
    Paint_SelectImage(frame);
    Paint_Clear(WHITE);
    if (tplDraw(activeTplJson, json, env)) {
        epdFlush(false);
        DevLog.printf("[tpl] rendered %s (%s)\n", activeTplId.c_str(), channel ? channel : "");
        return;
    }
    DevLog.printf("[tpl] %s invalid/incomplete, built-in fallback\n", activeTplId.c_str());
    if (json.length()) renderUsage(json, channel);
    else screenStatus();
}

static void renderCurrent() {
    if (lastUsage.length()) renderActiveUsage(lastUsage, lastChannel.c_str());
    else screenStatus();
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

// Envelope metadata (docs/power-state.md §9): active-hold tuning plus endpoint
// self-heal. The bridge advertises its own host/port; the endpoint MAC comes
// from the authenticated sender, so only a known bridge can update its record.
static void applyEnvelopeMeta(JsonDocument &doc, const String &mac) {
    long long hold = doc["active_hold_seconds"] | 0LL;
    if (hold >= 60 && hold <= 86400) activeHoldSec = (uint32_t)hold;
    JsonObject bridge = doc["bridge"].as<JsonObject>();
    const char *host = bridge["host"] | "";
    int port = bridge["port"] | 0;
    if (!mac.length() || !strlen(host) || port <= 0 || port > 65535) return;
    for (int i = 0; i < storeCount(); i++) {
        EndpointRec rec;
        if (!storeGet(i, rec)) continue;
        if (rec.mac != mac) continue;
        if (rec.host != host || rec.port != (uint16_t)port) {
            storeUpsert(rec.mac, host, (uint16_t)port, rec.token);
            DevLog.printf("[brg] endpoint self-heal %s:%d\n", host, port);
        }
        return;
    }
}

// Endpoint selection: when the active bridge synced recently, only try its
// endpoint; otherwise try same-BSSID endpoints first (MRU), then the rest.
// Per-endpoint timeout is 2 s.
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
        applyEnvelopeMeta(parsed, rec.mac);
        bool explicitActivate = parsed["activate"] | false;
        bool accepted = usageAccepted(rec.mac, explicitActivate);
        storeTouch(rec.mac);
        if (bssid.length()) storeSetBssid(rec.mac, bssid);
        markSynced();
        DevLog.printf("[wifi] usage from %s:%u accepted=%d\n",
                      rec.host.c_str(), rec.port, accepted ? 1 : 0);
        bleNotifyStatus("{\"ack\":\"wifi-usage\",\"ok\":true}");
        lastUsage = out;
        lastChannel = "WIFI";
        usageCacheSave(out);
        if (accepted) {
            setActiveMac(rec.mac);
            rtcActiveAt = timeKnown() ? (uint32_t)time(nullptr) : 0;
            renderActiveUsage(lastUsage, "WIFI");
        }
        return true;
    }
    return false;
}

// POST /usage from the bridge. Auth uses the endpoint token that the bridge
// itself wrote over BLE; the matching record identifies the bridge MAC.
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
    applyEnvelopeMeta(parsed, mac);
    bool explicitActivate = parsed["activate"] | false;
    bool accepted = usageAccepted(mac, explicitActivate);
    markSynced();
    usageCacheSave(body);
    if (accepted) {
        setActiveMac(mac);
        rtcActiveAt = timeKnown() ? (uint32_t)time(nullptr) : 0;
        lastUsage = body;
        lastChannel = "PUSH";
        renderActiveUsage(lastUsage, "PUSH");
    } else {
        DevLog.printf("[wifi] push ignored (active=%s)\n", rtcActiveMac);
    }
    server.send(200, "application/json",
                accepted ? "{\"accepted\":true}" : "{\"accepted\":false}");
}

static bool tplIdValid(const String &id) {
    if (id.length() == 0 || id.length() > 16) return false;
    for (size_t i = 0; i < id.length(); i++) {
        char c = id[i];
        if (!isalnum((unsigned char)c) && c != '_' && c != '-') return false;
    }
    return true;
}

// POST /template (docs/power-state.md §5/§9): templates travel over HTTP as an
// explicit user/agent action; BLE only carries identity (pairing, endpoint,
// tokens). Gated by the endpoint token the bridge wrote over BLE; `hash` in the
// query is the CRC32 of the raw body, validated together with min_fw/dry-run by
// tplValidateForStorage (never partially rendered).
static void handleTemplatePost() {
    String mac;
    if (!endpointTokenAuthorized(mac)) {
        server.send(401, "application/json", "{\"saved\":false,\"err\":\"unauthorized\"}");
        return;
    }
    if (server.clientContentLength() > 32768) {
        server.send(413, "application/json", "{\"saved\":false,\"err\":\"too_large\"}");
        return;
    }
    String id = server.arg("id");
    String hash = server.arg("hash");
    uint32_t version = server.arg("version").toInt();
    bool activate = server.hasArg("activate") && server.arg("activate") != "0";
    String body = server.arg("plain");
    if (!tplIdValid(id) || !body.length()) {
        server.send(400, "application/json", "{\"saved\":false,\"err\":\"args\"}");
        return;
    }
    String err;
    if (!tplValidateForStorage(body, hash, FW_VERSION, err)) {
        DevLog.printf("[tpl] http reject %s: %s\n", id.c_str(), err.c_str());
        server.send(400, "application/json",
                    String("{\"saved\":false,\"err\":\"") + err + "\"}");
        return;
    }
    TplMeta existing;
    bool unchanged = tplStoreFind(id, existing) && existing.hash == hash;
    if (!unchanged &&
        !tplStoreSave(id, version, hash, (const uint8_t *)body.c_str(), body.length())) {
        server.send(500, "application/json", "{\"saved\":false,\"err\":\"save\"}");
        return;
    }
    bool activated = false;
    if (activate) {
        tplStoreSetActive(id);
        tplStoreTouch(id);
        activated = true;
    }
    if (!unchanged || activated) {
        activeTplId = "";
        updateInfoExtra();
        renderCurrent();
    }
    DevLog.printf("[tpl] http %s id=%s hash=%s%s\n",
                  unchanged ? "unchanged" : "saved", id.c_str(), hash.c_str(),
                  activated ? " (activated)" : "");
    server.send(200, "application/json",
                String("{\"saved\":true,\"activated\":") + (activated ? "true" : "false") +
                    ",\"unchanged\":" + (unchanged ? "true" : "false") +
                    ",\"id\":\"" + id + "\"}");
}

// Deep-sleep wake sources: RTC timer plus BOOT (GPIO0) and PWR (GPIO18),
// active-low. The RTC pull-ups are armed explicitly so the buttons stay
// readable once the RTC domain is the only powered island.
static void armWakeSources(uint64_t timerUs) {
    rtc_gpio_pullup_en(GPIO_NUM_0);
    rtc_gpio_pulldown_dis(GPIO_NUM_0);
    rtc_gpio_pullup_en(GPIO_NUM_18);
    rtc_gpio_pulldown_dis(GPIO_NUM_18);
    esp_sleep_enable_ext1_wakeup((1ULL << 0) | (1ULL << 18), ESP_EXT1_WAKEUP_ANY_LOW);
    if (timerUs) esp_sleep_enable_timer_wakeup(timerUs);
}

// The only deep-sleep paths in v0.12: WIFI OFF on battery (timed retry) and AP
// without credentials on battery (timerUs=0: buttons only). Low battery calls
// powerOff() instead.
static void deepSleepFor(uint32_t sec) {
    epdPanelSleep();
    if (bleInitialized()) {
        bleAdvertiseStop();
        bleDeinit();
    }
    WiFi.disconnect(true);
    armWakeSources(sec ? (uint64_t)sec * 1000000ULL : 0);
    DevLog.printf("[pm] deep sleep %us\n", (unsigned)sec);
    esp_deep_sleep_start();
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

static int countWifiSlots() {
    prefs.begin("wifi", true);
    int n = 0;
    for (int i = 0; i < MAX_SLOTS; i++) {
        if (prefs.getString(("s" + String(i)).c_str(), "").length()) n++;
    }
    prefs.end();
    return n;
}

static bool hasWifiSlots() { return countWifiSlots() > 0; }

// Scan once and connect to the saved slot with the best signal; the last-used
// slot wins near-ties. When no saved network is visible, fall back to the
// last-used slot so the 60 s retry cadence has credentials to retry with.
static bool connectBest() {
    WiFi.mode(WIFI_STA);
    WiFi.setHostname(hostname.c_str());
    int n = WiFi.scanNetworks();
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
        if (last < MAX_SLOTS) {
            prefs.begin("wifi", true);
            String s = prefs.getString(("s" + String(last)).c_str(), "");
            prefs.end();
            if (s.length()) bestSlot = last;
        }
        if (bestSlot < 0) {
            for (int i = 0; i < MAX_SLOTS; i++) {
                prefs.begin("wifi", true);
                String s = prefs.getString(("s" + String(i)).c_str(), "");
                prefs.end();
                if (s.length()) { bestSlot = i; break; }
            }
        }
        if (bestSlot < 0) {
            DevLog.println("[wifi] no saved network");
            return false;
        }
        DevLog.printf("[wifi] scan: no saved network visible; trying slot %d\n", bestSlot);
    }
    prefs.begin("wifi", true);
    wifiSsid = prefs.getString(("s" + String(bestSlot)).c_str(), "");
    wifiPass = prefs.getString(("p" + String(bestSlot)).c_str(), "");
    prefs.end();
    if (!wifiSsid.length()) return false;
    screen({"CODEX STATUS", FW_VERSION, "", "Connecting:", wifiSsid});
    DevLog.printf("[wifi] slot %d (%s) rssi=%d\n", bestSlot, wifiSsid.c_str(), bestRssi);
    WiFi.begin(wifiSsid.c_str(), wifiPass.c_str());
    uint32_t t0 = millis();
    while (WiFi.status() != WL_CONNECTED && millis() - t0 < WIFI_CONNECT_MS) {
        delay(200);
        DevLog.print(".");
    }
    DevLog.println();
    if (WiFi.status() != WL_CONNECTED) {
        DevLog.println("[wifi] connect timeout");
        WiFi.disconnect(false);   // keep the radio up for the retry paths
        return false;
    }
    prefs.begin("wifi", false);
    prefs.putUChar("last", (uint8_t)bestSlot);
    prefs.end();
    DevLog.printf("[wifi] connected: %s ip=%s bssid=%s\n", wifiSsid.c_str(),
                  WiFi.localIP().toString().c_str(), WiFi.BSSIDstr().c_str());
    return true;
}

static bool retryWifi() {
    if (!wifiSsid.length()) return false;
    DevLog.printf("[wifi] retry %s\n", wifiSsid.c_str());
    WiFi.begin(wifiSsid.c_str(), wifiPass.c_str());
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
    html += "<li>State: " + String(deviceStateText()) + " (BLE " + String(bleOn ? "session" : "off") +
            ", USB " + String(plugged ? "plugged" : "battery") + ")</li>";
    html += "<li>SSID: " + WiFi.SSID() + "</li>";
    html += "<li>IP: " + WiFi.localIP().toString() + "</li>";
    html += "<li>RSSI: " + String(WiFi.RSSI()) + " dBm</li>";
    html += "<li>Battery: " + String(batteryPercent()) + "% (" +
            String(batteryMilliVolts()) + " mV)</li>";
    html += "<li>BLE connected: " + String(bleIsConnected() ? "yes" : "no") + "</li>";
    html += "<li>Endpoints stored: " + String(storeCount()) + "</li>";
    html += "<li>Last channel: " + lastChannel + "</li>";
    html += "<li>Last sync: " + fmtEpoch((long long)rtcLastSyncEpoch, "%m-%d %H:%M") + "</li>";
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
    doc["mac"] = macText();
    const esp_partition_t *running = esp_ota_get_running_partition();
    const esp_partition_t *next = esp_ota_get_next_update_partition(nullptr);
    doc["slot"] = running ? running->label : "?";
    doc["next_slot"] = next ? next->label : "?";
    doc["reset"] = resetReasonName();
    doc["wake"] = wakeCauseName(bootWakeCause);
    doc["pwr"] = digitalRead(18);
    doc["uptime_s"] = millis() / 1000;
    doc["ssid"] = configMode ? apSsid : WiFi.SSID();
    doc["ip"] = ipText();
    doc["rssi"] = WiFi.RSSI();
    doc["state"] = deviceStateText();
    doc["ble_on"] = bleOn;
    doc["ble"] = bleIsConnected();
    doc["plugged"] = plugged;
    doc["wifi_state"] = wifiStateText();
    doc["retry_stage"] = rtcRetryStage;
    doc["last_push"] = rtcLastSyncEpoch;
    doc["endpoints"] = storeCount();
    doc["channel"] = lastChannel;
    doc["pm_light_sleep"] = pmLightSleep;
    doc["ota"] = otaInProgress;
    doc["battery"] = batteryPercent();
    doc["battery_mv"] = batteryMilliVolts();
    doc["heap"] = ESP.getFreeHeap();
    doc["epd_writes"] = epdWriteCount;
    doc["epd_partial"] = epdPartialReady;
    doc["epd_streak"] = epdPartialCount;
    doc["wifi_slots"] = countWifiSlots();
    doc["ap_reason"] = AP_REASON_NAMES[rtcApReason < 3 ? rtcApReason : 0];
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

// PM light-sleep counters (CONFIG_PM_PROFILING) for remote diagnostics.
// Read-only, no token: same exposure level as /log and /status.json.
static String pmStatsText() {
#if defined(CODEX_PM)
    char *buf = nullptr;
    size_t len = 0;
    FILE *f = open_memstream(&buf, &len);
    if (!f) return String("pmstats: memstream failed");
    // With CONFIG_PM_PROFILING (enabled in this build) dump_locks appends the
    // mode/sleep stats itself; calling impl_dump_stats first would duplicate it.
    esp_pm_dump_locks(f);
    fflush(f);
    if (buf && strstr(buf, "Mode stats:") == nullptr) {
        esp_pm_impl_dump_stats(f);
    }
    fclose(f);
    String out = buf ? buf : "";
    free(buf);
    return out;
#else
    return String("stock core: no PM stats");
#endif
}

static void handlePmStats() {
    String text = pmStatsText();
    DevLog.printf("[pm] stats over HTTP (%u bytes)\n", (unsigned)text.length());
    server.send(200, "text/plain; charset=utf-8", text);
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
// reboots and deep sleep no longer invalidate it; it is disclosed only over the
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

// UDP announce (docs/power-state.md §9): the device broadcasts its endpoint so
// the bridge can track DHCP changes; ble=1 asks for a one-shot BLE handshake.
static void sendAnnounce(bool bleFlag) {
    if (!wifiUp || WiFi.status() != WL_CONNECTED) return;
    IPAddress ip = WiFi.localIP();
    String mac = macText();
    char payload[192];
    int n = snprintf(payload, sizeof(payload),
                     "{\"magic\":\"codex-status\",\"mac\":\"%s\",\"ip\":\"%u.%u.%u.%u\","
                     "\"port\":80,\"proto\":\"http\",\"ble\":%d,\"fw\":\"%s\"}",
                     mac.c_str(), ip[0], ip[1], ip[2], ip[3], bleFlag ? 1 : 0, FW_VERSION);
    announceUdp.beginPacket(IPAddress(255, 255, 255, 255), 8767);
    announceUdp.write((const uint8_t *)payload, n);
    announceUdp.endPacket();
    announcedIp = (uint32_t)ip;
    lastAnnounce = millis();
    DevLog.printf("[udp] announce %s ble=%d\n", ip.toString().c_str(), bleFlag ? 1 : 0);
}

static void requestAnnounce(bool bleFlag) {
    sendAnnounce(bleFlag);
}

static void serviceAnnounce() {
    if (!wifiUp) return;
    if ((uint32_t)WiFi.localIP() != announcedIp) { sendAnnounce(bleOn); return; }
    if (millis() - lastAnnounce > ANNOUNCE_MS) sendAnnounce(bleOn);
}

// ---------------- v0.12 state machine ----------------
static uint32_t retryDelaySec(uint8_t stage) {
    if (stage < 3) return 60;    // 1 min x3
    if (stage < 6) return 300;   // 5 min x3
    return 900;                  // 15 min forever
}

// GP3 green LED: active low, lit while the BLE session is on.
static void ledApply() {
    bool on = ledState;
    if (ledPulseUntil && (int32_t)(millis() - ledPulseUntil) < 0) on = !on;
    digitalWrite(3, on ? LOW : HIGH);
}

static void ledSet(bool on) {
    ledState = on;
    ledApply();
}

static void ledFlash() {
    ledPulseUntil = millis() + 80;
    ledApply();
}

static void serviceLed() {
    if (ledPulseUntil && (int32_t)(millis() - ledPulseUntil) >= 0) {
        ledPulseUntil = 0;
        ledApply();
    }
}

static void enterBleOn(bool userInitiated) {
    if (!wifiUp) return;
    if (!bleInitialized()) {
        bleBegin("CodexStatus-" + macSuffix(), FW_VERSION);
        bleSetHandlers(handleBleUsage, handleBleEndpoint);
        bleSetTemplateHandlers(tplXferHandleCtrl, tplXferHandleChunk, tplXferReset);
        bleSetAuthHandler(handleBleAuth);
    }
    updateInfoExtra();
    if (!bleOn) {
        bleOn = true;
        bleUserOff = false;
        ledSet(true);
        ledFlash();
        DevLog.println("[ble] session on");
    }
    bleAdvertiseStart();
    if (userInitiated) {
        bleOpenPairingWindow(120000);
    }
    bool autoCond = plugged && batteryPct > BLE_AUTO_PCT;
    lastBleAuto = autoCond;
    bleOffDeadline = autoCond ? 0 : millis() + BLE_GRACE_MS;
    requestAnnounce(true);
    renderCurrent();
}

static void bleOff(const char *reason) {
    if (!bleOn) return;
    bleOn = false;
    lastBleAuto = false;
    bleAdvertiseStop();
    bleDeinit();
    ledSet(false);
    ledFlash();
    DevLog.printf("[ble] session off (%s)\n", reason ? reason : "");
    requestAnnounce(false);
    renderCurrent();
}

// BLE keep-alive: infinite while plugged and >20%, otherwise 120 s after the
// last connection/transfer or after the keep-alive condition ends.
static void serviceBleSession() {
    if (!bleOn) return;
    bool autoCond = plugged && batteryPct > BLE_AUTO_PCT;
    if (bleIsConnected()) {
        bleOffDeadline = millis() + BLE_GRACE_MS;
        lastBleAuto = autoCond;
        return;
    }
    if (autoCond) {
        bleOffDeadline = 0;
        lastBleAuto = true;
        return;
    }
    if (lastBleAuto) {
        lastBleAuto = false;
        bleOffDeadline = millis() + BLE_GRACE_MS;
    }
    if (bleOffDeadline && (int32_t)(millis() - bleOffDeadline) >= 0) bleOff("grace expired");
}

static void pollPlug() {
    static uint32_t lastPoll = 0;
    if (millis() - lastPoll < 500) return;
    lastPoll = millis();
    bool now = usb_serial_jtag_is_connected();
    if (now == plugged) return;
    plugged = now;
    DevLog.printf("[pm] usb %s\n", plugged ? "plugged" : "unplugged");
    ledFlash();
    renderCurrent();
    if (plugged) {
        bleUserOff = false;
        batteryPct = batteryPercent();
        if (wifiUp && !bleOn && batteryPct > BLE_AUTO_PCT) enterBleOn(false);
    }
}

static void pollWifi() {
    bool nowUp = (WiFi.status() == WL_CONNECTED);
    if (wifiReconfiguring) {
        if (nowUp) {
            wifiReconfiguring = false;
            if (wifiUp && storeCount() > 0) tryWifiUsage();
        } else if ((int32_t)(millis() - wifiReconfigDeadline) < 0) {
            return;   // deliberate power-save reassociation in progress
        } else {
            wifiReconfiguring = false;
            DevLog.println("[wifi] power-save reassociation timed out");
        }
    }
    if (nowUp) {
        if (!wifiUp) {
            wifiUp = true;
            wifiLostHandled = false;
            wifiLostSince = 0;
            rtcRetryStage = 0;
            DevLog.printf("[wifi] connected ip=%s rssi=%d\n",
                          WiFi.localIP().toString().c_str(), WiFi.RSSI());
            configureWifiPowerSave();
            batteryPct = batteryPercent();
            if (plugged && !bleOn && !bleUserOff && batteryPct > BLE_AUTO_PCT) enterBleOn(false);
            sendAnnounce(bleOn);
            renderCurrent();
        }
        if ((uint32_t)WiFi.localIP() != announcedIp) sendAnnounce(bleOn);
        return;
    }
    if (wifiUp) {
        wifiUp = false;
        wifiLostSince = millis();
        DevLog.println("[wifi] link lost");
        renderCurrent();
    } else if (!wifiLostSince) {
        wifiLostSince = millis();
    }
    if (wifiLostHandled) {
        if (plugged && (int32_t)(millis() - nextWifiRetry) >= 0) {
            nextWifiRetry = millis() + WIFI_RETRY_MS;
            ledFlash();
            retryWifi();
        }
        return;
    }
    if (millis() - wifiLostSince < WIFI_LOST_MS) return;
    wifiLostHandled = true;
    if (bleOn) bleOff("wifi lost");
    if (plugged) {
        nextWifiRetry = millis() + WIFI_RETRY_MS;
        DevLog.println("[wifi] WIFI OFF (plugged): retry every 60s");
        renderCurrent();
        return;
    }
    uint32_t delaySec = retryDelaySec(rtcRetryStage);
    if (rtcRetryStage < 7) rtcRetryStage++;
    DevLog.printf("[wifi] WIFI OFF (battery): deep sleep %us stage=%u\n",
                  (unsigned)delaySec, (unsigned)rtcRetryStage);
    deepSleepFor(delaySec);
}

static void checkBattery() {
    if (millis() - lastBattCheck < BATT_CHECK_MS) return;
    lastBattCheck = millis();
    batteryPct = batteryPercent();
    DevLog.printf("[pm] battery %d%% (%u mV) plugged=%d\n",
                  batteryPct, (unsigned)batteryMilliVolts(), plugged ? 1 : 0);
    if (!plugged && batteryPct < LOW_BATT_PCT) {
        DevLog.println("[pm] battery <5%: power off");
        powerOff();
    }
    if (plugged && wifiUp && !bleOn && !bleUserOff && batteryPct > BLE_AUTO_PCT) enterBleOn(false);
}

static void handleBootClick() {
    if (millis() - lastBootAction < 1000) return;   // 1 s throttle
    lastBootAction = millis();
    if (!wifiUp) {
        if (plugged && retryWifi()) {
            DevLog.println("[boot] click: retry Wi-Fi now");
            ledFlash();
        }
        return;
    }
    if (bleOn) {
        bleUserOff = true;
        bleOff("user click");
    } else {
        enterBleOn(true);
    }
}

static void registerHttpRoutes() {
    server.on("/", HTTP_GET, handleStatus);
    server.on("/status.json", HTTP_GET, handleStatusJson);
    server.on("/log", HTTP_GET, handleLog);
    server.on("/pmstats", HTTP_GET, handlePmStats);
    server.on("/usage", HTTP_POST, handleUsagePost);
    server.on("/template", HTTP_POST, handleTemplatePost);
    server.on("/update", HTTP_GET, handleUpdatePage);
    server.on("/doUpdate", HTTP_POST,
        []() {
            server.sendHeader("Connection", "close");
            if (otaUploadDenied) {
                server.send(401, "text/plain", "unauthorized: negotiate a token over BLE first");
                return;
            }
            setOtaLock(false);
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
                setOtaLock(true);
                DevLog.printf("[ota] upload start: %s\n", up.filename.c_str());
                std::vector<String> lines = {"OTA update", FW_VERSION, up.filename};
                screen(lines);
                if (!Update.begin(UPDATE_SIZE_UNKNOWN)) Update.printError(Serial);
            } else if (up.status == UPLOAD_FILE_WRITE) {
                if (otaUploadDenied) return;
                if (Update.write(up.buf, up.currentSize) != up.currentSize) Update.printError(Serial);
            } else if (up.status == UPLOAD_FILE_END) {
                otaInProgress = false;
                if (otaUploadDenied) { setOtaLock(false); return; }
                if (Update.end(true)) {
                    DevLog.printf("[ota] success %u bytes, rebooting shortly\n", (unsigned)up.totalSize);
                    screen({"OTA success", "Rebooting..."});
                    otaRebootPending = true;
                    otaRebootAt = millis() + 1500;
                } else {
                    Update.printError(Serial);
                    setOtaLock(false);
                }
            }
        });
}

// Wi-Fi power save: WIFI_PS_MAX_MODEM with listen_interval=10 wakes the modem
// every 10 beacons (~1 s) instead of every DTIM, so bridge pushes still wake
// the device from light sleep with acceptable latency (T9). Applying the
// listen interval needs one reassociation; while that runs pollWifi must not
// read the deliberate drop as a lost link.
static bool configureWifiPowerSave() {
    wifi_config_t cfg = {};
    bool reassociate = false;
    if (esp_wifi_get_config(WIFI_IF_STA, &cfg) == ESP_OK && cfg.sta.listen_interval != 10) {
        cfg.sta.listen_interval = 10;
        esp_err_t err = esp_wifi_set_config(WIFI_IF_STA, &cfg);
        if (err == ESP_OK && WiFi.status() == WL_CONNECTED) {
            reassociate = true;
        }
        DevLog.printf("[pm] listen_interval=10 (%s)\n", esp_err_to_name(err));
    }
    if (reassociate) {
        wifiReconfiguring = true;
        wifiReconfigDeadline = millis() + 15000;
        WiFi.disconnect(false);
        delay(100);
        WiFi.reconnect();
        uint32_t t0 = millis();
        while (WiFi.status() != WL_CONNECTED && millis() - t0 < 15000) delay(100);
        DevLog.printf("[pm] reassociated for power save, connected=%d\n",
                      WiFi.status() == WL_CONNECTED);
        if (WiFi.status() == WL_CONNECTED) wifiReconfiguring = false;
    }
    esp_wifi_set_ps(WIFI_PS_MAX_MODEM);
    DevLog.println("[pm] WiFi PS = MAX_MODEM");
    return WiFi.status() == WL_CONNECTED;
}

static void startNormalMode() {
    configMode = false;
    hostname = "codex-status-" + macSuffix();
    configTzTime("CST-8", "pool.ntp.org");
    pinMode(0, INPUT_PULLUP);
    pinMode(18, INPUT_PULLUP);
    pinMode(3, OUTPUT);
    digitalWrite(3, HIGH);   // green LED off (active low)

    configurePowerManagement();
    setupOtaPmLock();
    announceUdp.begin(0);

    wifiUp = connectBest();
    registerHttpRoutes();
    server.begin();

    if (wifiUp) {
        wifiLostHandled = false;
        wifiLostSince = 0;
        lastBattCheck = millis();
        batteryPct = batteryPercent();
        configureWifiPowerSave();
        loadOrIssueAuthToken();
        ArduinoOTA.setHostname(hostname.c_str());
        ArduinoOTA.onStart([]() {
            setOtaLock(true);
            screen({"ArduinoOTA", "updating..."});
        });
        ArduinoOTA.onProgress([](unsigned int p, unsigned int t) {
            DevLog.printf("[ota] %u%%\r", t ? p * 100 / t : 0);
        });
        ArduinoOTA.onEnd([]() {
            screen({"OTA OK", "rebooting..."});
            delay(800);
            setOtaLock(false);
        });
        ArduinoOTA.onError([](ota_error_t e) {
            setOtaLock(false);
            DevLog.printf("[ota] error %u\n", e);
        });
        ArduinoOTA.begin();
        MDNS.addService("http", "tcp", 80);
        if (plugged && batteryPct > BLE_AUTO_PCT) enterBleOn(false);
        if (storeCount() > 0) tryWifiUsage();
        sendAnnounce(bleOn);
    } else {
        wifiLostHandled = true;
        wifiLostSince = millis() - WIFI_LOST_MS;   // already past loss detection
        if (plugged) {
            nextWifiRetry = millis() + WIFI_RETRY_MS;
            DevLog.println("[wifi] no link at boot (plugged): retry every 60s");
        } else {
            uint32_t delaySec = retryDelaySec(rtcRetryStage);
            if (rtcRetryStage < 7) rtcRetryStage++;
            DevLog.printf("[wifi] no link at boot (battery): deep sleep %us stage=%u\n",
                          (unsigned)delaySec, (unsigned)rtcRetryStage);
            deepSleepFor(delaySec);
        }
    }

    DevLog.printf("[net] ready state=%s ip=%s host=%s.local endpoints=%d\n",
                  deviceStateText(), ipText().c_str(), hostname.c_str(), storeCount());
}

void setup() {
    esp_sleep_wakeup_cause_t cause = esp_sleep_get_wakeup_cause();
    bootWakeCause = cause;
    bool woke = (cause == ESP_SLEEP_WAKEUP_TIMER || cause == ESP_SLEEP_WAKEUP_EXT1);
    loadHostMac();
    hostname = "codex-status-" + macSuffix();
    epdBegin(!woke);
    if (!woke) screen({"CODEX STATUS", FW_VERSION, "booting..."});
    if (rtcMagic != 0xC0DE0001) {
        rtcMagic = 0xC0DE0001;
        rtcActiveAt = 0;
        rtcActiveMac[0] = 0;
        rtcLastSyncEpoch = 0;
        rtcRetryStage = 0;
        rtcUsageHash = 0;
        DevLog.println("[pm] RTC state initialized");
    }
    // The last successful sync must survive OTA/software resets (RTC data is
    // not reliable across them on this board), so it is also kept in NVS.
    if (rtcLastSyncEpoch == 0) {
        Preferences p;
        p.begin("sync", true);
        rtcLastSyncEpoch = p.getUInt("last", 0);
        p.end();
    }
    lastPersistedSync = rtcLastSyncEpoch;
    if (cause == ESP_SLEEP_WAKEUP_EXT1) {
        rtcRetryStage = 0;   // button wake replays the retry cadence from the top
        DevLog.println("[pm] button wake: retry stage reset");
    }
    plugged = usb_serial_jtag_is_connected();
    batteryPct = batteryPercent();

    const esp_partition_t *running = esp_ota_get_running_partition();
    DevLog.printf("\n[codex-status] v%s mac=%s reset=%s slot=%s wake=%d(%s) usb=%d\n",
                  FW_VERSION, macText().c_str(), resetReasonName(),
                  running ? running->label : "?", (int)cause,
                  wakeCauseName(cause), plugged ? 1 : 0);
    { Preferences p; p.begin("brg", false); p.end(); }

    tplStoreBegin();
    tplXferBegin(FW_VERSION, []() { pendingTplChanged = true; });
    String cached;
    if (usageCacheLoad(cached)) lastUsage = cached;

    if (hasWifiSlots()) {
        startNormalMode();
    } else {
        rtcApReason = 1;
        DevLog.println("[config] no saved Wi-Fi slots; entering AP mode");
        startConfigMode();
    }
}

static void dumpPmStats() {
    DevLog.printf("[pm] stats:\n%s", pmStatsText().c_str());
}

// USB serial provisioning: `wifi <ssid> <pass>` saves to NVS and reboots;
// `status` prints IP/RSSI/heap/state; `batt` prints battery; `pair` opens a
// BLE session with a pairing window; `pmstats` dumps PM light-sleep counters.
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
            DevLog.printf("[cli] fw=%s state=%s ip=%s rssi=%d heap=%u\n",
                          FW_VERSION, deviceStateText(), ipText().c_str(),
                          WiFi.RSSI(), ESP.getFreeHeap());
        } else if (line == "batt") {
            DevLog.printf("[cli] battery=%d%% (%u mV)\n", batteryPercent(),
                          (unsigned)batteryMilliVolts());
        } else if (line == "pair") {
            enterBleOn(true);
            DevLog.println("[cli] BLE session on, pairing window 120s");
        } else if (line == "pmstats") {
            dumpPmStats();
        } else if (line.length()) {
            DevLog.println("[cli] commands: wifi <ssid> <pass> | status | batt | pair | pmstats");
        }
    }
}

// PWR (GPIO18) held for 3 s: software power-off by dropping the VBAT latch
// (GPIO17). On battery the MCU dies here; if USB/charger power keeps the board
// alive the hold degrades to a clean restart so the device never sits inert.
static void powerOff() {
    DevLog.println("[pm] PWR held 3s: power off");
    epdPanelSleep();
    if (bleInitialized()) {
        bleAdvertiseStop();
        bleDeinit();
    }
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
        // AP without credentials: battery sleeps after 5 idle minutes (buttons
        // only); plugged stays awake so provisioning/serial stay available.
        if (!plugged && millis() - configStartedAt > CONFIG_IDLE_MS) {
            DevLog.println("[config] AP idle timeout (battery): deep sleep");
            deepSleepFor(0);
        }
        delay(5);
        return;
    }

    ArduinoOTA.handle();

    static uint32_t bootDownAt = 0;
    static int      bootStage = 0;
    // Arm the button only after GPIO0 has read HIGH once: a strapping/mux
    // glitch that made it read LOW for the first seconds of boot otherwise
    // looked like a long hold.
    static bool     bootArmed = false;
    bool bootLow = (digitalRead(0) == LOW);
    if (!bootArmed && !bootLow) bootArmed = true;
    if (bootArmed && bootLow) {
        if (!bootDownAt) bootDownAt = millis();
        uint32_t held = millis() - bootDownAt;
        if (bootStage < 1 && held > 2000) {
            bootStage = 1;
            nextTemplate();
            enterBleOn(true);   // 2 s: next local template + BLE session
        }
        if (bootStage < 2 && held > 15000) {
            bootStage = 2;
            rtcApReason = 2;
            DevLog.println("[config] BOOT held 15s; entering AP mode");
            startConfigMode();
            return;
        }
        if (bootStage < 3 && held > 30000) {
            bootStage = 3;
            factoryReset();
        }
    } else {
        if (bootDownAt) {
            uint32_t held = millis() - bootDownAt;
            if (bootStage == 0 && held > 50 && held < 2000) handleBootClick();
            bootDownAt = 0;
            bootStage = 0;
        }
    }

    if (pendingTplChanged) {
        pendingTplChanged = false;
        activeTplId = "";
        updateInfoExtra();
        renderCurrent();
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
            String mac = blePeerAddress();
            applyEnvelopeMeta(parsed, mac);
            bool explicitActivate = parsed["activate"] | false;
            bool accepted = usageAccepted(mac, explicitActivate);
            markSynced();
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

    pollWifi();
    pollPlug();
    serviceBleSession();
    checkBattery();
    serviceAnnounce();
    serviceLed();

    // Keep the offline-minutes row honest while the bridge is unreachable:
    // once contact is lost (>= BRIDGE_LOST_MIN minutes since the last sync) the
    // template shows `OFF <n>M`, so redraw when the integer minute changes.
    // No periodic refresh while the bridge is heartbeating.
    static int lastOfflineMinute = -1;
    int offlineMinute = -1;
    if (rtcLastSyncEpoch > 1600000000 && timeKnown()) {
        long mins = ((long)time(nullptr) - (long)rtcLastSyncEpoch) / 60;
        if (mins > BRIDGE_LOST_MIN) offlineMinute = (int)mins;
    }
    if (offlineMinute != lastOfflineMinute) {
        bool wasShown = lastOfflineMinute > 0;
        lastOfflineMinute = offlineMinute;
        if (offlineMinute > 0 || wasShown) renderCurrent();
    }

    delay(5);
}
