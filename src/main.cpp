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
#ifdef CODEX_DEEPPULL_TEST
#include <HTTPClient.h>
#endif
#include <Preferences.h>
#include <LittleFS.h>
#include <ESPmDNS.h>
#include <ArduinoJson.h>
#include <time.h>
#include <sys/time.h>
#include <string.h>
#include <stdio.h>
#include <vector>
#include <esp_sleep.h>
#include <esp_ota_ops.h>
#include <esp_partition.h>
#include <esp_system.h>
#include <esp_pm.h>
#include <esp_wifi.h>
#include <esp_mac.h>
#include <esp_timer.h>
#include <esp_private/pm_impl.h>
#include <esp_rtc_time.h>
#include <driver/rtc_io.h>
#include <driver/gpio.h>
#include <driver/usb_serial_jtag.h>

#include "DEV_Config.h"
#include "dev_log.h"
#include "epd_target.h"
#include "GUI_Paint.h"
#include "fonts.h"
#include "ble_bridge.h"
#include "bridge_store.h"
#include "owner_store.h"
#include "usage_client.h"
#include "template_store.h"
#include "template_engine.h"
#include "template_xfer.h"
#include "refresh_policy.h"
#include "v2_state.h"
#include "v2_runtime.h"
#include "v2_data_command.h"
#include "v2_plan_command.h"
#include "v2_bundle_command.h"
#include "v2_activate_command.h"
#include "v2_claim_command.h"
#include "v2_command_envelope.h"
#include "v2_status_snapshot.h"
#include "bundle_store.h"

// v2 platform targets (src/platform_target.h): the render/firmware target
// contract is validated on both sides, including OTA.
#include "platform_target.h"

// A target whose panel combination is not hardware-verified refuses normal
// operation: the ROM builds and is host-tested, but it must not pretend the
// panel contract is proven. Set CODEX_GRAY4_PANEL_CONFIRMED only after the
// hardware facts (panel model, controller datasheet, pins, waveform) exist.
#if !TARGET_VERIFIED && !defined(CODEX_GRAY4_PANEL_CONFIRMED)
#define CODEX_TARGET_UNVERIFIED 1
#endif
static bool targetUnverified = false;

#ifdef CODEX_DEEPPULL_TEST
#define FW_VERSION    "0.13.9-dptest2"
#elif defined(CODEX_CLK_WINDOW_TEST)
#define FW_VERSION    "0.13.9-clkwin"
#elif defined(CODEX_TARGET_NOTE4)
#ifdef CODEX_NOTE4_ROM_B
#define FW_VERSION    "0.18.21-note4-b"
#else
#define FW_VERSION    "0.18.19-note4-a"
#endif
#else
#define FW_VERSION    "0.17.9-bw"
#endif
#define AP_PASSWORD   "codex1234"
#define MAX_SLOTS     3

static const int EPD_W = TARGET_WIDTH;
static const int EPD_H = TARGET_HEIGHT;
static const int EPD_FB_BYTES = (EPD_W / 8) * EPD_H;

#define ACTIVE_HOLD_S    600
#define CONFIG_IDLE_MS   (5UL * 60UL * 1000UL)   // AP idle sleep (battery)
#define WIFI_CONNECT_MS  30000UL                 // boot connect attempt
#define WIFI_BLINK_MS    1000UL                  // Wi-Fi icon phase while connecting
#define WIFI_LOST_MS     30000UL                 // link-loss declaration
#define WIFI_RETRY_MS    60000UL                 // plugged retry cadence
#define BLE_GRACE_MS     120000UL                // BLE keep-alive after last use
#define BATT_CHECK_MS    (5UL * 60UL * 1000UL)
#define ANNOUNCE_MS      (5UL * 60UL * 1000UL)   // UDP announce heartbeat
#define BRIDGE_LOST_MIN  6                       // bridge heartbeat 5 min + margin
#define LOW_BATT_PCT     5
#define BLE_AUTO_PCT     20
#define STORE_MAX_LOCAL  8

// v0.14 deep/light modes (docs/power-state.md §13).
#define MODE_DEEP              0
#define MODE_LIGHT             1
#define IDLE_DEEP_DEFAULT_S    600    // light -> deep after this much quiet
#define DEEP_CONTACT_DEFAULT_S 60     // deep network period (bridge overrides)
#define DEEP_CONTACT_MIN_S     30
#define DEEP_CONTACT_MAX_S     3600
#define DEEP_PENDING_WINDOW_MS 180000UL  // stay awake when pull reports pending
#define CLK_GHOST_LIMIT        90      // deep clock partials before a full redraw
// Reserved clock window cap. The 200x200 quad window is 60 B; the larger panel
// needs room for the proportional clock face, and the cap only bounds the
// window (the fast path still declines above it) so 200x200 behaviour is
// unchanged.
#if TARGET_WIDTH > 200
#define CLK_MAX_BYTES          512
#else
#define CLK_MAX_BYTES          64
#endif

// Plan C rendezvous timing: the BLE window is a hard 3 s cap for waiting on
// the bridge and closes shortly after the bridge's plan ACK. A connected
// handshake gets its own bounded budget: Windows connect+service discovery
// often exceeds 3 s before the first command can arrive. The single wake
// render runs after the radio is off (see v2Rendezvous / v2RendezvousRender).
#define V2_RENDEZVOUS_WINDOW_MS    3000
#define V2_RENDEZVOUS_CONNECTED_MS 6000
#define V2_RENDEZVOUS_ACK_GRACE_MS 200

static Preferences prefs;
#if defined(CODEX_TARGET_NOTE4)
// NVS `pm/panel_pwr`: 1 keeps the Note4 logic rail on in deep sleep; 0
// powers it off. Both modes turn the controller's internal HV supply off.
static bool note4KeepPanelPower = true;
RTC_DATA_ATTR static uint32_t rtcNote4FrameHash = 0;
static void note4SaveFrameBaseline();
static bool note4RestoreFrameBaseline(bool thin);
static bool setNote4PanelPower(bool keep) {
    Preferences p;
    if (!p.begin("pm", false)) return false;
    const bool saved = p.putUChar("panel_pwr", keep ? 1 : 0) == 1;
    p.end();
    if (!saved) return false;
    note4KeepPanelPower = keep;
    EPD_SSD2683_SetKeepPower(keep);
    return true;
}
#endif
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

// v0.14 deep/light mode state. RTC survives deep sleep; NVS backs it across
// OTA/software resets (docs/power-state.md §13.5).
RTC_DATA_ATTR static uint8_t  rtcMode = MODE_DEEP;
RTC_DATA_ATTR static uint16_t rtcNextContactS = DEEP_CONTACT_DEFAULT_S;
RTC_DATA_ATTR static uint32_t rtcNextNetAt = 0;     // epoch of the next pull
RTC_DATA_ATTR static uint32_t rtcUsageRev = 0;      // bridge usage_rev last seen
RTC_DATA_ATTR static uint32_t rtcEpochAtSleep = 0;  // clock at the last deep sleep
RTC_DATA_ATTR static uint64_t rtcClkUsAtSleep = 0;  // RTC timer at the last sleep
RTC_DATA_ATTR static uint32_t rtcDeepCycles = 0;    // thin clock wakes
RTC_DATA_ATTR static uint32_t rtcNetCycles = 0;     // deep network windows
RTC_DATA_ATTR static uint32_t rtcNetFails = 0;      // failed deep network windows
RTC_DATA_ATTR static uint32_t rtcClockTicks = 0;    // clock window writes
RTC_DATA_ATTR static uint8_t  rtcLastPullCode = 0;  // last deep-pull HTTP code
RTC_DATA_ATTR static uint16_t rtcClkPartials = 0;   // clock window partials
RTC_DATA_ATTR static uint32_t rtcApChannel = 0;     // cached AP for fast connect
RTC_DATA_ATTR static char     rtcApBssid[20] = {0};
RTC_DATA_ATTR static uint8_t  rtcApSlot = 0xFF;
RTC_DATA_ATTR static char     rtcTplActiveId[17] = {0};
RTC_DATA_ATTR static char     rtcTplHash[9] = {0};
// Deep-cycle trace: last stage code reached (survives deep sleep; read via
// /status.json after a manual wake). 1 setup, 2 thin, 3 thin-done, 10 net,
// 11 wifi, 12 pull-ok, 13 pull-fail, 14 light, 15 pending, 20 enter-deep,
// 90 raw-sleep, 99 normal boot.
RTC_DATA_ATTR static uint8_t  rtcStage = 0;
RTC_DATA_ATTR static uint8_t  rtcLastWake = 0xFF;
// Post-OTA minimum light window: set in NVS just before an OTA reboot (RTC
// memory does not survive a software/OTA reset on this board), read and
// cleared on the first post-OTA boot. The bridge extends it with a formal
// light PowerPlan.
static uint16_t bootPostOtaS = 0;
static uint32_t postOtaHoldUntilMs = 0;
// P1 diagnosis: did the light -> deep glyph render run? bit0 set = the
// `device.mode` template flag was set, bit1 = it was clear, bit2 = render
// returned. Read via /status.json `deep.glyph`.
RTC_DATA_ATTR static uint8_t  rtcDeepGlyph = 0;
// Debug capture gate: when enabled (see captureFrameToFs), the framebuffer is
// saved after a pre-sleep refresh; fixed files, overwritten, off by default.
RTC_DATA_ATTR static uint8_t  rtcFrameCapture = 0;
RTC_DATA_ATTR static uint16_t rtcFrameCaptures = 0;
// Diagnostic switch: allow deep sleep while USB is plugged (USB only supplies
// power; the serial link drops during sleep and re-enumerates on wake). It
// survives deep-sleep cycles (RTC domain) but is cleared on any non-timer boot
// (power-on/OTA/button) and via /diag?deep_usb=0 or the `deepusb off` CLI, so
// normal plugged behavior returns after a reboot.
RTC_DATA_ATTR static uint8_t  rtcDeepOnUsb = 0;

// P0 diagnosis (sleep-modes): the RTC trace above dies with the RTC domain when
// the board is power-cycled (plugging USB shows reset=power-on), which is how
// every battery hang has been recovered so far. When armed via
// /diag?nvs_stage=1 (RTC flag), mirror the suspense points into NVS so the
// last reached code survives and can be read as `nvs_stage_boot` from
// /status.json after recovery. Bounded writes; disabled by default.
RTC_DATA_ATTR static uint8_t nvsStageEnabled = 0;
static uint8_t nvsStageWrites = 0;
static uint8_t nvsStageLast = 0xFF;
static uint8_t nvsStageAtBoot = 0xFF;
#define NVS_STAGE_MAX_WRITES 40

static void nvsStageMark(uint8_t code) {
    if (!nvsStageEnabled || code == nvsStageLast || nvsStageWrites >= NVS_STAGE_MAX_WRITES) return;
    nvsStageLast = code;
    nvsStageWrites++;
    Preferences p;
    p.begin("pm", false);
    p.putUChar("stg", code);
    p.end();
}

static void setStage(uint8_t code) {
    rtcStage = code;
    nvsStageMark(code);
}

static bool     otaInProgress = false;
static uint32_t otaLastDataMs = 0;   // last UPLOAD_FILE_WRITE (stall watchdog)
static uint32_t configStartedAt = 0;
static uint32_t activeHoldSec = ACTIVE_HOLD_S;
static bool     pmLightSleep = false;

// Display timezone (POSIX TZ string). The bridge `server_time` is a UTC epoch
// and the firmware never configured TZ, so localtime() rendered UTC: the clock
// showed 14:45 while local (CST) was 22:45. Persisted in NVS `pm/tz`; change at
// runtime with POST /diag?tz=CST-8 (note POSIX sign: UTC+8 => "CST-8").
static char deviceTz[32] = "CST-8";

static void applyTimezone() {
    setenv("TZ", deviceTz, 1);
    tzset();
}

// Bridge-provided UTC offset in minutes (east positive: CST = +480). POSIX TZ
// uses the opposite sign, so +480 -> "UTC-8:00". Persisted in NVS `pm/tz` so
// the device follows the PC timezone; `/diag?tz=` still forces a manual value
// until the next contact. Out-of-range offsets are ignored, unchanged values
// skip the NVS write.
static bool applyTzOffsetMin(long offsetMin) {
    if (offsetMin < -840 || offsetMin > 840) return false;
    long absMin = offsetMin < 0 ? -offsetMin : offsetMin;
    char buf[32];
    snprintf(buf, sizeof(buf), "UTC%c%ld:%02ld", offsetMin >= 0 ? '-' : '+',
             absMin / 60, absMin % 60);
    if (!strcmp(buf, deviceTz)) return false;
    strncpy(deviceTz, buf, sizeof(deviceTz) - 1);
    deviceTz[sizeof(deviceTz) - 1] = '\0';
    applyTimezone();
    Preferences p;
    p.begin("pm", false);
    p.putString("tz", deviceTz);
    p.end();
    DevLog.printf("[pm] tz <- bridge %ldmin (%s)\n", offsetMin, deviceTz);
    return true;
}

// v0.14 mode runtime. `lastActivity` drives the light -> deep idle transition;
// `forceDeepAt` honors a bridge `mode:"deep"` hint with a short grace period so
// a final push/template can land first.
static uint32_t idleDeepS = IDLE_DEEP_DEFAULT_S;
static uint32_t lastActivityMs = 0;
static time_t   lastActivityEpoch = 0;
static uint32_t forceDeepAtMs = 0;
static uint32_t pendingWindowUntilMs = 0;
static bool     deepWakePath = false;      // setup(): this boot is a deep pull
static bool     wokeFromDeep = false;      // setup(): deep wake; panel holds the sleep frame
static bool     panelThinReady = false;    // epdThinBegin() ran on this boot

// task-10 B: the Wi-Fi icon blinks while a boot connect attempt is running.
// `wifiConnActive` switches the template state to WIFI CONN (blink on) /
// WIFI OFF (blink off); `rfnBlink` marks a blink tick so epdFlush keeps it a
// partial waveform and does not consume the region ghost budget.
static bool     wifiConnActive = false;
static bool     wifiBlinkOn = false;
static uint16_t wifiBlinkMs = WIFI_BLINK_MS;
static uint32_t blinkTicks = 0;
static bool     rfnBlink = false;

// BLE rendezvous protocol v2 gate (design §10). New installs default to v2;
// persisted in NVS `pm/rv2` so a rollback survives OTA/reboot and can be
// toggled at runtime with POST /diag?rv2=0|1. `rendezvous_v` in INFO reflects
// this gate; the GATT table itself never changes (no Windows re-pairing).
static uint8_t  rv2Enabled = 0;
static const uint8_t RV2_SUPPORTED = 2;

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
//
// Sleep diagnostics (task-2): bucket actual light-sleep durations and count
// wakeup causes to locate the ~6 ms cadence, see
// project-workflow/pmstats/task-2.md. Read via /pmstats?diag=1.
#if defined(CODEX_PM) && CONFIG_PM_LIGHT_SLEEP_CALLBACKS
struct SleepDiag {
    uint32_t count;
    uint32_t causes[4];          // timer / wifi / gpio / other
    uint64_t total_us;
    uint32_t min_us, max_us, last_us;
    uint32_t last_expected_us, last_next_alarm_us;
    uint32_t hist[12];
};

static SleepDiag sleepDiag = {};

static uint32_t sleepBucket(int64_t us) {
    static const int64_t edges[11] = {1000, 2000, 4000, 6000, 8000, 10000,
                                      20000, 50000, 100000, 500000, 1000000};
    for (uint32_t i = 0; i < 11; i++) {
        if (us < edges[i]) return i;
    }
    return 11;
}

static esp_err_t sleepEnterCb(int64_t sleep_time_us, void *) {
    int64_t gap = esp_timer_get_next_alarm_for_wake_up() - esp_timer_get_time();
    sleepDiag.last_expected_us = (uint32_t)sleep_time_us;
    sleepDiag.last_next_alarm_us = gap < 0 ? 0 : (uint32_t)gap;
    return ESP_OK;
}

static esp_err_t sleepExitCb(int64_t slept_us, void *) {
    sleepDiag.count++;
    sleepDiag.total_us += (uint64_t)slept_us;
    if (sleepDiag.count == 1 || slept_us < (int64_t)sleepDiag.min_us) {
        sleepDiag.min_us = (uint32_t)slept_us;
    }
    if ((uint64_t)slept_us > sleepDiag.max_us) sleepDiag.max_us = (uint32_t)slept_us;
    sleepDiag.last_us = (uint32_t)slept_us;
    sleepDiag.hist[sleepBucket(slept_us)]++;
    switch (esp_sleep_get_wakeup_cause()) {
        case ESP_SLEEP_WAKEUP_TIMER: sleepDiag.causes[0]++; break;
        case ESP_SLEEP_WAKEUP_WIFI:  sleepDiag.causes[1]++; break;
        case ESP_SLEEP_WAKEUP_GPIO:  sleepDiag.causes[2]++; break;
        default:                     sleepDiag.causes[3]++; break;
    }
    return ESP_OK;
}
#endif

// Arduino loop() cadence. Idle default is 25 ms: the sleep survey (task-2)
// showed light-sleep fragmentation drops sharply up to ~25-50 ms (134 -> 45
// wakeups/s) while HTTP latency stays acceptable. While a TCP client is
// connected (HTTP request/response, OTA upload) the loop falls back to 5 ms so
// transfer throughput and interactive latency are unaffected.
// POST /diag?loop_delay=N retunes the idle value at runtime (token-gated).
static uint32_t loopDelayMs = 25;

static uint32_t loopDelayForNow() {
    if (server.client().connected()) return 5;
    return loopDelayMs;
}

static void configurePowerManagement() {
#ifdef CODEX_PM
    esp_pm_config_t cfg = {};
    cfg.max_freq_mhz = 240;
    // Plan C: 80 MHz floor keeps the BLE controller stable during a rendezvous
    // window; DFS drops below it automatically while waiting (flash stays 40 MHz,
    // that is a separate setting).
    cfg.min_freq_mhz = 80;
    cfg.light_sleep_enable = true;
    esp_err_t err = esp_pm_configure(&cfg);
    pmLightSleep = (err == ESP_OK);
    DevLog.printf("[pm] esp_pm_configure(light_sleep=1, 240/80MHz): %s\n", esp_err_to_name(err));
#if CONFIG_PM_LIGHT_SLEEP_CALLBACKS
    esp_pm_sleep_cbs_register_config_t cbs = {};
    cbs.enter_cb = sleepEnterCb;
    cbs.exit_cb = sleepExitCb;
    esp_err_t cberr = esp_pm_light_sleep_register_cbs(&cbs);
    DevLog.printf("[pm] sleep diag callbacks: %s\n", esp_err_to_name(cberr));
#endif
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

// OTA: keep the Wi-Fi modem awake too. CPU light sleep is already blocked by
// otaPmLock, but WIFI_PS_MAX_MODEM lets the AP buffer frames until the next
// listen interval, which stalled the 1.7 MB upload on a weak link (observed as
// client resets at ~130-330 KB). Restored to MAX_MODEM after the upload.
static void setOtaWifiAwake(bool awake) {
    esp_err_t err = esp_wifi_set_ps(awake ? WIFI_PS_NONE : WIFI_PS_MAX_MODEM);
    DevLog.printf("[ota] wifi ps=%s (%s)\n", awake ? "NONE" : "MAX_MODEM",
                  esp_err_to_name(err));
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

// v0.15 deep/light transition history: a small RTC ring (survives deep sleep;
// cleared by a power loss, which is acceptable) so the timeline of a sleep
// session can be read back after a manual wake. Zero flash wear, no switch.
// Read via GET /history (oldest first; `since=<seq>` for incremental polls);
// /status.json exposes hist_count (sequence number of the newest record) and
// hist_head (ring slot the next write uses).
#define HIST_CAP 120
enum : uint8_t {
    HIST_BOOT = 1,        // wake/boot classified, aux = esp_sleep_wakeup_cause_t
    HIST_ENTER_DEEP = 2,  // light -> deep transition, aux = next_contact_s
    HIST_THIN = 3,        // minute clock wake, aux = 1 when the clock was drawn
    HIST_NET_OK = 4,      // network window success, aux = HTTP code (200)
    HIST_NET_FAIL = 5,    // network window failure, aux = HTTP code (0 unknown)
    HIST_TO_LIGHT = 6,    // returned/switched to light
    HIST_WAKE = 7,        // wake summary at deep entry, aux = wake result (below)
};
// Wake-result codes for HIST_WAKE (Plan C task-3). The summary record also
// carries dur_ms and the clock source of that wake.
enum : uint8_t {
    WAKE_LIGHT = 0,       // light session (idle/plugged/HTTP)
    WAKE_THIN = 1,        // thin deep clock wake
    WAKE_RV_SLEEP = 2,    // v2 rendezvous answered with a sleep plan
    WAKE_RV_LIGHT = 3,    // v2 rendezvous answered with a light plan
    WAKE_NET = 4,         // legacy deep network window
};
// Clock provenance for /status.json `time_source` and HIST_WAKE.src.
enum : uint8_t {
    TIME_NONE = 0,
    TIME_BLE = 1,
    TIME_RTC = 2,
};
struct HistRec {
    uint32_t epoch;
    uint8_t ev;
    uint8_t stage;
    uint8_t batt;      // 0xFF = unknown
    uint8_t src;       // TIME_* of the recorded wake (HIST_WAKE)
    uint16_t aux;
    uint32_t dur_ms;   // awake duration of the recorded wake (0 when unknown)
};
static_assert(sizeof(HistRec) == 16, "history record layout changed");
RTC_DATA_ATTR static HistRec histRing[HIST_CAP];
RTC_DATA_ATTR static uint32_t histCount = 0;   // records ever written
RTC_DATA_ATTR static uint16_t histHead = 0;    // next write slot

static void histAddFull(uint8_t ev, uint16_t aux, uint32_t durMs, uint8_t src) {
    if (histHead >= HIST_CAP) histHead = 0;
    HistRec &r = histRing[histHead];
    r.epoch = timeKnown() ? (uint32_t)time(nullptr) : 0;
    r.ev = ev;
    r.stage = rtcStage;
    r.batt = (batteryPct < 0 || batteryPct > 100) ? 0xFF : (uint8_t)batteryPct;
    r.src = src;
    r.aux = aux;
    r.dur_ms = durMs;
    histHead = (uint16_t)((histHead + 1) % HIST_CAP);
    histCount++;
}

static void histAdd(uint8_t ev, uint16_t aux) { histAddFull(ev, aux, 0, 0); }

// ---- Plan C wake telemetry (task-3) ----
// millis() is boot-relative on every deep wake, so `awake_ms` is live until the
// next deep sleep. deepSleepRaw() snapshots the final values into RTC for
// /status.json (`deep.last_*`) and /history (`HIST_WAKE`).
static uint32_t bootRenderCount = 0;      // waveforms written on this wake
static uint32_t wakeRenderMs = 0;         // waveform ms accumulated this wake
static uint32_t bleOnAccumMs = 0;         // BLE radio ms accumulated this wake
static uint32_t bleOnSinceMs = 0;         // millis() when BLE went on; 0 = off
static uint8_t  wakeResult = WAKE_LIGHT;  // what this wake turned out to be
static uint8_t  timeSource = TIME_NONE;   // clock provenance of this wake
RTC_DATA_ATTR static uint32_t rtcLastAwakeMs = 0;
RTC_DATA_ATTR static uint32_t rtcLastBleMs = 0;
RTC_DATA_ATTR static uint32_t rtcLastRenders = 0;
RTC_DATA_ATTR static uint8_t  rtcLastTimeSource = TIME_NONE;
RTC_DATA_ATTR static uint8_t  rtcLastWakeResult = WAKE_LIGHT;
// Plan C power estimate: cumulative totals over completed deep cycles only
// (light sessions are excluded), so /status.json can report per-cycle averages
// without a current meter. See task-3 "功耗预估" and tools/estimate-power.mjs.
RTC_DATA_ATTR static uint32_t rtcAccCycles = 0;
RTC_DATA_ATTR static uint32_t rtcAccAwakeMs = 0;
RTC_DATA_ATTR static uint32_t rtcAccBleMs = 0;
RTC_DATA_ATTR static uint32_t rtcAccRenderMs = 0;

static void bleRadioMark(bool on) {
    if (on && !bleOnSinceMs) {
        bleOnSinceMs = millis();
    } else if (!on && bleOnSinceMs) {
        bleOnAccumMs += millis() - bleOnSinceMs;
        bleOnSinceMs = 0;
    }
}

static uint32_t bleRadioMs() {
    return bleOnAccumMs + (bleOnSinceMs ? millis() - bleOnSinceMs : 0);
}

static const char *timeSourceName(uint8_t src) {
    return src == TIME_BLE ? "ble" : src == TIME_RTC ? "rtc" : "none";
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

// Template-visible state. While a boot connect attempt is running the Wi-Fi
// icon cell blinks at ~1 Hz (task-10 B): `WIFI CONN` is the visible phase and
// `WIFI OFF` the hidden one. The template can gate the icon on WIFI CONN and
// keep the crossed-link overlay steady on both values.
static const char *templateStateText() {
    if (wifiConnActive) return wifiBlinkOn ? "WIFI CONN" : "WIFI OFF";
    return deviceStateText();
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

// The bridge stamps `tz_offset_min` (local minutes east of UTC) into pull and
// push envelopes; follow the PC timezone when present. Returns true when the
// TZ string actually changed (callers may need to redraw the clock).
static bool adoptTzOffset(JsonDocument &doc) {
    if (doc["tz_offset_min"].isNull()) return false;
    return applyTzOffsetMin((long)(doc["tz_offset_min"] | 0L));
}

static void adoptServerTime(JsonDocument &doc) {
    long long st = doc["server_time"] | 0LL;
    if (!timeKnown() && st > 1600000000) {
        struct timeval tv = {(time_t)st, 0};
        settimeofday(&tv, nullptr);
        timeSource = TIME_BLE;
        DevLog.printf("[pm] clock set from bridge: %lld\n", st);
    }
    adoptTzOffset(doc);
}

// Deep network windows align to the bridge clock every pull (docs §13.3); the
// RTC-differential reconstruction is only a fallback between contacts. Returns
// true when the timezone changed, so the caller can force a redraw.
static bool adoptServerTimeForce(JsonDocument &doc) {
    long long st = doc["server_time"] | 0LL;
    if (st > 1600000000) {
        struct timeval tv = {(time_t)st, 0};
        settimeofday(&tv, nullptr);
        timeSource = TIME_BLE;
    }
    return adoptTzOffset(doc);
}

// Push envelopes carry the bridge's mode decision (the light-phase fallback
// channel): `deep` schedules a sleep after a short grace period, `light`
// cancels a pending descent. The pull response uses the same field.
static void applyBridgeModeHint(JsonDocument &doc) {
    const char *mode = doc["mode"] | "";
    if (!strcmp(mode, "deep")) {
        if (!forceDeepAtMs) forceDeepAtMs = millis() + 60000;
    } else if (!strcmp(mode, "light")) {
        forceDeepAtMs = 0;
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
static bool   activeTplHasNow = false;
static bool   activeTplHasMode = false;

// v2 platform runtime: one committed Bundle (A/B), one compiled active
// template, one active context, Bridge-owned PowerPlan.
static CtTemplate     v2Ct;
static bool           v2CtValid = false;
static BsProfile      v2Profile;
static bool           v2BundleReady = false;
static V2PlanState    v2Plan;
static V2DataSeq      v2DataSeq;
RTC_DATA_ATTR static V2DataCheckpoint v2DataCheckpoint = {};
static V2ContextGen   v2CtxGen;
static uint64_t       v2BootMs = 0;              // physical wake instant
static bool           v2Provisional = false;     // BOOT 300 s window active
static uint64_t       v2LightDeadlineMs = 0;     // accepted plan deadline (monotonic)
static uint64_t       v2LastAckAtMs = 0;
static uint64_t       v2LastAckSeq = 0;
static String         v2AppliedFields;           // bounded last applied Data fields
static uint8_t        v2DisplayState = 0;        // 0 none,1 displayed,2 pending,3 failed
static V2BundleRx     v2Rx;
static String         v2SessionNonce;
// Device safety cap: with a committed bundle but no formal PowerPlan (or before
// the Bridge answers) the light session is bounded by the max light lease.
static uint64_t       v2SafetyDeadlineMs = 0;
static String         v2RxPath = "/bundle/rx.bin";
static String         v2PlanReason = "init";
// Plan C: while the rendezvous window is open the screen is not touched; the
// single wake render happens after bleOff (clock window, or the light plan's
// first frame in startNormalMode). Other BLE paths keep their immediate render.
static bool           v2InRendezvous = false;
// A BLE data snapshot accepted during the window is rendered with the clock in
// one frame after the radio is off (v2RendezvousRender).
static bool           v2WakeRenderPending = false;

static uint64_t v2NowMs() { return (uint64_t)(esp_timer_get_time() / 1000ULL); }
static uint32_t v2Millis32() { return (uint32_t)(esp_timer_get_time() / 1000ULL); }

static String lastUsage;
static String lastChannel = "-";
static int      epdPartialCount = 0;
static bool     epdPartialReady = false;
static bool     epdAsleep = false;
static bool     epdFullLut = false;        // full-refresh LUT currently loaded
static bool     epdBaselineTrusted = true; // software old-frame matches the panel
// Panel BUSY timeouts (RTC: survives deep sleep, cleared on power-on). A
// non-zero count means at least one waveform had an unknown state; the
// baseline is then untrusted and the next required display is a full refresh.
RTC_DATA_ATTR static uint32_t rtcEpdBusyFails = 0;

// Display safety layer (design §8): semantic regions from the active template,
// conservative escalation, ghost budgets. `rgnPolicyOn=false` restores the
// legacy changed-pixels rule at runtime (token-gated /diag?policy=off).
static RgnSet   rgnSet;
static bool     rgnPolicyOn = true;
static uint32_t rfnDecisions = 0;
static String   rfnKind = "init";
static String   rfnReason = "init";
static uint16_t rfnDirty = 0;
static bool     forceCleanRefresh = false;
static uint16_t rfnLastMs = 0;       // duration of the last epdFlush waveform
// task-10 known issue: the timer pull path draws the wake frame inside
// deepNetworkCycle and startNormalMode drew it a second time (two full
// flashes). Set when the deep pull already removed the sleep glyph.
static bool     wakeBaselineDrawn = false;

static void screen(const std::vector<String> &lines, UBYTE color = BLACK);
static void epdFlush(bool forceFull = false);
static int  batteryPercent();
static bool requestAuthorized();
static void deepSleepFor(uint32_t sec);
static void renderCurrent();
static void persistMode();
static void otaUploadCleanup(const char *reason);
static void enterBleOn(bool userInitiated);
static void bleOff(const char *reason);
static void requestAnnounce(bool bleFlag);
static void handleBleUsage(const String &json);
static void handleBleEndpoint(const String &json);
static void handleBleAuth(const String &json);
static String fmtEpoch(long long ts, const char *fmt);
static void powerOff();
static bool configureWifiPowerSave();
static void noteActivity(const char *reason);
static void saveApInfo();
static void sleepToNextEvent();
static void enterDeep(const char *reason);

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
    retainSleepCriticalGpio();
    delay(20);
    DEV_Module_Init();
    epdFullLut = EPD_TGT_Init();
    epdBaselineTrusted = false;
    if (!epdFullLut) {
        rtcEpdBusyFails++;
        DevLog.println("[epd] init busy timeout; baseline untrusted");
    }
    if (clearPanel && EPD_TGT_Clear(EPD_TGT_WHITE)) {
        epdBaselineTrusted = true;   // panel is white, lastDisplayedFrame is white
    } else if (clearPanel) {
        rtcEpdBusyFails++;
        DevLog.println("[epd] clear busy timeout; baseline untrusted");
    }
    panelThinReady = true;
    // Allocate once per boot: epdBegin() may be re-entered (OOM retry / future
    // callers) and overwriting the pointers would leak both 5000-byte buffers.
    if (!frame) {
        frame = (UBYTE *)malloc(EPD_FB_BYTES);
        if (!frame) {
            DevLog.println("[epd] frame buffer malloc failed");
            return;
        }
    }
    if (!lastDisplayedFrame) {
        lastDisplayedFrame = (UBYTE *)malloc(EPD_FB_BYTES);
        if (lastDisplayedFrame) {
            memset(lastDisplayedFrame, 0xFF, EPD_FB_BYTES);
        } else {
            DevLog.println("[epd] last-display buffer malloc failed; writes will not be skipped");
        }
    }
    Paint_NewImage(frame, EPD_W, EPD_H, 0, WHITE);
    Paint_SetScale(2);
    Paint_SelectImage(frame);
    epdPartialReady = false;
    epdPartialCount = 0;
}

// Thin deep-sleep wake init: power the panel and bring up SPI only. The
// window calls (WakePartialWindow) reset the controller and load the partial
// LUT themselves, so the full init/clear and the framebuffer stay untouched.
static void epdThinBegin() {
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
    retainSleepCriticalGpio();
    DEV_Module_Init();
    panelThinReady = true;
    epdAsleep = false;
}

// Data screens default to partial refresh (~300ms, no flash). A full refresh
// (~1.5s, flashes) runs when the panel is not partial-ready, when the changed
// area exceeds 12.5% of the panel (layout/value jumps), or after 30 partials
// to clear ghosting. forceFull requests a full refresh explicitly.
// The panel sleeps (SSD1681 deep-sleep mode 1, RAM retained) after every
// refresh and is woken/re-initialized before the next draw.
static void epdPanelSleep() {
    if (epdAsleep) return;
    EPD_TGT_Sleep();
    epdAsleep = true;
}

// Refresh decision: the display-safety layer (design §8) classifies changes by
// semantic region and budgets; high-ink changes are conservative (full) until
// the photo gate passes. `rgnPolicyOn=false` keeps the legacy rule at runtime.
static void epdFlush(bool forceFull) {
    if (!frame) return;
    const bool blink = rfnBlink;
    rfnBlink = false;
    const uint32_t flushT0 = millis();

    RfnDecision d;
    if (rgnPolicyOn) {
        d = rgnDecide(rgnSet, lastDisplayedFrame, frame,
                      epdBaselineTrusted, forceFull, forceCleanRefresh);
        forceCleanRefresh = false;
        // Blink ticks must never flash a full waveform (task-10 B): the icon
        // toggles are transient, so an exhausted icon ghost budget is the only
        // full decision downgraded here. Any other full reason stays full.
        if (blink && d.action == RFN_FULL && d.reason == RFNR_BUDGET && epdPartialReady) {
            d.action = RFN_PARTIAL;
            d.reason = RFNR_OK;
        }
        rfnKind = rfnActionName(d.action);
        rfnReason = (blink && d.action == RFN_PARTIAL) ? "blink" : rfnReasonName(d.reason);
        rfnDirty = d.dirty;
        rfnDecisions++;
        if (d.action == RFN_NONE) return;
    } else {
        forceCleanRefresh = false;
        if (lastDisplayedFrame && memcmp(frame, lastDisplayedFrame, EPD_FB_BYTES) == 0) return;
        bool full = forceFull || !epdPartialReady || !epdBaselineTrusted;
        if (!full && lastDisplayedFrame) {
            int changed = 0;
            for (int i = 0; i < EPD_FB_BYTES; i++)
                changed += __builtin_popcount((unsigned char)(frame[i] ^ lastDisplayedFrame[i]));
            if (changed > EPD_W * EPD_H / 8) full = true;
        }
        d.action = full ? RFN_FULL : RFN_PARTIAL;
        rfnKind = rfnActionName(d.action);
        rfnReason = blink ? "blink" : "legacy";
        rfnDirty = 0;
    }

    if (!TARGET_PARTIAL && d.action == RFN_PARTIAL) {
        d.action = RFN_FULL;
        rfnKind = rfnActionName(RFN_FULL);
        rfnReason = "target_full_only";
    }
    bool partial = (d.action == RFN_PARTIAL) && epdPartialReady;
    if (partial && !blink && ++epdPartialCount > 30) partial = false;
    if (partial) {
        DirtyWindow window;
        bool ok = epdBaselineTrusted &&
                  rgnDirtyWindow(lastDisplayedFrame, frame, EPD_W, EPD_H, window);
        const size_t stride = ok ? (window.x1 - window.x0 + 1) / 8 : 0;
        const size_t bytes = ok ? stride * (window.y1 - window.y0 + 1) : 0;
        uint8_t *pixels = bytes ? (uint8_t *)malloc(bytes) : nullptr;
        ok = ok && pixels;
        if (ok) {
            for (int y = window.y0; y <= window.y1; ++y)
                memcpy(pixels + (y - window.y0) * stride,
                       lastDisplayedFrame + y * (EPD_W / 8) + window.x0 / 8, stride);
            ok = EPD_TGT_WakePartialWindow(window.x0, window.y0, window.x1, window.y1, pixels);
            if (ok) {
                for (int y = window.y0; y <= window.y1; ++y)
                    memcpy(pixels + (y - window.y0) * stride,
                           frame + y * (EPD_W / 8) + window.x0 / 8, stride);
                ok = EPD_TGT_DisplayPartWindow(window.x0, window.y0, window.x1, window.y1, pixels);
            }
        }
        free(pixels);
        if (ok) {
            epdWriteCount++;
            bootRenderCount++;
            if (blink) blinkTicks++;
            if (rgnPolicyOn && !blink) rgnOnPartial(rgnSet);
            if (lastDisplayedFrame) memcpy(lastDisplayedFrame, frame, EPD_FB_BYTES);
            epdPanelSleep();
            uint32_t elapsed = millis() - flushT0;
            rfnLastMs = (uint16_t)(elapsed > 0xFFFF ? 0xFFFF : elapsed);
            wakeRenderMs += elapsed;
            return;
        }
        // Unknown waveform state: never keep the stale software baseline, and
        // escalate to a full refresh so the panel converges (design §8.4).
        rtcEpdBusyFails++;
        epdBaselineTrusted = false;
        if (rgnPolicyOn) rgnOnFull(rgnSet);
        DevLog.println("[epd] partial failed; escalating to full refresh");
    }
    if (rgnPolicyOn && d.action == RFN_FULL) rgnOnFull(rgnSet);

    epdPartialCount = 0;
    if (!epdFullLut || epdAsleep) {
        if (!EPD_TGT_Init()) {
            rtcEpdBusyFails++;
            epdBaselineTrusted = false;
            DevLog.println("[epd] full init busy timeout");
        } else {
            epdFullLut = true;
        }
    }
    epdAsleep = false;
    bool ok = EPD_TGT_Display(frame);
    if (ok) {
        epdWriteCount++;
        bootRenderCount++;
        rtcClkPartials = 0;   // full waveform clears the clock-window ghosting
        if (lastDisplayedFrame) memcpy(lastDisplayedFrame, frame, EPD_FB_BYTES);
        epdBaselineTrusted = true;
        epdPartialReady = TARGET_PARTIAL && EPD_TGT_Init_Partial();
        epdFullLut = false;
        if (TARGET_PARTIAL && !epdPartialReady) {
            rtcEpdBusyFails++;
            DevLog.println("[epd] partial-mode init failed; next flush is full");
        }
    } else {
        rtcEpdBusyFails++;
        epdBaselineTrusted = false;
        DevLog.println("[epd] full refresh busy timeout; baseline untrusted");
    }
    epdPanelSleep();
    uint32_t elapsed = millis() - flushT0;
    rfnLastMs = (uint16_t)(elapsed > 0xFFFF ? 0xFFFF : elapsed);
    wakeRenderMs += elapsed;
}

// ---------------- clock window direct write (v0.14) ----------------
// The active template's `device.now` text element is reserved once per load:
// a byte-aligned window holding exactly "HH:MM". In deep sleep the minute tick
// rewrites only those bytes (60 B for quad v9) instead of rendering a full
// frame; docs/power-state.md §13.2.
#ifdef CODEX_CLK_WINDOW_TEST
#define CLK_TEST_TICKS 10
#endif

struct ClkRegion {
    bool     valid;
    uint8_t  fontId;
    uint8_t  x0b, x1b;    // byte columns of the window
    uint16_t y0, y1;      // pixel rows of the window
    uint8_t  bw, rows;    // bytes per row, rows
    uint8_t  scale;
    uint8_t  xOff, yOff;  // element x/y inside the window
};
RTC_DATA_ATTR static ClkRegion clkR = {};
RTC_DATA_ATTR static uint8_t  clkPixels[CLK_MAX_BYTES] = {};
RTC_DATA_ATTR static bool     clkPixelsValid = false;
RTC_DATA_ATTR static char     rtcClkContextId[BS_CTX_LEN] = {};

// The clock fast path reads the shared font registry (template_engine) instead
// of keeping its own copy of the font list; both the bitmap family (f8..f24)
// and the proportional family (nt16/nt30) are recognised.
static int clkFontId(const char *name) {
    return tplFontIndexByName(name);
}

// Reserve the clock cell from the active template. The bind prints exactly
// "HH:MM" (5 glyphs), so the max-width box is 5 x font cell at the element's
// scale/x/y. No `device.now` element -> no reservation (clkR.valid stays false).
static void clkComputeRect() {
    clkR.valid = false;
    if (!TARGET_PARTIAL) return;
    if (!activeTplJson.length()) { DevLog.println("[clk] no active template"); return; }
    JsonDocument doc;
    if (deserializeJson(doc, activeTplJson)) { DevLog.println("[clk] template parse failed"); return; }
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
        clkR.valid = true;
        clkR.fontId = (uint8_t)fid;
        clkR.x0b = x0 >> 3;
        clkR.x1b = x1 >> 3;
        clkR.y0 = (uint16_t)y;
        clkR.y1 = (uint16_t)(y + h - 1);
        clkR.bw = (uint8_t)bw;
        clkR.rows = (uint8_t)h;
        clkR.scale = (uint8_t)scale;
        clkR.xOff = (uint8_t)(x - x0);
        clkR.yOff = 0;
        clkPixelsValid = false;
        DevLog.printf("[clk] reserved x=%d..%d y=%d..%d box=%dx%d win=%dx%dB\n",
                      x, x + w - 1, y, y + h - 1, w, h, bw, bytes);
        return;
    }
    DevLog.println("[clk] template has no device.now; no reservation");
}

// Compiled-template variant: identical reservation rules without parsing the
// template JSON at activation time (v2 §8).
static void clkComputeRectCt() {
    const ClkRegion previous = clkR;
    const bool previousPixelsValid = clkPixelsValid;
    clkR.valid = false;
    clkPixelsValid = false;
    if (!TARGET_PARTIAL) return;
    if (!v2CtValid) return;
    for (uint8_t i = 0; i < v2Ct.opCount; i++) {
        const CtOp &op = v2Ct.ops[i];
        if (op.type != CT_TEXT) continue;
        if (op.bindIdx == CT_NONE_IDX) continue;
        if (strcmp(v2Ct.reqs[op.bindIdx].path, "device.now") != 0) continue;
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
        clkR.valid = true;
        clkR.fontId = op.font;
        clkR.x0b = x0 >> 3;
        clkR.x1b = x1 >> 3;
        clkR.y0 = (uint16_t)y;
        clkR.y1 = (uint16_t)(y + h - 1);
        clkR.bw = (uint8_t)bw;
        clkR.rows = (uint8_t)h;
        clkR.scale = (uint8_t)scale;
        clkR.xOff = (uint8_t)(x - x0);
        clkR.yOff = 0;
        // A rendezvous boot reloads the same compiled template. Its RTC
        // window pixels still describe the panel unless the context or the
        // clock region changed; only then must the first wake redraw fully.
        clkPixelsValid = previousPixelsValid && previous.valid &&
            v2Profile.contextId[0] != '\0' &&
            strcmp(rtcClkContextId, v2Profile.contextId) == 0 &&
            previous.fontId == clkR.fontId &&
            previous.x0b == clkR.x0b && previous.x1b == clkR.x1b &&
            previous.y0 == clkR.y0 && previous.y1 == clkR.y1 &&
            previous.bw == clkR.bw && previous.rows == clkR.rows &&
            previous.scale == clkR.scale &&
            previous.xOff == clkR.xOff && previous.yOff == clkR.yOff;
        DevLog.printf("[clk] reserved x=%d..%d y=%d..%d box=%dx%d win=%dx%dB\n",
                      x, x + w - 1, y, y + h - 1, w, h, bw, bytes);
        return;
    }
    DevLog.println("[clk] template has no device.now; no reservation");
}

// Copy the clock window bytes off the framebuffer that is currently on screen.
static void clkCaptureFromFramebuffer() {
    if (!clkR.valid || !lastDisplayedFrame) return;
    const int stride = EPD_W / 8;
    for (int row = 0; row < clkR.rows; row++) {
        memcpy(&clkPixels[row * clkR.bw],
               &lastDisplayedFrame[(clkR.y0 + row) * stride + clkR.x0b],
               clkR.bw);
    }
    clkPixelsValid = true;
    if (v2BundleReady) {
        strncpy(rtcClkContextId, v2Profile.contextId, sizeof(rtcClkContextId) - 1);
        rtcClkContextId[sizeof(rtcClkContextId) - 1] = '\0';
    } else {
        rtcClkContextId[0] = '\0';
    }
}

#if defined(CODEX_TARGET_NOTE4)
static uint32_t note4FrameHash(const uint8_t *pixels) {
    uint32_t hash = 2166136261u;
    for (int i = 0; i < EPD_FB_BYTES; ++i)
        hash = (hash ^ pixels[i]) * 16777619u;
    return hash ? hash : 1;
}

// Persist the actual panel image once when entering deep sleep. Thin minute
// wakes update only the RTC clock window, avoiding a flash write every minute.
static void note4SaveFrameBaseline() {
    if (!lastDisplayedFrame) return;  // thin clock wake: cached base is unchanged
    if (!epdBaselineTrusted) {
        rtcNote4FrameHash = 0;
        return;
    }
    const uint32_t hash = note4FrameHash(lastDisplayedFrame);
    if (hash == rtcNote4FrameHash) return;
    rtcNote4FrameHash = 0;
    if (!LittleFS.begin(false, "/littlefs", 10, "storage")) return;
    File file = LittleFS.open("/panel-base.tmp", "w");
    if (!file) return;
    const uint32_t header[2] = {0x344E5045u, hash};
    const bool written = file.write((const uint8_t *)header, sizeof(header)) == sizeof(header) &&
                         file.write(lastDisplayedFrame, EPD_FB_BYTES) == EPD_FB_BYTES;
    file.close();
    if (!written) { LittleFS.remove("/panel-base.tmp"); return; }
    LittleFS.remove("/panel-base.bin");
    if (LittleFS.rename("/panel-base.tmp", "/panel-base.bin"))
        rtcNote4FrameHash = header[1];
}

static bool note4RestoreFrameBaseline(bool thin) {
    if (!rtcNote4FrameHash || !LittleFS.begin(false, "/littlefs", 10, "storage"))
        return false;
    File file = LittleFS.open("/panel-base.bin", "r");
    if (!file || file.size() != EPD_FB_BYTES + 8) return false;
    uint32_t header[2] = {};
    uint8_t *pixels = thin ? (uint8_t *)malloc(EPD_FB_BYTES) : lastDisplayedFrame;
    if (!pixels) return false;
    const bool read = file.read((uint8_t *)header, sizeof(header)) == sizeof(header) &&
                      file.read(pixels, EPD_FB_BYTES) == EPD_FB_BYTES;
    file.close();
    bool valid = read && header[0] == 0x344E5045u &&
                 header[1] == rtcNote4FrameHash &&
                 note4FrameHash(pixels) == header[1];
    if (valid && clkPixelsValid && clkR.valid) {
        valid = clkR.x0b <= clkR.x1b && clkR.x1b < EPD_W / 8 &&
                clkR.y0 <= clkR.y1 && clkR.y1 < EPD_H &&
                clkR.bw == clkR.x1b - clkR.x0b + 1 &&
                clkR.rows == clkR.y1 - clkR.y0 + 1 &&
                (size_t)clkR.bw * clkR.rows <= sizeof(clkPixels);
    }
    if (valid && clkPixelsValid && clkR.valid) {
        const int stride = EPD_W / 8;
        for (int row = 0; row < clkR.rows; ++row)
            memcpy(pixels + (clkR.y0 + row) * stride + clkR.x0b,
                   clkPixels + row * clkR.bw, clkR.bw);
    }
    if (valid) {
        EPD_SSD2683_RestoreShadow(pixels);
        if (!thin) {
            epdBaselineTrusted = true;
            epdPartialReady = EPD_TGT_Init_Partial();
            epdFullLut = false;
        }
    }
    if (thin) free(pixels);
    DevLog.printf("[epd] baseline cache %s\n", valid ? "restored" : "invalid");
    return valid;
}
#endif

static void clkBlitString(uint8_t *buf, const char *s) {
    if (!tplFontDrawClock(buf, clkR.bw, clkR.rows, clkR.xOff, clkR.fontId, s, clkR.scale)) {
        DevLog.println("[clk] clock font unavailable at blit");
    }
}

// Clock tick: rebuild only the reserved window and push it to the panel.
// Used by the light-mode minute tick and by thin deep-sleep wakes; needs the
// panel to be powered/SPI-initialized (epdBegin or epdThinBegin).
static bool clockTickWake() {
    if (!clkR.valid || !panelThinReady) return false;
    if (!timeKnown()) return false;
    const uint64_t t0 = esp_timer_get_time();
    const size_t n = (size_t)clkR.bw * clkR.rows;
    static uint8_t buf[CLK_MAX_BYTES];
    memset(buf, 0xFF, n);
    time_t nowSec = time(nullptr);
    struct tm *lt = localtime(&nowSec);
    char s[8] = "--:--";
    if (lt) strftime(s, sizeof(s), "%H:%M", lt);
    clkBlitString(buf, s);
    const int x0 = clkR.x0b * 8, x1 = clkR.x1b * 8 + 7;
    const uint64_t t1 = esp_timer_get_time();
    bool ok = EPD_TGT_WakePartialWindow(x0, clkR.y0, x1, clkR.y1,
                                            clkPixelsValid ? clkPixels : nullptr);
    const uint64_t t2 = esp_timer_get_time();
    if (ok) ok = EPD_TGT_DisplayPartWindow(x0, clkR.y0, x1, clkR.y1, buf);
    const uint64_t t3 = esp_timer_get_time();
    if (!ok) {
        // The window waveform state is unknown: keep the previous clock pixels
        // and schedule a full rebuild at the next opportunity.
        rtcEpdBusyFails++;
        rtcClkPartials = CLK_GHOST_LIMIT;
        clkPixelsValid = false;
        epdPartialReady = false;
        epdFullLut = false;
        epdAsleep = false;
        DevLog.println("[clk] window write failed; full refresh required");
        return false;
    }
    memcpy(clkPixels, buf, n);
    clkPixelsValid = true;
    epdAsleep = false;
    epdPanelSleep();
    bootRenderCount++;
    rtcClockTicks++;
    rtcClkPartials++;
    if (rtcClkPartials > CLK_GHOST_LIMIT) rtcClkPartials = CLK_GHOST_LIMIT;
    epdPartialCount++;   // window writes count toward the light-mode ghost reset
    const uint64_t t4 = esp_timer_get_time();
    wakeRenderMs += (uint32_t)((t4 - t0) / 1000);
    DevLog.printf("[clk] tick build=%uus wake=%uus write=%uus zzz=%uus total=%uus\n",
                  (unsigned)(t1 - t0), (unsigned)(t2 - t1), (unsigned)(t3 - t2),
                  (unsigned)(t4 - t3), (unsigned)(t4 - t0));
    return true;
}

#ifdef CODEX_CLK_WINDOW_TEST
static void clkTestTick() {
    static uint32_t ticks = 0;   // survives light sleep (RAM kept)
    if (!clkR.valid) return;
    ticks++;
    if (ticks > CLK_TEST_TICKS) {
        if (ticks == CLK_TEST_TICKS + 1) {
            DevLog.printf("[clk] A/B test done (%u ticks)\n", (unsigned)CLK_TEST_TICKS);
        }
        renderCurrent();
        return;
    }
    const uint64_t t0 = esp_timer_get_time();
    if (!clkPixelsValid) clkCaptureFromFramebuffer();
    if (ticks % 2 == 0) {
        clockTickWake();
    } else {
        renderCurrent();
        clkCaptureFromFramebuffer();
        DevLog.printf("[clk] A total=%uus\n", (unsigned)(esp_timer_get_time() - t0));
    }
}
#endif

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
    String items = "\"mac\":\"" + macText() + "\",\"ip\":\"" + ipText() +
                   "\",\"http_port\":80,\"rendezvous_v\":" + String((unsigned)(rv2Enabled ? RV2_SUPPORTED : 0)) +
                   ",\"rv_max\":" + String((unsigned)RV2_SUPPORTED) +
                   ",\"v2_bundle\":" + String(v2BundleReady ? "true" : "false") + ",\"templates\":[";
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
    if (!id.length()) {
        activeTplJson = ""; activeTplId = ""; activeTplHasNow = false;
        rgnReset(rgnSet); rgnSet.wholeFrame = true;
        return false;
    }
    if (id == activeTplId && activeTplJson.length()) return true;
    String json;
    if (!tplStoreLoad(id, json)) {
        activeTplJson = ""; activeTplId = ""; activeTplHasNow = false;
        rgnReset(rgnSet); rgnSet.wholeFrame = true;
        return false;
    }
    activeTplJson = json;
    activeTplId = id;
    // Scan the (multi-KB, possibly PSRAM-backed) template once per load, not
    // once per loop: the per-loop scan thrashs the wake-path cache and light
    // sleep never recovers on this 40 MHz-flash board.
    activeTplHasNow = activeTplJson.indexOf("device.now") >= 0;
    activeTplHasMode = activeTplJson.indexOf("device.mode") >= 0;
    clkComputeRect();
    // Semantic refresh regions for the display-safety layer (design §8.1).
    // Failure to derive them leaves wholeFrame=true (conservative full).
    bool rgnOk = rgnBuild(activeTplJson, rgnSet);
    DevLog.printf("[rgn] %s n=%u whole=%d\n", rgnOk ? "derived" : "fallback",
                  (unsigned)rgnSet.n, rgnSet.wholeFrame ? 1 : 0);
    return true;
}

// Load the committed Bundle's initial compiled template (or the current one)
// and prime refresh regions + clock reservation from the compiled record.
static bool v2ActiveLoad() {
    if (!v2BundleReady) { v2CtValid = false; return false; }
    String err;
    if (!bsLoadCompiled(v2Profile.initial, v2Ct, err)) {
        DevLog.printf("[v2] active load failed: %s\n", err.c_str());
        v2CtValid = false;
        return false;
    }
    v2CtValid = true;
    activeTplId = v2Profile.ids[v2Profile.initial];
    activeTplJson = "";
    activeTplHasNow = false;
    activeTplHasMode = false;
    for (int i = 0; i < v2Ct.reqCount; i++) {
        const char *p = v2Ct.reqs[i].path;
        if (!strcmp(p, "device.now")) activeTplHasNow = true;
        if (!strcmp(p, "device.mode")) activeTplHasMode = true;
    }
    bool rgnOk = rgnBuildCt(v2Ct, rgnSet);
    DevLog.printf("[rgn] ct %s n=%u whole=%d\n", rgnOk ? "derived" : "fallback",
                  (unsigned)rgnSet.n, rgnSet.wholeFrame ? 1 : 0);
    clkComputeRectCt();
    return true;
}

// Active-template switch (BOOT key cycle): new context, re-prime regions.
static bool v2SwitchActive(uint8_t index) {
    char ctx[BS_CTX_LEN];
    snprintf(ctx, sizeof(ctx), "%08x%08x", v2CtxGen.next(), (unsigned)esp_random());
    String err;
    if (!bsSetActive(index, ctx, err)) {
        DevLog.printf("[v2] activate failed: %s\n", err.c_str());
        return false;
    }
    bsProfile(v2Profile);
    v2DataSeq.beginContext(v2NowMs(), 1);
    v2AppliedFields = "";
    if (!v2ActiveLoad()) {
        return false;
    }
    DevLog.printf("[v2] active=%s ctx=%s\n", activeTplId.c_str(), v2Profile.contextId);
    return true;
}

static void renderActiveUsage(const String &json, const char *channel) {
    if (!frame) return;
    bool ready = v2BundleReady ? v2ActiveLoad() : tplCacheLoad();
    if (!ready) { screenStatus(); return; }
    TplEnv env;
    env.channel  = channel ? channel : "";
    env.ip       = ipText();
    env.syncHHMM = nowHHMM();
    env.battery  = batteryPercent();
    env.state    = (rtcMode == MODE_DEEP) ? "DEEP" : templateStateText();
    env.mode     = (rtcMode == MODE_DEEP) ? "deep" : "light";
    env.offlineMins = -1;
    if (rtcLastSyncEpoch > 1600000000 && timeKnown()) {
        long mins = ((long)time(nullptr) - (long)rtcLastSyncEpoch) / 60;
        // The row means "bridge unreachable": the bridge pushes at least every
        // 5 minutes, so only expose the value once contact is clearly lost.
        if (mins > BRIDGE_LOST_MIN) env.offlineMins = (int)mins;
    }
    Paint_SelectImage(frame);
    Paint_Clear(WHITE);
    bool drawn = v2CtValid ? tplDrawCt(v2Ct, json, env)
                           : tplDraw(activeTplJson, json, env);
    if (drawn) {
        epdFlush(false);
        if (epdBaselineTrusted) clkCaptureFromFramebuffer();
        else clkPixelsValid = false;
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

// Put the sleep frame back without entering deep: used when a wake render
// already removed Zzz but the link never came up, so the panel does not sit on
// a stale light frame. Full baseline for the sleep glyph (see enterDeep/0.14.13).
static void renderSleepGlyph() {
    uint8_t saved = rtcMode;
    rtcMode = MODE_DEEP;
    epdPartialReady = false;
    renderCurrent();
    rtcMode = saved;
}

static void nextTemplate() {
    // v2 Bundle: every installed template participates in the key cycle, and a
    // switch generates a new unreusable context (v2 §4/§10).
    if (v2BundleReady && v2Profile.count > 0) {
        uint8_t next = (uint8_t)((v2Profile.initial + 1) % v2Profile.count);
        v2SwitchActive(next);
        pendingTplChanged = true;
        noteActivity("template-switch");
        DevLog.printf("[v2] local switch -> %s\n", v2Profile.ids[v2Profile.initial]);
        return;
    }
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
    noteActivity("template-switch");
    DevLog.printf("[tpl] local switch -> %s\n", m.id.c_str());
}

static void factoryReset() {
    screen({"FACTORY RESET", "", "clearing..."});
    bleClearBonds();
    storeClear();
    tplStoreClear();
    ownerClear(true);
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
    if (v2BundleReady) return false;
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
        applyBridgeModeHint(parsed);
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
            noteActivity("pull");
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
    // Occupancy layer (task-4): only the owner may write; no exceptions (an
    // `activate` flag or a matching endpoint token is not a bypass). With no
    // valid owner the legacy behavior applies and no owner is ever created.
    String bridgeId = parsed["bridge"]["hostId"] | "";
    if (!ownerAllows(bridgeId)) {
        DevLog.printf("[owner] usage push rejected (occupied, id=%s)\n", bridgeId.c_str());
        server.send(409, "application/json",
                    String("{\"accepted\":false,\"error\":\"occupied\",\"owner\":") +
                        ownerJson() + "}");
        return;
    }
    if (v2BundleReady) {
        server.send(409, "application/json", "{\"accepted\":false,\"error\":\"v2_required\"}");
        return;
    }
    adoptServerTime(parsed);
    applyEnvelopeMeta(parsed, mac);
    applyBridgeModeHint(parsed);
    bool explicitActivate = parsed["activate"] | false;
    bool accepted = usageAccepted(mac, explicitActivate);
    markSynced();
    usageCacheSave(body);
    // Heartbeat pushes repeat the same `usage_rev`; they must not reset the
    // local 10-minute idle fallback (design §6), or the device could never go
    // deep while the bridge keeps a 5-minute heartbeat. A missing rev (older
    // bridge) is treated as a change to stay conservative.
    bool hasRev = !parsed["usage_rev"].isNull();
    uint32_t pushRev = (uint32_t)(parsed["usage_rev"] | 0L);
    bool usageChanged = !hasRev || pushRev != rtcUsageRev;
    if (hasRev) rtcUsageRev = pushRev;
    if (accepted) {
        setActiveMac(mac);
        rtcActiveAt = timeKnown() ? (uint32_t)time(nullptr) : 0;
        lastUsage = body;
        lastChannel = "PUSH";
        if (usageChanged) noteActivity("push");
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
    // Occupancy layer (task-4): template writes carry `bridge_id` (= hostId);
    // non-owners are rejected with the current owner, `activate` is no bypass.
    String bridgeId = server.arg("bridge_id");
    if (!bridgeId.length()) bridgeId = server.header("X-Bridge-Id");
    if (!ownerAllows(bridgeId)) {
        DevLog.printf("[owner] template rejected (occupied, id=%s)\n", bridgeId.c_str());
        server.send(409, "application/json",
                    String("{\"saved\":false,\"err\":\"occupied\",\"owner\":") +
                        ownerJson() + "}");
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
        noteActivity("template");
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
//
// The 1.54 panel keeps GPIO6 low to retain SSD1681 RAM. Note4 holds its
// selected GPIO6 level (high for keep, low for off_cache) and restores its
// software baseline from LittleFS. GPIO17 keeps the VBAT latch asserted.
// gpio_hold_* only persists RTC-capable pins (0..21).
static void holdPinsForDeepSleep() {
    gpio_hold_en(GPIO_NUM_6);
    gpio_hold_en(GPIO_NUM_17);
    gpio_deep_sleep_hold_en();
}

// Glitch-free release: a held pad follows the (deep-sleep reset) GPIO output
// register the moment the hold drops. Releasing GPIO17 while that register is 0
// pulls the VBAT latch low and cuts battery power mid-boot -- the 0.14.0-0.14.3
// battery-deep hang (NVS trace stopped at stage 90, BOOT dead, USB = power-on).
// So restore every pad level first, then drop the holds.
static void releaseWakeHolds() {
    pinMode(GPIO_NUM_17, OUTPUT);
    digitalWrite(GPIO_NUM_17, HIGH);
    pinMode(GPIO_NUM_6, OUTPUT);
#if defined(CODEX_TARGET_NOTE4)
    digitalWrite(GPIO_NUM_6, note4KeepPanelPower ? HIGH : LOW);
#else
    digitalWrite(GPIO_NUM_6, LOW);
#endif
    gpio_hold_dis(GPIO_NUM_17);
    gpio_hold_dis(GPIO_NUM_6);
    gpio_deep_sleep_hold_dis();
}

// Bare sleep: no Wi-Fi/BLE/panel calls at all, so it is safe on the thin wake
// path where those drivers were never initialized (an uninitialized
// WiFi.disconnect(true) is a known hang risk).
static void deepSleepRaw(uint32_t sec) {
    if (sec < 2) sec = 2;
    if (sec > 3600) sec = 3600;
    // Plan C telemetry: snapshot this wake's totals into RTC (RAM accounting
    // dies with the boot) for /status.json `deep.last_*` and /history.
    rtcLastAwakeMs = millis();
    rtcLastBleMs = bleRadioMs();
    rtcLastRenders = bootRenderCount;
    rtcLastTimeSource = timeSource;
    rtcLastWakeResult = wakeResult;
    histAddFull(HIST_WAKE, wakeResult, rtcLastAwakeMs, timeSource);
    // Power estimate (no current meter): accumulate only deep cycles so the
    // per-cycle averages are not dominated by long light sessions.
    if (wakeResult != WAKE_LIGHT) {
        rtcAccCycles++;
        rtcAccAwakeMs += rtcLastAwakeMs;
        rtcAccBleMs += rtcLastBleMs;
        rtcAccRenderMs += wakeRenderMs;
    }
    rtcEpochAtSleep = timeKnown() ? (uint32_t)time(nullptr) : 0;
    rtcClkUsAtSleep = esp_rtc_get_time_us();
    armWakeSources((uint64_t)sec * 1000000ULL);
    setStage(90);
    DevLog.printf("[pm] deep sleep %us (raw)\n", (unsigned)sec);
    holdPinsForDeepSleep();
    esp_deep_sleep_start();
}

// Full transition cleanup (light -> deep and the network-window paths): put
// the panel to sleep, stop BLE and disconnect Wi-Fi if it was ever up.
static void deepSleepFor(uint32_t sec) {
#if defined(CODEX_TARGET_NOTE4)
    note4SaveFrameBaseline();
#endif
    epdPanelSleep();
    nvsStageMark(48);
    if (bleInitialized()) {
        bleRadioMark(false);
        bleAdvertiseStop();
        bleDeinit();
    }
    nvsStageMark(49);
    if (WiFi.getMode() != WIFI_MODE_NULL) WiFi.disconnect(true);
    nvsStageMark(50);
    deepSleepRaw(sec);
}

static void handleBleUsage(const String &json) {
    if (v2BundleReady) {
        bleNotifyStatusQuiet("{\"ack\":\"usage\",\"ok\":false,\"err\":\"v2_required\"}");
        return;
    }

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
static bool connectBest(bool showProgress = true) {
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
    if (showProgress) screen({"CODEX STATUS", FW_VERSION, "", "Connecting:", wifiSsid});
    DevLog.printf("[wifi] slot %d (%s) rssi=%d\n", bestSlot, wifiSsid.c_str(), bestRssi);
    WiFi.begin(wifiSsid.c_str(), wifiPass.c_str());
    uint32_t t0 = millis();
    uint32_t lastBlink = t0;
    bool blinkDrew = false;
    // task-10 B: blink the Wi-Fi icon cell while associating. Only when a
    // template frame is actually on the panel and its baseline is usable (a
    // cached usage + stored template, first frame already drawn); otherwise the
    // Connecting/status page must not be disturbed and an untrusted baseline
    // would turn every tick into a full flash.
    const bool blinkAllowed = lastUsage.length() > 0 && epdPartialReady && epdBaselineTrusted;
    if (blinkAllowed) {
        wifiConnActive = true;
        wifiBlinkOn = false;
    }
    while (WiFi.status() != WL_CONNECTED && millis() - t0 < WIFI_CONNECT_MS) {
        delay(200);
        if (blinkAllowed && wifiBlinkMs &&
            (uint32_t)(millis() - lastBlink) >= wifiBlinkMs) {
            lastBlink = millis();
            wifiBlinkOn = !wifiBlinkOn;
            rfnBlink = true;
            renderCurrent();
            blinkDrew = true;
        }
        DevLog.print(".");
    }
    wifiConnActive = false;
    wifiBlinkOn = false;
    // A failed attempt must not leave the last blink frame (icon visible) on
    // the panel: settle on the real state, which hides the icon again.
    if (blinkDrew && WiFi.status() != WL_CONNECTED) renderCurrent();
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
            String(epdPartialReady ? "ready" : "off") + ", streak " + String(epdPartialCount) +
            ", busy_fails " + String(rtcEpdBusyFails) + ", baseline " +
            String(epdBaselineTrusted ? "trusted" : "untrusted") + ")</li>";
    html += "<li>Wake: " + String(millis()) + " ms awake, BLE " + String(bleRadioMs()) +
            " ms, renders " + String(bootRenderCount) + ", time " +
            String(timeSourceName(timeSource)) + " (last: " +
            String(wakeResultName(rtcLastWakeResult)) + " " + String(rtcLastAwakeMs) +
            " ms, renders " + String(rtcLastRenders) + ", time " +
            String(timeSourceName(rtcLastTimeSource)) + ")</li>";
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
#if defined(CODEX_TARGET_NOTE4)
    doc["heap_max_alloc"] = ESP.getMaxAllocHeap();
    doc["psram_free"] = ESP.getFreePsram();
#endif
    doc["epd_writes"] = epdWriteCount;
    doc["epd_partial"] = epdPartialReady;
    doc["epd_streak"] = epdPartialCount;
    doc["epd_busy_fails"] = rtcEpdBusyFails;
    doc["epd_trusted"] = epdBaselineTrusted;
#if defined(CODEX_TARGET_NOTE4)
    doc["panel_power_mode"] = note4KeepPanelPower ? "keep" : "off_cache";
#endif
    doc["refresh_kind"] = rfnKind;
    doc["refresh_reason"] = rfnReason;
    doc["dirty_pixels"] = rfnDirty;
    doc["refresh_decisions"] = rfnDecisions;
    doc["refresh_ms"] = rfnLastMs;
    doc["blink_ms"] = wifiBlinkMs;
    doc["blink_on"] = wifiBlinkOn;
    doc["blink_ticks"] = blinkTicks;
    doc["wifi_conn"] = wifiConnActive;
    doc["rgn"] = rgnSet.n;
    doc["rgn_whole"] = rgnSet.wholeFrame;
    doc["rgn_policy"] = rgnPolicyOn;
    doc["wifi_slots"] = countWifiSlots();
    doc["ap_reason"] = AP_REASON_NAMES[rtcApReason < 3 ? rtcApReason : 0];
    // v0.14 deep/light mode (docs/power-state.md §13).
    doc["mode"] = (rtcMode == MODE_DEEP) ? "deep" : "light";
    doc["idle_deep_s"] = idleDeepS;
    doc["next_contact_s"] = rtcNextContactS;
    doc["usage_rev"] = rtcUsageRev;
    doc["clk"] = clkR.valid;
    doc["clk_partials"] = rtcClkPartials;
    doc["stage"] = rtcStage;
    doc["last_wake_code"] = rtcLastWake;
    doc["nvs_stage_boot"] = nvsStageAtBoot;
    doc["deep_usb"] = rtcDeepOnUsb;
    doc["frame_capture"] = rtcFrameCapture;
    doc["rv2"] = rv2Enabled;
    doc["rv_max"] = RV2_SUPPORTED;
    doc["tz"] = deviceTz;
    doc["hist_count"] = histCount;
    doc["hist_head"] = histHead;
    // Plan C wake telemetry (task-3): live values of the current wake; the
    // previous wake's final values are in `deep.last_*` below.
    doc["awake_ms"] = millis();
    doc["ble_on_ms"] = bleRadioMs();
    doc["render_count"] = bootRenderCount;
    doc["time_source"] = timeSourceName(timeSource);
    doc["render_ms"] = wakeRenderMs;
    // Post-OTA minimum light window remaining (0 = not in one), so the bridge
    // can verify the window it is responsible for extending.
    doc["post_ota_hold_s"] =
        postOtaHoldUntilMs && (int32_t)(millis() - postOtaHoldUntilMs) < 0
            ? (uint32_t)((postOtaHoldUntilMs - millis()) / 1000)
            : 0;
#if defined(CODEX_PM) && CONFIG_PM_LIGHT_SLEEP_CALLBACKS
    // Time-based power estimate (no current meter): light-sleep share lets the
    // estimator subtract it from the awake window instead of charging CPU mA.
    doc["light_sleep_ms"] = (uint32_t)(sleepDiag.total_us / 1000ULL);
    doc["light_sleep_count"] = sleepDiag.count;
#endif
    // ------------------------------------------------------------------
    // v2 platform state (device is authoritative; the Bridge reconciles).
    // ------------------------------------------------------------------
    doc["fw_target"] = FW_TARGET_ID;
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
    doc["v2_bundle"] = v2BundleReady;
    doc["commit_seq"] = (unsigned)bsCommitSeq();
    doc["active_context_id"] = v2Profile.contextId;
    doc["active_template_id"] = (v2BundleReady && v2Profile.count)
                                    ? String(v2Profile.ids[v2Profile.initial])
                                    : tplStoreActive();
    doc["committed_job_id"] = v2Profile.jobId;
    doc["data_seq"] = v2DataSeq.appliedSeq();
    doc["applied_seq"] = v2DataSeq.appliedSeq();
    doc["last_acked_at_ms"] = (unsigned long)v2LastAckAtMs;
    doc["display_state"] = v2DisplayState == 1 ? "displayed"
                          : v2DisplayState == 2 ? "pending"
                          : v2DisplayState == 3 ? "failed"
                                                : "unchanged";
    doc["v2_templates"] = v2Profile.count;
    {
        JsonObject power = doc["power"].to<JsonObject>();
        power["mode"] = (rtcMode == MODE_DEEP) ? "sleep" : "light";
        power["plan_id"] = v2Plan.acceptedId();
        power["remaining_s"] = v2Plan.remainingS(v2NowMs());
        power["granted_s"] = v2Plan.grantedS();
        power["provisional"] = v2Provisional && !v2Plan.accepted();
        power["provisional_remaining_s"] =
            v2Provisional ? V2PlanState::bootProvisionalRemaining(v2BootMs, v2NowMs()) : 0;
        power["rendezvous_period_s"] = V2_RENDEZVOUS_S;
    }
    {
        uint32_t nowEpoch = timeKnown() ? (uint32_t)time(nullptr) : 0;
        doc["next_contact_in_s"] = (rtcNextNetAt && nowEpoch && rtcNextNetAt > nowEpoch)
                                       ? (rtcNextNetAt - nowEpoch)
                                       : 0;
        JsonObject deep = doc["deep"].to<JsonObject>();
        deep["clock_wakes"] = rtcDeepCycles;
        deep["net_windows"] = rtcNetCycles;
        deep["net_fails"] = rtcNetFails;
        deep["clock_ticks"] = rtcClockTicks;
        deep["last_code"] = rtcLastPullCode;
        deep["last_contact"] = rtcLastSyncEpoch;
        deep["idle_s"] = lastActivityEpoch && nowEpoch ? (uint32_t)(nowEpoch - lastActivityEpoch) : 0;
        deep["glyph"] = rtcDeepGlyph;
        deep["tpl_has_mode"] = activeTplHasMode ? 1 : 0;
        deep["captures"] = rtcFrameCaptures;
        // Last completed wake (RTC snapshot taken at deep entry, task-3).
        deep["last_wake_result"] = wakeResultName(rtcLastWakeResult);
        deep["last_awake_ms"] = rtcLastAwakeMs;
        deep["last_ble_ms"] = rtcLastBleMs;
        deep["last_renders"] = rtcLastRenders;
        deep["last_time_source"] = timeSourceName(rtcLastTimeSource);
        // Cumulative deep-cycle totals for the time-based power estimate.
        deep["acc_cycles"] = rtcAccCycles;
        deep["acc_awake_ms"] = rtcAccAwakeMs;
        deep["acc_ble_ms"] = rtcAccBleMs;
        deep["acc_render_ms"] = rtcAccRenderMs;
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
        uint32_t now = millis() / 1000;
        uint32_t elapsed = now - ownerCur.lastSeen;
        own["expires_in_s"] = (elapsed >= ownerCur.lease) ? 0 : (ownerCur.lease - elapsed);
    } else {
        doc["owner"] = nullptr;
    }
    String out;
    serializeJson(doc, out);
    server.send(200, "application/json", out);
}

static void handleLog() {
    server.send(200, "text/plain; charset=utf-8", DevLog.dump());
}

// GET /history: deep/light transition history from the RTC ring, oldest first.
// `since=<seq>` returns only records with a higher sequence number, so the
// bridge can poll incrementally using `/status.json`'s hist_count. Read-only,
// no token (same exposure as /log and /status.json).
static void handleHistory() {
    uint32_t since = 0;
    if (server.hasArg("since")) {
        long long v = server.arg("since").toInt();
        if (v > 0) since = (uint32_t)v;
    }
    uint32_t avail = histCount < HIST_CAP ? histCount : HIST_CAP;
    uint32_t firstSeq = histCount - avail + 1;
    JsonDocument doc;
    JsonArray arr = doc.to<JsonArray>();
    for (uint32_t i = 0; i < avail; i++) {
        uint32_t seq = firstSeq + i;
        if (seq <= since) continue;
        const HistRec &r = histRing[(seq - 1) % HIST_CAP];
        JsonObject item = arr.add<JsonObject>();
        item["seq"] = seq;
        item["t"] = r.epoch;
        item["ev"] = r.ev;
        item["stage"] = r.stage;
        item["batt"] = r.batt;
        item["aux"] = r.aux;
        item["dur_ms"] = r.dur_ms;
        item["src"] = r.src;
    }
    String out;
    serializeJson(doc, out);
    server.send(200, "application/json", out);
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
    SleepDiag d;
    memcpy(&d, (const void *)&sleepDiag, sizeof(d));
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

static void handlePmStats() {
    String text = pmStatsText();
    if (server.hasArg("timers")) {
        text += "\n";
        text += timerStatsText();
    }
    if (server.hasArg("diag")) {
        text += "\n";
        text += sleepDiagText();
        text += "\n";
        text += taskStatsText();
        text += "\n";
        text += timerStatsText();
    }
    DevLog.printf("[pm] stats over HTTP (%u bytes)\n", (unsigned)text.length());
    server.send(200, "text/plain; charset=utf-8", text);
}

// Debug capture: save the framebuffer as binary PBM (P4; 1 = black) right after
// a pre-sleep refresh, so a deep frame can be inspected later (the panel itself
// is unreachable while sleeping and the SSD1681 has no read-back on this board).
// Gated by rtcFrameCapture (off by default); fixed files, overwritten each time.
static void captureFrameToFs(const char *tag) {
    if (!rtcFrameCapture || !frame) return;
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
        for (int i = 0; i < n; i++) buf[i] = (uint8_t)(~frame[off + i] & 0xFF);
        f.write(buf, n);
    }
    f.close();
    File m = LittleFS.open("/frames/meta.txt", FILE_WRITE);
    if (m) {
        m.printf("tag=%s mode=%s stage=%u epoch=%lu\n", tag,
                 rtcMode == MODE_DEEP ? "deep" : "light", (unsigned)rtcStage,
                 (unsigned long)time(nullptr));
        m.close();
    }
    rtcFrameCaptures++;
    DevLog.printf("[cap] frame saved (%s)\n", tag);
}

// Debug screenshot: dump a framebuffer as binary PBM (P4; 1 = black).
// `?which=frame` (default) = live buffer, `last` = last pushed to the panel,
// `saved` = the pre-sleep capture from LittleFS. Read-only, token-free like /log.
static void handleFrame() {
    String which = server.hasArg("which") ? server.arg("which") : String("frame");
    if (which == "saved") {
        if (!LittleFS.exists("/frames/last.pbm")) {
            server.send(404, "text/plain", "no captured frame");
            return;
        }
        File f = LittleFS.open("/frames/last.pbm", FILE_READ);
        if (!f) {
            server.send(500, "text/plain", "open failed");
            return;
        }
        server.streamFile(f, "application/x-portable-bitmap");
        f.close();
        return;
    }
    UBYTE *src = frame;
    if (which == "last") src = lastDisplayedFrame;
    if (!src) {
        server.send(503, "text/plain", "no frame buffer");
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
    server.send(200, "application/x-portable-bitmap", out);
}

// Token-gated runtime knob for task-2 experiments: POST /diag?loop_delay=20
// changes the loop() yield without reflashing. Diagnostic-only endpoint.
static void handleDiag() {
    if (!requestAuthorized()) {
        server.send(401, "text/plain", "unauthorized");
        return;
    }
#if defined(CODEX_TARGET_NOTE4)
    if (server.hasArg("panel_power")) {
        const String mode = server.arg("panel_power");
        if (mode != "keep" && mode != "off_cache") {
            server.send(400, "text/plain", "panel_power must be keep|off_cache");
            return;
        }
        if (!setNote4PanelPower(mode == "keep")) {
            server.send(500, "text/plain", "panel_power NVS write failed");
            return;
        }
        if (epdAsleep) digitalWrite(EPD_PWR_PIN, note4KeepPanelPower ? HIGH : LOW);
        DevLog.printf("[diag] panel_power=%s\n", mode.c_str());
    }
#endif
    // Diagnostic BLE scan (Plan C task-6 §1.1): independent receiver for the
    // Windows publisher spike / bridge_first SCAN half. Blocking for the scan
    // duration; keep the client timeout above it.
    if (server.hasArg("blescan")) {
        long sec = server.arg("blescan").toInt();
        if (sec < 1 || sec > 30) {
            server.send(400, "text/plain", "blescan out of range 1..30");
            return;
        }
        long company = server.hasArg("company") ? server.arg("company").toInt() : 65535;
        if (company < 0 || company > 65535) {
            server.send(400, "text/plain", "company out of range 0..65535");
            return;
        }
        String out = bleScanJson((uint32_t)sec, (uint16_t)company, 48);
        DevLog.printf("[diag] blescan %lds company=%ld -> %u bytes\n",
                      sec, company, (unsigned)out.length());
        server.send(200, "application/json", out);
        return;
    }
    // Remote recovery for a stuck/aborted OTA (UpdateClass left "running").
    if (server.hasArg("ota_abort")) {
        otaUploadCleanup("diag");
        server.send(200, "text/plain", "ota aborted");
        return;
    }
    if (server.hasArg("loop_delay")) {
        long v = server.arg("loop_delay").toInt();
        if (v < 1 || v > 500) {
            server.send(400, "text/plain", "loop_delay out of range 1..500");
            return;
        }
        loopDelayMs = (uint32_t)v;
        DevLog.printf("[diag] loop_delay=%u ms\n", (unsigned)loopDelayMs);
    }
    if (server.hasArg("idle_deep_s")) {
        long v = server.arg("idle_deep_s").toInt();
        if (v < 15 || v > 86400) {
            server.send(400, "text/plain", "idle_deep_s out of range 15..86400");
            return;
        }
        idleDeepS = (uint32_t)v;
        noteActivity("diag");
        DevLog.printf("[diag] idle_deep_s=%u\n", (unsigned)idleDeepS);
    }
    if (server.hasArg("nvs_stage")) {
        nvsStageEnabled = server.arg("nvs_stage").toInt() ? 1 : 0;
        nvsStageWrites = 0;
        DevLog.printf("[diag] nvs_stage=%u\n", (unsigned)nvsStageEnabled);
    }
    if (server.hasArg("deep_usb")) {
        rtcDeepOnUsb = server.arg("deep_usb").toInt() ? 1 : 0;
        DevLog.printf("[diag] deep_usb=%u\n", (unsigned)rtcDeepOnUsb);
    }
    if (server.hasArg("frame_capture")) {
        rtcFrameCapture = server.arg("frame_capture").toInt() ? 1 : 0;
        DevLog.printf("[diag] frame_capture=%u\n", (unsigned)rtcFrameCapture);
    }
    // BLE rendezvous v2 gate (design §10): persisted rollback switch. Off by
    // a diagnostic rollback; enabling advertises `rendezvous_v` once the transaction
    // layer exists (stage 3+).
    if (server.hasArg("rv2")) {
        rv2Enabled = server.arg("rv2").toInt() ? 1 : 0;
        Preferences p;
        p.begin("pm", false);
        p.putUChar("rv2", rv2Enabled);
        p.end();
        updateInfoExtra();
        DevLog.printf("[diag] rv2=%u\n", (unsigned)rv2Enabled);
    }
    // Display-safety layer controls (design §8).
    if (server.hasArg("policy")) {
        rgnPolicyOn = strcmp(server.arg("policy").c_str(), "off") != 0;
        DevLog.printf("[diag] region policy %s\n", rgnPolicyOn ? "on" : "off");
    }
    if (server.hasArg("clean")) {
        forceCleanRefresh = server.arg("clean").toInt() != 0;
        if (forceCleanRefresh) renderCurrent();
    }
    if (server.hasArg("busy_fail")) {
        rtcEpdBusyFails++;
        epdBaselineTrusted = false;
        DevLog.println("[diag] injected display failure: baseline untrusted");
    }
    // task-10 B measurement/ops knobs: `blink_ms` sets the connect-blink phase
    // (0 disables), `blink_test=N` runs N blink ticks on the live template and
    // reports the per-tick waveform cost so the shipped period is measured.
    if (server.hasArg("blink_ms")) {
        long v = server.arg("blink_ms").toInt();
        if (v < 0 || v > 10000) {
            server.send(400, "text/plain", "blink_ms out of range 0..10000");
            return;
        }
        wifiBlinkMs = (uint16_t)v;
        DevLog.printf("[diag] blink_ms=%u\n", (unsigned)wifiBlinkMs);
    }
    if (server.hasArg("blink_test")) {
        long n = server.arg("blink_test").toInt();
        if (n < 1 || n > 60) {
            server.send(400, "text/plain", "blink_test out of range 1..60");
            return;
        }
        uint32_t total = 0, worst = 0, sumRefresh = 0;
        wifiConnActive = true;
        for (long i = 0; i < n; i++) {
            wifiBlinkOn = !wifiBlinkOn;
            rfnBlink = true;
            uint32_t t0 = millis();
            renderCurrent();
            uint32_t dt = millis() - t0;
            total += dt;
            sumRefresh += rfnLastMs;
            if (dt > worst) worst = dt;
        }
        wifiConnActive = false;
        wifiBlinkOn = false;
        renderCurrent();   // restore the real link state on the panel
        DevLog.printf("[diag] blink_test n=%ld avg=%ums worst=%ums refresh_avg=%ums\n",
                      n, (unsigned)(total / (uint32_t)n), (unsigned)worst,
                      (unsigned)(sumRefresh / (uint32_t)n));
        server.send(200, "text/plain",
                    "blink_test n=" + String(n) +
                    " avg_ms=" + String(total / (uint32_t)n) +
                    " worst_ms=" + String(worst) +
                    " refresh_avg_ms=" + String(sumRefresh / (uint32_t)n) +
                    " blink_ticks=" + String((unsigned)blinkTicks) + "\n");
        return;
    }
    if (server.hasArg("rgn")) {
        JsonDocument doc;
        JsonArray arr = doc.to<JsonArray>();
        for (uint8_t i = 0; i < rgnSet.n; i++) {
            Rgn &r = rgnSet.r[i];
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
        server.send(200, "application/json", out);
        return;
    }
    // Debug: render the template as deep/light without sleeping (RAM only, not
    // persisted) so the sleep glyph can be inspected while the device is online.
    if (server.hasArg("render_mode")) {
        String m = server.arg("render_mode");
        if (m == "deep") rtcMode = MODE_DEEP;
        else if (m == "light") rtcMode = MODE_LIGHT;
        else {
            server.send(400, "text/plain", "render_mode must be deep|light");
            return;
        }
        renderCurrent();
        DevLog.printf("[diag] render_mode=%s\n", m.c_str());
    }
    if (server.hasArg("tz")) {
        String tz = server.arg("tz");
        tz.trim();
        if (tz.length() == 0 || tz.length() >= (int)sizeof(deviceTz)) {
            server.send(400, "text/plain", "tz must be a POSIX TZ string, e.g. CST-8");
            return;
        }
        strncpy(deviceTz, tz.c_str(), sizeof(deviceTz) - 1);
        deviceTz[sizeof(deviceTz) - 1] = '\0';
        applyTimezone();
        Preferences p;
        p.begin("pm", false);
        p.putString("tz", deviceTz);
        p.end();
        DevLog.printf("[diag] tz=%s\n", deviceTz);
        renderCurrent();
    }
    bool deepNow = server.hasArg("deep_now") && server.arg("deep_now").toInt();
    if (deepNow) {
        noteActivity("diag");   // clears any pending forceDeepAt, so set it after
        forceDeepAtMs = millis() + 1500;
        DevLog.println("[diag] deep_now scheduled");
    }
    server.send(200, "text/plain", "loop_delay=" + String(loopDelayMs) + " ms, idle_deep_s=" +
                                          String(idleDeepS) + " s, nvs_stage=" +
                                          String((unsigned)nvsStageEnabled) + ", deep_usb=" +
                                          String((unsigned)rtcDeepOnUsb) + ", deep_now=" +
                                          String(deepNow ? 1 : 0) + ", tz=" + deviceTz +
                                          ", frame_capture=" + String((unsigned)rtcFrameCapture) +
                                          ", rv2=" + String((unsigned)rv2Enabled) +
                                          ", blink_ms=" + String((unsigned)wifiBlinkMs) +
#if defined(CODEX_TARGET_NOTE4)
                                          ", panel_power=" + String(note4KeepPanelPower ? "keep" : "off_cache") +
#endif
                                          "\n\n" + sleepDiagText());
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

static void handleClaim() {
    if (!requestAuthorized()) {
        server.send(401, "application/json",
                    String("{\"error\":\"unauthorized\",\"owner\":") + ownerJson() + "}");
        return;
    }
    V2ClaimArgs args = v2PrepareClaim(
        server.arg("id"), server.arg("name"), server.arg("host"),
        server.arg("port"), server.arg("lease"), server.hasArg("lease"),
        server.hasArg("force") && server.arg("force") != "0",
        server.hasArg("release") && server.arg("release") != "0");
    if (!args.validId) {
        server.send(400, "application/json", "{\"error\":\"args\"}");
        return;
    }
    OwnerRec cur;
    bool have = ownerGet(cur);
    V2ClaimDecision decision = v2DecideClaim(args, have, cur);

    if (decision.action == V2_CLAIM_RELEASE_EMPTY) {
        server.send(200, "application/json", "{\"owner\":null,\"released\":false}");
        return;
    }
    if (decision.action == V2_CLAIM_OCCUPIED) {
        if (!args.release)
            DevLog.printf("[owner] claim denied: held by %s\n", cur.id.c_str());
        server.send(409, "application/json",
                    String("{\"error\":\"occupied\",\"owner\":") + ownerJson() + "}");
        return;
    }
    if (decision.action == V2_CLAIM_RELEASE) {
        DevLog.printf("[owner] released by id=%s force=%d\n",
                      args.request.id.c_str(), args.force ? 1 : 0);
        ownerClear(true);
        server.send(200, "application/json", "{\"owner\":null,\"released\":true}");
        return;
    }

    ownerClaim(decision.request, decision.keepSince);
    // Design §6: a lease renewal is a protocol keep-alive, not user activity;
    // it must not extend the light phase (idleDeepDue would never fire while
    // the bridge renews every 60 s). Only a new claim resets the idle timer.
    if (decision.newClaim) noteActivity("claim");
    DevLog.printf("[owner] %s id=%s name=%s host=%s:%u lease=%us force=%d\n",
                  decision.keepSince ? "renew" : "claim", decision.request.id.c_str(),
                  decision.request.name.c_str(), decision.request.host.c_str(),
                  decision.request.port, (unsigned)decision.request.lease,
                  args.force ? 1 : 0);
    server.send(200, "application/json",
                String("{\"owner\":") + ownerJson() + ",\"renew\":" +
                    (decision.keepSince ? "true" : "false") + "}");
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

// Bounded mailbox from NimBLE's callback task to the device loop task.
static QueueHandle_t v2BleQueue = nullptr;
struct V2BleMessage { String body; String peer; };
static void handleBleV2Ctrl(const String &json) {
    if (!v2BleQueue || json.length() > 8192 || !blePeerIsBonded() || !blePeerIsEncrypted()) return;
    auto *message = new V2BleMessage{json, blePeerAddress()};
    if (xQueueSend(v2BleQueue, &message, 0) != pdTRUE) delete message;
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
    if (millis() - lastAnnounce > ANNOUNCE_MS) { sendAnnounce(bleOn); return; }
    // IP-change detection at 1 Hz: esp_netif_get_ip_info() is not free and
    // calling it every loop fragmented light sleep (see the 1 Hz housekeeping
    // tick in loop()).
    static uint32_t lastIpCheckMs = 0;
    if (lastIpCheckMs && (uint32_t)(millis() - lastIpCheckMs) < 1000) return;
    lastIpCheckMs = millis();
    if ((uint32_t)WiFi.localIP() != announcedIp) sendAnnounce(bleOn);
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
    if (!wifiUp && !(rv2Enabled && v2BundleReady)) return;
    if (!v2BleQueue) v2BleQueue = xQueueCreate(2, sizeof(V2BleMessage *));
    if (!bleInitialized()) {
        bleBegin("CodexStatus-" + macSuffix(), FW_VERSION);
        bleSetHandlers(handleBleUsage, handleBleEndpoint);
        bleSetTemplateHandlers(tplXferHandleCtrl, tplXferHandleChunk, tplXferReset);
        bleSetAuthHandler(handleBleAuth);
        bleSetV2Handler(handleBleV2Ctrl);
    }
    updateInfoExtra();
    if (!bleOn) {
        bleOn = true;
        bleUserOff = false;
        ledSet(true);
        ledFlash();
        DevLog.println("[ble] session on");
    }
    bleRadioMark(true);
    bleAdvertiseStart();
    if (userInitiated) {
        bleOpenPairingWindow(120000);
    }
    bool autoCond = plugged && !rtcDeepOnUsb && batteryPct > BLE_AUTO_PCT;
    lastBleAuto = autoCond;
    bleOffDeadline = autoCond ? 0 : millis() + BLE_GRACE_MS;
    requestAnnounce(true);
    if (!v2InRendezvous) renderCurrent();
}

static void bleOff(const char *reason) {
    if (!bleOn) return;
    bleOn = false;
    lastBleAuto = false;
    bleRadioMark(false);
    bleAdvertiseStop();
    bleDeinit();
    V2BleMessage *pending = nullptr;
    while (v2BleQueue && xQueueReceive(v2BleQueue, &pending, 0) == pdTRUE) delete pending;
    ledSet(false);
    ledFlash();
    DevLog.printf("[ble] session off (%s)\n", reason ? reason : "");
    requestAnnounce(false);
    if (!v2InRendezvous) renderCurrent();
}

// BLE keep-alive: infinite while plugged and >20%, otherwise 120 s after the
// last connection/transfer or after the keep-alive condition ends.
static void serviceBleSession() {
    if (!bleOn) return;
    bool autoCond = plugged && !rtcDeepOnUsb && batteryPct > BLE_AUTO_PCT;
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
    static uint32_t unplugStarted = 0;
    if (millis() - lastPoll < 500) return;
    lastPoll = millis();
    bool now = usb_serial_jtag_is_connected();
    if (now) unplugStarted = 0;
    else if (plugged) {
        if (!unplugStarted) unplugStarted = millis();
        if (millis() - unplugStarted < 10000) return;
    }
    if (now == plugged) return;
    plugged = now;
    DevLog.printf("[pm] usb %s\n", plugged ? "plugged" : "unplugged");
    ledFlash();
    renderCurrent();
    if (plugged) {
        bleUserOff = false;
        batteryPct = batteryPercent();
        if (wifiUp && !bleOn && !rtcDeepOnUsb && batteryPct > BLE_AUTO_PCT) enterBleOn(false);
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
            if (plugged && !rtcDeepOnUsb && !bleOn && !bleUserOff && batteryPct > BLE_AUTO_PCT) enterBleOn(false);
            sendAnnounce(bleOn);
            renderCurrent();
        }
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
        if (plugged && !rtcDeepOnUsb && (int32_t)(millis() - nextWifiRetry) >= 0) {
            nextWifiRetry = millis() + WIFI_RETRY_MS;
            ledFlash();
            retryWifi();
        }
        return;
    }
    if (millis() - wifiLostSince < WIFI_LOST_MS) return;
    wifiLostHandled = true;
    if (bleOn) bleOff("wifi lost");
    if (plugged && !rtcDeepOnUsb) {
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
    if (plugged && !rtcDeepOnUsb && wifiUp && !bleOn && !bleUserOff && batteryPct > BLE_AUTO_PCT) enterBleOn(false);
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

// Abort a running/half-finished OTA and restore the normal UI. Without this an
// aborted upload (client drop / stall) leaves UpdateClass "already running"
// (every later begin() fails), the OTA PM lock held (no light sleep) and the
// screen stuck on the OTA view -- the 0.14.10 battery OTA hang.
static void otaUploadCleanup(const char *reason) {
    DevLog.printf("[ota] abort (%s) running=%d err=%u\n", reason, Update.isRunning() ? 1 : 0,
                  (unsigned)Update.getError());
    Update.abort();
    otaInProgress = false;
    setOtaLock(false);
    setOtaWifiAwake(false);
    otaUploadDenied = false;
    if (frame) renderCurrent();
}

// ===========================================================================
// v2 protocol endpoints (auth: endpoint token + owner; no implicit renewal).
// ===========================================================================

// `bridge_id` arrives in the JSON body for data/plan; bundle/activate calls also
// carry it there (the query/form fallback exists for simple curl tests).
static bool v2ReplyOverBle = false;
static String v2RequestId;
static void v2Response(int status, const String &body) {
    if (!v2ReplyOverBle) { server.send(status, "application/json", body); return; }
    JsonDocument doc;
    if (deserializeJson(doc, body)) return;
    doc["ack"] = "v2";
    doc["request_id"] = v2RequestId;
    String reply;
    serializeJson(doc, reply);
    bleNotifyStatusQuiet(reply);
}

static bool v2OwnerOk(const char *bridgeId) {
    if (!ownerAllows(bridgeId)) {
        v2Response(409,
                    String("{\"result\":\"rejected\",\"error\":\"occupied\",\"owner\":") +
                        ownerJson() + "}");
        DevLog.printf("[v2] rejected: owner conflict id=%s\n", bridgeId ? bridgeId : "?");
        return false;
    }
    return true;
}

static void v2Ack(const char *op, const char *result, const char *display,
                  const char *retention, const char *error, int64_t seq, uint64_t planId,
                  const char *context, uint32_t acceptedRemainingS) {
    v2Response(200, v2BuildAck(op, result, display, retention, error, seq,
                               planId, context, acceptedRemainingS, FW_TARGET_ID));
}

// Only authenticated status exposes this boot session nonce.
static const String &v2Nonce() {
    if (!v2SessionNonce.length()) {
        char nonce[33];
        snprintf(nonce, sizeof(nonce), "%08x%08x%08x%08x", (unsigned)esp_random(),
                 (unsigned)esp_random(), (unsigned)esp_random(), (unsigned)esp_random());
        v2SessionNonce = nonce;
    }
    return v2SessionNonce;
}

static bool v2Command(const String &body, JsonDocument &doc) {
    if (v2ParseCommand(body, doc)) {
        v2Ack("command", "rejected", "unchanged", "ram", "json", -1, 0, nullptr, UINT32_MAX);
        return false;
    }
    if (!v2OwnerOk(doc["bridge_id"] | "")) return false;
    const char *request = doc["request_id"] | "";
    v2RequestId = request;
    String currentMac = macText();
    V2CommandSessionDecision session = v2CheckCommandSession(doc, currentMac, nullptr);
    if (session.needsNonce) {
        const String &nonce = v2Nonce();
        session = v2CheckCommandSession(doc, currentMac, &nonce);
    }
    if (!session.accepted) {
        v2Ack("command", "rejected", "unchanged", "ram", "session", -1, 0, nullptr, UINT32_MAX);
        return false;
    }
    return true;
}

// GET /v2/status: authenticated authoritative state (the beacon only points).
// Business channel auth = endpoint token (same trust as /usage and /template);
// the device operation token keeps gating /claim, /update, /doUpdate, /diag.
static void handleV2Status() {
    DevLog.printf("[v2] req status heap=%u", (unsigned)ESP.getFreeHeap());
    String mac;
    if (!endpointTokenAuthorized(mac)) {
        server.send(401, "application/json", "{\"result\":\"unauthorized\"}");
        return;
    }
    markSynced();   // authenticated bridge contact (v2 HTTP channel)
    V2StatusSnapshot snapshot;
    snapshot.mac = macText();
    snapshot.sessionNonce = v2Nonce();
    snapshot.profile = &v2Profile;
    snapshot.configured = v2BundleReady;
    snapshot.activeTemplateId = (v2BundleReady && v2Profile.count)
        ? v2Profile.ids[v2Profile.initial] : activeTplId;
    snapshot.dataSeq = &v2DataSeq;
    snapshot.displayState = v2DisplayState;
    snapshot.commitSeq = bsCommitSeq();
    snapshot.deepSleep = rtcMode == MODE_DEEP;
    snapshot.plan = &v2Plan;
    snapshot.provisional = v2Provisional;
    snapshot.bootMs = v2BootMs;
    snapshot.nowMs = v2NowMs();
    snapshot.battery = batteryPercent();
    server.send(200, "application/json", v2BuildStatusSnapshot(snapshot));
}

// POST /v2/data: atomic complete snapshot inside the current context.
static void applyV2Data(const String &body) {
    JsonDocument peek;
    if (!v2Command(body, peek)) return;
    uint64_t seq = peek["seq"] | 0ULL;
    V2DataDecision decision = v2DecideData(v2BundleReady && v2CtValid,
                                            v2CtValid ? &v2Ct : nullptr, body, seq,
                                            v2Profile.contextId, v2DataSeq);
    if (!decision.firstApplied) {
        v2Ack("data", decision.result, decision.display, "ram",
              decision.error.length() ? decision.error.c_str() : nullptr, decision.seq, 0,
              decision.includeContext ? v2Profile.contextId : nullptr, UINT32_MAX);
        return;
    }
    const String &usage = decision.usage;
    v2AppliedFields = "";
    serializeJson(peek["fields"], v2AppliedFields);
    v2LastAckSeq = seq;
    v2LastAckAtMs = v2NowMs();
    // Update on every accepted snapshot, before replying. No NVS wear and no
    // truncated field JSON; timer/physical deep wakes retain the exact baseline.
    v2DataCheckpoint.save(v2Profile.contextId, v2DataSeq);
    usageCacheSave(usage);
    lastUsage = usage;
    lastChannel = v2ReplyOverBle ? "BLE" : "PULL";
    const char *display = "unchanged";
    if (v2ReplyOverBle) {
        // Plan C: do not render inside the rendezvous window. The snapshot is
        // drawn once after the radio is off, together with the clock
        // (v2RendezvousRender); the ACK reports it as pending.
        v2WakeRenderPending = true;
        v2DisplayState = 2;
        display = "pending";
    } else {
        uint32_t before = epdWriteCount;
        uint32_t busyBefore = rtcEpdBusyFails;
        renderActiveUsage(usage, lastChannel.c_str());
        if (rtcEpdBusyFails != busyBefore) {
            display = "failed";
            v2DisplayState = 3;
        } else if (epdWriteCount != before) {
            display = "displayed";
            v2DisplayState = 1;
        } else {
            v2DisplayState = 1;
        }
    }
    v2Ack("data", "applied", display, "ram", nullptr, (int64_t)seq, 0, v2Profile.contextId,
          UINT32_MAX);
}

static void handleV2Data() {
    DevLog.printf("[v2] req data heap=%u", (unsigned)ESP.getFreeHeap());
    String mac;
    if (!endpointTokenAuthorized(mac)) {
        server.send(401, "application/json", "{\"result\":\"unauthorized\"}");
        return;
    }
    markSynced();   // authenticated bridge contact (v2 HTTP channel)
    applyV2Data(server.arg("plain"));
}

// POST /v2/plan: the only way to change the light deadline.
static void applyV2Plan(const String &body) {
    JsonDocument doc;
    if (!v2Command(body, doc)) return;
    V2PlanDecision decision = v2DecidePlan(doc, v2Plan, v2NowMs(), v2Provisional);
    if (!decision.accepted) {
        v2Ack("plan", decision.result, decision.display, "ram", decision.error,
              -1, decision.planId, decision.includeContext ? v2Profile.contextId : nullptr,
              UINT32_MAX);
        return;
    }
    const V2PowerPlan &plan = decision.plan;
    v2PlanReason = "bridge";
    if (plan.mode == V2_PLAN_LIGHT) {
        v2LightDeadlineMs = v2Plan.deadlineMs();
        v2Provisional = false;
        if (rtcMode != MODE_LIGHT) {
            rtcMode = MODE_LIGHT;
            persistMode();
        }
        DevLog.printf("[v2] plan %lu light %us\n", (unsigned long)plan.planId,
                      (unsigned)v2Plan.grantedS());
    } else {
        v2LightDeadlineMs = 0;
        v2Provisional = false;
        DevLog.printf("[v2] plan %lu sleep\n", (unsigned long)plan.planId);
    }
    v2Ack("plan", decision.result, decision.display, "ram", decision.error,
          -1, decision.planId, decision.includeContext ? v2Profile.contextId : nullptr,
          decision.grantedS);
}

static void handleV2Plan() {
    DevLog.printf("[v2] req plan heap=%u", (unsigned)ESP.getFreeHeap());
    String mac;
    if (!endpointTokenAuthorized(mac)) {
        server.send(401, "application/json", "{\"result\":\"unauthorized\"}");
        return;
    }
    markSynced();   // authenticated bridge contact (v2 HTTP channel)
    applyV2Plan(server.arg("plain"));
}

// One activation result per boot session. Expected context prevents delayed
// commands from activating after a local switch or a subsequent publication.
static String v2ActivateRequest, v2ActivateOwner, v2ActivateExpected;
static String v2ActivateTemplate, v2ActivateContext;
static void handleV2Activate() {
    String mac;
    if (!endpointTokenAuthorized(mac)) {
        server.send(401, "application/json", "{\"result\":\"unauthorized\"}");
        return;
    }
    JsonDocument doc;
    if (!v2Command(server.arg("plain"), doc)) return;
    V2ActivateDecision decision = v2DecideActivate(
        doc, v2BundleReady, v2Profile, v2ActivateRequest.c_str(),
        v2ActivateOwner.c_str(), v2ActivateTemplate.c_str(),
        v2ActivateExpected.c_str(), v2ActivateContext.c_str());
    if (decision.action != V2_ACTIVATE_SWITCH) {
        v2Ack("activate", decision.result, decision.display, "flash",
              decision.error, -1, 0, decision.context, UINT32_MAX);
        return;
    }
    if (!v2SwitchActive((uint8_t)decision.index)) {
        v2Ack("activate", "rejected", "unchanged", "flash",
              "activation_failed", -1, 0,
              v2Profile.contextId, UINT32_MAX);
        return;
    }
    v2ActivateRequest = decision.request; v2ActivateOwner = decision.owner;
    v2ActivateExpected = decision.expected; v2ActivateTemplate = decision.templateId;
    v2ActivateContext = v2Profile.contextId;
    renderCurrent();
    v2Ack("activate", "applied", "displayed", "flash", nullptr, -1, 0,
          v2Profile.contextId, UINT32_MAX);
}

// Owner/session-bound transfer; reading and writing never renews power.
static String v2CommittedRequest, v2CommittedOwner, v2CommittedContext;
static uint32_t v2CommittedCrc = 0, v2CommittedLength = 0;

static void v2BundleError(const char *error) {
    v2Ack("bundle", "rejected", "unchanged", "flash", error, -1, 0,
          v2Profile.contextId, UINT32_MAX);
}

static void handleV2BundleBegin() {
    String mac;
    if (!endpointTokenAuthorized(mac)) {
        server.send(401, "application/json", "{\"result\":\"unauthorized\"}"); return;
    }
    JsonDocument doc;
    if (!v2Command(server.arg("plain"), doc)) return;
    const String nonce = v2Nonce();
    const uint64_t nowMs = v2NowMs();
    const V2BundleFingerprint committed{
        v2CommittedOwner.c_str(), v2CommittedRequest.c_str(), v2CommittedCrc,
        v2CommittedLength, v2CommittedContext.c_str()
    };
    V2BundleBeginDecision decision = v2DecideBundleBegin(
        doc, v2Rx, committed, nonce.c_str(), nowMs);
    if (decision.action == V2_BUNDLE_BEGIN_REPLAY) {
        v2Ack("bundle", "applied", "unchanged", "flash", nullptr, -1, 0,
              decision.replayContext, UINT32_MAX);
        return;
    }
    if (decision.action == V2_BUNDLE_BEGIN_REJECT) {
        v2BundleError(decision.error);
        return;
    }
    if (decision.action == V2_BUNDLE_BEGIN_START) {
        if (!LittleFS.exists("/bundle")) LittleFS.mkdir("/bundle");
        File f = LittleFS.open(v2RxPath, "w");
        if (!f) { v2BundleError("open"); return; }
        f.close();
        v2Rx = decision.candidate;
    }
    server.send(200, "application/json", String("{\"result\":\"applied\",\"next_offset\":") +
                String(decision.nextOffset) + "}");
}

// WebServer's ordinary POST parser duplicates the complete body several times
// before calling the handler. On the BLE-enabled Note4 its internal heap is
// too small for repeated 4096-byte chunks. The raw callback streams from the
// parser's fixed 1436-byte buffer directly to LittleFS.
static File v2ChunkFile;
static bool v2ChunkOk = false;
static bool v2ChunkReplay = false;
static bool v2ChunkWriting = false;
static uint32_t v2ChunkOffset = 0;
static uint32_t v2ChunkBytes = 0;
static const char *v2ChunkFailure = "session";

static void handleV2BundleChunkRaw() {
    HTTPRaw &raw = server.raw();
    if (raw.status == RAW_START) {
        if (v2ChunkFile) v2ChunkFile.close();
        v2ChunkOk = false;
        v2ChunkWriting = false;
        v2ChunkBytes = 0;
        v2ChunkFailure = "session";
        String mac;
        if (!endpointTokenAuthorized(mac)) { v2ChunkFailure = "unauthorized"; return; }
        if (!ownerAllows(v2Rx.owner)) { v2ChunkFailure = "occupied"; return; }
        String request = server.header("X-Request-Id");
        String nonce = server.header("X-Session-Nonce");
        String offsetText = server.header("X-Offset");
        V2BundleChunkStartDecision decision = v2DecideBundleChunkStart(
            v2Rx, request.c_str(), nonce.c_str(), offsetText.c_str(), v2NowMs());
        if (!decision.allowed) { v2ChunkFailure = decision.error; return; }
        v2ChunkOffset = decision.offset;
        v2ChunkReplay = decision.replay;
        v2ChunkWriting = !v2ChunkReplay;
        v2ChunkFile = LittleFS.open(v2RxPath, v2ChunkReplay ? "r" : "a");
        if (!v2ChunkFile || (v2ChunkReplay && !v2ChunkFile.seek(v2ChunkOffset))) {
            v2ChunkFailure = "open"; return;
        }
        v2ChunkOk = true;
        return;
    }
    if (raw.status == RAW_WRITE && v2ChunkOk) {
        V2BundleChunkWriteDecision decision = v2DecideBundleChunkWrite(
            v2Rx, v2ChunkOffset, v2ChunkReplay, v2ChunkBytes,
            (uint32_t)raw.currentSize);
        if (!decision.allowed) {
            v2ChunkFailure = decision.error;
            v2ChunkOk = false;
            return;
        }
        if (v2ChunkReplay) {
            for (size_t i = 0; i < raw.currentSize; ++i) {
                if (v2ChunkFile.read() != raw.buf[i]) {
                    v2ChunkFailure = "chunk_conflict";
                    v2ChunkOk = false;
                    break;
                }
            }
        } else if (v2ChunkFile.write(raw.buf, raw.currentSize) != raw.currentSize) {
            v2ChunkFailure = "write";
            v2ChunkOk = false;
        }
        if (v2ChunkOk) v2ChunkBytes += raw.currentSize;
        return;
    }
    if (raw.status == RAW_ABORTED) {
        v2ChunkFailure = "aborted";
        v2ChunkOk = false;
    }
    if (raw.status == RAW_END || raw.status == RAW_ABORTED) {
        if (v2ChunkFile) v2ChunkFile.close();
        if (v2ChunkOk) {
            V2BundleChunkEndDecision decision = v2DecideBundleChunkEnd(
                v2Rx, v2ChunkOffset, v2ChunkReplay, v2ChunkBytes, v2NowMs());
            if (!decision.allowed) {
                v2ChunkFailure = decision.error;
                v2ChunkOk = false;
            } else {
                v2Rx.offset = decision.nextOffset;
            }
        }
        if (!v2ChunkOk && v2ChunkWriting) v2Rx.deadline = 0;
    }
}

static void handleV2BundleChunk() {
    if (!v2ChunkOk) {
        if (!strcmp(v2ChunkFailure, "unauthorized")) {
            server.send(401, "application/json", "{\"result\":\"unauthorized\"}");
        } else {
            v2BundleError(v2ChunkFailure);
        }
        return;
    }
    server.send(200, "application/json", String("{\"result\":\"applied\",\"next_offset\":") +
                String(v2Rx.offset) + "}");
}

static void handleV2BundleCommit() {
    String mac;
    if (!endpointTokenAuthorized(mac)) {
        server.send(401, "application/json", "{\"result\":\"unauthorized\"}"); return;
    }
    JsonDocument doc;
    if (!v2Command(server.arg("plain"), doc)) return;
    const String nonce = v2Nonce();
    const uint64_t nowMs = v2NowMs();
    const String bridgeIdFallback = server.arg("bridge_id");
    const V2BundleFingerprint committed{
        v2CommittedOwner.c_str(), v2CommittedRequest.c_str(), v2CommittedCrc,
        v2CommittedLength, v2CommittedContext.c_str()
    };
    String body;
    V2BundleCommitDecision decision = v2DecideBundleCommit(
        doc, v2Rx, committed, nonce.c_str(), nowMs, v2RxPath.c_str(),
        bridgeIdFallback.c_str(), body);
    if (decision.action == V2_BUNDLE_COMMIT_REJECT) {
        v2BundleError(decision.error);
        return;
    }
    if (decision.action == V2_BUNDLE_COMMIT_REPLAY) {
        v2Ack("bundle", "applied", "unchanged", "flash", nullptr, -1, 0,
              decision.replayContext, UINT32_MAX);
        return;
    }
    if (decision.action == V2_BUNDLE_COMMIT_ALREADY_ACTIVE) {
        bsProfile(v2Profile);
        v2CommittedOwner = decision.owner; v2CommittedRequest = decision.request;
        v2CommittedCrc = decision.crc; v2CommittedLength = decision.length;
        v2CommittedContext = v2Profile.contextId;
        v2Rx.deadline = 0;
        LittleFS.remove(v2RxPath);
        v2Ack("bundle", "applied", "unchanged", "flash", nullptr, -1, 0,
              v2Profile.contextId, UINT32_MAX);
        return;
    }
    char ctx[BS_CTX_LEN];
    snprintf(ctx, sizeof(ctx), "%08x%08x", v2CtxGen.next(), (unsigned)esp_random());
    String err;
    if (!bsInstall(body, FW_TARGET_ID, RENDER_TARGET_ID, ctx, err)) {
        v2BundleError(err.c_str()); return;
    }
    v2CommittedOwner = decision.owner; v2CommittedRequest = decision.request;
    v2CommittedCrc = decision.crc; v2CommittedLength = decision.length;
    v2CommittedContext = ctx;
    v2Rx.deadline = 0;
    LittleFS.remove(v2RxPath);
    bsProfile(v2Profile);
    v2BundleReady = true;
    v2DataSeq.beginContext(v2NowMs(), 1);
    v2AppliedFields = "";
    v2ActiveLoad();
    renderCurrent();
    v2Ack("bundle", "applied", "displayed", "flash", nullptr, -1, 0, v2Profile.contextId,
          UINT32_MAX);
}

static void serviceV2Ble() {
    if (!v2BleQueue) return;
    V2BleMessage *message = nullptr;
    if (xQueueReceive(v2BleQueue, &message, 0) != pdTRUE) return;
    String body = message->body, peer = message->peer;
    delete message;
    if (!blePeerIsBonded() || !blePeerIsEncrypted() || peer != blePeerAddress()) return;
    JsonDocument doc;
    if (deserializeJson(doc, body)) return;
    v2RequestId = doc["request_id"] | "";
    if (!v2RequestId.length() || v2RequestId.length() > 64) return;
    bool authenticated = false;
    for (int i = 0; i < storeCount(); ++i) {
        EndpointRec endpoint;
        if (storeGet(i, endpoint) && endpoint.token.length() &&
            endpoint.token == (doc["token"] | "") && endpoint.mac == peer) {
            authenticated = true; break;
        }
    }
    v2ReplyOverBle = true;
    if (!authenticated || !rv2Enabled) {
        v2Ack("command", "rejected", "unchanged", "ram",
              authenticated ? "disabled" : "unauthorized", -1, 0, nullptr, UINT32_MAX);
    } else {
        // Plan C: the bridge stamps server_time/tz_offset_min into every
        // rendezvous command (status/plan/data); adopt them before any handler
        // renders or replies. Missing fields keep the local RTC fallback.
        if (!doc["server_time"].isNull()) adoptServerTimeForce(doc);
        // An authenticated rendezvous command is a successful bridge contact:
        // refresh the sync timestamp that drives the offline_mins row. The v2
        // channel never went through the legacy HTTP markSynced() paths, so
        // rv2 devices used to show a stale "offline N hours" while in contact.
        markSynced();
        const char *op = doc["op"] | "";
        if (!strcmp(op, "status")) {
            JsonDocument state;
            state["result"] = "applied";
            state["session_nonce"] = v2Nonce();
            state["device_mac"] = macText();
            state["active_context_id"] = v2Profile.contextId;
            state["active_template_id"] = v2Profile.count ? v2Profile.ids[v2Profile.initial] : "";
            state["committed_job_id"] = v2Profile.jobId;
            state["applied_seq"] = v2DataSeq.appliedSeq();
            state["data_seq"] = v2DataSeq.appliedSeq();
            state["power"]["mode"] = v2Plan.lightActive(v2NowMs()) ? "light" : "sleep";
            state["power"]["plan_id"] = v2Plan.acceptedId();
            state["power"]["remaining_s"] = v2Plan.remainingS(v2NowMs());
            state["power"]["provisional_remaining_s"] = v2Provisional ?
                V2PlanState::bootProvisionalRemaining(v2BootMs, v2NowMs()) : 0;
            String out;
            serializeJson(state, out);
            v2Response(200, out);
        } else if (!strcmp(op, "data")) {
            applyV2Data(body);
        } else if (!strcmp(op, "plan")) {
            applyV2Plan(body);
        } else {
            v2Ack(op, "rejected", "unchanged", "ram", "http_required", -1, 0, nullptr, UINT32_MAX);
        }
    }
    v2ReplyOverBle = false;
    v2RequestId = "";
}

// Timer wakes only open BLE; Wi-Fi requires an accepted light plan.
// Plan C: hard 3 s window; after a plan ACK it closes 200 ms later. No screen
// work happens while the radio is on -- the caller renders once afterwards.
static void v2RendezvousRender(bool light);
static bool v2Rendezvous() {
    v2InRendezvous = true;
    enterBleOn(false);
    const uint64_t windowStart = v2NowMs();
    uint64_t deadline = windowStart + V2_RENDEZVOUS_WINDOW_MS;
    uint64_t answeredAt = 0;
    uint64_t connectedAt = 0;
    while (v2NowMs() < deadline) {
        blePoll();
        serviceV2Ble();
        if (!connectedAt && bleIsConnected()) {
            // The central is in; the wait deadline is done. Give the command
            // exchange its own bound instead of cutting a working link.
            connectedAt = v2NowMs();
            deadline = connectedAt + V2_RENDEZVOUS_CONNECTED_MS;
            DevLog.printf("[v2] rendezvous peer connected at %ums\n",
                          (unsigned)(connectedAt - windowStart));
        }
        if (v2Plan.accepted()) {
            if (!answeredAt) answeredAt = v2NowMs();
            if (v2NowMs() - answeredAt >= V2_RENDEZVOUS_ACK_GRACE_MS) break;
        }
        delay(10);
    }
    bool light = v2Plan.lightActive(v2NowMs());
    bleOff("rendezvous complete");
    v2InRendezvous = false;
    wakeResult = light ? WAKE_RV_LIGHT : WAKE_RV_SLEEP;
    rtcNetCycles++;
    DevLog.printf("[v2] rendezvous %s plan=%lu time=%s awake=%ums ble=%ums win=%ums\n",
                  answeredAt ? "answered" : "timeout", (unsigned long)v2Plan.acceptedId(),
                  timeKnown() ? timeSourceName(timeSource) : "none", (unsigned)millis(),
                  (unsigned)bleRadioMs(), (unsigned)(v2NowMs() - windowStart));
    v2RendezvousRender(light);
    return light;
}

// Plan C: the single wake render, after the BLE window is closed. The reserved
// clock window is written directly (same partial path as thin deep wakes) so
// the sleep frame stays untouched; a full frame is only the failure fallback.
// No clock in the template (or unknown time) -> nothing to draw.
static void v2RendezvousClockRender() {
    if (!clkR.valid || !activeTplHasNow || !timeKnown()) {
        DevLog.printf("[v2] rendezvous render skipped (clk=%d now=%d time=%d)\n",
                      clkR.valid ? 1 : 0, activeTplHasNow ? 1 : 0, timeKnown() ? 1 : 0);
        return;
    }
    if (!panelThinReady) epdThinBegin();
    const uint32_t t0 = millis();
    // First wake after a cold boot/OTA: the software old-window is unknown, so
    // seed it from the framebuffer (full baseline) instead of a window write.
    if (!clkPixelsValid) {
        forceCleanRefresh = true;
        renderCurrent();
        clkCaptureFromFramebuffer();
        DevLog.printf("[v2] rendezvous full frame (no clock baseline) in %ums\n",
                      (unsigned)(millis() - t0));
        return;
    }
    if (!clockTickWake()) {
        DevLog.println("[v2] rendezvous clock window failed; full frame");
        forceCleanRefresh = true;
        renderCurrent();
        clkCaptureFromFramebuffer();
        return;
    }
    DevLog.printf("[v2] rendezvous clock rendered in %ums (src=%s)\n",
                  (unsigned)(millis() - t0), timeSourceName(timeSource));
}

// Plan C: the one render after the window. A BLE snapshot applied during the
// window is drawn as a full frame (data + clock together); a light plan keeps
// its clock for the light first frame; a sleep plan otherwise writes only the
// reserved clock window.
static void v2RendezvousRender(bool light) {
    if (v2WakeRenderPending) {
        const uint32_t t0 = millis();
        const uint32_t busyBefore = rtcEpdBusyFails;
        // Light: this frame is the Zzz-removing wake baseline, so
        // startNormalMode must not force a second full refresh (task-10).
        if (light) forceCleanRefresh = true;
        renderActiveUsage(lastUsage, lastChannel.c_str());
        if (light) wakeBaselineDrawn = true;
        v2WakeRenderPending = false;
        clkCaptureFromFramebuffer();
        v2DisplayState = (rtcEpdBusyFails != busyBefore) ? 3 : 1;
        DevLog.printf("[v2] rendezvous %s frame (data+clock) in %ums\n",
                      light ? "light" : "sleep", (unsigned)(millis() - t0));
        return;
    }
    if (light) {
        // The clock rides in the light first frame (startNormalMode).
        DevLog.println("[v2] rendezvous light plan; clock in the first light frame");
        return;
    }
    v2RendezvousClockRender();
}

static void registerHttpRoutes() {
    const char *bundleHeaders[] = {"X-Request-Id", "X-Session-Nonce", "X-Offset"};
    server.collectHeaders(bundleHeaders, 3);
    server.on("/", HTTP_GET, handleStatus);
    server.on("/status.json", HTTP_GET, handleStatusJson);
    server.on("/log", HTTP_GET, handleLog);
    server.on("/history", HTTP_GET, handleHistory);
    server.on("/pmstats", HTTP_GET, handlePmStats);
    server.on("/frame", HTTP_GET, handleFrame);
    server.on("/diag", HTTP_POST, handleDiag);
    server.on("/usage", HTTP_POST, handleUsagePost);
    server.on("/template", HTTP_POST, handleTemplatePost);
    server.on("/claim", HTTP_POST, handleClaim);
    // v2 platform protocol: authenticated business endpoints (token + owner);
    // none of them extend the light deadline implicitly.
    server.on("/v2/status", HTTP_GET, handleV2Status);
    server.on("/v2/data", HTTP_POST, handleV2Data);
    server.on("/v2/plan", HTTP_POST, handleV2Plan);
    server.on("/v2/activate", HTTP_POST, handleV2Activate);

    server.on("/v2/bundle/begin", HTTP_POST, handleV2BundleBegin);
    server.on("/v2/bundle/chunk", HTTP_POST, handleV2BundleChunk, handleV2BundleChunkRaw);
    server.on("/v2/bundle/commit", HTTP_POST, handleV2BundleCommit);
    server.on("/update", HTTP_GET, handleUpdatePage);
    server.on("/doUpdate", HTTP_POST,
        []() {
            server.sendHeader("Connection", "close");
            if (otaUploadDenied) {
                server.send(401, "text/plain", "unauthorized: negotiate a token over BLE first");
                return;
            }
            setOtaLock(false);
            setOtaWifiAwake(false);
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
                // Dual-side target check: a ROM built for another firmware
                // target must never be accepted (v2 §5/§9).
                if (server.hasArg("target") && server.arg("target") != FW_TARGET_ID) {
                    otaUploadDenied = true;
                    DevLog.printf("[ota] rejected: target %s != %s\n",
                                  server.arg("target").c_str(), FW_TARGET_ID);
                    return;
                }
                otaUploadDenied = false;
                otaInProgress = true;
                setOtaLock(true);
                setOtaWifiAwake(true);
                otaLastDataMs = millis();
                DevLog.printf("[ota] upload start: %s\n", up.filename.c_str());
                std::vector<String> lines = {"OTA update", FW_VERSION, up.filename};
                screen(lines);
                if (!Update.begin(UPDATE_SIZE_UNKNOWN)) {
                    DevLog.printf("[ota] begin failed: %u\n", (unsigned)Update.getError());
                    otaUploadCleanup("begin-failed");
                }
            } else if (up.status == UPLOAD_FILE_WRITE) {
                if (otaUploadDenied || !otaInProgress) return;
                otaLastDataMs = millis();
                if (Update.write(up.buf, up.currentSize) != up.currentSize) {
                    DevLog.printf("[ota] write failed: %u\n", (unsigned)Update.getError());
                    otaUploadCleanup("write-failed");
                }
            } else if (up.status == UPLOAD_FILE_END) {
                otaInProgress = false;
                if (otaUploadDenied) { setOtaLock(false); return; }
                if (Update.end(true)) {
                    DevLog.printf("[ota] success %u bytes, rebooting shortly\n", (unsigned)up.totalSize);
                    screen({"OTA success", "Rebooting..."});
                    // 5 min light window for the bridge (NVS survives the OTA).
                    Preferences p;
                    p.begin("pm", false);
                    p.putUShort("post_ota_s", 300);
                    p.end();
                    otaRebootPending = true;
                    otaRebootAt = millis() + 1500;
                } else {
                    DevLog.printf("[ota] end failed: %u\n", (unsigned)Update.getError());
                    otaUploadCleanup("end-failed");
                }
            } else if (up.status == UPLOAD_FILE_ABORTED) {
                otaUploadCleanup("aborted");
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

static void startNormalMode(bool skipConnect = false) {
    configMode = false;
    hostname = "codex-status-" + macSuffix();
    // Keep the active TZ (NVS/bridge-provided); a hard-coded "CST-8" here used
    // to clobber a bridge-synced timezone on every light start.
    configTzTime(deviceTz, "pool.ntp.org");
    pinMode(0, INPUT_PULLUP);
    pinMode(18, INPUT_PULLUP);
    pinMode(3, OUTPUT);
    digitalWrite(3, HIGH);   // green LED off (active low)

    configurePowerManagement();
    setupOtaPmLock();
    announceUdp.begin(0);

    // Deep wake: leave the sleep frame at once (Zzz gone) so BOOT has instant
    // feedback; Wi-Fi is not up yet, so the template keeps its icon hidden.
    // This first wake frame is a clean full baseline; a partial here can leave
    // a Zzz ghost that reads as "still asleep". The timer pull path already
    // drew that baseline inside deepNetworkCycle (wakeBaselineDrawn), so it
    // must not draw a second full frame here.
    if (!wakeBaselineDrawn && wokeFromDeep && rtcMode == MODE_LIGHT && (rtcDeepGlyph & 1)) {
        forceCleanRefresh = true;
        renderCurrent();
    }
    wakeBaselineDrawn = false;

    // task-10 A: cold boot with a cached usage snapshot shows the template at
    // once (WIFI OFF, no Connecting page) instead of blocking on association.
    // The link-up render below then adds the Wi-Fi icon with one partial.
    const bool cachedColdBoot = !wokeFromDeep && lastUsage.length() > 0;
    if (cachedColdBoot) renderCurrent();

    if (skipConnect && WiFi.status() == WL_CONNECTED) {
        // Deep pull already fast-connected: no scan, no second association.
        wifiUp = true;
    } else {
        wifiUp = connectBest(!wokeFromDeep && !cachedColdBoot);
    }
    registerHttpRoutes();
    server.begin();

    if (wifiUp) {
        wifiLostHandled = false;
        wifiLostSince = 0;
        lastBattCheck = millis();
        batteryPct = batteryPercent();
        configureWifiPowerSave();
        saveApInfo();
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
        // task-3: no mDNS responder. Name resolution (.local) is dropped on
        // purpose (the bridge discovers via UDP/ARP/BLE); the DHCP hostname
        // (option 12) set by WiFi.setHostname still shows in the router.
        ArduinoOTA.setMdnsEnabled(false);
        ArduinoOTA.begin();
        // Wi-Fi is up: render again so the Wi-Fi icon appears (the wake render
        // drew the template while the state was still WIFI OFF). rtcDeepGlyph
        // bit0 = the last deep entry actually drew the sleep glyph.
        if (wokeFromDeep && (rtcDeepGlyph & 1)) renderCurrent();
        if (plugged && !rtcDeepOnUsb && batteryPct > BLE_AUTO_PCT) enterBleOn(false);
        if (!v2BundleReady && !deepWakePath && storeCount() > 0) tryWifiUsage();
        // Cold boot: never leave the boot/connecting page up when the first
        // pull failed. The cached usage (or the built-in status screen) is
        // rendered once the link is up, so the panel is never stale. An
        // identical frame costs no write (RFN_NONE). With a cached snapshot
        // this is also the single link-up partial that lights the Wi-Fi icon
        // (the template was already drawn with WIFI OFF before the connect).
        if (!wokeFromDeep) renderCurrent();
        noteActivity(deepWakePath ? "deep-light" : "boot");
        sendAnnounce(bleOn);
    } else {
        wifiLostHandled = true;
        wifiLostSince = millis() - WIFI_LOST_MS;   // already past loss detection
        if (plugged && !rtcDeepOnUsb) {
            nextWifiRetry = millis() + WIFI_RETRY_MS;
            DevLog.println("[wifi] no link at boot (plugged): retry every 60s");
        } else {
            uint32_t delaySec = retryDelaySec(rtcRetryStage);
            if (rtcRetryStage < 7) rtcRetryStage++;
            DevLog.printf("[wifi] no link at boot (battery): deep sleep %us stage=%u\n",
                          (unsigned)delaySec, (unsigned)rtcRetryStage);
            // The wake render already replaced the sleep frame; put it back
            // before returning to deep so the panel does not sit on a stale
            // light frame (full baseline, same reason as enterDeep).
            if (wokeFromDeep && rtcMode == MODE_LIGHT && (rtcDeepGlyph & 1)) renderSleepGlyph();
            rtcNextNetAt = timeKnown() ? (uint32_t)time(nullptr) + delaySec : 0;
            sleepToNextEvent();
        }
    }

    DevLog.printf("[net] ready state=%s ip=%s host=%s.local endpoints=%d\n",
                  deviceStateText(), ipText().c_str(), hostname.c_str(), storeCount());
}

// ---------------- v0.14 deep mode (docs/power-state.md §13) ----------------
// deep: minute RTC wake -> clock-window direct write only; every
// `rtcNextContactS` one network window fast-connects and pulls GET /usage.
// light: current always-connected behavior. The bridge decides the mode in
// the pull response; the device falls back to a local idle timer.

static void persistMode() {
    Preferences p;
    p.begin("pm", false);
    p.putUChar("mode", rtcMode);
    p.putUShort("next", rtcNextContactS);
    p.putUInt("next_at", rtcNextNetAt);
    p.end();
}

// Deep sleep does not lose the RTC timer; reconstruct the wall clock from the
// last sleep anchor so minute ticks stay aligned without a network contact.
static void restoreTimeFromRtc() {
    if (!rtcEpochAtSleep || !rtcClkUsAtSleep) return;
    uint64_t nowUs = esp_rtc_get_time_us();
    if (nowUs <= rtcClkUsAtSleep) return;
    time_t t = (time_t)rtcEpochAtSleep + (time_t)((nowUs - rtcClkUsAtSleep) / 1000000ULL);
    struct timeval tv = {t, 0};
    settimeofday(&tv, nullptr);
}

static uint32_t secsToNextMinute() {
    if (!timeKnown()) return 60;
    time_t n = time(nullptr);
    uint32_t s = (uint32_t)(60 - (n % 60));
    if (s < 5) s += 60;   // keep a margin so ticks never double-fire
    return s;
}

static void noteActivity(const char *reason) {
    lastActivityEpoch = timeKnown() ? time(nullptr) : 0;
    lastActivityMs = millis();
    forceDeepAtMs = 0;
    (void)reason;
}

// Local idle fallback: quiet for `idleDeepS` (default 10 min) on battery with
// no BLE/OTA session -> deep. A bridge `mode:"deep"` hint shortens the last
// stretch to 60 s of grace (see the push handlers).
static bool idleDeepDue() {
    if ((plugged && !rtcDeepOnUsb) || bleOn || otaInProgress || configMode) return false;
    if (!wifiUp || !timeKnown()) return false;
    // Post-OTA minimum window: never descend before it elapses; the bridge can
    // still extend the online time with a formal light PowerPlan.
    if (postOtaHoldUntilMs && (int32_t)(millis() - postOtaHoldUntilMs) < 0) return false;
    if (forceDeepAtMs && (int32_t)(millis() - forceDeepAtMs) >= 0) return true;
    time_t since = lastActivityEpoch ? lastActivityEpoch : (time_t)rtcLastSyncEpoch;
    if (!since) return false;
    return (long)(time(nullptr) - since) >= (long)idleDeepS;
}

static void saveApInfo() {
    if (WiFi.status() != WL_CONNECTED) return;
    rtcApChannel = (uint8_t)WiFi.channel();
    strncpy(rtcApBssid, WiFi.BSSIDstr().c_str(), sizeof(rtcApBssid) - 1);
    rtcApBssid[sizeof(rtcApBssid) - 1] = '\0';
    prefs.begin("wifi", true);
    rtcApSlot = prefs.getUChar("last", 0xFF);
    prefs.end();
}

static bool parseBssid(const char *s, uint8_t out[6]) {
    unsigned v[6];
    if (sscanf(s, "%x:%x:%x:%x:%x:%x", &v[0], &v[1], &v[2], &v[3], &v[4], &v[5]) != 6) {
        return false;
    }
    for (int i = 0; i < 6; i++) out[i] = (uint8_t)v[i];
    return true;
}

// Fast connect with the cached BSSID/channel/slot (no scan, ~1.3-1.5 s); falls
// back to a plain association when the AP moved.
static bool deepFastConnect() {
    uint8_t slot = rtcApSlot;
    prefs.begin("wifi", true);
    if (slot >= MAX_SLOTS) slot = prefs.getUChar("last", 0);
    wifiSsid = prefs.getString(("s" + String(slot)).c_str(), "");
    wifiPass = prefs.getString(("p" + String(slot)).c_str(), "");
    prefs.end();
    if (!wifiSsid.length()) return false;
    WiFi.mode(WIFI_STA);
    WiFi.setHostname(hostname.c_str());
    uint8_t bssid[6];
    bool fast = rtcApChannel > 0 && rtcApBssid[0] && parseBssid(rtcApBssid, bssid);
    uint32_t t0 = millis();
    if (fast) WiFi.begin(wifiSsid.c_str(), wifiPass.c_str(), rtcApChannel, bssid);
    else      WiFi.begin(wifiSsid.c_str(), wifiPass.c_str());
    while (WiFi.status() != WL_CONNECTED && millis() - t0 < 6000) delay(50);
    if (WiFi.status() != WL_CONNECTED && fast) {
        DevLog.println("[deep] fast connect failed; plain retry");
        WiFi.disconnect(false);
        delay(50);
        WiFi.begin(wifiSsid.c_str(), wifiPass.c_str());
        t0 = millis();
        while (WiFi.status() != WL_CONNECTED && millis() - t0 < 10000) delay(50);
    }
    bool up = WiFi.status() == WL_CONNECTED;
    DevLog.printf("[deep] wifi %s fast=%d %ums ip=%s\n", up ? "up" : "fail",
                  fast ? 1 : 0, (unsigned)(millis() - t0),
                  up ? WiFi.localIP().toString().c_str() : "-");
    if (up) saveApInfo();
    return up;
}

// Preferred endpoint for a pull: the active bridge when known, else the MRU.
static bool pickEndpoint(EndpointRec &rec) {
    int n = storeCount();
    if (n <= 0) return false;
    if (rtcActiveMac[0]) {
        for (int i = 0; i < n; i++) {
            EndpointRec r;
            if (storeGet(i, r) && r.mac == String(rtcActiveMac)) { rec = r; return true; }
        }
    }
    int best = -1;
    uint32_t bestMru = 0;
    for (int i = 0; i < n; i++) {
        EndpointRec r;
        if (!storeGet(i, r)) continue;
        if (best < 0 || r.mru >= bestMru) { best = i; bestMru = r.mru; }
    }
    return best >= 0 && storeGet(best, rec);
}

static void rememberActiveTemplate() {
    String id = tplStoreActive();
    TplMeta meta;
    if (!id.length() || !tplStoreFind(id, meta)) return;
    strncpy(rtcTplActiveId, id.c_str(), sizeof(rtcTplActiveId) - 1);
    rtcTplActiveId[sizeof(rtcTplActiveId) - 1] = '\0';
    strncpy(rtcTplHash, meta.hash.c_str(), sizeof(rtcTplHash) - 1);
    rtcTplHash[sizeof(rtcTplHash) - 1] = '\0';
}

static bool activeTemplateChanged() {
    String id = tplStoreActive();
    TplMeta meta;
    if (!id.length() || !tplStoreFind(id, meta)) return true;
    return id != String(rtcTplActiveId) || meta.hash != String(rtcTplHash);
}

// When the next timer wake is due (no net contact planned -> immediate retry).
static bool deepNetDue() {
    if (!rtcNextNetAt) return true;
    if (!timeKnown()) return true;
    return (time_t)time(nullptr) >= (time_t)rtcNextNetAt;
}

// One deep network window: fast connect -> GET /usage -> execute the bridge
// response. Returns 0 stay deep, 1 switch to light, 2 stay awake for `pending`.
static int deepNetworkCycle() {
    const time_t before = timeKnown() ? time(nullptr) : 0;
    setStage(10);
    if (!deepFastConnect()) {
        setStage(13);
        rtcNetFails++;
        histAdd(HIST_NET_FAIL, 0);
        if (rtcRetryStage < 7) rtcRetryStage++;
        uint32_t delaySec = retryDelaySec(rtcRetryStage);
        rtcNextNetAt = timeKnown() ? (uint32_t)time(nullptr) + delaySec : 0;
        rtcLastPullCode = 0;
        DevLog.printf("[deep] pull skipped: no wifi; retry %us stage=%u\n",
                      (unsigned)delaySec, (unsigned)rtcRetryStage);
        return 0;
    }
    EndpointRec rec;
    if (!pickEndpoint(rec)) {
        setStage(13);
        rtcNetFails++;
        histAdd(HIST_NET_FAIL, 0);
        rtcNextNetAt = timeKnown() ? (uint32_t)time(nullptr) + 900 : 0;
        DevLog.println("[deep] pull skipped: no bridge endpoint");
        return 0;
    }
    String path = "/usage?next_contact_s=" + String((unsigned)rtcNextContactS) +
                  "&mode=deep&usage_rev=" + String((unsigned)rtcUsageRev);
    String body, err;
    const uint32_t t0 = millis();
    bool ok = usageHttpGet(rec, body, err, 4000, path);
    DevLog.printf("[deep] pull %s:%u %lums %s len=%u\n",
                  rec.host.c_str(), rec.port, (unsigned)(millis() - t0),
                  ok ? "ok" : err.c_str(), (unsigned)body.length());
    rtcLastPullCode = ok ? 200 : 255;
    if (!ok) {
        setStage(13);
        rtcNetFails++;
        histAdd(HIST_NET_FAIL, 0);
        if (rtcRetryStage < 7) rtcRetryStage++;
        uint32_t delaySec = retryDelaySec(rtcRetryStage);
        rtcNextNetAt = timeKnown() ? (uint32_t)time(nullptr) + delaySec : 0;
        return 0;
    }
    JsonDocument doc;
    if (deserializeJson(doc, body) || doc.as<JsonObject>().isNull()) {
        setStage(13);
        rtcNetFails++;
        histAdd(HIST_NET_FAIL, 0);
        rtcNextNetAt = timeKnown() ? (uint32_t)time(nullptr) + 300 : 0;
        DevLog.println("[deep] pull rejected: invalid JSON");
        return 0;
    }
    rtcNetCycles++;
    setStage(12);
    histAdd(HIST_NET_OK, 200);
    bool tzChanged = adoptServerTimeForce(doc);   // bridge clock is authoritative on contact
    markSynced();
    nvsStageMark(41);
    bool firstPull = lastUsage.length() == 0;
    lastUsage = body;
    lastChannel = "PULL";

    const char *mode = doc["mode"] | "deep";
    // Decide the mode before rendering: leaving deep must be drawn as a light
    // frame in the *same* full refresh that erases the Zzz glyph. Rendering
    // the deep frame first and then a partial over it leaves a Zzz ghost that
    // looks like the device is still asleep (design §3: the wake frame is a
    // full baseline).
    const bool toLight = !strcmp(mode, "light");
    const bool leavingDeep = toLight && (rtcDeepGlyph & 1);
    if (toLight) {
        rtcMode = MODE_LIGHT;
        persistMode();
    }
    long ncs = doc["next_contact_s"] | 0L;
    if (ncs >= DEEP_CONTACT_MIN_S && ncs <= DEEP_CONTACT_MAX_S) {
        rtcNextContactS = (uint16_t)ncs;
    }
    bool hasRev = !doc["usage_rev"].isNull();
    uint32_t rev = (uint32_t)(doc["usage_rev"] | 0L);
    bool usageChanged = true;
    if (hasRev && rtcUsageRev) usageChanged = rev != rtcUsageRev;
    if (hasRev) rtcUsageRev = rev;
    JsonObject pending = doc["pending"].as<JsonObject>();
    bool pendingOta = pending["ota"] | false;
    int pendingTpl = (int)pending["templates"].as<JsonArray>().size();
    nvsStageMark(42);

    if (!frame || usageChanged || firstPull || activeTemplateChanged() ||
        !clkPixelsValid || tzChanged || rtcClkPartials >= CLK_GHOST_LIMIT ||
        leavingDeep) {
        nvsStageMark(43);
        // setup() already ran epdBegin() on this boot (frame allocated); only
        // retry on OOM. A second full init used to re-enter
        // DEV_Module_Init/SPI.beginTransaction and deadlock on the Arduino SPI
        // paramLock (taken once, never released) -- the 0.14.x battery hang.
        if (!frame) epdBegin(false);
        nvsStageMark(44);
        // The clock budget path and the deep exit request a clean full
        // waveform even when the pixels are unchanged (design §8.4).
        if (rtcClkPartials >= CLK_GHOST_LIMIT || !epdBaselineTrusted || leavingDeep) {
            forceCleanRefresh = true;
        }
        renderActiveUsage(body, "PULL");
        // The Zzz-removing clean baseline is done: startNormalMode must not
        // request a second full refresh for the same wake frame (task-10).
        if (leavingDeep) wakeBaselineDrawn = true;
        nvsStageMark(45);
        clkCaptureFromFramebuffer();
        rememberActiveTemplate();
        rtcClkPartials = 0;
        // Only persist when the screen content really changed: a rolling
        // `resetsAt` on a 0%-used window must not wear the NVS.
        usageCacheSave(body);
        nvsStageMark(46);
        captureFrameToFs("pull");
    }
    rtcNextNetAt = timeKnown() ? (uint32_t)time(nullptr) + rtcNextContactS : 0;
    nvsStageMark(47);
    DevLog.printf("[deep] mode=%s next=%us rev=%u changed=%d pending=%d/%d batt=%u%% t=%lld\n",
                  mode, (unsigned)rtcNextContactS, (unsigned)rev, usageChanged ? 1 : 0,
                  pendingOta ? 1 : 0, pendingTpl, (unsigned)batteryPercent(),
                  (long long)(before ? (long long)(time(nullptr) - before) : 0));

    if (toLight) {
        setStage(14);
        histAdd(HIST_TO_LIGHT, 0);
        return 1;
    }
    if (pendingOta || pendingTpl > 0) {
        setStage(15);
        return 2;
    }
    return 0;
}

// Light -> deep transition: capture the clock window, notify the bridge, mark
// the mode and sleep. Never returns (called from loop()/setup()).
static void enterDeep(const char *reason) {
    // Every deep-sleep path must honor the PC USB keep-awake policy.
    if (plugged && !rtcDeepOnUsb) return;
    setStage(19);
    if (clkR.valid && lastDisplayedFrame) clkCaptureFromFramebuffer();
    if (v2BundleReady && rv2Enabled) rtcNextContactS = V2_RENDEZVOUS_S;
    else if (!rtcNextContactS) rtcNextContactS = DEEP_CONTACT_DEFAULT_S;
    if (WiFi.status() == WL_CONNECTED && storeCount() > 0) {
        EndpointRec rec;
        if (pickEndpoint(rec)) {
            String body = String("{\"next_contact_s\":") + rtcNextContactS +
                          ",\"usage_rev\":" + rtcUsageRev + "}";
            int code = 0;
            String out, err;
            usageHttpPost(rec, "/deep", body, code, out, err, 2000);
            DevLog.printf("[deep] notify %s:%u /deep -> %d\n",
                          rec.host.c_str(), rec.port, code);
        }
    }
    setStage(21);
    rtcMode = MODE_DEEP;
    rememberActiveTemplate();
    persistMode();
    rtcNextNetAt = timeKnown() ? (uint32_t)time(nullptr) + rtcNextContactS : 0;
    setStage(22);
    // Draw the sleep-state glyph (device.mode -> deep) before sleeping so the
    // screen shows the moon/Zzz immediately instead of at the next contact.
    // Unconditional: the cached flag was suspected of skipping this render.
    rtcDeepGlyph = activeTplHasMode ? 1 : 2;
    // Force a full refresh for the sleep glyph. Partial refreshes compare
    // against the firmware's `lastDisplayedFrame`, which can drift from the
    // panel's physical image (the thin-wake panel power pulse resets the
    // controller), so the glyph could be left half-drawn or missing.
    epdPartialReady = false;
    renderCurrent();
    rtcDeepGlyph |= 4;
    captureFrameToFs("enter-deep");
    setStage(20);
    histAdd(HIST_ENTER_DEEP, rtcNextContactS);
    DevLog.printf("[deep] enter (%s) next_net=%us rev=%u clk=%d\n", reason,
                  (unsigned)rtcNextContactS, (unsigned)rtcUsageRev, clkR.valid ? 1 : 0);
    sleepToNextEvent();
}

// Sleep to the next minute tick (clock reserved) or the next network contact;
// records the wall-clock/RTC anchors used to reconstruct time on wake.
static void sleepToNextEvent() {
    uint32_t secs = 60;
    if (clkR.valid) {
        secs = secsToNextMinute();
    } else if (rtcNextNetAt && timeKnown() &&
               (time_t)rtcNextNetAt > time(nullptr)) {
        long d = (long)((time_t)rtcNextNetAt - time(nullptr));
        secs = (uint32_t)(d > 3600 ? 3600 : (d < 2 ? 2 : d));
    }
    if (secs < 2) secs = 2;
    deepSleepFor(secs);   // deepSleepRaw records the sleep-time anchors
}

// Thin deep wake: panel + clock window only, no Wi-Fi/NVS/template work.
static bool deepThinWake() {
    const uint64_t t0 = esp_timer_get_time();
    setStage(2);
    epdThinBegin();
#if defined(CODEX_TARGET_NOTE4)
    note4RestoreFrameBaseline(true);
#endif
    setStage(30);
    bool drew = clockTickWake();
    rtcDeepCycles++;
    histAdd(HIST_THIN, drew ? 1 : 0);
    setStage(3);
    DevLog.printf("[deep] thin wake drew=%d total=%uus\n", drew ? 1 : 0,
                  (unsigned)(esp_timer_get_time() - t0));
    return drew;
}

// ---------------- deep-pull test rig (CODEX_DEEPPULL_TEST only) ----------------
// Measures: timer deep-sleep wake -> saved-BSSID fast connect -> GET /usage from
// the bridge -> light sleep. 5 cycles of RTC-measured timings are kept in RTC RAM
// (deep sleep wipes the DevLog ring), then the device stays online in light sleep
// so the original ROM can be OTA'd back. Zero impact on release builds.
#ifdef CODEX_DEEPPULL_TEST
#define DP_TEST_CYCLES     5
#define DP_TEST_DEEP_S     30
#define DP_TEST_CONNECT_MS 6000

RTC_DATA_ATTR static uint32_t dpDone = 0;
RTC_DATA_ATTR static uint8_t  dpChannel = 0;
RTC_DATA_ATTR static char     dpBssid[20] = {0};
RTC_DATA_ATTR static uint8_t  dpSlot = 0xFF;
// Per cycle: boot_ms, wifi_ms, http_ms, parse_ms, total_ms, code.
RTC_DATA_ATTR static uint32_t dpLog[DP_TEST_CYCLES][6] = {};
// Last cycle's full PM stats text (DevLog.printf truncates at 200 bytes, so it
// is kept in RTC and dumped in chunks while the device is online).
RTC_DATA_ATTR static char dpPm[768] = {0};
static uint64_t dpBootUs = 0;

static bool dpParseBssid(const char *s, uint8_t out[6]) {
    unsigned v[6];
    if (sscanf(s, "%x:%x:%x:%x:%x:%x", &v[0], &v[1], &v[2], &v[3], &v[4], &v[5]) != 6) {
        return false;
    }
    for (int i = 0; i < 6; i++) out[i] = (uint8_t)v[i];
    return true;
}

static void dpSaveAp() {
    if (WiFi.status() != WL_CONNECTED) return;
    dpChannel = (uint8_t)WiFi.channel();
    strncpy(dpBssid, WiFi.BSSIDstr().c_str(), sizeof(dpBssid) - 1);
    dpBssid[sizeof(dpBssid) - 1] = '\0';
    prefs.begin("wifi", true);
    dpSlot = prefs.getUChar("last", 0xFF);
    prefs.end();
    DevLog.printf("[dp] saved ap ch=%u bssid=%s slot=%u\n",
                  (unsigned)dpChannel, dpBssid, (unsigned)dpSlot);
}

static bool dpFastConnect(uint32_t &wallMs, bool &fast) {
    uint8_t slot = dpSlot;
    prefs.begin("wifi", true);
    if (slot >= MAX_SLOTS) slot = prefs.getUChar("last", 0);
    wifiSsid = prefs.getString(("s" + String(slot)).c_str(), "");
    wifiPass = prefs.getString(("p" + String(slot)).c_str(), "");
    prefs.end();
    if (!wifiSsid.length()) return false;
    WiFi.mode(WIFI_STA);
    WiFi.setHostname(hostname.c_str());
    uint8_t bssid[6];
    fast = dpChannel > 0 && dpParseBssid(dpBssid, bssid);
    const uint32_t t0 = (uint32_t)(esp_timer_get_time() / 1000);
    if (fast) WiFi.begin(wifiSsid.c_str(), wifiPass.c_str(), dpChannel, bssid);
    else      WiFi.begin(wifiSsid.c_str(), wifiPass.c_str());
    while (WiFi.status() != WL_CONNECTED &&
           (uint32_t)(esp_timer_get_time() / 1000) - t0 < DP_TEST_CONNECT_MS) {
        delay(50);
    }
    wallMs = (uint32_t)(esp_timer_get_time() / 1000) - t0;
    return WiFi.status() == WL_CONNECTED;
}

static void dpCycle() {
    configurePowerManagement();
    setupOtaPmLock();
    const uint64_t startUs = esp_timer_get_time();
    uint32_t wifiMs = 0, httpMs = 0, parseMs = 0;
    bool fast = false;
    const bool up = dpFastConnect(wifiMs, fast);
    int code = -1;
    if (up) {
        EndpointRec rec;
        bool have = false;
        for (int i = 0; i < storeCount(); i++) {
            if (storeGet(i, rec) && rec.host.length() && rec.port) { have = true; break; }
        }
        if (have) {
            HTTPClient http;
            http.setConnectTimeout(2000);
            http.setTimeout(4000);
            http.begin("http://" + rec.host + ":" + String(rec.port) + "/usage");
            http.addHeader("Authorization", "Bearer " + rec.token);
            const uint32_t h0 = (uint32_t)(esp_timer_get_time() / 1000);
            code = http.GET();
            String body;
            if (code == 200) body = http.getString();
            httpMs = (uint32_t)(esp_timer_get_time() / 1000) - h0;
            if (body.length()) {
                const uint32_t fnv = fnv1a(body);
                JsonDocument doc;
                const uint32_t p0 = (uint32_t)(esp_timer_get_time() / 1000);
                deserializeJson(doc, body);
                parseMs = (uint32_t)(esp_timer_get_time() / 1000) - p0;
                DevLog.printf("[dp] pull host=%s:%u code=%d len=%u fnv=%08x\n",
                              rec.host.c_str(), (unsigned)rec.port, code,
                              (unsigned)body.length(), (unsigned)fnv);
            } else {
                DevLog.printf("[dp] pull host=%s:%u code=%d len=0\n",
                              rec.host.c_str(), (unsigned)rec.port, code);
            }
            http.end();
        } else {
            DevLog.println("[dp] no bridge endpoint stored");
        }
    }
    const uint32_t totalMs = (uint32_t)((esp_timer_get_time() - startUs) / 1000);
    const uint32_t cycle = dpDone;
    if (cycle < DP_TEST_CYCLES) {
        dpLog[cycle][0] = (uint32_t)(dpBootUs / 1000);
        dpLog[cycle][1] = wifiMs;
        dpLog[cycle][2] = httpMs;
        dpLog[cycle][3] = parseMs;
        dpLog[cycle][4] = totalMs;
        dpLog[cycle][5] = (uint32_t)code;
    }
    dpDone = cycle + 1;
    DevLog.printf("[dp] cyc=%u/%u up=%d fast=%d boot_ms=%u wifi_ms=%u http_ms=%u parse_ms=%u total_ms=%u code=%d batt=%u%%\n",
                  (unsigned)(cycle + 1), (unsigned)DP_TEST_CYCLES, up ? 1 : 0, fast ? 1 : 0,
                  (unsigned)(dpBootUs / 1000), (unsigned)wifiMs, (unsigned)httpMs,
                  (unsigned)parseMs, (unsigned)totalMs, code, (unsigned)batteryPercent());
    const String pm = pmStatsText();
    strncpy(dpPm, pm.c_str(), sizeof(dpPm) - 1);
    dpPm[sizeof(dpPm) - 1] = '\0';
    DevLog.printf("[dp] pm text captured (%u bytes)\n", (unsigned)strlen(dpPm));
}

static void dpDump() {
    for (uint32_t i = 0; i < DP_TEST_CYCLES && i < dpDone; i++) {
        DevLog.printf("[dp] rec %u: boot_ms=%u wifi_ms=%u http_ms=%u parse_ms=%u total_ms=%u code=%u\n",
                      (unsigned)i, (unsigned)dpLog[i][0], (unsigned)dpLog[i][1],
                      (unsigned)dpLog[i][2], (unsigned)dpLog[i][3],
                      (unsigned)dpLog[i][4], (unsigned)dpLog[i][5]);
    }
    const size_t n = strlen(dpPm);
    if (n) {
        DevLog.print("[dp] pm dump begin\n");
        for (size_t off = 0; off < n; off += 180) {
            const size_t len = (n - off > 180) ? 180 : (n - off);
            char chunk[181];
            memcpy(chunk, dpPm + off, len);
            chunk[len] = '\0';
            DevLog.print(chunk);
        }
        DevLog.print("\n[dp] pm dump end\n");
    }
}

// Returns true when this boot was fully handled (armed/cycled/deep-slept).
static bool dpTestMain(esp_sleep_wakeup_cause_t cause) {
    if (plugged) { DevLog.println("[dp] USB plugged: skip test, normal mode"); return false; }
    if (!hasWifiSlots()) return false;
    if (dpDone >= DP_TEST_CYCLES) return false;
    if (cause == ESP_SLEEP_WAKEUP_EXT1) return false;   // button wake: normal
    if (cause == ESP_SLEEP_WAKEUP_TIMER) {
        dpCycle();
        if (dpDone >= DP_TEST_CYCLES) {
            DevLog.println("[dp] done; normal light-sleep mode (rollback OTA window)");
            dpDump();
            return false;   // setup() falls through to startNormalMode()
        }
        deepSleepFor(DP_TEST_DEEP_S);
        return true;        // unreachable
    }
    // Fresh boot after OTA: connect once to learn the AP channel/BSSID, then cycle.
    startNormalMode();
    if (wifiUp) {
        dpSaveAp();
        DevLog.printf("[dp] armed: %u cycles x %us deep sleep\n",
                      (unsigned)DP_TEST_CYCLES, (unsigned)DP_TEST_DEEP_S);
        deepSleepFor(DP_TEST_DEEP_S);
        return true;        // unreachable
    }
    return false;
}
#endif

void setup() {
#ifdef CODEX_DEEPPULL_TEST
    dpBootUs = esp_timer_get_time();
#endif
    esp_sleep_wakeup_cause_t cause = esp_sleep_get_wakeup_cause();
    bootWakeCause = cause;
    rtcLastWake = (uint8_t)cause;
    rtcStage = 1;
    if (cause != ESP_SLEEP_WAKEUP_TIMER) {
        rtcDeepOnUsb = 0;      // reboot/reset clears the test switches
        rtcFrameCapture = 0;
    }
    {
        Preferences p;
        p.begin("pm", true);
        nvsStageAtBoot = p.getUChar("stg", 0xFF);
        rv2Enabled = p.getUChar("rv2", 1) ? 1 : 0;
#if defined(CODEX_TARGET_NOTE4)
        note4KeepPanelPower = p.getUChar("panel_pwr", 1) != 0;
#endif
        bootPostOtaS = p.getUShort("post_ota_s", 0);
        String tz = p.getString("tz", "");
        if (tz.length() && tz.length() < (int)sizeof(deviceTz)) {
            strncpy(deviceTz, tz.c_str(), sizeof(deviceTz) - 1);
            deviceTz[sizeof(deviceTz) - 1] = '\0';
        }
        p.end();
    }
#if defined(CODEX_TARGET_NOTE4)
    EPD_SSD2683_SetKeepPower(note4KeepPanelPower);
#endif
    applyTimezone();
    nvsStageLast = 0xFF;
    nvsStageMark(1);      // wake reached setup (diagnostic sessions only)
    bool woke = (cause == ESP_SLEEP_WAKEUP_TIMER || cause == ESP_SLEEP_WAKEUP_EXT1);
    loadHostMac();
    hostname = "codex-status-" + macSuffix();
    releaseWakeHolds();   // clear deep-sleep GPIO holds from the previous cycle
    nvsStageMark(4);      // holds released without losing the VBAT latch
    plugged = usb_serial_jtag_is_connected();
    batteryPct = batteryPercent();
    if (rtcMagic == 0xC0DE0001) restoreTimeFromRtc();
    if (timeKnown()) timeSource = TIME_RTC;

    // A deep timer wake before the next contact first tries the retained clock
    // window. If the panel lacks a usable baseline, setup falls through to the
    // local full-render fallback below. Buttons and USB use the normal path.
    bool deepTimerBoot = (cause == ESP_SLEEP_WAKEUP_TIMER && rtcMode == MODE_DEEP &&
                          (!plugged || rtcDeepOnUsb));
    bool deepClockOnlyBoot = deepTimerBoot && !deepNetDue();
    nvsStageMark(5);      // wake classified (deepTimerBoot known)
    histAdd(HIST_BOOT, (uint16_t)cause);
    wakeResult = deepTimerBoot ? WAKE_NET : WAKE_LIGHT;
    if (deepClockOnlyBoot) {
        wakeResult = WAKE_THIN;
        if (deepThinWake()) sleepToNextEvent();
    }

    epdBegin(!woke);
#if defined(CODEX_TARGET_NOTE4)
    if (woke && rtcMagic == 0xC0DE0001) note4RestoreFrameBaseline(false);
#endif
    if (!woke) screen({"CODEX STATUS", FW_VERSION, "booting..."});
    if (rtcMagic != 0xC0DE0001) {
        rtcMagic = 0xC0DE0001;
        rtcActiveAt = 0;
        rtcActiveMac[0] = 0;
        rtcLastSyncEpoch = 0;
        rtcRetryStage = 0;
        rtcUsageHash = 0;
        rtcMode = MODE_LIGHT;
        rtcNextContactS = DEEP_CONTACT_DEFAULT_S;
        rtcNextNetAt = 0;
        rtcUsageRev = 0;
        rtcEpochAtSleep = 0;
        rtcClkUsAtSleep = 0;
        rtcApChannel = 0;
        rtcApBssid[0] = 0;
        rtcApSlot = 0xFF;
        rtcClkPartials = 0;
        rtcEpdBusyFails = 0;
        rtcTplActiveId[0] = 0;
        rtcTplHash[0] = 0;
        rtcClkContextId[0] = 0;
        rtcAccCycles = 0;
        rtcAccAwakeMs = 0;
        rtcAccBleMs = 0;
        rtcAccRenderMs = 0;
        clkR.valid = false;
        clkPixelsValid = false;
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
    // v2: the BOOT provisional window starts at the physical wake instant; a
    // timer wake never gets this fallback (docs/generic-display-platform-design-v2 §7).
    v2BootMs = v2NowMs();
    v2Provisional = (cause == ESP_SLEEP_WAKEUP_EXT1);
    if (cause == ESP_SLEEP_WAKEUP_EXT1) {
        // Manual wake (BOOT/PWR): leave deep and behave as light (docs §13.3).
        rtcMode = MODE_LIGHT;
        persistMode();
        rtcRetryStage = 0;   // button wake replays the retry cadence from the top
        histAdd(HIST_TO_LIGHT, 1);   // aux=1: button wake
        DevLog.printf("[pm] button wake: light mode, provisional %us\n",
                      (unsigned)V2_BOOT_PROVISIONAL_S);
    }
    if (plugged && !rtcDeepOnUsb) rtcMode = MODE_LIGHT;
    if (bootPostOtaS) {
        // Fresh firmware after an OTA: stay light for the minimum window so the
        // bridge can reach HTTP and install/refresh the formal light plan.
        rtcMode = MODE_LIGHT;
        persistMode();
        postOtaHoldUntilMs = millis() + (uint32_t)bootPostOtaS * 1000UL;
        DevLog.printf("[ota] post-OTA light window %us (bridge may extend)\n",
                      (unsigned)bootPostOtaS);
        Preferences p;
        p.begin("pm", false);
        p.remove("post_ota_s");
        p.end();
        bootPostOtaS = 0;
    }
    deepWakePath = deepTimerBoot;
    // Any wake from deep wakes up on top of the sleep glyph: connect silently
    // and replace it as soon as the wake render runs (docs/ble-rendezvous-power-design §3).
    wokeFromDeep = woke;

#ifdef CODEX_TARGET_UNVERIFIED
    targetUnverified = true;
    epdPartialReady = false;
    epdBaselineTrusted = false;
    DevLog.printf("[target] %s (%s) unverified: compile/host-tested only; "
                  "hardware facts missing (blocked_by_hardware_arrival)\n",
                  FW_TARGET_ID, RENDER_TARGET_ID);
#endif
    const esp_partition_t *running = esp_ota_get_running_partition();
    DevLog.printf("\n[codex-status] v%s mac=%s reset=%s slot=%s wake=%d(%s) usb=%d deepusb=%d mode=%s\n",
                  FW_VERSION, macText().c_str(), resetReasonName(),
                  running ? running->label : "?", (int)cause,
                  wakeCauseName(cause), plugged ? 1 : 0, (unsigned)rtcDeepOnUsb,
                  rtcMode == MODE_DEEP ? "deep" : "light");
    { Preferences p; p.begin("brg", false); p.end(); }
    ownerBegin();

    tplStoreBegin();
    tplXferBegin(FW_VERSION, []() { pendingTplChanged = true; });
    // v2 Bundle store: a committed A/B bundle takes over the active template
    // path; without one the legacy store continues to serve templates.
    v2CtxGen.seed(esp_random());
    v2BundleReady = bsBegin();
    if (v2BundleReady) {
        bsProfile(v2Profile);
        DevLog.printf("[v2] bundle job=%s active=%s templates=%u ctx=%s commit=%u\n",
                      v2Profile.jobId, v2Profile.ids[v2Profile.initial],
                      (unsigned)v2Profile.count, v2Profile.contextId,
                      (unsigned)bsCommitSeq());
        v2ActiveLoad();
        if (!deepClockOnlyBoot) {
            v2DataSeq.beginContext(v2NowMs(), 1);
            if (esp_reset_reason() == ESP_RST_DEEPSLEEP) {
                v2DataCheckpoint.restore(v2Profile.contextId, v2DataSeq);
            }
            if (!v2DataSeq.haveApplied()) {
                // Unknown retention (cold reset, corrupt RTC, or new context): rotate
                // context so an in-flight old snapshot cannot be mistaken for new.
                if (!v2SwitchActive(v2Profile.initial)) {
                    v2CtValid = false;
                    DevLog.println("[v2] cannot rotate unknown-retention context; data disabled");
                }
            }
        }
        v2Provisional = v2Provisional || (cause == ESP_SLEEP_WAKEUP_EXT1);
        if (!deepClockOnlyBoot) {
            v2SafetyDeadlineMs = v2NowMs() + (uint64_t)V2_MAX_LIGHT_S * 1000ULL;
        }
    } else {
        DevLog.println("[v2] no committed bundle; legacy template store active");
    }
    String cached;
    if (usageCacheLoad(cached)) lastUsage = cached;

    // Clock-only wakes never initialize data sequence state or a safety plan.
    // If the retained window failed, use the local Bundle and usage cache to
    // rebuild the full frame, then keep the existing rendezvous schedule.
    if (deepClockOnlyBoot) {
        bool clockTemplateReady = v2BundleReady
            ? v2CtValid && activeTplHasNow
            : tplCacheLoad() && activeTplHasNow;
        if (clockTemplateReady && lastUsage.length()) {
            renderActiveUsage(lastUsage, "RTC");
        }
        sleepToNextEvent();   // no network, owner claim, or data-sequence change
    }

#ifdef CODEX_DEEPPULL_TEST
    if (dpTestMain(cause)) return;
#endif
    if (hasWifiSlots()) {
        if (deepWakePath && v2BundleReady) {
            if (rv2Enabled && !v2Rendezvous()) {
                // Plan C: v2Rendezvous already rendered once after the radio
                // was closed (clock window or deferred data frame); the sleep
                // plan path only has to schedule the next rendezvous.
                rtcNextContactS = V2_RENDEZVOUS_S;
                rtcNextNetAt = timeKnown() ? (uint32_t)time(nullptr) + rtcNextContactS : 0;
                sleepToNextEvent();
            }
            if (!rv2Enabled) {
                // Explicit diagnostic rollback: bounded HTTP opportunity, never
                // legacy envelope application into a v2 context.
                v2SafetyDeadlineMs = v2NowMs() + 15000;
                rtcMode = MODE_LIGHT;
            }
            startNormalMode();
        } else if (deepWakePath) {
            // Network window: pull the envelope, then either go back to sleep
            // or switch to light / stay awake for a pending push.
            int r = deepNetworkCycle();
            if (r == 0) {
                nvsStageMark(51);
                sleepToNextEvent();   // never returns
            }
            if (r == 2) pendingWindowUntilMs = millis() + DEEP_PENDING_WINDOW_MS;
            startNormalMode(true);
            if (activeTplHasMode) renderCurrent();   // hide the sleep glyph
        } else {
            rtcStage = 99;
            startNormalMode();
        }
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
#if defined(CODEX_TARGET_NOTE4)
            const esp_partition_t *running = esp_ota_get_running_partition();
            DevLog.printf("[cli] slot=%s epd_writes=%u busy_fails=%u trusted=%d\n",
                          running ? running->label : "?", (unsigned)epdWriteCount,
                          (unsigned)rtcEpdBusyFails, epdBaselineTrusted ? 1 : 0);
            uint32_t black = 0;
            if (frame) for (int i = 0; i < EPD_FB_BYTES; ++i)
                black += __builtin_popcount((unsigned char)~frame[i]);
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
                if (!setNote4PanelPower(keep)) {
                    DevLog.println("[cli] panel power NVS write failed");
                } else {
                    if (epdAsleep) digitalWrite(EPD_PWR_PIN, keep ? HIGH : LOW);
                    DevLog.printf("[cli] panel_power=%s\n", keep ? "keep" : "off_cache");
                }
            } else {
                DevLog.printf("[cli] panel_power=%s\n",
                              note4KeepPanelPower ? "keep" : "off_cache");
            }
#endif
        } else if (line == "batt") {
            DevLog.printf("[cli] battery=%d%% (%u mV)\n", batteryPercent(),
                          (unsigned)batteryMilliVolts());
        } else if (line == "pair") {
            enterBleOn(true);
            DevLog.println("[cli] BLE session on, pairing window 120s");
        } else if (line == "deep") {
            DevLog.println("[cli] deep sleep now");
            enterDeep("cli");
        } else if (line == "light") {
            rtcMode = MODE_LIGHT;
            forceDeepAtMs = 0;
            persistMode();
            noteActivity("cli");
            DevLog.println("[cli] light mode");
        } else if (line == "deepusb on") {
            rtcDeepOnUsb = 1;
            DevLog.println("[cli] deep_usb=1 (deep sleep allowed while plugged)");
        } else if (line == "deepusb off") {
            rtcDeepOnUsb = 0;
            DevLog.println("[cli] deep_usb=0");
        } else if (line == "framecap on") {
            rtcFrameCapture = 1;
            DevLog.println("[cli] frame_capture=1 (pre-sleep frame -> /frames/last.pbm)");
        } else if (line == "framecap off") {
            rtcFrameCapture = 0;
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
            DevLog.println("[cli] commands: wifi <ssid> <pass> | status | batt | pair | deep | light | deepusb on|off | framecap on|off | pmstats | blescan [1..30] | timers | diag");
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
    // OTA stall watchdog: no upload chunk for 20 s -> abort and restore the UI
    // (the WebServer also reports UPLOAD_FILE_ABORTED; this is the backstop).
    if (otaInProgress && otaLastDataMs && (int32_t)(millis() - otaLastDataMs) > 20000) {
        otaUploadCleanup("stalled");
    }
    handleSerialCli();
    server.handleClient();
    if (targetUnverified) {
        // Unverified panel combination: stay recoverable over USB serial and
        // never drive data, templates, radios or the panel runtime.
        static uint32_t lastNote = 0;
        if (millis() - lastNote > 10000UL) {
            lastNote = millis();
            DevLog.printf("[target] %s unverified; normal operation refused\n", FW_TARGET_ID);
        }
        delay(50);
        return;
    }
    blePoll();
    serviceV2Ble();

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
            nextTemplate();     // 2 s: next local template (no BLE session)
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
            String bridgeId = parsed["bridge"]["hostId"] | "";
            if (!ownerAllows(bridgeId)) {
                DevLog.printf("[owner] BLE usage ignored (occupied, id=%s)\n", bridgeId.c_str());
            } else {
                adoptServerTime(parsed);
                applyBridgeModeHint(parsed);
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
                    noteActivity("ble-usage");
                    renderActiveUsage(lastUsage, lastChannel.c_str());
                } else {
                    DevLog.printf("[usage] BLE usage ignored (active=%s)\n", rtcActiveMac);
                }
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

    // v2: only a new formal PowerPlan moves the light deadline; reads, data,
    // claims and status polls never do. The BOOT provisional 300 s closes the
    // radio when the Bridge stays unreachable (v2 §7/§12).
    if (v2BundleReady && rtcMode == MODE_LIGHT && (!plugged || rtcDeepOnUsb)) {
        if (v2Plan.accepted()) {
            if (!v2Plan.lightActive(v2NowMs())) {
                DevLog.printf("[v2] formal light window ended (%s)\n", v2PlanReason.c_str());
                enterDeep("v2 plan");
            }
        } else if (v2Provisional &&
                   V2PlanState::bootProvisionalRemaining(v2BootMs, v2NowMs()) == 0) {
            DevLog.println("[v2] boot provisional 300s expired without a formal plan");
            enterDeep("v2 provisional");
        } else if (!v2Provisional && v2SafetyDeadlineMs &&
                   v2NowMs() >= v2SafetyDeadlineMs) {
            DevLog.println("[v2] no formal plan within the max light lease; sleeping");
            enterDeep("v2 safety");
        }
    }

    // v0.14 mode transitions: a pending-window expiry or the local idle timer
    // (plus a bridge `mode:"deep"` hint) sends the device back to deep sleep.
    // With a v2 bundle the Bridge-owned plan is authoritative.
    if (!v2BundleReady) {
        if (pendingWindowUntilMs && (int32_t)(millis() - pendingWindowUntilMs) >= 0) {
            pendingWindowUntilMs = 0;
            enterDeep("pending window");
        }
        if (idleDeepDue()) {
            enterDeep(forceDeepAtMs ? "bridge deep" : "idle");
        }
    }

    // 1 Hz housekeeping tick: read the clock once per second and share it
    // between the offline-minutes row and the `device.now` clock bind. Both
    // used to call timeKnown()/time() every loop; 0.13.7 only gated the clock
    // block, and the remaining per-loop clock read still cut into light sleep.
    //
    // Offline-minutes row: once contact is lost (>= BRIDGE_LOST_MIN minutes
    // since the last sync) the template shows `OFF <n>M`, so redraw when the
    // integer minute changes. No periodic refresh while the bridge is
    // heartbeating.
    static int lastOfflineMinute = -1;
    static long lastClockMinute = -1;
    static uint32_t lastClockCheckMs = 0;
    uint32_t clockMs = millis();
    if ((activeTplHasNow || rtcLastSyncEpoch > 1600000000) &&
        (lastClockCheckMs == 0 || (uint32_t)(clockMs - lastClockCheckMs) >= 1000)) {
        lastClockCheckMs = clockMs;
        if (timeKnown()) {
            time_t nowSec = time(nullptr);
            if (rtcLastSyncEpoch > 1600000000) {
                long mins = ((long)nowSec - (long)rtcLastSyncEpoch) / 60;
                int offlineMinute = (mins > BRIDGE_LOST_MIN) ? (int)mins : -1;
                if (offlineMinute != lastOfflineMinute) {
                    bool wasShown = lastOfflineMinute > 0;
                    lastOfflineMinute = offlineMinute;
                    if (offlineMinute > 0 || wasShown) renderCurrent();
                }
            }
            // Clock bind: while the active template shows `device.now`, redraw
            // when the local minute changes (partial refresh, ~1/min; the
            // bridge only pushes every 5 min, so server_time looks frozen).
            if (activeTplHasNow) {
                long minute = (long)nowSec / 60;
                if (lastClockMinute < 0) {
                    lastClockMinute = minute;
                    if (clkR.valid) clkCaptureFromFramebuffer();
                } else if (minute != lastClockMinute) {
                    lastClockMinute = minute;
#ifdef CODEX_CLK_WINDOW_TEST
                    clkTestTick();
#else
                    // Direct window write (v0.14): ~796 ms vs ~864 ms for a full
                    // re-render; a full re-render every 30 partials clears
                    // ghosting, and the clean request bypasses the identical-
                    // frame early return (design §8.4).
                    if (epdPartialCount >= 30) {
                        forceCleanRefresh = true;
                        renderCurrent();
                    } else if (!clockTickWake()) {
                        renderCurrent();
                    }
#endif
                }
            }
        }
    }

    delay(loopDelayForNow());
}
