#pragma once
#include <stdint.h>

// One physical LED: 80 ms phases, idle after 160 ms without application bytes.
struct DataActivityLed {
    uint32_t lastDataAt = 0;
    bool active = false, on = false;
    bool activity(uint32_t now) {
        lastDataAt = now;
        if (active) return false;
        active = on = true;
        return true;
    }
    bool tick(uint32_t now) {
        if (!active || uint32_t(now - lastDataAt) >= 160) active = on = false;
        else on = !on;
        return active;
    }
};

void dataLedBegin();
void dataLedActivity();
void dataLedStop();
