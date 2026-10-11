#pragma once
#include "v2_plan_command.h"
#include <Arduino.h>

// One actual plan, shared by HTTP/BLE. The caller owns live monotonic state.
class PlanStore {
public:
    bool begin(V2PlanState &state, uint64_t nowMs, uint64_t utc);
    V2PlanDecision apply(JsonDocument &doc, V2PlanState &state, uint64_t nowMs,
                         uint64_t utc, bool provisional);
    bool reanchor(const V2PlanState &state, uint64_t nowMs, uint64_t utc);
    bool clear();
    const char *status() const { return status_; }
private:
    enum Commit { Failed, Unknown, Saved };
    Commit save(const V2PowerPlan &plan, uint32_t grant, const String &principal,
                uint64_t start, uint64_t expiry);
    String principal_;
    V2PowerPlan request_{};
    bool uncertain_ = false;
    const char *status_ = "missing";
};

inline bool planBootHttp(bool usb, bool deepOnUsb, const V2PlanState &plan, uint64_t nowMs) {
    return (usb && !deepOnUsb) || plan.lightActive(nowMs);
}
