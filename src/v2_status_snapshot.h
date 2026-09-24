#pragma once

#include <ArduinoJson.h>
#include "bundle_store.h"
#include "v2_state.h"

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
};

String v2BuildStatusSnapshot(const V2StatusSnapshot &snapshot);
