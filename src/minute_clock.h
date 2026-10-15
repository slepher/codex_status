#pragma once
#include <stdint.h>

// Wall-clock minutes and network appointments are independent. A failed panel
// write must not advance displayedMinute; a backwards correction is due too.
inline bool minuteClockDue(int64_t epoch, int64_t displayedMinute) {
    return epoch > 1600000000 && epoch / 60 != displayedMinute;
}
inline uint32_t nextDisplayWakeSeconds(int64_t epoch, bool hasClock, int64_t nextContact) {
    uint32_t seconds = hasClock && epoch > 1600000000 ? 60 - epoch % 60 : 60;
    if (nextContact && epoch > 1600000000) {
        const int64_t remaining = nextContact - epoch;
        const uint32_t contact = remaining <= 0 ? 1 : remaining > 3600 ? 3600 : remaining;
        if (contact < seconds) seconds = contact;
    }
    return seconds;
}
// Account for panel/radio shutdown and the current subsecond phase. An event
// already due gets the earliest timer wake, never a new relative sleep period.
inline uint64_t displaySleepMicros(uint64_t wakeEpochUs, uint64_t nowEpochUs) {
    return wakeEpochUs > nowEpochUs ? wakeEpochUs - nowEpochUs : 1000;
}
