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
#include <vector>
#include <esp_sleep.h>

#include "DEV_Config.h"
#include "EPD_SSD1681.h"
#include "GUI_Paint.h"
#include "fonts.h"
#include "ble_bridge.h"
#include "bridge_store.h"
#include "usage_client.h"
#include "template_store.h"
#include "template_engine.h"
#include "template_xfer.h"

#define FW_VERSION    "0.4.2-bw"
#define AP_PASSWORD   "codex1234"
#define OTA_PASSWORD  "codexota"
#define MAX_SLOTS     3
#define SLOT_TIMEOUT  15000
#define WIFI_LOST_RESTART_MS 120000
#define FETCH_INTERVAL_MS    30000

static const int EPD_W = EPD_SSD1681_WIDTH;
static const int EPD_H = EPD_SSD1681_HEIGHT;
static const int EPD_FB_BYTES = (EPD_W / 8) * EPD_H;

static Preferences prefs;
static WebServer   server(80);
static UBYTE      *frame = nullptr;
static bool        configMode = false;
static uint32_t    lastConnectedMs = 0;
static uint32_t    nextFetchAt = 0;
static String      hostname;
static String      apSsid;

static volatile bool pendingUsageReady = false;
static String        pendingUsage;
static String        pendingChannel;
static volatile bool pendingEndpoint = false;
static volatile bool pendingTplChanged = false;
static bool pairingOverlay = false;
static bool drawingPairingOverlay = false;

static String activeTplJson;
static String activeTplId;
static uint32_t gNextSyncSec = 300;

static String lastUsage;
static String lastUsageSig;
static String lastChannel = "-";
static String renderedIp;
static uint32_t lastSyncMs = 0;
static time_t   lastSyncEpoch = 0;
static uint32_t lastOkMs = 0;
static bool     usageOnScreen = false;
static String   screenSig;
static int      epdPartialCount = 0;
static bool     epdPartialReady = false;

static void screen(const std::vector<String> &lines, UBYTE color = BLACK);
static void epdFlush(bool fullRefresh = false);

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
    for (JsonObject b : buckets) {
        JsonArray wins = b["windows"].as<JsonArray>();
        for (JsonObject w : wins) {
            int wm = w["windowMins"] | 0;
            bool match = fiveHour ? (wm == 300) : (wm >= 10080);
            if (!match) continue;
            r.found = true;
            r.used = w["usedPercent"] | -1;
            r.resets = w["resetsAt"] | 0LL;
            return r;
        }
    }
    return r;
}

static String usageSig(const String &json) {
    JsonDocument doc;
    if (deserializeJson(doc, json)) return String("bad");
    const char *plan  = doc["account"]["plan"] | "?";
    const char *label = doc["bridge"]["label"] | "?";
    int rc = doc["resetCredits"]["availableCount"] | -1;
    WinInfo fh = findWindow(doc, true);
    WinInfo wk = findWindow(doc, false);
    char buf[128];
    snprintf(buf, sizeof(buf), "%s|%s|%d|%d|%lld|%d|%lld",
             plan, label, rc, wk.used, wk.resets, fh.used, fh.resets);
    return String(buf);
}

static void screenIdle() {
    std::vector<String> lines;
    lines.push_back("CODEX STATUS");
    lines.push_back(FW_VERSION);
    lines.push_back(String("IP ") + ipText());
    lines.push_back(String("BRG ") + (bridgeOk() ? "OK" : "--"));
    lines.push_back(String("BLE ") + (bleIsConnected() ? "LINK" : "READY"));
    lines.push_back(String("EP ") + String(storeCount()));
    lines.push_back(String("SYNC ") + (lastSyncEpoch ? nowHHMM() : String("--:--")));
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

static void epdBegin() {
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
    EPD_SSD1681_Clear(EPD_SSD1681_WHITE);
    frame = (UBYTE *)malloc(EPD_FB_BYTES);
    if (!frame) {
        Serial.println("[epd] frame buffer malloc failed");
        return;
    }
    Paint_NewImage(frame, EPD_W, EPD_H, 0, WHITE);
    Paint_SetScale(2);
    Paint_SelectImage(frame);
    epdPartialReady = false;
    epdPartialCount = 0;
}

// fullRefresh=true (default for data screens): high-contrast full refresh
// (~1.5s, flashes). Otherwise partial refresh (~300ms, no flash) with a
// full refresh every 30 partials to clear ghosting.
static void epdFlush(bool fullRefresh) {
    if (!frame) return;
    if (!fullRefresh && epdPartialReady) {
        EPD_SSD1681_DisplayPart(frame);
        if (++epdPartialCount < 30) return;
    }
    if (epdPartialReady) EPD_SSD1681_Init();   // reload full-refresh LUT
    EPD_SSD1681_Display(frame);
    EPD_SSD1681_Init_Partial();
    epdPartialReady = true;
    epdPartialCount = 0;
}

// ---- Quad built-in screen (B/W design, partial refresh) ----
// Block size matches the factory UI proportions (~52% x 34% of the panel).
static const int Q_BLK_W = 104;
static const int Q_BLK_H = 68;
static const int Q_TL_X  = 4;
static const int Q_TL_Y  = 4;
static const int Q_BR_X  = EPD_W - 4 - Q_BLK_W;
static const int Q_BR_Y  = EPD_H - 4 - Q_BLK_H;
static const int Q_TR_RIGHT = EPD_W - 4;
static const int Q_TR_Y0 = 8;
static const int Q_LINE_H = 14;
static const int Q_BL_X = 4;
static const int Q_BL_SLOTS[4] = {140, 154, 168, 182};

static int batteryPercent() {
    uint32_t mv = 0;
    for (int i = 0; i < 8; i++) mv += analogReadMilliVolts(4);
    uint32_t vbat = (mv / 8) * 2;   // board divider: VBAT = VADC x 2
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

static void drawInfinityBlock(int x, int y) {
    Paint_DrawRectangle(x, y, x + Q_BLK_W - 1, y + Q_BLK_H - 1, BLACK, DOT_PIXEL_1X1, DRAW_FILL_FULL);
    int cx = x + Q_BLK_W / 2;
    int cy = y + Q_BLK_H / 2;
    Paint_DrawCircle(cx - 17, cy, 20, WHITE, DOT_PIXEL_1X1, DRAW_FILL_FULL);
    Paint_DrawCircle(cx + 17, cy, 20, WHITE, DOT_PIXEL_1X1, DRAW_FILL_FULL);
    Paint_DrawCircle(cx - 17, cy, 9, BLACK, DOT_PIXEL_1X1, DRAW_FILL_FULL);
    Paint_DrawCircle(cx + 17, cy, 9, BLACK, DOT_PIXEL_1X1, DRAW_FILL_FULL);
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
        Serial.println("[usage] parse failed");
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
    else          drawInfinityBlock(Q_BR_X, Q_BR_Y);

    String rcLine = (rc >= 0) ? String("RC ") + rc : String("RC --");
    std::vector<String> tr, bl;

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
    bl.push_back(String("BATT ") + batteryPercent() + "%");
    bl.push_back(String("SYNC ") + nowHHMM());

    for (int i = 0; i < (int)tr.size(); i++)
        drawSmallRight(tr[i], Q_TR_RIGHT, Q_TR_Y0 + i * Q_LINE_H);
    int first = 4 - (int)bl.size();
    for (int i = 0; i < (int)bl.size(); i++)
        drawSmallLeft(bl[i], Q_BL_X, Q_BL_SLOTS[first + i]);

    epdFlush(true);
    usageOnScreen = true;
    renderedIp = ipText();
    screenSig = String("usage");
    Serial.printf("[ui] quad rendered (wk=%d fh=%d ch=%s)\n",
                  wk.used, fh.used, channel ? channel : "");
}

static void updateInfoExtra() {
    String items = "\"templates\":[";
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
    if (pairingOverlayActive()) return;
    if (!frame) return;
    if (tplCacheLoad()) {
        TplEnv env;
        env.channel   = channel ? channel : "";
        env.ip        = ipText();
        env.syncHHMM  = nowHHMM();
        Paint_SelectImage(frame);
        Paint_Clear(WHITE);
        if (tplDraw(activeTplJson, json, env)) {
            epdFlush(true);
            usageOnScreen = true;
            renderedIp = ipText();
            screenSig = String("usage");
            Serial.printf("[tpl] rendered %s (%s)\n", activeTplId.c_str(), channel ? channel : "");
            return;
        }
        Serial.printf("[tpl] %s invalid, fallback built-in\n", activeTplId.c_str());
    }
    renderUsage(json, channel);
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
                Serial.printf("[tpl] local %s rejected: %s\n", id.c_str(), localErr.c_str());
            }
        } else if (tplStoreFind(id, local)) {
            localHash = local.hash;
        }
        if (localValid) continue;
        String out, err;
        if (usageTemplateGet(rec, id, localHash, out, err)) {
            String acceptErr;
            if (!tplValidateForStorage(out, hash, FW_VERSION, acceptErr)) {
                Serial.printf("[tpl] HTTP %s rejected: %s\n", id.c_str(), acceptErr.c_str());
                continue;
            }
            if (tplStoreSave(id, ver, hash, (const uint8_t *)out.c_str(), out.length())) {
                if (tplStoreActive().length() == 0 || tplStoreActive() == id) {
                    tplStoreSetActive(id);
                    activeTplId = "";
                    lastUsageSig = "";
                }
                updateInfoExtra();
                Serial.printf("[tpl] fetched %s hash=%s\n", id.c_str(), hash.c_str());
            }
        } else if (err != "http 304") {
            Serial.printf("[tpl] fetch %s failed: %s\n", id.c_str(), err.c_str());
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
    Serial.printf("[tpl] local switch -> %s\n", m.id.c_str());
}

static void factoryReset() {
    screen({"FACTORY RESET", "", "clearing..."});
    bleClearBonds();
    storeClear();
    tplStoreClear();
    prefs.begin("wifi", false);
    prefs.clear();
    prefs.end();
    delay(1000);
    ESP.restart();
}

static bool tryWifiUsage() {
    if (WiFi.status() != WL_CONNECTED) return false;
    int n = storeCount();
    if (n <= 0) return false;
    bool tried[8] = {false};
    for (int k = 0; k < n; k++) {
        int idx = -1;
        uint32_t mx = 0;
        for (int i = 0; i < n; i++) {
            if (tried[i]) continue;
            EndpointRec r;
            if (!storeGet(i, r)) { tried[i] = true; continue; }
            if (idx < 0 || r.mru > mx) { idx = i; mx = r.mru; }
        }
        if (idx < 0) break;
        tried[idx] = true;
        EndpointRec rec;
        if (!storeGet(idx, rec)) continue;
        String out, err;
        if (usageHttpGet(rec, out, err)) {
            storeTouch(rec.mac);
            lastSyncMs = millis();
            lastOkMs = millis();
            lastSyncEpoch = time(nullptr);
            Serial.printf("[wifi] usage from %s:%u\n", rec.host.c_str(), rec.port);
            bleNotifyStatus("{\"ack\":\"wifi-usage\",\"ok\":true}");
            maybeFetchTemplate(rec, out);
            {
                JsonDocument d;
                if (!deserializeJson(d, out)) {
                    uint32_t ns = d["next_sync_seconds"] | 0;
                    if (ns >= 60 && ns <= 86400) gNextSyncSec = ns;
                }
            }
            String sig = usageSig(out);
            if (sig != lastUsageSig || lastChannel != "WIFI" || !usageOnScreen) {
                lastUsage = out;
                lastUsageSig = sig;
                lastChannel = "WIFI";
                renderActiveUsage(out, "WIFI");
            }
            return true;
        }
        Serial.printf("[wifi] %s:%u failed: %s\n", rec.host.c_str(), rec.port, err.c_str());
    }
    return false;
}

static void handleBleUsage(const String &json) {
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

static bool connectStored() {
    prefs.begin("wifi", true);
    for (int i = 0; i < MAX_SLOTS; i++) {
        String ssid = prefs.getString(("s" + String(i)).c_str(), "");
        String pass = prefs.getString(("p" + String(i)).c_str(), "");
        if (!ssid.length()) continue;
        screen({"CODEX STATUS", FW_VERSION, "", "Connecting:", ssid});
        Serial.printf("[wifi] trying slot %d: %s\n", i, ssid.c_str());
        WiFi.begin(ssid.c_str(), pass.c_str());
        uint32_t t0 = millis();
        while (WiFi.status() != WL_CONNECTED && millis() - t0 < SLOT_TIMEOUT) {
            delay(250);
            Serial.print(".");
        }
        Serial.println();
        if (WiFi.status() == WL_CONNECTED) {
            prefs.end();
            Serial.printf("[wifi] connected: %s ip=%s\n", ssid.c_str(), WiFi.localIP().toString().c_str());
            return true;
        }
        WiFi.disconnect(true);
    }
    prefs.end();
    return false;
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
    prefs.end();
    server.send(200, "text/html", "<h3>Saved. Rebooting...</h3>");
    screen({"Wi-Fi saved:", ssid, "", "Rebooting..."});
    delay(800);
    ESP.restart();
}

static void startConfigMode() {
    configMode = true;
    WiFi.mode(WIFI_AP);
    apSsid = "CodexStatus-" + macSuffix();
    WiFi.softAP(apSsid.c_str(), AP_PASSWORD);
    Serial.printf("[config] AP=%s pass=%s url=http://192.168.4.1\n", apSsid.c_str(), AP_PASSWORD);
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
static void handleStatus() {
    String html = F("<!DOCTYPE html><html><head><meta charset='utf-8'><title>Codex Status</title></head><body>");
    html += F("<h2>Codex Status</h2><ul>");
    html += "<li>Version: " FW_VERSION "</li>";
    html += "<li>SSID: " + WiFi.SSID() + "</li>";
    html += "<li>IP: " + WiFi.localIP().toString() + "</li>";
    html += "<li>RSSI: " + String(WiFi.RSSI()) + " dBm</li>";
    html += "<li>BLE connected: " + String(bleIsConnected() ? "yes" : "no") + "</li>";
    html += "<li>Endpoints stored: " + String(storeCount()) + "</li>";
    html += "<li>Last channel: " + lastChannel + "</li>";
    html += "<li>Templates: " + String(tplStoreCount()) + " (active: " + tplStoreActive() + ")</li>";
    html += "<li>Free heap: " + String(ESP.getFreeHeap()) + "</li>";
    html += F("</ul><p><a href='/update'>Firmware OTA update</a></p></body></html>");
    server.send(200, "text/html", html);
}

static void handleUpdatePage() {
    server.send(200, "text/html",
        F("<!DOCTYPE html><html><head><meta charset='utf-8'><title>OTA</title></head><body>"
          "<h2>Firmware OTA</h2>"
          "<form method='POST' action='/doUpdate' enctype='multipart/form-data'>"
          "<input type='file' name='firmware' accept='.bin'>"
          "<button type='submit'>Upload</button></form></body></html>"));
}

static void startNormalMode() {
    configMode = false;
    hostname = "codex-status-" + macSuffix();
    WiFi.setHostname(hostname.c_str());
    lastConnectedMs = millis();

    configTzTime("CST-8", "pool.ntp.org");
    pinMode(0, INPUT_PULLUP);
    pinMode(18, INPUT_PULLUP);

    bleBegin("CodexStatus-" + macSuffix(), FW_VERSION);
    bleSetHandlers(handleBleUsage, handleBleEndpoint);
    bleSetTemplateHandlers(tplXferHandleCtrl, tplXferHandleChunk, tplXferReset);
    updateInfoExtra();

    server.on("/", HTTP_GET, handleStatus);
    server.on("/update", HTTP_GET, handleUpdatePage);
    server.on("/doUpdate", HTTP_POST,
        []() {
            server.sendHeader("Connection", "close");
            server.send(200, "text/plain", Update.hasError() ? "UPDATE FAILED" : "UPDATE OK");
        },
        []() {
            HTTPUpload &up = server.upload();
            if (up.status == UPLOAD_FILE_START) {
                Serial.printf("[ota] upload start: %s\n", up.filename.c_str());
                std::vector<String> lines = {"OTA update", up.filename};
                screen(lines);
                if (!Update.begin(UPDATE_SIZE_UNKNOWN)) Update.printError(Serial);
            } else if (up.status == UPLOAD_FILE_WRITE) {
                if (Update.write(up.buf, up.currentSize) != up.currentSize) Update.printError(Serial);
            } else if (up.status == UPLOAD_FILE_END) {
                if (Update.end(true)) {
                    Serial.printf("[ota] success %u bytes, rebooting\n", (unsigned)up.totalSize);
                    screen({"OTA success", "Rebooting..."});
                    delay(1000);
                    ESP.restart();
                } else {
                    Update.printError(Serial);
                }
            }
        });
    server.begin();

    ArduinoOTA.setHostname(hostname.c_str());
    ArduinoOTA.setPassword(OTA_PASSWORD);
    ArduinoOTA.onStart([]() { screen({"ArduinoOTA", "updating..."}); });
    ArduinoOTA.onProgress([](unsigned int p, unsigned int t) {
        Serial.printf("[ota] %u%%\r", t ? p * 100 / t : 0);
    });
    ArduinoOTA.onEnd([]() { screen({"OTA OK", "rebooting..."}); delay(800); });
    ArduinoOTA.onError([](ota_error_t e) { Serial.printf("[ota] error %u\n", e); });
    ArduinoOTA.begin();
    MDNS.addService("http", "tcp", 80);

    Serial.printf("[net] ready: http://%s/  host=%s.local  endpoints=%d\n",
                  WiFi.localIP().toString().c_str(), hostname.c_str(), storeCount());

    if (storeCount() > 0) {
        if (!tryWifiUsage()) screenIdle();
    } else {
        screenIdle();
    }
    nextFetchAt = millis() + FETCH_INTERVAL_MS;
}

void setup() {
    epdBegin();
    screen({"CODEX STATUS", FW_VERSION, "booting..."});
    Serial.printf("\n[codex-status] v%s mac=%s\n", FW_VERSION, WiFi.macAddress().c_str());
    { Preferences p; p.begin("brg", false); p.end(); }

    tplStoreBegin();
    tplXferBegin(FW_VERSION, []() { pendingTplChanged = true; });

    if (connectStored()) {
        startNormalMode();
    } else {
        startConfigMode();
    }
}

static bool batteryMode() {
    Preferences p;
    p.begin("cfg", true);
    bool b = p.getBool("batt", false);
    p.end();
    return b;
}

static void enterDeepSleep(uint32_t sec) {
    Serial.printf("[pm] deep sleep %us\n", (unsigned)sec);
    screen({"BATTERY MODE", "sleep " + String(sec) + "s"});
    delay(500);
    WiFi.disconnect(true);
    esp_sleep_enable_timer_wakeup((uint64_t)sec * 1000000ULL);
    esp_sleep_enable_ext1_wakeup(1ULL << 0, ESP_EXT1_WAKEUP_ANY_LOW);
    esp_deep_sleep_start();
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
                prefs.end();
                Serial.printf("[cli] wifi saved slot %d ssid=%s, rebooting\n", slot, ssid.c_str());
                screen({"Wi-Fi saved via USB:", ssid, "", "Rebooting..."});
                delay(800);
                ESP.restart();
            } else {
                Serial.println("[cli] usage: wifi <ssid> <pass>");
            }
        } else if (line == "status") {
            Serial.printf("[cli] fw=%s ip=%s rssi=%d heap=%u\n",
                          FW_VERSION, ipText().c_str(), WiFi.RSSI(), ESP.getFreeHeap());
        } else if (line == "batt") {
            Serial.printf("[cli] battery=%d%%\n", batteryPercent());
        } else if (line.length()) {
            Serial.println("[cli] commands: wifi <ssid> <pass> | status | batt");
        }
    }
}

void loop() {
    handleSerialCli();
    server.handleClient();
    blePoll();
    if (!configMode) {
        ArduinoOTA.handle();

        static uint32_t bootDownAt = 0;
        static int      bootStage = 0;
        if (digitalRead(0) == LOW) {
            if (!bootDownAt) bootDownAt = millis();
            uint32_t held = millis() - bootDownAt;
            if (bootStage < 1 && held > 2000) {
                bootStage = 1;
                bleOpenPairingWindow(120000);
                pairingOverlay = true;
                screenPairingOverlay();
            }
            if (bootStage < 2 && held > 10000) {
                bootStage = 2;
                pairingOverlay = false;
                factoryReset();
            }
        } else {
            if (bootDownAt) {
                uint32_t held = millis() - bootDownAt;
                if (bootStage == 0 && held > 50 && held < 1500) nextTemplate();
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
            tryWifiUsage();
        }
        if (pendingUsageReady) {
            pendingUsageReady = false;
            lastUsage = pendingUsage;
            lastUsageSig = usageSig(pendingUsage);
            lastChannel = pendingChannel;
            lastSyncMs = millis();
            lastOkMs = millis();
            lastSyncEpoch = time(nullptr);
            renderActiveUsage(lastUsage, lastChannel.c_str());
        }

        static uint32_t lastStatusAt = 0;
        if (millis() - lastStatusAt > 10000) {
            lastStatusAt = millis();
            String sig = String("idle|") + ipText() + "|" + String(bridgeOk()) + "|" + String(bleIsConnected());
            if (usageOnScreen) {
                if (renderedIp != ipText() && lastUsage.length()) {
                    renderedIp = ipText();
                    renderActiveUsage(lastUsage, lastChannel.c_str());
                }
            } else if (sig != screenSig) {
                screenIdle();
            }
        }

        if (millis() > nextFetchAt) {
            nextFetchAt = millis() + FETCH_INTERVAL_MS;
            if (storeCount() > 0 && tryWifiUsage()) {
                if (batteryMode()) enterDeepSleep(gNextSyncSec);
            }
        }

        if (WiFi.status() != WL_CONNECTED) {
            if (millis() - lastConnectedMs > WIFI_LOST_RESTART_MS) {
                Serial.println("[net] wifi lost, restarting");
                ESP.restart();
            }
        } else {
            lastConnectedMs = millis();
        }
    }
    delay(5);
}
