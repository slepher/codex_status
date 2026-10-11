#pragma once
#include <stdint.h>

inline int batteryPercentFromMilliVolts(uint32_t mv) {
    if (mv >= 4200) return 100;
    if (mv <= 3300) return 0;
    return (int)((mv - 3300) * 100 / 900);
}

// Display-only state. Raw voltage and low-battery decisions never use this EMA.
class BatteryDisplay {
public:
    static constexpr uint32_t periodMs = 5000;
    template<class ReadMilliVolts>
    bool poll(uint32_t now, ReadMilliVolts read) {
        const uint32_t elapsed = now - phaseAt_;
        if (scaled_ >= 0 && elapsed < periodMs) return false;
        // Skip missed periods; one present reading cannot replace past samples.
        phaseAt_ = scaled_ < 0 ? now : phaseAt_ + (elapsed / periodMs) * periodMs;
        sampledAt_ = now;
        rawMv_ = read();
        const int value = batteryPercentFromMilliVolts(rawMv_) * 100;
        scaled_ = scaled_ < 0 ? value : (scaled_ * 9 + value + 5) / 10;
        return true;
    }
    int percent() const { return scaled_ < 0 ? -1 : (scaled_ + 50) / 100; }
    uint32_t rawMilliVolts() const { return rawMv_; }
    uint32_t sampledAt() const { return sampledAt_; }
private:
    uint32_t phaseAt_ = 0;
    uint32_t sampledAt_ = 0;
    int32_t scaled_ = -1;
    uint32_t rawMv_ = 0;
};
