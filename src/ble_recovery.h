#pragma once
#include <stdint.h>

// Physical maintenance authorization, independent of PowerPlan and USB BLE.
struct BleRecoveryWindow {
    uint64_t until = 0;
    bool active(uint64_t now) const { return until && now < until; }
    void open(uint64_t now) { if (!active(now)) until = now + 120000; }
    void close() { until = 0; }
    uint32_t remaining(uint64_t now) const {
        return active(now) ? (uint32_t)((until - now + 999) / 1000) : 0;
    }
};
