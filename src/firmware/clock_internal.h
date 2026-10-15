#pragma once

// Firmware integration only: state is owned here; never copy it as a snapshot.
#include "config.h"
#include "dependencies.h"

namespace firmware::clock {

struct State {

    // Display timezone (POSIX TZ string). The bridge `server_time` is a UTC epoch
    // and the firmware never configured TZ, so localtime() rendered UTC: the clock
    // showed 14:45 while local (CST) was 22:45. Persisted in NVS `pm/tz`; change at
    // runtime with POST /diag?tz=CST-8 (note POSIX sign: UTC+8 => "CST-8").
    char deviceTz[32] = "CST-8";
    uint8_t  timeSource = TIME_NONE;   // clock provenance of this wake
    ExternalRtc externalRtc;
    uint64_t externalRtcLastPollMs = 0;
};
extern State state;

struct Retained {
    uint64_t rtcEpochUsAtSleep = 0; // wall clock at the last deep sleep
    uint64_t rtcClkUsAtSleep = 0;  // RTC timer at the last sleep
};
extern Retained retained;

void applyTimezone();
bool timeKnown();
uint64_t planUtc();
const char *timeSourceName(uint8_t src);
void ntpTimeSynced(struct timeval *tv);
void flushNtpRtcSync();
bool readExternalRtc(bool boot);
void writeClockStatus(JsonObject clock);
bool adoptServerTimeForce(JsonDocument &doc);
uint64_t v2NowMs();
String nowHHMM();
String fmtEpoch(long long ts, const char *fmt);
void restoreTimeFromRtc();

} // namespace firmware::clock
