#pragma once
#include <stdint.h>

// Contact freshness includes both BLE and HTTP; RF state is a separate fact.
inline int bridgeOfflineMinutes(uint32_t now, uint32_t lastContact, bool timeKnown,
                                int lostMinutes, int unknownValue) {
    if (!timeKnown || lastContact <= 1600000000) return unknownValue;
    const uint32_t minutes = now > lastContact ? (now - lastContact) / 60 : 0;
    return minutes > (uint32_t)lostMinutes ? (int)minutes : -1;
}

// A network glyph must converge even when no business Data is delivered.
inline bool sleepFrameNeedsRefresh(uint8_t glyph, int bridgeOffline) {
    return !(glyph & 4) || bool(glyph & 8) != (bridgeOffline >= 0);
}

struct NetworkDisplayState {
    bool known = false, connected = false, pending = false;
    uint8_t attempts = 0;
    uint64_t nextAt = 0;
    int bridgeOffline = -1;
    void observe(bool up, int offline = -1) {
        if (!known || connected != up || bridgeOffline != offline) {
            bridgeOffline = offline;
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
