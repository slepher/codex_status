#pragma once

// Firmware integration only: state is owned here; never copy it as a snapshot.
#include "config.h"
#include "dependencies.h"
#include "plan_store.h"
#include "battery_display.h"

namespace firmware::power {

#if defined(CODEX_PM) && CONFIG_PM_LIGHT_SLEEP_CALLBACKS
struct SleepDiag {
    uint32_t count;
    uint32_t causes[4];          // timer / wifi / gpio / other
    uint64_t total_us;
    uint32_t min_us, max_us, last_us;
    uint32_t last_expected_us, last_next_alarm_us;
    uint32_t hist[12];
};
#endif

struct State {

    // Post-OTA minimum light window: set in NVS just before an OTA reboot (RTC
    // memory does not survive a software/OTA reset on this board), read and
    // cleared on the first post-OTA boot. The bridge extends it with a formal
    // light PowerPlan.
    uint16_t bootPostOtaS = 0;
    uint32_t postOtaHoldUntilMs = 0;
    bool     pmLightSleep = false;

    // v0.14 mode runtime. `lastActivity` drives the light -> deep idle transition;
    // `forceDeepAt` honors a bridge `mode:"deep"` hint with a short grace period so
    // a final push/template can land first.
    uint32_t idleDeepS = IDLE_DEEP_DEFAULT_S;
    time_t   lastActivityEpoch = 0;
    uint32_t forceDeepAtMs = 0;
    bool     deepWakePath = false;      // setup(): this boot is a deep pull
    bool     wokeFromDeep = false;      // setup(): deep wake; panel holds the sleep frame

    // Keep the GATT table stable across the single-protocol rollout so existing
    // Windows bonds can continue to use the control characteristic.

    // v0.12 runtime state (docs/power-state.md §3-§6)
    bool     plugged = false;          // PC USB host present (SOF)
    int      batteryPct = -1;
    uint32_t lastBattCheck = 0;
#if defined(CODEX_TARGET_NOTE4)
    BatteryDisplay batteryDisplay;
#endif
#if defined(CODEX_PM) && CONFIG_PM_LIGHT_SLEEP_CALLBACKS
    SleepDiag sleepDiag = {};
#endif

    // Arduino loop() cadence. Idle default is 25 ms: the sleep survey (task-2)
    // showed light-sleep fragmentation drops sharply up to ~25-50 ms (134 -> 45
    // wakeups/s) while HTTP latency stays acceptable. While a TCP client is
    // connected (HTTP request/response, OTA upload) the loop falls back to 5 ms so
    // transfer throughput and interactive latency are unaffected.
    // POST /diag?loop_delay=N retunes the idle value at runtime (token-gated).
    uint32_t loopDelayMs = 25;
    V2PlanState    v2Plan;
    PlanStore     planStore;
    uint64_t       v2BootMs = 0;              // physical wake instant
    bool           v2Provisional = false;     // BOOT 300 s window active
    uint64_t       v2LightDeadlineMs = 0;     // accepted plan deadline (monotonic)

    // Device safety cap: with a committed bundle but no formal PowerPlan (or before
    // the Bridge answers) the light session is bounded by the max light lease.
    uint64_t       v2SafetyDeadlineMs = 0;
    BleRecoveryWindow bleRecovery;
    uint64_t offlineBootGraceUntilMs = 0;
    String         v2PlanReason = "init";
};
extern State state;

struct Retained {
    uint32_t rtcMagic = 0;
    uint8_t  rtcRetryStage = 0;

    // v0.14 deep/light mode state. RTC survives deep sleep; NVS backs it across
    // OTA/software resets (docs/power-state.md §13.5).
    uint8_t  rtcMode = MODE_DEEP;
    uint16_t rtcNextContactS = DEEP_CONTACT_DEFAULT_S;
    uint32_t rtcNextNetAt = 0;     // epoch of the next pull

    // Diagnostic switch: allow deep sleep while USB is plugged (USB only supplies
    // power; the serial link drops during sleep and re-enumerates on wake). It
    // survives deep-sleep cycles (RTC domain) but is cleared on any non-timer boot
    // (power-on/OTA/button) and via /diag?deep_usb=0 or the `deepusb off` CLI, so
    // normal plugged behavior returns after a reboot.
    uint8_t  rtcDeepOnUsb = 0;
};
extern Retained retained;

uint32_t loopDelayForNow();
bool setBleAwake(bool held);
void configurePowerManagement();
void setupOtaPmLock();
void setOtaLock(bool held);
void setOtaWifiAwake(bool awake);
void retainSleepCriticalGpio();
uint32_t v2ManualBleHoldRemainingS();
bool v2ManualBleHoldActive();
uint32_t batteryMilliVolts();
int  batteryPercent();
int  displayBatteryPercent();
void serviceBatteryDisplay();
void releaseWakeHolds();
void deepSleepFor(uint32_t sec, uint64_t wakeEpochUs = 0);
void pollPlug();
void checkBattery();
void handleBootClick(uint32_t physicalMs = 0);
void persistMode();
void noteActivity(const char *reason);
bool deepNetDue();
void enterDeep(const char *reason);
void sleepToNextEvent();
bool deepThinWake();
void powerOff();

} // namespace firmware::power
