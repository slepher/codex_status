#pragma once
#include <stdint.h>

// A network glyph must converge even when no business Data is delivered.
struct NetworkDisplayState {
    bool known = false, connected = false, pending = false;
    uint8_t attempts = 0;
    uint64_t nextAt = 0;
    void observe(bool up) {
        if (!known || connected != up) {
            known = true; connected = up; pending = true; attempts = 0; nextAt = 0;
        }
    }
    bool due(uint64_t now) const { return pending && attempts < 3 && now >= nextAt; }
    void rendered(bool success, uint64_t now) {
        if (!pending) return;
        if (success) { pending = false; return; }
        ++attempts; nextAt = now + 5000;
    }
};
