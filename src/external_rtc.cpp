#include "external_rtc.h"
#include <Wire.h>
#include <Arduino.h>
#include <string.h>

namespace {
constexpr uint8_t kAddress = 0x51;
// Manufacturer maps for Note4 V1.0 and Waveshare ESP32-S3-ePaper-1.54G.
constexpr int kSda = 47, kScl = 48;
#if defined(CODEX_TARGET_NOTE4)
constexpr uint8_t kSeconds = 0x02; // PCF8563: sec/min/hour/day/weekday/month/year
constexpr uint8_t kBadControl = 0xA8; // STOP, TEST1, TESTC
#else
constexpr uint8_t kSeconds = 0x04; // PCF85063A: same order, different base
constexpr uint8_t kBadControl = 0xA2; // STOP, EXT_TEST, 12-hour mode
#endif
// Note4's manufacturer forces GPIO42 high for every shared I2C access.
// Restore its previous level after the complete RTC operation so audio remains
// off while idle/deep. The other board has no such rail requirement.
class RtcBusPower {
public:
#if defined(CODEX_TARGET_NOTE4)
    RtcBusPower() : previous_(digitalRead(42)) {
        pinMode(42, OUTPUT);
        digitalWrite(42, HIGH);
    }
    ~RtcBusPower() { digitalWrite(42, previous_); }
private:
    int previous_;
#endif
};
constexpr uint8_t kStop = 0x20;
constexpr time_t kFirstUtc = 1600000001; // existing timeKnown() threshold
constexpr time_t kEndUtc = 4102444800LL; // 2100-01-01; century unsupported

bool leap(int year) { return year % 4 == 0 && (year % 100 != 0 || year % 400 == 0); }
int monthDays(int year, int month) {
    static const uint8_t days[] = {31,28,31,30,31,30,31,31,30,31,30,31};
    return days[month - 1] + (month == 2 && leap(year) ? 1 : 0);
}
int bcd(uint8_t value) {
    if ((value & 15) > 9 || (value >> 4) > 9) return -1;
    return (value >> 4) * 10 + (value & 15);
}
uint8_t toBcd(int value) { return (uint8_t)((value / 10) * 16 + value % 10); }

bool decode(const uint8_t *r, time_t &utc) {
    if (r[0] & 0x80) return false; // PCF8563 VL / PCF85063A OS
#if defined(CODEX_TARGET_NOTE4)
    // Convention: C=0 denotes 2000..2099. C toggles at 2099->2100.
    if (r[5] & 0x80) return false;
#endif
    int second = bcd(r[0] & 0x7F), minute = bcd(r[1] & 0x7F);
    int hour = bcd(r[2] & 0x3F), day = bcd(r[3] & 0x3F);
    int weekday = r[4] & 7, month = bcd(r[5] & 0x1F), yy = bcd(r[6]);
    if (second < 0 || second > 59 || minute < 0 || minute > 59 ||
        hour < 0 || hour > 23 || weekday > 6 || month < 1 || month > 12 ||
        yy < 0 || day < 1 || day > monthDays(2000 + yy, month)) return false;
    // Explicit UTC conversion avoids mktime()/the display timezone normalizing
    // invalid dates or interpreting RTC UTC as local time.
    int days = 0;
    for (int year = 1970; year < 2000 + yy; ++year) days += leap(year) ? 366 : 365;
    for (int m = 1; m < month; ++m) days += monthDays(2000 + yy, m);
    time_t candidate = (time_t)(days + day - 1) * 86400 + hour * 3600 + minute * 60 + second;
    if (candidate < kFirstUtc || candidate >= kEndUtc) return false;
    utc = candidate;
    return true;
}
}

const char *ExternalRtc::model() const {
#if defined(CODEX_TARGET_NOTE4)
    return "pcf8563";
#else
    return "pcf85063a";
#endif
}

bool ExternalRtc::begin() {
    if (begun_) return true;
    // Largest transfer is 11 bytes, but ESP32 Wire requires its 32-byte FIFO
    // minimum per direction. Driver/semaphore allocations are separate.
    if (Wire.setBufferSize(32) < 32) return fail("buffer_allocation_failed");
    Wire.setTimeOut(25);
    begun_ = Wire.begin(kSda, kScl, 100000);
    return begun_ || fail("bus_init_failed");
}

bool ExternalRtc::readRegisters(uint8_t reg, uint8_t *bytes, uint8_t count) {
    Wire.beginTransmission(kAddress);
    if (Wire.write(reg) != 1) { Wire.endTransmission(); return fail("i2c_error"); }
    if (Wire.endTransmission(false) != 0) return fail("i2c_error");
    if (Wire.requestFrom(kAddress, count, true) != count) return fail("short_read");
    for (uint8_t i = 0; i < count; ++i) {
        if (!Wire.available()) return fail("short_read");
        bytes[i] = (uint8_t)Wire.read();
    }
    return true;
}

bool ExternalRtc::writeRegisters(uint8_t reg, const uint8_t *bytes, uint8_t count) {
    Wire.beginTransmission(kAddress);
    if (Wire.write(reg) != 1 || Wire.write(bytes, count) != count) {
        Wire.endTransmission();
        return fail("i2c_error");
    }
    return Wire.endTransmission() == 0 || fail("i2c_error");
}

bool ExternalRtc::read(time_t &utc) {
    [[maybe_unused]] RtcBusPower power;
    // A failed trusted correction must not be overwritten by the old RTC value
    // later in this boot. A new trusted write/readback clears this barrier.
    if (writeUnconfirmed_) return false;
    if (!begin()) return false;
    uint8_t registers[kSeconds + 7];
    if (!readRegisters(0, registers, sizeof(registers))) return false;
    if (registers[0] & kBadControl) return fail("stopped_or_test_mode");
    if (registers[kSeconds] & 0x80) return fail("clock_integrity_lost");
    time_t candidate;
    if (!decode(registers + kSeconds, candidate)) return fail("invalid_calendar");
    utc = sampledUtc_ = candidate;
    valid_ = true;
    status_ = "ready";
    return true;
}

bool ExternalRtc::write(time_t utc) {
    [[maybe_unused]] RtcBusPower power;
    writeUnconfirmed_ = true;
    if (utc < kFirstUtc || utc >= kEndUtc) return fail("unsupported_timestamp");
    if (!begin()) return false;
    struct tm calendar;
    if (!gmtime_r(&utc, &calendar)) return fail("invalid_calendar");
    const uint8_t bytes[7] = {
        toBcd(calendar.tm_sec), toBcd(calendar.tm_min), toBcd(calendar.tm_hour),
        toBcd(calendar.tm_mday), (uint8_t)calendar.tm_wday,
        toBcd(calendar.tm_mon + 1), toBcd(calendar.tm_year - 100)
    };
    uint8_t control;
    if (!readRegisters(0, &control, 1)) return false;
    const uint8_t running = control & ~kBadControl;
    const uint8_t stopped = running | kStop;
    if (!writeRegisters(0, &stopped, 1) ||
        !writeRegisters(kSeconds, bytes, sizeof(bytes))) return false;
    uint8_t verified[7];
    if (!readRegisters(kSeconds, verified, sizeof(verified))) return false;
    // Leave STOP set after a torn/corrupted write; never publish a mixed date.
    if (memcmp(bytes, verified, sizeof(bytes))) return fail("write_verify_failed");
    if (!writeRegisters(0, &running, 1)) return false;
    writeUnconfirmed_ = false;
    time_t observed;
    if (!read(observed)) { writeUnconfirmed_ = true; return false; }
    if (observed < utc || observed > utc + 1) {
        writeUnconfirmed_ = true;
        return fail("write_verify_failed");
    }
    return true;
}
