#include "clock_internal.h"
#include "power_internal.h"

namespace firmware::clock {

State state;
RTC_DATA_ATTR Retained retained;

static portMUX_TYPE ntpRtcMux = portMUX_INITIALIZER_UNLOCKED;

static bool ntpRtcPending = false;

static uint64_t ntpRtcEpochUs = 0, ntpRtcMonotonicUs = 0;

static bool applyTzOffsetMin(long offsetMin);
static void writeExternalRtcFromSystem();
static bool adoptTzOffset(JsonDocument &doc);
static void adoptServerTime(JsonDocument &doc);
static uint32_t v2Millis32();

void applyTimezone() {
    setenv("TZ", state.deviceTz, 1);
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
    if (!strcmp(buf, state.deviceTz)) return false;
    strncpy(state.deviceTz, buf, sizeof(state.deviceTz) - 1);
    state.deviceTz[sizeof(state.deviceTz) - 1] = '\0';
    applyTimezone();
    Preferences p;
    p.begin("pm", false);
    p.putString("tz", state.deviceTz);
    p.end();
    DevLog.printf("[pm] tz <- bridge %ldmin (%s)\n", offsetMin, state.deviceTz);
    return true;
}

bool timeKnown() { return time(nullptr) > 1600000000; }

uint64_t planUtc() {
#if defined(CODEX_TARGET_NOTE4)
    time_t utc;
    return state.externalRtc.read(utc) ? (uint64_t)utc : 0;
#else
    return timeKnown() ? (uint64_t)time(nullptr) : 0;
#endif
}

const char *timeSourceName(uint8_t src) {
    return src == TIME_BLE ? "ble" : src == TIME_RTC ? "rtc" :
           src == TIME_EXTERNAL_RTC ? "external_rtc" : src == TIME_NTP ? "ntp" : "none";
}

// Only a fresh trusted Bridge/SNTP correction writes the board RTC. Writing
// the internal-RC fallback here would turn offline drift into a new RTC anchor.
static void writeExternalRtcFromSystem() {
    time_t utc = time(nullptr);
    time_t previous;
    if ((!state.externalRtc.read(previous) || previous > utc + 1 || utc > previous + 1) &&
        !power::state.planStore.reanchor(power::state.v2Plan, v2NowMs(), (uint64_t)utc)) {
        DevLog.println("[clock] RTC correction deferred: plan checkpoint not confirmed");
        return;
    }
    if (!state.externalRtc.write(utc)) {
        DevLog.printf("[clock] %s correction failed: %s\n", state.externalRtc.model(), state.externalRtc.status());
    }
}

void ntpTimeSynced(struct timeval *tv) {
    // SNTP runs on TCP/IP's task. Capture its actual UTC and elapsed-time anchor:
    // a concurrent RTC poll must not replace the new NTP time before writeback.
    // No I2C, logging or allocation in this callback/critical section.
    if (!tv || tv->tv_sec <= 1600000000) return;
    const uint64_t epochUs = (uint64_t)tv->tv_sec * 1000000ULL + tv->tv_usec;
    const uint64_t monotonicUs = (uint64_t)esp_timer_get_time();
    portENTER_CRITICAL(&ntpRtcMux);
    ntpRtcEpochUs = epochUs;
    ntpRtcMonotonicUs = monotonicUs;
    ntpRtcPending = true;
    portEXIT_CRITICAL(&ntpRtcMux);
}

void flushNtpRtcSync() {
    portENTER_CRITICAL(&ntpRtcMux);
    const bool pending = ntpRtcPending;
    const uint64_t epochUs = ntpRtcEpochUs, monotonicUs = ntpRtcMonotonicUs;
    ntpRtcPending = false;
    portEXIT_CRITICAL(&ntpRtcMux);
    if (!pending) return;
    // setup/network/render work may delay this flush. Use the captured sample
    // plus elapsed monotonic time, never a potentially overwritten system time.
    const uint64_t nowUs = (uint64_t)esp_timer_get_time();
    const uint64_t currentUs = epochUs + (nowUs >= monotonicUs ? nowUs - monotonicUs : 0);
    struct timeval tv = {(time_t)(currentUs / 1000000ULL),
                         (suseconds_t)(currentUs % 1000000ULL)};
    if (settimeofday(&tv, nullptr) != 0) return;
    state.timeSource = TIME_NTP;
    writeExternalRtcFromSystem();
}

bool readExternalRtc(bool boot) {
    time_t utc;
    state.externalRtcLastPollMs = (uint64_t)(esp_timer_get_time() / 1000ULL);
    if (!state.externalRtc.read(utc)) return false;
    time_t current = time(nullptr);
    // The chip has whole-second resolution. Preserve a recent system clock's
    // subsecond phase within 1 s; offline repeated wakes use the chip's absolute
    // UTC, never a cumulative, rounded sleep delta.
    if (boot || !timeKnown() || current < utc - 1 || current > utc + 1) {
        struct timeval tv = {utc, 0};
        if (settimeofday(&tv, nullptr) != 0) return false;
        state.timeSource = TIME_EXTERNAL_RTC;
    }
    return true;
}

void writeClockStatus(JsonObject clock) {
    clock["model"] = state.externalRtc.model();
    clock["status"] = state.externalRtc.status();
    clock["valid"] = state.externalRtc.valid();
    if (state.externalRtc.valid()) clock["sampled_utc"] = (int64_t)state.externalRtc.sampledUtc();
    else clock["sampled_utc"] = nullptr;
    clock["source"] = timeSourceName(state.timeSource);
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
        if (settimeofday(&tv, nullptr) == 0) {
            state.timeSource = TIME_BLE;
            writeExternalRtcFromSystem();
        }
        DevLog.printf("[pm] clock set from bridge: %lld\n", st);
    }
    adoptTzOffset(doc);
}

// Deep network windows align to the bridge clock every pull (docs §13.3); the
// RTC-differential reconstruction is only a fallback between contacts. Returns
// true when the timezone changed, so the caller can force a redraw.
bool adoptServerTimeForce(JsonDocument &doc) {
    long long st = doc["server_time"] | 0LL;
    if (st > 1600000000) {
        struct timeval tv = {(time_t)st, 0};
        if (settimeofday(&tv, nullptr) == 0) {
            state.timeSource = TIME_BLE;
            writeExternalRtcFromSystem();
        }
    }
    return adoptTzOffset(doc);
}

uint64_t v2NowMs() { return (uint64_t)(esp_timer_get_time() / 1000ULL); }

static uint32_t v2Millis32() { return (uint32_t)(esp_timer_get_time() / 1000ULL); }

String nowHHMM() {
    char b[8] = "--:--";
    time_t n = time(nullptr);
    struct tm *t = localtime(&n);
    if (t && n > 1600000000) strftime(b, sizeof(b), "%H:%M", t);
    return String(b);
}

String fmtEpoch(long long ts, const char *fmt) {
    if (ts <= 0) return String("--");
    time_t t = (time_t)ts;
    struct tm *lt = localtime(&t);
    char b[24];
    if (!lt || !strftime(b, sizeof(b), fmt, lt)) return String("--");
    return String(b);
}

// Deep sleep does not lose the RTC timer; reconstruct the wall clock from the
// last sleep anchor so minute ticks stay aligned without a network contact.
void restoreTimeFromRtc() {
    if (!retained.rtcEpochUsAtSleep || !retained.rtcClkUsAtSleep) return;
    uint64_t nowUs = esp_rtc_get_time_us();
    if (nowUs <= retained.rtcClkUsAtSleep) return;
    uint64_t epochUs = retained.rtcEpochUsAtSleep + nowUs - retained.rtcClkUsAtSleep;
    struct timeval tv = {(time_t)(epochUs / 1000000ULL),
                         (suseconds_t)(epochUs % 1000000ULL)};
    settimeofday(&tv, nullptr);
}

uint32_t secsToNextMinute() {
    if (!timeKnown()) return 60;
    time_t n = time(nullptr);
    uint32_t s = (uint32_t)(60 - (n % 60));
    if (s < 5) s += 60;   // keep a margin so ticks never double-fire
    return s;
}

} // namespace firmware::clock
