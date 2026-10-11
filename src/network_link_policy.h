#pragma once
#include <cstring>

// Bridge delivery failure cannot revoke a live association. Power policy owns
// the BOOT/formal/temporary deadlines and closes radios when those windows end.
inline bool networkFailureDisconnects(const char *reason, bool hasIp) {
    return !hasIp || std::strcmp(reason, "bridge_http_unreachable") != 0;
}

// A matching accepted response proves this direction is reachable. The cached
// reverse HTTP proof authorizes no action here and must not count as a failure.
inline bool bridgePresenceAccepted(int code, bool matchingPrincipal, bool accepted) {
    return code == 202 && matchingPrincipal && accepted;
}
