#include "diagnostics_internal.h"
#include "application_internal.h"
#include "clock_internal.h"
#include "display_internal.h"
#include "platform_internal.h"
#include "power_internal.h"
#include "radio_internal.h"
#include "sync_internal.h"
#include "transport_internal.h"
#include "status_pages.h"

namespace firmware::diagnostics {
static StatusPages statusPages;
void expireStatusRead() { statusPages.expire(clock::v2NowMs()); }

State state;
RTC_DATA_ATTR Retained retained;

RTC_DATA_ATTR static uint32_t rtcNetFails = 0;      // failed deep network windows

RTC_DATA_ATTR static uint8_t  rtcLastPullCode = 0;  // last deep-pull HTTP code

RTC_DATA_ATTR static uint16_t rtcFrameCaptures = 0;

// P0 diagnosis (sleep-modes): the RTC trace above dies with the RTC domain when
// the board is power-cycled (plugging USB shows reset=power-on), which is how
// every battery hang has been recovered so far. When armed via
// /diag?nvs_stage=1 (RTC flag), mirror the suspense points into NVS so the
// last reached code survives and can be read as `nvs_stage_boot` from
// /status.json after recovery. Bounded writes; disabled by default.
RTC_DATA_ATTR static uint8_t nvsStageEnabled = 0;

static uint8_t nvsStageWrites = 0;

RTC_DATA_ATTR static uint32_t histCount = 0;   // records ever written

RTC_DATA_ATTR static uint16_t histHead = 0;    // next write slot

RTC_DATA_ATTR static uint32_t rtcTraceMagic = 0;

RTC_DATA_ATTR static bool rtcTraceActive = false;

RTC_DATA_ATTR static WakeTraceRec rtcTraceShadow = {};

RTC_DATA_ATTR static uint32_t rtcTraceShadowSeq = 0;

static uint32_t traceStartMs = 0;

static bool traceBleConnectedSeen = false;

static uint32_t bleOnAccumMs = 0;         // BLE radio ms accumulated this wake

static uint32_t bleOnSinceMs = 0;         // millis() when BLE went on; 0 = off

static uint32_t traceCrc(const WakeTraceRec &r);
static bool traceValid(const WakeTraceRec &r);
static void traceCommit(WakeTraceRec &r);
static void tracePrepare(WakeTraceRec &r);
static const char *traceWakeTypeName(uint8_t type);
static const char *traceResultName(uint8_t result);
static const char *traceTransportName(uint8_t transport);
static uint8_t traceWakeType(esp_sleep_wakeup_cause_t cause);
static uint8_t traceExt1Summary();
static void traceSetError(uint8_t error);
static void traceSetResult(uint8_t result, uint8_t error = TRACE_ERR_NONE);
static void traceFinishInterrupted();
static uint8_t traceFinalResult();
static void histAddFull(uint8_t ev, uint16_t aux, uint32_t, uint8_t);
static const char *wakeResultName(uint8_t result);
static bool traceGetComplete(uint32_t seq, WakeTraceRec &out);
static uint32_t traceEarliestComplete();
static uint32_t traceLatestComplete();
static bool traceRecordActive(uint32_t seq);
static void traceAppendRow(JsonArray arr, const WakeTraceRec &r, bool compact = false);
static String pmStatsText();
static String timerStatsText();
static String sleepDiagText();
static String taskStatsText();
static void dumpPmStats();

void nvsStageMark(uint8_t code) {
    if (!nvsStageEnabled || code == state.nvsStageLast || nvsStageWrites >= NVS_STAGE_MAX_WRITES) return;
    state.nvsStageLast = code;
    nvsStageWrites++;
    Preferences p;
    p.begin("pm", false);
    p.putUChar("stg", code);
    p.end();
}

void setStage(uint8_t code) {
    retained.rtcStage = code;
    nvsStageMark(code);
}

uint32_t fnv1a(const String &s) {
    uint32_t h = 2166136261u;
    for (size_t i = 0; i < s.length(); i++) {
        h ^= (uint8_t)s[i];
        h *= 16777619u;
    }
    return h;
}

static uint32_t traceCrc(const WakeTraceRec &r) { return wakeTraceCrc(r); }

static bool traceValid(const WakeTraceRec &r) { return wakeTraceValid(r); }

WakeTraceRec *traceCurrent() {
    return rtcTraceActive ? &rtcTraceShadow : nullptr;
}

static void traceCommit(WakeTraceRec &r) {
    r.valid_end = TRACE_VALID_END;
    r.crc = traceCrc(r);
    rtcTraceShadow = r;
    rtcTraceShadowSeq = r.seq;
}

static void tracePrepare(WakeTraceRec &r) {
    rtcTraceShadow = r;
    rtcTraceShadowSeq = r.seq;
}

const char *traceStageName(uint8_t stage) {
    switch (stage) {
    case TRACE_STAGE_WIRELESS: return "wireless_start";
    case TRACE_STAGE_BLE_ADV: return "ble_broadcast";
    case TRACE_STAGE_BLE_CONNECTED: return "ble_connected";
    case TRACE_STAGE_COMMAND: return "command_received";
    case TRACE_STAGE_REPLY: return "reply_committed";
    case TRACE_STAGE_BLE_OFF: return "ble_off";
    case TRACE_STAGE_WIFI_IP: return "wifi_ip";
    case TRACE_STAGE_HTTP: return "http_result";
    case TRACE_STAGE_SLEEP: return "sleep";
    default: return "none";
    }
}

static const char *traceWakeTypeName(uint8_t type) {
    switch (type) {
    case TRACE_WAKE_TIMER: return "timer";
    case TRACE_WAKE_BUTTON: return "button";
    case TRACE_WAKE_THIN: return "thin";
    case TRACE_WAKE_BOOT: return "boot";
    default: return "other";
    }
}

static const char *traceResultName(uint8_t result) {
    switch (result) {
    case TRACE_RESULT_THIN: return "thin";
    case TRACE_RESULT_ANSWERED: return "answered";
    case TRACE_RESULT_NO_CONNECTION: return "no_connection";
    case TRACE_RESULT_NO_COMMAND: return "no_command";
    case TRACE_RESULT_WIFI_NO_IP: return "wifi_no_ip";
    case TRACE_RESULT_HTTP_FAILED: return "http_failed";
    case TRACE_RESULT_INTERRUPTED: return "interrupted";
    default: return "active";
    }
}

static const char *traceTransportName(uint8_t transport) {
    if ((transport & 3) == 3) return "ble+wifi";
    if (transport & 1) return "ble";
    if (transport & 2) return "wifi";
    return "none";
}

static uint8_t traceWakeType(esp_sleep_wakeup_cause_t cause) {
    if (cause == ESP_SLEEP_WAKEUP_TIMER) return TRACE_WAKE_TIMER;
    if (cause == ESP_SLEEP_WAKEUP_EXT1) return TRACE_WAKE_BUTTON;
    if (cause == ESP_SLEEP_WAKEUP_UNDEFINED) return TRACE_WAKE_BOOT;
    return TRACE_WAKE_OTHER;
}

static uint8_t traceExt1Summary() {
    const uint64_t bits = esp_sleep_get_ext1_wakeup_status();
    return (bits & (1ULL << 0) ? 1 : 0) | (bits & (1ULL << 18) ? 2 : 0);
}

void traceSetStage(uint8_t stage) {
    WakeTraceRec *r = traceCurrent();
    if (!r || stage < 1 || stage > TRACE_STAGE_COUNT) return;
    const uint16_t bit = (uint16_t)(1u << (stage - 1));
    tracePrepare(*r);
    r->valid_end = 0;
    if (!(r->stage_mask & bit)) {
        r->stage_mask |= bit;
        uint32_t elapsed = millis() - traceStartMs;
        uint32_t ticks = elapsed / 100u;
        r->timing[stage - 1] = ticks > 0xFFFFu ? 0xFFFFu : (uint16_t)ticks;
    }
    if (stage > r->furthest) r->furthest = stage;
    r->awake_ms = millis() - traceStartMs;
    traceCommit(*r);
}

void traceSetTransport(uint8_t transport) {
    WakeTraceRec *r = traceCurrent();
    if (!r || (r->transport & transport) == transport) return;
    tracePrepare(*r);
    r->valid_end = 0;
    r->transport |= transport;
    traceCommit(*r);
}

void traceSetRequest(const char *request) {
    WakeTraceRec *r = traceCurrent();
    if (!r || !request || !*request) return;
    tracePrepare(*r);
    r->valid_end = 0;
    r->request_crc = (uint16_t)fnv1a(String(request));
    traceCommit(*r);
}

static void traceSetError(uint8_t error) {
    WakeTraceRec *r = traceCurrent();
    if (!r || r->error) return;
    tracePrepare(*r);
    r->valid_end = 0;
    r->error = error;
    traceCommit(*r);
}

static void traceSetResult(uint8_t result, uint8_t error) {
    WakeTraceRec *r = traceCurrent();
    if (!r) return;
    tracePrepare(*r);
    r->valid_end = 0;
    r->result = result;
    if (error) r->error = error;
    traceCommit(*r);
}

void traceMarkHttp(bool ok, int code) {
    WakeTraceRec *r = traceCurrent();
    if (!r) return;
    traceSetTransport(2);
    if (!(r->stage_mask & (1u << (TRACE_STAGE_HTTP - 1)))) {
        traceSetStage(TRACE_STAGE_HTTP);
        if (!ok) traceSetResult(TRACE_RESULT_HTTP_FAILED,
                                code > 0 ? (uint8_t)code : TRACE_ERR_HTTP);
    }
}

void traceMarkThin() {
    WakeTraceRec *r = traceCurrent();
    if (!r) return;
    tracePrepare(*r);
    r->valid_end = 0;
    r->wake_type = TRACE_WAKE_THIN;
    r->result = TRACE_RESULT_THIN;
    traceCommit(*r);
}

static void traceFinishInterrupted() {
    WakeTraceRec *r = traceCurrent();
    if (!r) return;
    tracePrepare(*r);
    r->valid_end = 0;
    r->result = TRACE_RESULT_INTERRUPTED;
    if (!r->error) r->error = TRACE_ERR_RESET;
    traceCommit(*r);
    syncAppend(sync::retained.syncRtc, SYNC_WAKE_SUMMARY, 0, r->seq, r->awake_ms,
               reinterpret_cast<const uint8_t *>(r), sizeof(*r));
    rtcTraceActive = false;
}

void traceBegin(esp_sleep_wakeup_cause_t cause) {
    uint8_t diagGeneration[16];
    for (uint8_t i = 0; i < 16; i += 4) {
        uint32_t word = esp_random();
        memcpy(diagGeneration + i, &word, 4);
    }
    syncRecover(sync::retained.syncRtc, diagGeneration);
    sync::state.syncLogReady = true;
    if (rtcTraceMagic != TRACE_MAGIC) {
        histCount = 0;
        histHead = 0;
        retained.rtcWakeGeneration = esp_random();
        if (!retained.rtcWakeGeneration) retained.rtcWakeGeneration = 1;
        rtcTraceMagic = TRACE_MAGIC;
        retained.rtcTraceCurrentSeq = 0;
        rtcTraceActive = false;
    } else if (traceCurrent()) {
        // A reset can interrupt the small validity-marker update itself; the
        // slot is still the best available evidence and is finalized as such.
        traceFinishInterrupted();
    }
    if (!retained.rtcWakeGeneration) {
        retained.rtcWakeGeneration = esp_random();
        if (!retained.rtcWakeGeneration) retained.rtcWakeGeneration = 1;
    }
    WakeTraceRec &r = rtcTraceShadow;
    memset(&r, 0, sizeof(r));
    r.wake_generation = retained.rtcWakeGeneration;
    r.seq = ++histCount;
    r.wake_type = traceWakeType(cause);
    r.wake_cause = (uint8_t)cause;
    r.ext1 = traceExt1Summary();
    r.valid_start = TRACE_VALID_START;
    r.result = TRACE_RESULT_ACTIVE;
    traceStartMs = millis();
    histHead = (uint16_t)((histHead + 1) % HIST_CAP);
    rtcTraceActive = true;
    retained.rtcTraceCurrentSeq = r.seq;
    sync::state.syncLogWakeSeq = r.seq;
    traceBleConnectedSeen = false;
    traceCommit(r);
}

static uint8_t traceFinalResult() {
    WakeTraceRec *r = traceCurrent();
    if (!r) return TRACE_RESULT_INTERRUPTED;
    if (r->result == TRACE_RESULT_THIN || r->result == TRACE_RESULT_HTTP_FAILED) return r->result;
    if (r->stage_mask & (1u << (TRACE_STAGE_REPLY - 1))) return TRACE_RESULT_ANSWERED;
    if (r->stage_mask & (1u << (TRACE_STAGE_COMMAND - 1))) return TRACE_RESULT_ANSWERED;
    if (r->stage_mask & (1u << (TRACE_STAGE_BLE_CONNECTED - 1))) return TRACE_RESULT_NO_COMMAND;
    if (r->stage_mask & (1u << (TRACE_STAGE_BLE_ADV - 1))) return TRACE_RESULT_NO_CONNECTION;
    if (r->stage_mask & (1u << (TRACE_STAGE_WIFI_IP - 1))) return TRACE_RESULT_NO_COMMAND;
    return TRACE_RESULT_WIFI_NO_IP;
}

void traceFinishSleep() {
    WakeTraceRec *r = traceCurrent();
    if (!r) return;
    traceSetStage(TRACE_STAGE_SLEEP);
    traceSetResult(traceFinalResult());
    r = traceCurrent();
    if (r) {
        tracePrepare(*r);
        r->valid_end = 0;
        r->awake_ms = millis() - traceStartMs;
        traceCommit(*r);
    }
    if (r) syncAppend(sync::retained.syncRtc, SYNC_WAKE_SUMMARY, 0, r->seq, r->awake_ms,
                      reinterpret_cast<const uint8_t *>(r), sizeof(*r));
    rtcTraceActive = false;
}

void tracePollConnection() {
    const bool connected = bleIsConnected();
    if (connected && !traceBleConnectedSeen) {
        traceBleConnectedSeen = true;
        traceSetTransport(1);
        traceSetStage(TRACE_STAGE_BLE_CONNECTED);
    } else if (!connected) {
        traceBleConnectedSeen = false;
    }
}

// Old event calls remain source-compatible but no longer allocate records.
static void histAddFull(uint8_t ev, uint16_t aux, uint32_t, uint8_t) {
    if (ev == HIST_NET_OK) traceMarkHttp(true, aux);
    else if (ev == HIST_NET_FAIL) traceMarkHttp(false, aux);
    else if (ev == HIST_THIN) traceMarkThin();
}

void histAdd(uint8_t ev, uint16_t aux) { histAddFull(ev, aux, 0, 0); }

void bleRadioMark(bool on) {
    if (on && !bleOnSinceMs) {
        bleOnSinceMs = millis();
    } else if (!on && bleOnSinceMs) {
        bleOnAccumMs += millis() - bleOnSinceMs;
        bleOnSinceMs = 0;
    }
}

uint32_t bleRadioMs() {
    return bleOnAccumMs + (bleOnSinceMs ? millis() - bleOnSinceMs : 0);
}

static const char *wakeResultName(uint8_t result) {
    switch (result) {
        case WAKE_THIN: return "thin";
        case WAKE_RV_SLEEP: return "rendezvous-sleep";
        case WAKE_RV_LIGHT: return "rendezvous-light";
        case WAKE_NET: return "net";
        default: return "light";
    }
}

// ---------------- 正常模式 ----------------
const char *resetReasonName() {
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

const char *wakeCauseName(esp_sleep_wakeup_cause_t cause) {
    switch (cause) {
    case ESP_SLEEP_WAKEUP_UNDEFINED: return "power-on";
    case ESP_SLEEP_WAKEUP_EXT0:      return "ext0";
    case ESP_SLEEP_WAKEUP_EXT1:      return "ext1";
    case ESP_SLEEP_WAKEUP_TIMER:     return "timer";
    default:                         return "other";
    }
}

void handleStatus() {
    String html = F("<!DOCTYPE html><html><head><meta charset='utf-8'><title>Codex Status</title></head><body>");
    html += F("<h2>Codex Status</h2><ul>");
    html += String("<li>Version: ") + application::state.firmwareVersion + "</li>";
    const esp_partition_t *running = esp_ota_get_running_partition();
    const esp_partition_t *next = esp_ota_get_next_update_partition(nullptr);
    html += "<li>Running: " + String(running ? running->label : "?") +
            " (next OTA slot: " + String(next ? next->label : "?") + ")</li>";
    html += "<li>Reset reason: " + String(resetReasonName()) +
            " (uptime " + String(millis() / 1000) + "s)</li>";
    html += "<li>State: " + String(radio::deviceStateText()) + " (BLE " + String(radio::state.bleOn ? "session" : "off") +
            ", USB " + String(power::state.plugged ? "plugged" : "battery") + ")</li>";
    html += "<li>SSID: " + WiFi.SSID() + "</li>";
    html += "<li>IP: " + WiFi.localIP().toString() + "</li>";
    html += "<li>RSSI: " + String(WiFi.RSSI()) + " dBm</li>";
    html += "<li>Battery: " + String(power::batteryPercent()) + "% (" +
            String(power::batteryMilliVolts()) + " mV)</li>";
    html += "<li>BLE connected: " + String(bleIsConnected() ? "yes" : "no") + "</li>";
    html += "<li>Endpoints stored: " + String(storeCount()) + "</li>";
    html += "<li>Last channel: " + platform::state.lastChannel + "</li>";
    html += "<li>Last sync: " + clock::fmtEpoch((long long)radio::retained.rtcLastSyncEpoch, "%m-%d %H:%M") + "</li>";
    String activeId = tplStoreActive();
    TplMeta activeMeta;
    String activeHash = tplStoreFind(activeId, activeMeta) ? activeMeta.hash : "";
    html += "<li>Templates: " + String(tplStoreCount()) + " (active: " + activeId +
            (activeHash.length() ? String(" hash ") + activeHash : String("")) + ")</li>";
    html += "<li>EPD writes: " + String(display::state.epdWriteCount) + " (partial " +
            String(display::state.epdPartialReady ? "ready" : "off") + ", streak " + String(display::state.epdPartialCount) +
            ", busy_fails " + String(display::retained.rtcEpdBusyFails) + ", baseline " +
            String(display::state.epdBaselineTrusted ? "trusted" : "untrusted") + ")</li>";
    html += "<li>Wake: " + String(millis()) + " ms awake, BLE " + String(bleRadioMs()) +
            " ms, renders " + String(state.bootRenderCount) + ", time " +
            String(clock::timeSourceName(clock::state.timeSource)) + " (last: " +
            String(wakeResultName(retained.rtcLastWakeResult)) + " " + String(retained.rtcLastAwakeMs) +
            " ms, renders " + String(retained.rtcLastRenders) + ", time " +
            String(clock::timeSourceName(retained.rtcLastTimeSource)) + ")</li>";
    {
        OwnerRec ownerCur;
        if (ownerGet(ownerCur)) {
            html += "<li>Owner: " + ownerCur.name + " (" + ownerCur.id + ") " +
                    ownerCur.host + ":" + String(ownerCur.port) + "</li>";
        } else {
            html += "<li>Owner: -</li>";
        }
    }
    html += "<li>Free heap: " + String(ESP.getFreeHeap()) + "</li>";
    html += F("</ul><p>");
    if (transport::requestAuthorized()) {
        html += "<a href='/update?token=" + transport::state.authToken + "'>Firmware OTA update</a>";
    } else {
        html += "Firmware OTA: negotiate a token over BLE first (tools/device-auth)";
    }
    html += F("</p></body></html>");
    transport::state.server.send(200, "text/html", html);
}

void handleStatusJson() {
    JsonDocument doc;
    doc["fw"] = application::state.firmwareVersion;
    doc["ota_identity"] = application::state.imageIdentity;
    doc["mac"] = radio::macText();
    const esp_partition_t *running = esp_ota_get_running_partition();
    const esp_partition_t *next = esp_ota_get_next_update_partition(nullptr);
    doc["slot"] = running ? running->label : "?";
    doc["next_slot"] = next ? next->label : "?";
    doc["reset"] = resetReasonName();
    doc["wake"] = wakeCauseName(application::state.bootWakeCause);
    doc["pwr"] = digitalRead(18);
    doc["uptime_s"] = millis() / 1000;
    doc["ssid"] = radio::state.configMode ? radio::state.apSsid : WiFi.SSID();
    doc["ip"] = radio::ipText();
    doc["rssi"] = WiFi.RSSI();
    doc["state"] = radio::deviceStateText();
    doc["ble_on"] = radio::state.bleOn;
    doc["ble"] = bleIsConnected();
    doc["plugged"] = power::state.plugged;
    doc["wifi_state"] = radio::wifiStateText();
    doc["retry_stage"] = power::retained.rtcRetryStage;
    doc["last_push"] = radio::retained.rtcLastSyncEpoch;
    doc["endpoints"] = storeCount();
    doc["channel"] = platform::state.lastChannel;
    doc["pm_light_sleep"] = power::state.pmLightSleep;
    doc["ota"] = transport::state.otaInProgress;
    doc["battery"] = power::batteryPercent();
    doc["battery_mv"] = power::batteryMilliVolts();
    doc["heap"] = ESP.getFreeHeap();
#if defined(CODEX_TARGET_NOTE4)
    doc["heap_max_alloc"] = ESP.getMaxAllocHeap();
    doc["psram_free"] = ESP.getFreePsram();
#endif
    doc["epd_writes"] = display::state.epdWriteCount;
    doc["epd_partial"] = display::state.epdPartialReady;
    doc["epd_streak"] = display::state.epdPartialCount;
    doc["epd_busy_fails"] = display::retained.rtcEpdBusyFails;
    doc["epd_trusted"] = display::state.epdBaselineTrusted;
#if defined(CODEX_TARGET_NOTE4)
    doc["panel_power_mode"] = display::state.note4KeepPanelPower ? "keep" : "off_cache";
#endif
    doc["refresh_kind"] = display::state.rfnKind;
    doc["refresh_reason"] = display::state.rfnReason;
    doc["dirty_pixels"] = display::state.rfnDirty;
    doc["refresh_decisions"] = display::state.rfnDecisions;
    doc["refresh_ms"] = display::state.rfnLastMs;
    doc["blink_ms"] = radio::state.wifiBlinkMs;
    doc["blink_on"] = radio::state.wifiBlinkOn;
    doc["blink_ticks"] = radio::state.blinkTicks;
    doc["wifi_conn"] = radio::state.wifiConnActive;
    doc["rgn"] = display::state.rgnSet.n;
    doc["rgn_whole"] = display::state.rgnSet.wholeFrame;
    doc["rgn_policy"] = display::state.rgnPolicyOn;
    doc["wifi_slots"] = radio::countWifiSlots();
    doc["ap_reason"] = radio::state.AP_REASON_NAMES[radio::retained.rtcApReason < 3 ? radio::retained.rtcApReason : 0];
    // v0.14 deep/light mode (docs/power-state.md §13).
    doc["mode"] = (power::retained.rtcMode == MODE_DEEP) ? "deep" : "light";
    doc["idle_deep_s"] = power::state.idleDeepS;
    doc["next_contact_s"] = power::retained.rtcNextContactS;
    doc["usage_rev"] = platform::retained.rtcUsageRev;
    doc["clk"] = display::retained.clkR.valid;
    doc["clk_partials"] = display::retained.rtcClkPartials;
    doc["stage"] = retained.rtcStage;
    doc["last_wake_code"] = retained.rtcLastWake;
    doc["nvs_stage_boot"] = state.nvsStageAtBoot;
    doc["deep_usb"] = power::retained.rtcDeepOnUsb;
    doc["frame_capture"] = retained.rtcFrameCapture;
    doc["tz"] = clock::state.deviceTz;
    doc["hist_count"] = histCount;
    doc["hist_head"] = histHead;
    doc["wake_generation"] = retained.rtcWakeGeneration;
    doc["wake_seq"] = retained.rtcTraceCurrentSeq;
    const uint32_t wakeEarliestSeq = traceEarliestComplete();
    const bool wakeOverwritten = histCount > HIST_CAP;
    doc["wake_earliest_seq"] = wakeEarliestSeq;
    doc["wake_overwritten"] = wakeOverwritten;
    doc["earliest_seq"] = wakeEarliestSeq;
    doc["overwritten"] = wakeOverwritten;
    doc["wake_stage"] = traceCurrent() ? traceStageName(traceCurrent()->furthest) : "none";
    doc["wake_cause"] = wakeCauseName(application::state.bootWakeCause);
    // Plan C wake telemetry (task-3): live values of the current wake; the
    // previous wake's final values are in `deep.last_*` below.
    doc["awake_ms"] = millis();
    doc["ble_on_ms"] = bleRadioMs();
    doc["render_count"] = state.bootRenderCount;
    doc["time_source"] = clock::timeSourceName(clock::state.timeSource);
    clock::writeClockStatus(doc["clock_rtc"].to<JsonObject>());
    doc["render_ms"] = state.wakeRenderMs;
    // Post-OTA minimum light window remaining (0 = not in one), so the bridge
    // can verify the window it is responsible for extending.
    doc["post_ota_hold_s"] =
        power::state.postOtaHoldUntilMs && (int32_t)(millis() - power::state.postOtaHoldUntilMs) < 0
            ? (uint32_t)((power::state.postOtaHoldUntilMs - millis()) / 1000)
            : 0;
#if defined(CODEX_PM) && CONFIG_PM_LIGHT_SLEEP_CALLBACKS
    // Time-based power estimate (no current meter): light-sleep share lets the
    // estimator subtract it from the awake window instead of charging CPU mA.
    doc["light_sleep_ms"] = (uint32_t)(power::state.sleepDiag.total_us / 1000ULL);
    doc["light_sleep_count"] = power::state.sleepDiag.count;
#endif
    // ------------------------------------------------------------------
    // v2 platform state (device is authoritative; the Bridge reconciles).
    // ------------------------------------------------------------------
    doc["fw_target"] = FW_TARGET_ID;
    doc["wifi_setup_protocol"] = 1;
    doc["wifi_management"] = 3;
    doc["ble_recovery_remaining_s"] = power::v2ManualBleHoldRemainingS();
    doc["render_target"] = RENDER_TARGET_ID;
    doc["compiler_abi"] = CT_ABI;
    doc["width"] = TARGET_WIDTH;
    doc["height"] = TARGET_HEIGHT;
    doc["pixel_format"] = TARGET_PIXEL_FORMAT;
    doc["colors"] = TARGET_COLORS;
    doc["partial"] = bool(TARGET_PARTIAL);
    doc["hardware_verified"] = bool(TARGET_VERIFIED);
    doc["max_templates"] = 8;
    doc["max_bundle_bytes"] = BS_MAX_BUNDLE_BYTES;
    doc["asset_publish_protocol"] = 0; // Existing complete Bundle path only.
#ifdef CODEX_TARGET_NOTE4
    doc["bundle_font_protocol"] = 1;
#else
    doc["bundle_font_protocol"] = 0;
#endif
    doc["bundle_configured"] = platform::state.v2BundleReady;
    doc["commit_seq"] = (unsigned)bsCommitSeq();
    doc["active_context_id"] = platform::state.v2Profile.contextId;
    doc["active_template_id"] = (platform::state.v2BundleReady && platform::state.v2Profile.count)
                                    ? String(platform::state.v2Profile.ids[platform::state.v2Profile.initial])
                                    : tplStoreActive();
    doc["committed_job_id"] = platform::state.v2Profile.jobId;
    doc["data_seq"] = platform::state.v2DataSeq.appliedSeq();
    doc["applied_seq"] = platform::state.v2DataSeq.appliedSeq();
    doc["last_acked_at_ms"] = (unsigned long)platform::state.v2LastAckAtMs;
    doc["display_state"] = platform::state.v2DisplayState == 1 ? "displayed"
                          : platform::state.v2DisplayState == 2 ? "pending"
                          : platform::state.v2DisplayState == 3 ? "failed"
                                                : "unchanged";
    doc["template_count"] = platform::state.v2Profile.count;
    {
        JsonObject power = doc["power"].to<JsonObject>();
        power["mode"] = (power::retained.rtcMode == MODE_DEEP) ? "sleep" : "light";
        power["plan_id"] = power::state.v2Plan.acceptedId();
        power["remaining_s"] = power::state.v2Plan.remainingS(clock::v2NowMs());
        power["granted_s"] = power::state.v2Plan.grantedS();
        power["provisional"] = power::state.v2Provisional && !power::state.v2Plan.accepted();
        power["provisional_remaining_s"] =
            power::state.v2Provisional ? V2PlanState::bootProvisionalRemaining(power::state.v2BootMs, clock::v2NowMs()) : 0;
        power["manual_ble_hold_remaining_s"] = power::v2ManualBleHoldRemainingS();
        power["rendezvous_period_s"] = V2_RENDEZVOUS_S;
    }
    {
        uint32_t nowEpoch = clock::timeKnown() ? (uint32_t)time(nullptr) : 0;
        doc["next_contact_in_s"] = (power::retained.rtcNextNetAt && nowEpoch && power::retained.rtcNextNetAt > nowEpoch)
                                       ? (power::retained.rtcNextNetAt - nowEpoch)
                                       : 0;
        JsonObject deep = doc["deep"].to<JsonObject>();
        deep["clock_wakes"] = retained.rtcDeepCycles;
        deep["net_windows"] = retained.rtcNetCycles;
        deep["net_fails"] = rtcNetFails;
        deep["clock_ticks"] = display::retained.rtcClockTicks;
        deep["last_code"] = rtcLastPullCode;
        deep["last_contact"] = radio::retained.rtcLastSyncEpoch;
        deep["idle_s"] = power::state.lastActivityEpoch && nowEpoch ? (uint32_t)(nowEpoch - power::state.lastActivityEpoch) : 0;
        deep["glyph"] = retained.rtcDeepGlyph;
        deep["tpl_has_mode"] = platform::state.activeTplHasMode ? 1 : 0;
        deep["captures"] = rtcFrameCaptures;
        // Last completed wake (RTC snapshot taken at deep entry, task-3).
        deep["last_wake_result"] = wakeResultName(retained.rtcLastWakeResult);
        deep["last_awake_ms"] = retained.rtcLastAwakeMs;
        deep["last_ble_ms"] = retained.rtcLastBleMs;
        deep["last_renders"] = retained.rtcLastRenders;
        deep["last_time_source"] = clock::timeSourceName(retained.rtcLastTimeSource);
        // Cumulative deep-cycle totals for the time-based power estimate.
        deep["acc_cycles"] = retained.rtcAccCycles;
        deep["acc_awake_ms"] = retained.rtcAccAwakeMs;
        deep["acc_ble_ms"] = retained.rtcAccBleMs;
        deep["acc_render_ms"] = retained.rtcAccRenderMs;
    }
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
    OwnerRec ownerCur;
    if (ownerGet(ownerCur)) {
        JsonObject own = doc["owner"].to<JsonObject>();
        own["id"] = ownerCur.id;
        own["name"] = ownerCur.name;
        own["host"] = ownerCur.host;
        own["port"] = ownerCur.port;
        own["since_s"] = ownerCur.since;
        own["last_seen_s"] = ownerCur.lastSeen;
        own["lease_s"] = ownerCur.lease;
        own["expires_in_s"] = ownerRemainingS();
    } else {
        doc["owner"] = nullptr;
    }
    String out;
    serializeJson(doc, out);
    transport::state.server.send(200, "application/json", out);
}

void handleLog() {
    transport::state.server.send(200, "text/plain; charset=utf-8", DevLog.dump());
}

static bool traceGetComplete(uint32_t seq, WakeTraceRec &out) {
    if (!seq || seq > histCount) return false;
    if (traceCurrent() && rtcTraceShadow.seq == seq && traceValid(rtcTraceShadow)) {
        out = rtcTraceShadow;
        out.awake_ms = millis() - traceStartMs;
        return true;
    }
    for (uint16_t offset = 0, next = 0; offset < sync::retained.syncRtc.used; offset = next) {
        SyncRecord record;
        if (!syncReadAt(sync::retained.syncRtc, offset, record, next)) break;
        if (record.kind != SYNC_WAKE_SUMMARY || record.payloadLen != sizeof(out)) continue;
        memcpy(&out, record.payload, sizeof(out));
        if (out.seq == seq && traceValid(out)) return true;
    }
    return false;
}

static uint32_t traceEarliestComplete() {
    for (uint16_t offset = 0, next = 0; offset < sync::retained.syncRtc.used; offset = next) {
        SyncRecord record;
        if (!syncReadAt(sync::retained.syncRtc, offset, record, next)) break;
        if (record.kind != SYNC_WAKE_SUMMARY || record.payloadLen != sizeof(WakeTraceRec)) continue;
        WakeTraceRec wake;
        memcpy(&wake, record.payload, sizeof(wake));
        if (traceValid(wake)) return wake.seq;
    }
    if (traceCurrent() && traceValid(rtcTraceShadow)) return rtcTraceShadow.seq;
    return 0;
}

static uint32_t traceLatestComplete() {
    if (traceCurrent() && traceValid(rtcTraceShadow)) return rtcTraceShadow.seq;
    uint32_t latest = 0;
    for (uint16_t offset = 0, next = 0; offset < sync::retained.syncRtc.used; offset = next) {
        SyncRecord record;
        if (!syncReadAt(sync::retained.syncRtc, offset, record, next)) break;
        if (record.kind != SYNC_WAKE_SUMMARY || record.payloadLen != sizeof(WakeTraceRec)) continue;
        WakeTraceRec wake;
        memcpy(&wake, record.payload, sizeof(wake));
        if (traceValid(wake)) latest = wake.seq;
    }
    return latest;
}

static bool traceRecordActive(uint32_t seq) {
    return seq && seq == retained.rtcTraceCurrentSeq && traceCurrent() &&
           traceCurrent()->seq == seq;
}

static void traceAppendRow(JsonArray arr, const WakeTraceRec &r, bool compact) {
    if(compact){wakeTraceCompactRow(arr,r,!traceRecordActive(r.seq));return;}
    JsonObject item = arr.add<JsonObject>();
    item["format"] = TRACE_FORMAT;
    item["wake_generation"] = r.wake_generation;
    item["seq"] = r.seq;
    item["wake_type"] = traceWakeTypeName(r.wake_type);
    if (!compact) {
        item["wake_cause"] = wakeCauseName((esp_sleep_wakeup_cause_t)r.wake_cause);
        item["ext1"] = r.ext1;
    }
    item["awake_ms"] = r.awake_ms;
    item["result"] = traceResultName(r.result);
    item["complete"] = !traceRecordActive(r.seq);
    if (!compact) item["error"] = r.error;
    if (compact) item["stage"] = r.furthest;
    else item["stage"] = traceStageName(r.furthest);
    JsonArray timing = item["timing"].to<JsonArray>();
    for (uint8_t i = 0; i < TRACE_STAGE_COUNT; ++i)
        timing.add((uint32_t)r.timing[i] * 100u);
    if (!compact && r.request_crc) item["request_crc"] = r.request_crc;
    item["transport"] = traceTransportName(r.transport);
}

String traceHistoryJson(uint32_t since, uint8_t limit, size_t maxBytes,
                               bool &more) {
    if (!limit) limit = 1;
    if (limit > HIST_CAP) limit = HIST_CAP;
    const uint32_t latest = traceLatestComplete();
    const uint32_t earliest = traceEarliestComplete();
    JsonDocument doc;
    doc["wake_generation"] = retained.rtcWakeGeneration;
    doc["earliest_seq"] = earliest;
    doc["latest_seq"] = latest;
    JsonArray records = doc["records"].to<JsonArray>();
    more = false;
    uint8_t added = 0;
    if (earliest && latest) {
        for (uint32_t seq = earliest; seq <= latest; ++seq) {
            if (seq <= since) continue;
            WakeTraceRec r;
            if (!traceGetComplete(seq, r)) continue;
            if (added >= limit) { more = true; break; }
            traceAppendRow(records, r, true);
            String probe;
            serializeJson(doc, probe);
            if (probe.length() > maxBytes && added == 0) {
                records.remove(records.size() - 1);
                more = true;
                break;
            }
            if (probe.length() > maxBytes) {
                records.remove(records.size() - 1);
                more = true;
                break;
            }
            added++;
        }
    }
    doc["more"] = more;
    String out;
    serializeJson(doc, out);
    return out;
}

// GET /history keeps its array shape and incremental `since` semantics.
void handleHistory() {
    uint32_t since = 0;
    if (transport::state.server.hasArg("since")) {
        long long v = transport::state.server.arg("since").toInt();
        if (v > 0) since = (uint32_t)v;
    }
    JsonDocument doc;
    JsonArray arr = doc.to<JsonArray>();
    const uint32_t earliest = traceEarliestComplete();
    const uint32_t latest = traceLatestComplete();
    if (earliest && latest) {
        for (uint32_t seq = earliest; seq <= latest; ++seq) {
            if (seq <= since) continue;
            WakeTraceRec r;
            if (traceGetComplete(seq, r)) traceAppendRow(arr, r);
        }
    }
    String out;
    serializeJson(doc, out);
    transport::state.server.send(200, "application/json", out);
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

// Active esp_timer list (?timers=1), used to correlate light-sleep wakeups
// with the next timer alarm (see project-workflow/pmstats/task-2.md).
static String timerStatsText() {
#if defined(CODEX_PM)
    char *buf = nullptr;
    size_t len = 0;
    FILE *f = open_memstream(&buf, &len);
    if (!f) return String("timers: memstream failed");
    esp_timer_dump(f);
    fclose(f);
    String out = buf ? buf : "";
    free(buf);
    return out;
#else
    return String("stock core: no timer dump");
#endif
}

// Light-sleep diagnostics: sleep duration histogram + wakeup causes (task-2).
static String sleepDiagText() {
#if defined(CODEX_PM) && CONFIG_PM_LIGHT_SLEEP_CALLBACKS
    static const char *buckets[12] = {"<1ms", "1-2ms", "2-4ms", "4-6ms", "6-8ms",
                                      "8-10ms", "10-20ms", "20-50ms", "50-100ms",
                                      "100-500ms", "0.5-1s", ">=1s"};
    power::SleepDiag d;
    memcpy(&d, (const void *)&power::state.sleepDiag, sizeof(d));
    String out;
    out += "Sleep diag:\n";
    out += "  count=" + String(d.count) +
           " avg_us=" + String(d.count ? (double)d.total_us / d.count : 0.0, 1) +
           " min_us=" + String(d.min_us) + " max_us=" + String(d.max_us) +
           " last_us=" + String(d.last_us) + "\n";
    out += "  wake_cause: timer=" + String(d.causes[0]) + " wifi=" + String(d.causes[1]) +
           " gpio=" + String(d.causes[2]) + " other=" + String(d.causes[3]) + "\n";
    out += "  last_expected_us=" + String(d.last_expected_us) +
           " last_next_alarm_gap_us=" + String(d.last_next_alarm_us) + "\n";
    out += "  slept_hist:";
    for (int i = 0; i < 12; i++) {
        if (d.hist[i]) out += String(" ") + buckets[i] + "=" + String(d.hist[i]);
    }
    out += "\n";
    return out;
#else
    return String("Sleep diag: disabled (CONFIG_PM_LIGHT_SLEEP_CALLBACKS off)\n");
#endif
}

// FreeRTOS task snapshot + run-time stats (stats formatting functions are on).
static String taskStatsText() {
#if defined(CODEX_PM)
    const size_t size = 8192;
    char *buf = (char *)malloc(size);
    if (!buf) return String("Tasks: no mem\n");
    String out = "Tasks (name, state, prio, stack free, num):\n";
    vTaskList(buf);
    out += buf;
    out += "\nRun time (abs, %):\n";
    vTaskGetRunTimeStats(buf);
    out += buf;
    free(buf);
    return out;
#else
    return String("Tasks: stock core\n");
#endif
}

void handlePmStats() {
    String text = pmStatsText();
    if (transport::state.server.hasArg("timers")) {
        text += "\n";
        text += timerStatsText();
    }
    if (transport::state.server.hasArg("diag")) {
        text += "\n";
        text += sleepDiagText();
        text += "\n";
        text += taskStatsText();
        text += "\n";
        text += timerStatsText();
    }
    DevLog.printf("[pm] stats over HTTP (%u bytes)\n", (unsigned)text.length());
    transport::state.server.send(200, "text/plain; charset=utf-8", text);
}

// Debug capture: save the framebuffer as binary PBM (P4; 1 = black) right after
// a pre-sleep refresh, so a deep frame can be inspected later (the panel itself
// is unreachable while sleeping and the SSD1681 has no read-back on this board).
// Gated by rtcFrameCapture (off by default); fixed files, overwritten each time.
void captureFrameToFs(const char *tag) {
    if (!retained.rtcFrameCapture || !display::state.frame) return;
    if (!LittleFS.exists("/frames")) LittleFS.mkdir("/frames");
    File f = LittleFS.open("/frames/last.pbm", FILE_WRITE);
    if (!f) {
        DevLog.println("[cap] open failed");
        return;
    }
    f.printf("P4\n%d %d\n", EPD_W, EPD_H);
    uint8_t buf[256];
    for (int off = 0; off < EPD_FB_BYTES; off += (int)sizeof(buf)) {
        int n = EPD_FB_BYTES - off;
        if (n > (int)sizeof(buf)) n = (int)sizeof(buf);
        for (int i = 0; i < n; i++) buf[i] = (uint8_t)(~display::state.frame[off + i] & 0xFF);
        f.write(buf, n);
    }
    f.close();
    File m = LittleFS.open("/frames/meta.txt", FILE_WRITE);
    if (m) {
        m.printf("tag=%s mode=%s stage=%u epoch=%lu\n", tag,
                 power::retained.rtcMode == MODE_DEEP ? "deep" : "light", (unsigned)retained.rtcStage,
                 (unsigned long)time(nullptr));
        m.close();
    }
    rtcFrameCaptures++;
    DevLog.printf("[cap] frame saved (%s)\n", tag);
}

// Debug screenshot: dump a framebuffer as binary PBM (P4; 1 = black).
// `?which=frame` (default) = live buffer, `last` = last pushed to the panel,
// `saved` = the pre-sleep capture from LittleFS. Read-only, token-free like /log.
void handleFrame() {
    String which = transport::state.server.hasArg("which") ? transport::state.server.arg("which") : String("frame");
    if (which == "saved") {
        if (!LittleFS.exists("/frames/last.pbm")) {
            transport::state.server.send(404, "text/plain", "no captured frame");
            return;
        }
        File f = LittleFS.open("/frames/last.pbm", FILE_READ);
        if (!f) {
            transport::state.server.send(500, "text/plain", "open failed");
            return;
        }
        transport::state.server.streamFile(f, "application/x-portable-bitmap");
        f.close();
        return;
    }
    UBYTE *src = display::state.frame;
    if (which == "last") src = display::state.lastDisplayedFrame;
    if (!src) {
        transport::state.server.send(503, "text/plain", "no frame buffer");
        return;
    }
    String out;
    out.reserve(EPD_FB_BYTES + 16);
    out = "P4\n";
    out += EPD_W;
    out += ' ';
    out += EPD_H;
    out += '\n';
    for (int i = 0; i < EPD_FB_BYTES; i++) out += (char)(~src[i] & 0xFF);
    transport::state.server.send(200, "application/x-portable-bitmap", out);
}

// Token-gated runtime knob for task-2 experiments: POST /diag?loop_delay=20
// changes the loop() yield without reflashing. Diagnostic-only endpoint.
void handleDiag() {
    if (!transport::requestAuthorized()) {
        transport::state.server.send(401, "text/plain", "unauthorized");
        return;
    }
#if defined(CODEX_TARGET_NOTE4)
    if (transport::state.server.hasArg("realign")) {
        const String variant = transport::state.server.arg("realign");
        const long run = transport::state.server.arg("run_id").toInt();
        if (variant.length() != 1 || (variant[0] != 'a' && variant[0] != 'b' && variant[0] != 'c') ||
            run <= 0) {
            transport::state.server.send(400, "text/plain", "realign must be a|b|c and run_id positive");
            return;
        }
        if (bleInitialized()) {
            transport::state.server.send(409, "text/plain", "BLE busy");
            return;
        }
        const String result = bleRecoveryTrialJson(variant[0], (uint32_t)run);
        transport::state.server.send(200, "application/json", result);
        return;
    }
    if (transport::state.server.hasArg("panel_power")) {
        const String mode = transport::state.server.arg("panel_power");
        if (mode != "keep" && mode != "off_cache") {
            transport::state.server.send(400, "text/plain", "panel_power must be keep|off_cache");
            return;
        }
        if (!display::setNote4PanelPower(mode == "keep")) {
            transport::state.server.send(500, "text/plain", "panel_power NVS write failed");
            return;
        }
        if (display::state.epdAsleep) digitalWrite(EPD_PWR_PIN, display::state.note4KeepPanelPower ? HIGH : LOW);
        DevLog.printf("[diag] panel_power=%s\n", mode.c_str());
    }
#endif
    // Diagnostic BLE scan (Plan C task-6 §1.1): independent receiver for the
    // Windows publisher spike / bridge_first SCAN half. Blocking for the scan
    // duration; keep the client timeout above it.
    if (transport::state.server.hasArg("blescan")) {
        long sec = transport::state.server.arg("blescan").toInt();
        if (sec < 1 || sec > 30) {
            transport::state.server.send(400, "text/plain", "blescan out of range 1..30");
            return;
        }
        long company = transport::state.server.hasArg("company") ? transport::state.server.arg("company").toInt() : 65535;
        if (company < 0 || company > 65535) {
            transport::state.server.send(400, "text/plain", "company out of range 0..65535");
            return;
        }
        String out = bleScanJson((uint32_t)sec, (uint16_t)company, 48);
        DevLog.printf("[diag] blescan %lds company=%ld -> %u bytes\n",
                      sec, company, (unsigned)out.length());
        transport::state.server.send(200, "application/json", out);
        return;
    }
    // Remote recovery for a stuck/aborted OTA (UpdateClass left "running").
    if (transport::state.server.hasArg("ota_abort")) {
        transport::otaUploadCleanup("diag");
        transport::state.server.send(200, "text/plain", "ota aborted");
        return;
    }
    if (transport::state.server.hasArg("loop_delay")) {
        long v = transport::state.server.arg("loop_delay").toInt();
        if (v < 1 || v > 500) {
            transport::state.server.send(400, "text/plain", "loop_delay out of range 1..500");
            return;
        }
        power::state.loopDelayMs = (uint32_t)v;
        DevLog.printf("[diag] loop_delay=%u ms\n", (unsigned)power::state.loopDelayMs);
    }
    if (transport::state.server.hasArg("idle_deep_s")) {
        long v = transport::state.server.arg("idle_deep_s").toInt();
        if (v < 15 || v > 86400) {
            transport::state.server.send(400, "text/plain", "idle_deep_s out of range 15..86400");
            return;
        }
        power::state.idleDeepS = (uint32_t)v;
        power::noteActivity("diag");
        DevLog.printf("[diag] idle_deep_s=%u\n", (unsigned)power::state.idleDeepS);
    }
    if (transport::state.server.hasArg("nvs_stage")) {
        nvsStageEnabled = transport::state.server.arg("nvs_stage").toInt() ? 1 : 0;
        nvsStageWrites = 0;
        DevLog.printf("[diag] nvs_stage=%u\n", (unsigned)nvsStageEnabled);
    }
    if (transport::state.server.hasArg("deep_usb")) {
        power::retained.rtcDeepOnUsb = transport::state.server.arg("deep_usb").toInt() ? 1 : 0;
        DevLog.printf("[diag] deep_usb=%u\n", (unsigned)power::retained.rtcDeepOnUsb);
    }
    if (transport::state.server.hasArg("frame_capture")) {
        retained.rtcFrameCapture = transport::state.server.arg("frame_capture").toInt() ? 1 : 0;
        DevLog.printf("[diag] frame_capture=%u\n", (unsigned)retained.rtcFrameCapture);
    }
    // Display-safety layer controls (design §8).
    if (transport::state.server.hasArg("policy")) {
        display::state.rgnPolicyOn = strcmp(transport::state.server.arg("policy").c_str(), "off") != 0;
        DevLog.printf("[diag] region policy %s\n", display::state.rgnPolicyOn ? "on" : "off");
    }
    if (transport::state.server.hasArg("clean")) {
        display::state.forceCleanRefresh = transport::state.server.arg("clean").toInt() != 0;
        if (display::state.forceCleanRefresh) display::renderCurrent();
    }
    if (transport::state.server.hasArg("busy_fail")) {
        display::retained.rtcEpdBusyFails++;
        display::state.epdBaselineTrusted = false;
        DevLog.println("[diag] injected display failure: baseline untrusted");
    }
    // task-10 B measurement/ops knobs: `blink_ms` sets the connect-blink phase
    // (0 disables), `blink_test=N` runs N blink ticks on the live template and
    // reports the per-tick waveform cost so the shipped period is measured.
    if (transport::state.server.hasArg("blink_ms")) {
        long v = transport::state.server.arg("blink_ms").toInt();
        if (v < 0 || v > 10000) {
            transport::state.server.send(400, "text/plain", "blink_ms out of range 0..10000");
            return;
        }
        radio::state.wifiBlinkMs = (uint16_t)v;
        DevLog.printf("[diag] blink_ms=%u\n", (unsigned)radio::state.wifiBlinkMs);
    }
    if (transport::state.server.hasArg("blink_test")) {
        long n = transport::state.server.arg("blink_test").toInt();
        if (n < 1 || n > 60) {
            transport::state.server.send(400, "text/plain", "blink_test out of range 1..60");
            return;
        }
        uint32_t total = 0, worst = 0, sumRefresh = 0;
        radio::state.wifiConnActive = true;
        for (long i = 0; i < n; i++) {
            radio::state.wifiBlinkOn = !radio::state.wifiBlinkOn;
            display::state.rfnBlink = true;
            uint32_t t0 = millis();
            display::renderCurrent();
            uint32_t dt = millis() - t0;
            total += dt;
            sumRefresh += display::state.rfnLastMs;
            if (dt > worst) worst = dt;
        }
        radio::state.wifiConnActive = false;
        radio::state.wifiBlinkOn = false;
        display::renderCurrent();   // restore the real link state on the panel
        DevLog.printf("[diag] blink_test n=%ld avg=%ums worst=%ums refresh_avg=%ums\n",
                      n, (unsigned)(total / (uint32_t)n), (unsigned)worst,
                      (unsigned)(sumRefresh / (uint32_t)n));
        transport::state.server.send(200, "text/plain",
                    "blink_test n=" + String(n) +
                    " avg_ms=" + String(total / (uint32_t)n) +
                    " worst_ms=" + String(worst) +
                    " refresh_avg_ms=" + String(sumRefresh / (uint32_t)n) +
                    " blink_ticks=" + String((unsigned)radio::state.blinkTicks) + "\n");
        return;
    }
    if (transport::state.server.hasArg("rgn")) {
        JsonDocument doc;
        JsonArray arr = doc.to<JsonArray>();
        for (uint8_t i = 0; i < display::state.rgnSet.n; i++) {
            Rgn &r = display::state.rgnSet.r[i];
            JsonObject o = arr.add<JsonObject>();
            o["cls"] = rgnClassName(r.cls);
            o["hi"] = r.highInk;
            o["x0"] = r.px0; o["x1"] = r.px1; o["y0"] = r.py0; o["y1"] = r.py1;
            o["bx0"] = r.x0b; o["bx1"] = r.x1b;
            o["area"] = r.area;
            o["changed"] = r.changed; o["w2b"] = r.w2b; o["b2w"] = r.b2w;
            o["bOld"] = r.bOld; o["bNew"] = r.bNew;
            o["budget"] = r.budget; o["partials"] = r.partials; o["cumS"] = r.cumS;
        }
        String out;
        serializeJson(doc, out);
        transport::state.server.send(200, "application/json", out);
        return;
    }
    // Debug: render the template as deep/light without sleeping (RAM only, not
    // persisted) so the sleep glyph can be inspected while the device is online.
    if (transport::state.server.hasArg("render_mode")) {
        String m = transport::state.server.arg("render_mode");
        if (m == "deep") power::retained.rtcMode = MODE_DEEP;
        else if (m == "light") power::retained.rtcMode = MODE_LIGHT;
        else {
            transport::state.server.send(400, "text/plain", "render_mode must be deep|light");
            return;
        }
        display::renderCurrent();
        DevLog.printf("[diag] render_mode=%s\n", m.c_str());
    }
    if (transport::state.server.hasArg("tz")) {
        String tz = transport::state.server.arg("tz");
        tz.trim();
        if (tz.length() == 0 || tz.length() >= (int)sizeof(clock::state.deviceTz)) {
            transport::state.server.send(400, "text/plain", "tz must be a POSIX TZ string, e.g. CST-8");
            return;
        }
        strncpy(clock::state.deviceTz, tz.c_str(), sizeof(clock::state.deviceTz) - 1);
        clock::state.deviceTz[sizeof(clock::state.deviceTz) - 1] = '\0';
        clock::applyTimezone();
        Preferences p;
        p.begin("pm", false);
        p.putString("tz", clock::state.deviceTz);
        p.end();
        DevLog.printf("[diag] tz=%s\n", clock::state.deviceTz);
        display::renderCurrent();
    }
    bool deepNow = transport::state.server.hasArg("deep_now") && transport::state.server.arg("deep_now").toInt();
    if (deepNow) {
        power::noteActivity("diag");   // clears any pending forceDeepAt, so set it after
        power::state.forceDeepAtMs = millis() + 1500;
        DevLog.println("[diag] deep_now scheduled");
    }
    transport::state.server.send(200, "text/plain", "loop_delay=" + String(power::state.loopDelayMs) + " ms, idle_deep_s=" +
                                          String(power::state.idleDeepS) + " s, nvs_stage=" +
                                          String((unsigned)nvsStageEnabled) + ", deep_usb=" +
                                          String((unsigned)power::retained.rtcDeepOnUsb) + ", deep_now=" +
                                          String(deepNow ? 1 : 0) + ", tz=" + clock::state.deviceTz +
                                          ", frame_capture=" + String((unsigned)retained.rtcFrameCapture) +
                                          ", blink_ms=" + String((unsigned)radio::state.wifiBlinkMs) +
#if defined(CODEX_TARGET_NOTE4)
                                          ", panel_power=" + String(display::state.note4KeepPanelPower ? "keep" : "off_cache") +
#endif
                                          "\n\n" + sleepDiagText());
}

// Keep the snapshot builder in a separate Xtensa frame: the HTTP handler
// must stay below the 1 KiB frame budget without stacking another full state.
static __attribute__((noinline)) void buildPlatformStatus(JsonDocument &doc) {
    V2StatusSnapshot snapshot;
    snapshot.mac = radio::macText();
    snapshot.firmwareVersion = application::state.firmwareVersion;
    snapshot.sessionNonce = transport::v2Nonce();
    snapshot.profile = &platform::state.v2Profile;
    snapshot.configured = platform::state.v2BundleReady;
    snapshot.compiled = platform::state.v2CtValid ? &platform::state.v2Ct : nullptr;
    snapshot.activeTemplateId = (platform::state.v2BundleReady && platform::state.v2Profile.count)
        ? platform::state.v2Profile.ids[platform::state.v2Profile.initial] : platform::state.activeTplId;
    snapshot.dataSeq = &platform::state.v2DataSeq;
    snapshot.displayState = platform::state.v2DisplayState;
    snapshot.commitSeq = bsCommitSeq();
    snapshot.deepSleep = power::retained.rtcMode == MODE_DEEP;
    snapshot.plan = &power::state.v2Plan;
    snapshot.provisional = power::state.v2Provisional;
    snapshot.bootMs = power::state.v2BootMs;
    snapshot.nowMs = clock::v2NowMs();
    snapshot.battery = power::batteryPercent();
    // The shared builder stays authoritative; the local wake diagnostics ride
    // along as an optional block so the host/simulator document is unchanged.
    V2WakeSnapshot wake;
    wake.generation = retained.rtcWakeGeneration;
    wake.seq = retained.rtcTraceCurrentSeq;
    wake.stage = traceCurrent() ? traceStageName(traceCurrent()->furthest) : "none";
    wake.cause = wakeCauseName(application::state.bootWakeCause);
    snapshot.wake = &wake;
    deserializeJson(doc, v2BuildStatusSnapshot(snapshot));
}

// GET /api/status: authenticated authoritative state (the beacon only points).
// Business channel auth = endpoint token (same trust as /usage and /template);
// the device operation token keeps gating /claim, /update, /doUpdate, /diag.
static __attribute__((noinline)) void pageStatus(String &out, const String &token) {
    char id[33];
    snprintf(id, sizeof(id), "%08x%08x%08x%08x", (unsigned)esp_random(),
             (unsigned)esp_random(), (unsigned)esp_random(), (unsigned)esp_random());
    out = statusPages.begin(out, token, id, clock::v2NowMs());
    DevLog.println("[platform] status paged");
}

void handleV2Status() {
    DevLog.printf("[platform] req status heap=%u", (unsigned)ESP.getFreeHeap());
    String mac;
    if (!transport::endpointTokenAuthorized(mac)) {
        traceMarkHttp(false, 401);
        transport::v2Response(401, "{\"result\":\"unauthorized\"}");
        return;
    }
    traceMarkHttp(true, 200);
    JsonDocument options;
    if (deserializeJson(options, transport::state.v2HttpBody)) {
        transport::v2Response(400, "{\"result\":\"rejected\",\"error\":\"payload\"}"); return;
    }
    const String readId = options["status_read"] | "";
    const String readToken = transport::state.server.header("Authorization");
    if (readId.length()) {
        if (!options["offset"].is<uint32_t>()) {
            transport::v2Response(400, "{\"result\":\"rejected\",\"error\":\"status_offset\"}"); return;
        }
        String page = statusPages.read(readToken, readId, options["offset"].as<uint32_t>(), clock::v2NowMs());
        transport::v2Response(page.indexOf("\"rejected\"") >= 0 ? 409 : 200, page); return;
    }
    statusPages.clear();
    // Read-only Status does not advance the Data contact clock or owner lease.
    JsonDocument doc;
    buildPlatformStatus(doc);
    const esp_partition_t *currentPartition = esp_ota_get_running_partition();
    doc["firmware_target"] = FW_TARGET_ID;
    doc["running_slot"] = currentPartition ? currentPartition->label : nullptr;
    doc["reset_reason"] = (int)esp_reset_reason();
    doc["boot_id"] = transport::v2Nonce();
    doc["uptime_ms"] = millis();
    if (clock::timeKnown()) doc["sampled_wall"] = (int64_t)time(nullptr);
    else doc["sampled_wall"] = nullptr;
    doc["heap_free"] = ESP.getFreeHeap();
    doc["heap_min"] = ESP.getMinFreeHeap();
    JsonObject radio = doc["radio"].to<JsonObject>();
    radio["wifi_connected"] = WiFi.status() == WL_CONNECTED;
    radio["rssi"] = WiFi.status() == WL_CONNECTED ? WiFi.RSSI() : 0;
    radio["ble_connected"] = bleIsConnected();
    JsonObject display = doc["display"].to<JsonObject>();
    display["epd_writes"] = display::state.epdWriteCount;
    display["epd_busy_fails"] = display::retained.rtcEpdBusyFails;
    JsonObject rendezvous = doc["rendezvous"].to<JsonObject>();
    rendezvous["v"] = 1;
    rendezvous["id"] = radio::state.v2RvId;
    rendezvous["ready"] = radio::state.v2Rv.ready() && WiFi.status() == WL_CONNECTED;
    rendezvous["wake_generation"] = retained.rtcWakeGeneration;
    rendezvous["wake_seq"] = retained.rtcTraceCurrentSeq;
    rendezvous["remaining_ms"] = radio::state.v2Rv.active() && radio::state.v2Rv.deadline() > clock::v2NowMs() ? radio::state.v2Rv.deadline()-clock::v2NowMs() : 0;
    OwnerRec activeOwner;
    doc["owner_id"] = ownerGet(activeOwner) ? activeOwner.id : "";
    EndpointRec endpoint;
    if(storeAuthenticate(transport::state.server.header("Authorization").substring(7),endpoint))radio::wifiDeviceStatus(doc,&endpoint);
    doc["wifi_management"] = 3;
    doc["ota_auth"] = "token";
    doc["image_identity"].to<JsonArray>().add("sha256-running-prefix-v1");
    if (syncStoreAvailable()) {
        doc["sync_v1"] = 1;
        doc["diag_format"] = 1;
        doc["diag_capacity"] = 4096;
    }
    JsonObject sync = doc["sync"].to<JsonObject>();
    OwnerRec owner;
    sync["enabled"] = sync::state.syncEnabled && ownerGet(owner) && owner.id == sync::state.syncOwner;
    sync["phase"] = sync::state.syncOpenThisWake ? "WIFI_SYNC_ONCE" :
                    power::retained.rtcMode == MODE_LIGHT ? "WIFI_LIGHT" : "DEEP";
    const uint64_t syncUntil = sync::state.syncOpenAnsweredAt + 90000;
    sync["remaining_ms"] = sync::state.syncOpenThisWake && syncUntil > clock::v2NowMs() ? syncUntil-clock::v2NowMs() : 0;
    sync["rounds"] = sync::retained.syncRtc.rounds;
    sync["due"] = (sync::retained.syncRtc.flags & 1) != 0;
    sync["retry_skip"] = sync::retained.syncRtc.retrySkip;
    sync["baseline"] = (sync::retained.syncRtc.flags & 2) ? "known" : "unknown";
    sync["diag_earliest_seq"] = sync::syncNumber(syncEarliestSeq(sync::retained.syncRtc));
    sync["diag_next_seq"] = sync::syncNumber(sync::retained.syncRtc.nextSeq);
    JsonArray pendingReasons = sync["reasons"].to<JsonArray>();
    const char *reasonNames[5] = {"periodic", "light_enter", "light_exit", "ota_confirm", "bundle_confirm"};
    for (uint8_t i = 1; i < 5; ++i)
        if ((sync::retained.syncRtc.reasonBits & (1u << i)) &&
            (i != 3 || (sync::retained.syncRtc.flags & 1))) pendingReasons.add(reasonNames[i]);
    SyncStoreBatch batch;
    if (syncStoreActive(batch)) {
        if (syncStoreLost()) { sync::syncReply("sync_begin", "rejected", "batch_lost", 503); return; }
        JsonDocument fields;
        sync::syncBatchFields(fields, batch);
        sync["pending_batch"] = fields.as<JsonVariantConst>();
    } else sync["pending_batch"] = nullptr;
    if (syncStoreReceipt(batch)) {
        JsonDocument fields;
        sync::syncBatchFields(fields, batch);
        sync["last_completed"] = fields.as<JsonVariantConst>();
    } else sync["last_completed"] = nullptr;
    sync["last_error"] = syncStoreLost() ? "batch_lost" : nullptr;
    sync["confirmation_pending"] = false;
    wifiSchemesStorageStatus(doc["wifi_storage"].to<JsonObject>());
    clock::writeClockStatus(doc["clock_rtc"].to<JsonObject>());
    if (options["compact"] == true) {
        doc["status_projection"] = "compact";
        doc.remove("data_contract");
        doc.remove("wifi_storage");
        doc.remove("clock_rtc");
        doc.remove("sync");
        doc.remove("radio");
    }
    String out;
    serializeJson(doc, out);
    if (out.length() > 4096 - COMPACT_FRAME_HEADER) {
        doc.clear();
        pageStatus(out, readToken);
    }
    transport::v2Response(200, out);
}

static void dumpPmStats() {
    DevLog.printf("[pm] stats:\n%s", pmStatsText().c_str());
}

// USB Wi-Fi writes: `wifi-json <authenticated apply/result JSON>` uses the
// same transaction/permit/owner gate as HTTP; legacy `wifi` never writes.
// `status` prints IP/RSSI/heap/state; `batt` prints battery; `pair` opens a
// BLE session with a pairing window; `pmstats` dumps PM light-sleep counters.
void handleSerialCli() {
    static String rx;
    static bool overflow = false;
    while (Serial.available()) {
        char c = (char)Serial.read();
        if (c == '\r') continue;
        if (c != '\n') {
            if (rx.length() < 8204) rx += c; else overflow = true;
            continue;
        }
        if (overflow) { if (rx.length()) memset(const_cast<char *>(rx.c_str()), 0, rx.length()); rx = ""; overflow = false; Serial.println("Request too long"); continue; }
        String line = rx;
        if (rx.length()) memset(const_cast<char *>(rx.c_str()), 0, rx.length());
        rx = "";
        line.trim();
        if (line.startsWith("wifi-json ")) {
            JsonDocument request;
            String body = line.substring(10);
            bool authorized = false;
            if (body.length() <= 8192 && !deserializeJson(request, body)) {
                const String token = request["token"] | "";
                for (int i = 0; i < storeCount(); ++i) {
                    EndpointRec endpoint;
                    if (storeGet(i, endpoint) && token.length() && token == endpoint.token) authorized = true;
                }
            }
            if (!authorized) Serial.println("{\"ok\":false,\"error\":\"unauthorized\"}");
            else {
                const String op = request["op"] | "";
                if (op != "apply" && op != "result") Serial.println("{\"ok\":false,\"error\":\"shape\"}");
                else Serial.println(radio::wifiManagementRequest(request, op.c_str()));
            }
            if (body.length()) memset(const_cast<char *>(body.c_str()), 0, body.length());
        } else if (line.startsWith("wifi ")) {
            Serial.println("Legacy Wi-Fi writes retired. Use Bridge bonded setup or authenticated wifi-json transactions.");
        } else if (line == "status") {
            DevLog.printf("[cli] fw=%s state=%s ip=%s rssi=%d heap=%u\n",
                          application::state.firmwareVersion, radio::deviceStateText(), radio::ipText().c_str(),
                          WiFi.RSSI(), ESP.getFreeHeap());
#if defined(CODEX_TARGET_NOTE4)
            const esp_partition_t *running = esp_ota_get_running_partition();
            DevLog.printf("[cli] slot=%s epd_writes=%u busy_fails=%u trusted=%d\n",
                          running ? running->label : "?", (unsigned)display::state.epdWriteCount,
                          (unsigned)display::retained.rtcEpdBusyFails, display::state.epdBaselineTrusted ? 1 : 0);
            uint32_t black = 0;
            if (display::state.frame) for (int i = 0; i < EPD_FB_BYTES; ++i)
                black += __builtin_popcount((unsigned char)~display::state.frame[i]);
            const esp_partition_t *storage = esp_partition_find_first(
                ESP_PARTITION_TYPE_DATA, ESP_PARTITION_SUBTYPE_ANY, "storage");
            DevLog.printf("[cli] frame_black=%u flash=%u storage=%s\n",
                          (unsigned)black, (unsigned)ESP.getFlashChipSize(),
                          storage ? "found" : "missing");
#endif
#if defined(CODEX_TARGET_NOTE4)
        } else if (line == "log") {
            Serial.print(DevLog.dump());
        } else if (line == "panelpower" || line == "panelpower keep" ||
                   line == "panelpower off_cache") {
            if (line != "panelpower") {
                const bool keep = line == "panelpower keep";
                if (!display::setNote4PanelPower(keep)) {
                    DevLog.println("[cli] panel power NVS write failed");
                } else {
                    if (display::state.epdAsleep) digitalWrite(EPD_PWR_PIN, keep ? HIGH : LOW);
                    DevLog.printf("[cli] panel_power=%s\n", keep ? "keep" : "off_cache");
                }
            } else {
                DevLog.printf("[cli] panel_power=%s\n",
                              display::state.note4KeepPanelPower ? "keep" : "off_cache");
            }
#endif
        } else if (line == "batt") {
            DevLog.printf("[cli] battery=%d%% (%u mV)\n", power::batteryPercent(),
                          (unsigned)power::batteryMilliVolts());
        } else if (line == "pair") {
            radio::enterBleOn(true);
            DevLog.println("[cli] BLE session on, pairing window 120s");
        } else if (line == "deep") {
            DevLog.println("[cli] deep sleep now");
            power::enterDeep("cli");
        } else if (line == "light") {
            power::retained.rtcMode = MODE_LIGHT;
            power::state.forceDeepAtMs = 0;
            power::persistMode();
            power::noteActivity("cli");
            DevLog.println("[cli] light mode");
        } else if (line == "deepusb on") {
            power::retained.rtcDeepOnUsb = 1;
            DevLog.println("[cli] deep_usb=1 (deep sleep allowed while plugged)");
        } else if (line == "deepusb off") {
            power::retained.rtcDeepOnUsb = 0;
            DevLog.println("[cli] deep_usb=0");
        } else if (line == "framecap on") {
            retained.rtcFrameCapture = 1;
            DevLog.println("[cli] frame_capture=1 (pre-sleep frame -> /frames/last.pbm)");
        } else if (line == "framecap off") {
            retained.rtcFrameCapture = 0;
            DevLog.println("[cli] frame_capture=0");
        } else if (line == "pmstats") {
            dumpPmStats();
        } else if (line.startsWith("blescan")) {
            long sec = line.length() > 8 ? line.substring(8).toInt() : 10;
            if (sec < 1 || sec > 30) sec = 10;
            DevLog.printf("[cli] blescan %lds\n", sec);
            DevLog.printf("[ble] scan: %s\n", bleScanJson((uint32_t)sec, 0xFFFF, 48).c_str());
        } else if (line == "timers") {
            DevLog.printf("[pm] timers:\n%s", timerStatsText().c_str());
        } else if (line == "diag") {
            DevLog.printf("[pm] %s", sleepDiagText().c_str());
            DevLog.printf("[pm] %s", taskStatsText().c_str());
            DevLog.printf("[pm] timers:\n%s", timerStatsText().c_str());
        } else if (line.length()) {
            DevLog.println("[cli] commands: wifi-json <authenticated transaction JSON> | status | batt | pair | deep | light | deepusb on|off | framecap on|off | pmstats | blescan [1..30] | timers | diag");
        }
        if (line.length()) memset(const_cast<char *>(line.c_str()), 0, line.length());
    }
}

} // namespace firmware::diagnostics
