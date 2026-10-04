#pragma once
#include <stdint.h>
#include <time.h>

// Board RTC holds UTC. No alarms/CLKOUT changes, no NVS, no dynamic records.
// All calls belong to Arduino's loop task (including setup), never SNTP's task.
class ExternalRtc {
public:
    bool read(time_t &utc);
    bool write(time_t utc);
    const char *model() const;
    const char *status() const { return status_; }
    bool valid() const { return valid_; }
    time_t sampledUtc() const { return sampledUtc_; }
private:
    bool begin();
    bool readRegisters(uint8_t reg, uint8_t *bytes, uint8_t count);
    bool writeRegisters(uint8_t reg, const uint8_t *bytes, uint8_t count);
    bool fail(const char *reason) { status_ = reason; valid_ = false; return false; }
    bool begun_ = false;
    bool valid_ = false;
    bool writeUnconfirmed_ = false;
    const char *status_ = "not_read";
    time_t sampledUtc_ = 0;
};
