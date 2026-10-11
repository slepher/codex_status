#pragma once
#include <stdint.h>

// GPIO edge capture shared by startup and the running loop, under its critical
// section. Eight completed presses bound ISR memory while HTTP/EPD blocks.
struct BootCapture {
    enum Action : uint8_t { None, Click, Template, Config, Reset };
    struct Press { uint32_t at, held; uint8_t stage; };
    bool low = false, ignore = false;
    uint32_t downAt = 0, takenAt = 0;
    uint8_t stage = 0, first = 0, count = 0, dropped = 0;
    Press presses[8]{};
    void begin(bool pressed, bool alreadyConsumed, uint32_t now) {
        low = pressed; ignore = alreadyConsumed; downAt = now;
        stage = first = count = dropped = 0;
    }
#if defined(__GNUC__)
    __attribute__((always_inline)) inline
#else
    __forceinline
#endif
    void edge(bool pressed, uint32_t now) {
        if (pressed == low) return;
        low = pressed;
        if (pressed) { downAt = now; stage = 0; }
        else {
            if (!ignore && now - downAt >= 50) {
                if (count < 8) {
                    Press &p = presses[(first + count) % 8];
                    p.at = downAt; p.held = now - downAt; p.stage = stage; ++count;
                } else if (dropped != 255) ++dropped;
            }
            ignore = false;
        }
    }
    bool take(uint32_t now) {
        if (count) {
            takenAt = presses[first].at; first = (first + 1) % 8; --count; return true;
        }
        if (ignore || !low || now - downAt < 50) return false;
        takenAt = downAt; ignore = true; return true;
    }
    Action action(uint32_t now, uint32_t &at) {
        while (count) {
            Press &p = presses[first]; at = p.at;
            const Action result = advance(p.held, p.stage, true);
            if (result == None || p.stage == 3 || (p.stage == 1 && p.held < 15000) ||
                (p.stage == 2 && p.held < 30000)) {
                first = (first + 1) % 8; --count;
            }
            if (result != None) return result;
        }
        at = downAt;
        return low && !ignore ? advance(now - downAt, stage, false) : None;
    }
    static Action advance(uint32_t held, uint8_t &stage, bool released) {
        if (!stage && held >= 2000) { stage = 1; return Template; }
        if (stage < 2 && held >= 15000) { stage = 2; return Config; }
        if (stage < 3 && held >= 30000) { stage = 3; return Reset; }
        if (!stage && released && held >= 50) { stage = 3; return Click; }
        return None;
    }
};
