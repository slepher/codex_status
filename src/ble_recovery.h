#pragma once
#include <stdint.h>

// Physical maintenance authorization, independent of PowerPlan and USB BLE.
struct BleRecoveryWindow {
    uint64_t until = 0;
    uint64_t lightUntil = 0;
    uint64_t startupUntil = 0;
    void startup(uint64_t now) { startupUntil = now + 120000; lightUntil = now + 300000; }
    uint64_t physicalUntil() const { return until > startupUntil ? until : startupUntil; }
    bool active(uint64_t now) const { return until && now < until; }
    void open(uint64_t now) {
        if (!active(now)) { until = now + 120000; lightUntil = now + 300000; }
    }
    // Only a new physical OFF -> ON action renews either window.
    bool toggle(uint64_t now) {
        startupUntil = 0;
        if (active(now)) { close(); return false; }
        open(now); return true;
    }
    uint32_t lightRemaining(uint64_t now) const {
        return lightUntil > now ? (uint32_t)((lightUntil - now + 999) / 1000) : 0;
    }
    void close() { until = 0; }
    uint32_t remaining(uint64_t now) const {
        return active(now) ? (uint32_t)((until - now + 999) / 1000) : 0;
    }
};
