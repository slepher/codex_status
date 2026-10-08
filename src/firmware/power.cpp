#include "power_internal.h"
#include "clock_internal.h"
#include "diagnostics_internal.h"
#include "display_internal.h"
#include "platform_internal.h"
#include "radio_internal.h"
#include "transport_internal.h"

namespace firmware::power {

State state;
RTC_DATA_ATTR Retained retained;

static esp_pm_lock_handle_t otaPmLock = nullptr;

static esp_pm_lock_handle_t blePmLock = nullptr;

static bool blePmHeld = false;

static bool        otaLockHeld = false;

static uint32_t activeHoldSec = ACTIVE_HOLD_S;

static uint32_t lastActivityMs = 0;

static uint32_t pendingWindowUntilMs = 0;

static uint32_t lastBootAction = 0;

static uint32_t ledPulseUntil = 0;

static bool     ledState = false;         // logical LED: BLE ON

#if defined(CODEX_PM) && CONFIG_PM_LIGHT_SLEEP_CALLBACKS
static uint32_t sleepBucket(int64_t us);
#endif
#if defined(CODEX_PM) && CONFIG_PM_LIGHT_SLEEP_CALLBACKS
static esp_err_t sleepEnterCb(int64_t sleep_time_us, void *);
#endif
#if defined(CODEX_PM) && CONFIG_PM_LIGHT_SLEEP_CALLBACKS
static esp_err_t sleepExitCb(int64_t slept_us, void *);
#endif
static void armWakeSources(uint64_t timerUs);
static void holdPinsForDeepSleep();
static void deepSleepRaw(uint32_t sec);
static void ledApply();
static bool idleDeepDue();
static void rememberActiveTemplate();

#if defined(CODEX_PM) && CONFIG_PM_LIGHT_SLEEP_CALLBACKS
static uint32_t sleepBucket(int64_t us) {
    static const int64_t edges[11] = {1000, 2000, 4000, 6000, 8000, 10000,
                                      20000, 50000, 100000, 500000, 1000000};
    for (uint32_t i = 0; i < 11; i++) {
        if (us < edges[i]) return i;
    }
    return 11;
}
#endif

#if defined(CODEX_PM) && CONFIG_PM_LIGHT_SLEEP_CALLBACKS
static esp_err_t sleepEnterCb(int64_t sleep_time_us, void *) {
    int64_t gap = esp_timer_get_next_alarm_for_wake_up() - esp_timer_get_time();
    state.sleepDiag.last_expected_us = (uint32_t)sleep_time_us;
    state.sleepDiag.last_next_alarm_us = gap < 0 ? 0 : (uint32_t)gap;
    return ESP_OK;
}
#endif

#if defined(CODEX_PM) && CONFIG_PM_LIGHT_SLEEP_CALLBACKS
static esp_err_t sleepExitCb(int64_t slept_us, void *) {
    state.sleepDiag.count++;
    state.sleepDiag.total_us += (uint64_t)slept_us;
    if (state.sleepDiag.count == 1 || slept_us < (int64_t)state.sleepDiag.min_us) {
        state.sleepDiag.min_us = (uint32_t)slept_us;
    }
    if ((uint64_t)slept_us > state.sleepDiag.max_us) state.sleepDiag.max_us = (uint32_t)slept_us;
    state.sleepDiag.last_us = (uint32_t)slept_us;
    state.sleepDiag.hist[sleepBucket(slept_us)]++;
    switch (esp_sleep_get_wakeup_cause()) {
        case ESP_SLEEP_WAKEUP_TIMER: state.sleepDiag.causes[0]++; break;
        case ESP_SLEEP_WAKEUP_WIFI:  state.sleepDiag.causes[1]++; break;
        case ESP_SLEEP_WAKEUP_GPIO:  state.sleepDiag.causes[2]++; break;
        default:                     state.sleepDiag.causes[3]++; break;
    }
    return ESP_OK;
}
#endif

uint32_t loopDelayForNow() {
    if (transport::state.server.client().connected()) return 5;
    return state.loopDelayMs;
}

// BLE and HTTP overlap only while the CPU is awake. This lock is independent
// of OTA and physical recovery, and remains held for every actual BLE session.
bool setBleAwake(bool held) {
#ifdef CODEX_PM
    if (!blePmLock && held && esp_pm_lock_create(ESP_PM_NO_LIGHT_SLEEP, 0, "ble-rv", &blePmLock) != ESP_OK)
        return false;
    if (held != blePmHeld && blePmLock) {
        esp_err_t err = held ? esp_pm_lock_acquire(blePmLock) : esp_pm_lock_release(blePmLock);
        if (err != ESP_OK) return false;
        blePmHeld = held;
    }
#endif
    return true;
}

void configurePowerManagement() {
#ifdef CODEX_PM
    esp_pm_config_t cfg = {};
    cfg.max_freq_mhz = 240;
    // Plan C: 80 MHz floor keeps the BLE controller stable during a rendezvous
    // window; DFS drops below it automatically while waiting (flash stays 40 MHz,
    // that is a separate setting).
    cfg.min_freq_mhz = 80;
    cfg.light_sleep_enable = true;
    esp_err_t err = esp_pm_configure(&cfg);
    state.pmLightSleep = (err == ESP_OK);
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

void setupOtaPmLock() {
#ifdef CODEX_PM
    if (!otaPmLock) {
        esp_pm_lock_create(ESP_PM_NO_LIGHT_SLEEP, 0, "ota", &otaPmLock);
    }
#endif
}

void setOtaLock(bool held) {
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
void setOtaWifiAwake(bool awake) {
    esp_err_t err = esp_wifi_set_ps(awake ? WIFI_PS_NONE : WIFI_PS_MAX_MODEM);
    DevLog.printf("[ota] wifi ps=%s (%s)\n", awake ? "NONE" : "MAX_MODEM",
                  esp_err_to_name(err));
}

// CONFIG_PM_SLP_DISABLE_GPIO floats every pad during automatic light sleep.
// The VBAT latch (GPIO17 high), the panel power enable (GPIO6 low = panel
// powered) and the audio amp power (GPIO42 low = off) must keep their levels,
// so they opt out of the sleep switch.
void retainSleepCriticalGpio() {
    gpio_sleep_sel_dis(GPIO_NUM_17);
    gpio_sleep_sel_dis(GPIO_NUM_6);
    gpio_sleep_sel_dis(GPIO_NUM_42);
    DevLog.println("[pm] GPIO sleep retention: 17/6/42");
}

uint32_t v2ManualBleHoldRemainingS() {
    return state.bleRecovery.remaining(clock::v2NowMs());
}

bool v2ManualBleHoldActive() { return state.bleRecovery.active(clock::v2NowMs()); }

uint32_t batteryMilliVolts() {
    uint32_t mv = 0;
    for (int i = 0; i < 8; i++) mv += analogReadMilliVolts(4);
    return (mv / 8) * 2;   // board divider: VBAT = VADC x 2
}

int batteryPercent() {
    uint32_t vbat = batteryMilliVolts();
    if (vbat >= 4200) return 100;
    if (vbat <= 3300) return 0;
    return (int)((vbat - 3300) * 100 / 900);
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
void releaseWakeHolds() {
    pinMode(GPIO_NUM_17, OUTPUT);
    digitalWrite(GPIO_NUM_17, HIGH);
    pinMode(GPIO_NUM_6, OUTPUT);
#if defined(CODEX_TARGET_NOTE4)
    digitalWrite(GPIO_NUM_6, display::state.note4KeepPanelPower ? HIGH : LOW);
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
    clock::flushNtpRtcSync();  // do not lose a successful SNTP correction at sleep/reset
    if (sec < 2) sec = 2;
    if (sec > 3600) sec = 3600;
    // Plan C telemetry: snapshot this wake's totals into RTC (RAM accounting
    // dies with the boot) for /status.json `deep.last_*` and /history.
    diagnostics::retained.rtcLastAwakeMs = millis();
    diagnostics::retained.rtcLastBleMs = diagnostics::bleRadioMs();
    diagnostics::retained.rtcLastRenders = diagnostics::state.bootRenderCount;
    diagnostics::retained.rtcLastTimeSource = clock::state.timeSource;
    diagnostics::retained.rtcLastWakeResult = diagnostics::state.wakeResult;
    diagnostics::traceFinishSleep();
    // Power estimate (no current meter): accumulate only deep cycles so the
    // per-cycle averages are not dominated by long light sessions.
    if (diagnostics::state.wakeResult != WAKE_LIGHT) {
        diagnostics::retained.rtcAccCycles++;
        diagnostics::retained.rtcAccAwakeMs += diagnostics::retained.rtcLastAwakeMs;
        diagnostics::retained.rtcAccBleMs += diagnostics::retained.rtcLastBleMs;
        diagnostics::retained.rtcAccRenderMs += diagnostics::state.wakeRenderMs;
    }
    struct timeval wallAtSleep;
    clock::retained.rtcEpochUsAtSleep = clock::timeKnown() && gettimeofday(&wallAtSleep, nullptr) == 0
        ? (uint64_t)wallAtSleep.tv_sec * 1000000ULL + (uint64_t)wallAtSleep.tv_usec : 0;
    clock::retained.rtcClkUsAtSleep = esp_rtc_get_time_us();
    armWakeSources((uint64_t)sec * 1000000ULL);
    diagnostics::setStage(90);
    DevLog.printf("[pm] deep sleep %us (raw)\n", (unsigned)sec);
    holdPinsForDeepSleep();
    esp_deep_sleep_start();
}

// Full transition cleanup (light -> deep and the network-window paths): put
// the panel to sleep, stop BLE and disconnect Wi-Fi if it was ever up.
void deepSleepFor(uint32_t sec) {
#if defined(CODEX_TARGET_NOTE4)
    display::note4SaveFrameBaseline();
#endif
    display::epdPanelSleep();
    diagnostics::nvsStageMark(48);
    if (bleInitialized()) {
        diagnostics::bleRadioMark(false);
        bleAdvertiseStop();
        bleDeinit();
    }
    diagnostics::nvsStageMark(49);
    if (WiFi.getMode() != WIFI_MODE_NULL) WiFi.disconnect(true);
    diagnostics::nvsStageMark(50);
    deepSleepRaw(sec);
}

// GP3 green LED: active low, lit while the BLE session is on.
static void ledApply() {
    bool on = ledState;
    if (ledPulseUntil && (int32_t)(millis() - ledPulseUntil) < 0) on = !on;
    digitalWrite(3, on ? LOW : HIGH);
}

void ledSet(bool on) {
    ledState = on;
    ledApply();
}

void ledFlash() {
    ledPulseUntil = millis() + 80;
    ledApply();
}

void serviceLed() {
    if (ledPulseUntil && (int32_t)(millis() - ledPulseUntil) >= 0) {
        ledPulseUntil = 0;
        ledApply();
    }
}

void pollPlug() {
    static uint32_t lastPoll = 0;
    static uint32_t unplugStarted = 0;
    if (millis() - lastPoll < 500) return;
    lastPoll = millis();
    bool now = usb_serial_jtag_is_connected();
    if (now) unplugStarted = 0;
    else if (state.plugged) {
        if (!unplugStarted) unplugStarted = millis();
        if (millis() - unplugStarted < 10000) return;
    }
    if (now == state.plugged) return;
    state.plugged = now;
    DevLog.printf("[pm] usb %s\n", state.plugged ? "plugged" : "unplugged");
    ledFlash();
    display::renderCurrent();
    if (state.plugged) {
        radio::state.bleUserOff = false;
        state.batteryPct = batteryPercent();
        if (radio::state.wifiUp && !radio::state.bleOn && !retained.rtcDeepOnUsb && state.batteryPct > BLE_AUTO_PCT) radio::enterBleOn(false);
    }
}

void checkBattery() {
    if (millis() - state.lastBattCheck < BATT_CHECK_MS) return;
    state.lastBattCheck = millis();
    state.batteryPct = batteryPercent();
    DevLog.printf("[pm] battery %d%% (%u mV) plugged=%d\n",
                  state.batteryPct, (unsigned)batteryMilliVolts(), state.plugged ? 1 : 0);
    if (v2BatteryPowerOff(state.plugged, state.batteryPct)) {
        DevLog.println("[pm] battery <5%: power off");
        powerOff();
    }
    if (state.plugged && !retained.rtcDeepOnUsb && radio::state.wifiUp && !radio::state.bleOn && !radio::state.bleUserOff && state.batteryPct > BLE_AUTO_PCT) radio::enterBleOn(false);
}

void handleBootClick() {
    if (millis() - lastBootAction < 1000) return;   // 1 s throttle
    lastBootAction = millis();
    if (v2ManualBleHoldActive()) {
        radio::state.bleUserOff = true;
        radio::bleOff("user click");
    } else {
        radio::enterBleOn(true);
    }
}

// Deep sleep uses minute clock wakes and a bounded BLE rendezvous on contact
// windows. A formal PowerPlan controls the light deadline.
void persistMode() {
    Preferences p;
    p.begin("pm", false);
    p.putUChar("mode", retained.rtcMode);
    p.putUShort("next", retained.rtcNextContactS);
    p.putUInt("next_at", retained.rtcNextNetAt);
    p.end();
}

void noteActivity(const char *reason) {
    state.lastActivityEpoch = clock::timeKnown() ? time(nullptr) : 0;
    lastActivityMs = millis();
    state.forceDeepAtMs = 0;
    (void)reason;
}

// Local idle fallback: quiet for `idleDeepS` (default 10 min) on battery with
// no BLE/OTA session -> deep. A bridge `mode:"deep"` hint shortens the last
// stretch to 60 s of grace (see the push handlers).
static bool idleDeepDue() {
    if ((state.plugged && !retained.rtcDeepOnUsb) || radio::state.bleOn || transport::state.otaInProgress || radio::state.configMode) return false;
    if (!radio::state.wifiUp || !clock::timeKnown()) return false;
    // Post-OTA minimum window: never descend before it elapses; the bridge can
    // still extend the online time with a formal light PowerPlan.
    if (state.postOtaHoldUntilMs && (int32_t)(millis() - state.postOtaHoldUntilMs) < 0) return false;
    if (state.forceDeepAtMs && (int32_t)(millis() - state.forceDeepAtMs) >= 0) return true;
    time_t since = state.lastActivityEpoch ? state.lastActivityEpoch : (time_t)radio::retained.rtcLastSyncEpoch;
    if (!since) return false;
    return (long)(time(nullptr) - since) >= (long)state.idleDeepS;
}

static void rememberActiveTemplate() {
    String id = tplStoreActive();
    TplMeta meta;
    if (!id.length() || !tplStoreFind(id, meta)) return;
    strncpy(platform::retained.rtcTplActiveId, id.c_str(), sizeof(platform::retained.rtcTplActiveId) - 1);
    platform::retained.rtcTplActiveId[sizeof(platform::retained.rtcTplActiveId) - 1] = '\0';
    strncpy(platform::retained.rtcTplHash, meta.hash.c_str(), sizeof(platform::retained.rtcTplHash) - 1);
    platform::retained.rtcTplHash[sizeof(platform::retained.rtcTplHash) - 1] = '\0';
}

// When the next timer wake is due (no net contact planned -> immediate retry).
bool deepNetDue() {
    if (!retained.rtcNextNetAt) return true;
    if (!clock::timeKnown()) return true;
    return (time_t)time(nullptr) >= (time_t)retained.rtcNextNetAt;
}

// Light -> deep transition: capture the clock window, mark the mode and sleep.
void enterDeep(const char *reason) {
    // Every deep-sleep path must honor the PC USB keep-awake policy.
    if ((state.plugged && !retained.rtcDeepOnUsb) || v2ManualBleHoldActive()) return;
    diagnostics::setStage(19);
    if (display::retained.clkR.valid && display::state.lastDisplayedFrame) display::clkCaptureFromFramebuffer();
    retained.rtcNextContactS = V2_RENDEZVOUS_S;
    diagnostics::setStage(21);
    retained.rtcMode = MODE_DEEP;
    rememberActiveTemplate();
    persistMode();
    retained.rtcNextNetAt = clock::timeKnown() ? (uint32_t)time(nullptr) + retained.rtcNextContactS : 0;
    diagnostics::setStage(22);
    // Draw the sleep-state glyph (device.mode -> deep) before sleeping so the
    // screen shows the moon/Zzz immediately instead of at the next contact.
    // Unconditional: the cached flag was suspected of skipping this render.
    diagnostics::retained.rtcDeepGlyph = (diagnostics::retained.rtcDeepGlyph & 8) | (platform::state.activeTplHasMode ? 1 : 2);
    // Force a full refresh for the sleep glyph. Partial refreshes compare
    // against the firmware's `lastDisplayedFrame`, which can drift from the
    // panel's physical image (the thin-wake panel power pulse resets the
    // controller), so the glyph could be left half-drawn or missing.
    display::state.epdPartialReady = false;
    display::renderCurrent();
    diagnostics::retained.rtcDeepGlyph |= 4;
    diagnostics::captureFrameToFs("enter-deep");
    diagnostics::setStage(20);
    diagnostics::histAdd(HIST_ENTER_DEEP, retained.rtcNextContactS);
    DevLog.printf("[deep] enter (%s) next_net=%us rev=%u clk=%d\n", reason,
                  (unsigned)retained.rtcNextContactS, (unsigned)platform::retained.rtcUsageRev, display::retained.clkR.valid ? 1 : 0);
    sleepToNextEvent();
}

// Sleep to the next minute tick (clock reserved) or the next network contact;
// records the wall-clock/RTC anchors used to reconstruct time on wake.
void sleepToNextEvent() {
    uint32_t secs = 60;
    if (display::retained.clkR.valid) {
        secs = clock::secsToNextMinute();
    } else if (retained.rtcNextNetAt && clock::timeKnown() &&
               (time_t)retained.rtcNextNetAt > time(nullptr)) {
        long d = (long)((time_t)retained.rtcNextNetAt - time(nullptr));
        secs = (uint32_t)(d > 3600 ? 3600 : (d < 2 ? 2 : d));
    }
    if (secs < 2) secs = 2;
    deepSleepFor(secs);   // deepSleepRaw records the sleep-time anchors
}

// Thin deep wake: panel + clock window only, no Wi-Fi/NVS/template work.
bool deepThinWake() {
    const uint64_t t0 = esp_timer_get_time();
    diagnostics::setStage(2);
    display::epdThinBegin();
#if defined(CODEX_TARGET_NOTE4)
    display::note4RestoreFrameBaseline(true);
#endif
    diagnostics::setStage(30);
    bool drew = display::clockTickWake();
    diagnostics::retained.rtcDeepCycles++;
    diagnostics::histAdd(HIST_THIN, drew ? 1 : 0);
    diagnostics::setStage(3);
    DevLog.printf("[deep] thin wake drew=%d total=%uus\n", drew ? 1 : 0,
                  (unsigned)(esp_timer_get_time() - t0));
    return drew;
}

// PWR (GPIO18) held for 3 s: software power-off by dropping the VBAT latch
// (GPIO17). On battery the MCU dies here; if USB/charger power keeps the board
// alive the hold degrades to a clean restart so the device never sits inert.
void powerOff() {
    DevLog.println("[pm] PWR held 3s: power off");
    display::epdPanelSleep();
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

} // namespace firmware::power
