#pragma once

#include <ArduinoJson.h>
#include "bundle_store.h"
#include "v2_state.h"

// Optional firmware-only wake diagnostics. The host preview and the device
// simulator leave this unbound, so their status document keeps the shared
// baseline shape byte-for-byte; only the device attaches its live wake trace.
// A plain struct rather than a JSON bag, so four scalar fields cost the device
// no extra document allocation.
struct V2WakeSnapshot {
    uint32_t generation = 0;
    uint32_t seq = 0;
    const char *stage = "none";
    const char *cause = "";
};

struct V2StatusSnapshot {
    String mac;
    String sessionNonce;
    const BsProfile *profile = nullptr;
    bool configured = false;
    String activeTemplateId;
    const V2DataSeq *dataSeq = nullptr;
    uint8_t displayState = 0;
    uint32_t commitSeq = 0;
    bool deepSleep = false;
    const V2PlanState *plan = nullptr;
    bool provisional = false;
    uint64_t bootMs = 0;
    uint64_t nowMs = 0;
    uint8_t battery = 0;
    const V2WakeSnapshot *wake = nullptr;
};

String v2BuildStatusSnapshot(const V2StatusSnapshot &snapshot);
