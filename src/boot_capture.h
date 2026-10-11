#pragma once
#include <stdint.h>

// Startup-only edge capture. Access under the GPIO critical section.
struct BootCapture {
    bool low = false, ignore = false, pending = false;
    uint32_t downAt = 0;
    void begin(bool pressed, bool alreadyConsumed, uint32_t now) {
        low = pressed; ignore = alreadyConsumed; pending = false; downAt = now;
    }
#if defined(__GNUC__)
    __attribute__((always_inline)) inline
#else
    __forceinline
#endif
    void edge(bool pressed, uint32_t now) {
        if (pressed == low) return;
        low = pressed;
        if (pressed) downAt = now;
        else {
            if (!ignore && now - downAt >= 50) pending = true;
            ignore = false;
        }
    }
    bool take(uint32_t now) {
        if (!pending && (ignore || !low || now - downAt < 50)) return false;
        pending = false; ignore = low; return true;
    }
};
