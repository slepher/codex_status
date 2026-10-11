#pragma once
#include <stdint.h>

enum class WifiPhase : uint8_t { Idle, Connecting, Connected, Off };
inline WifiPhase wifiPhaseFor(bool connecting, bool connected, bool pending, bool terminal) {
    if (connecting) return WifiPhase::Connecting;
    if (connected) return WifiPhase::Connected;
    if (pending) return WifiPhase::Idle;
    return terminal ? WifiPhase::Off : WifiPhase::Idle;
}
inline const char *wifiPhaseText(WifiPhase phase) {
    switch (phase) {
    case WifiPhase::Connecting: return "connecting";
    case WifiPhase::Connected: return "connected";
    case WifiPhase::Off: return "off";
    default: return "idle";
    }
}

// Contact freshness includes both BLE and HTTP; RF state is a separate fact.
inline int bridgeOfflineMinutes(uint32_t now, uint32_t lastContact, bool timeKnown,
                                int lostMinutes, int unknownValue, bool knownUnavailable = false) {
    if (knownUnavailable) return 0;
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
    WifiPhase wifiPhase = WifiPhase::Idle;
    int wifiBlink = -1;
    bool bleManual = false;
    void observe(bool up, int offline = -1, WifiPhase phase = WifiPhase::Idle, int blink = -1, bool manual = false) {
        if (!known || connected != up || bridgeOffline != offline || wifiPhase != phase || wifiBlink != blink || bleManual != manual) {
            bleManual = manual;
            wifiPhase = phase; wifiBlink = blink;
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
