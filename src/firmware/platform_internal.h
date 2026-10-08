#pragma once

// Firmware integration only: state is owned here; never copy it as a snapshot.
#include "config.h"
#include "dependencies.h"

namespace firmware::platform {

struct State {

    // Template transfer sets a flag; the loop task performs the redraw.
    volatile bool pendingTplChanged = false;
    String activeTplJson;
    String activeTplId;
    bool   activeTplHasNow = false;
    bool   activeTplHasMode = false;

    // v2 platform runtime: one committed Bundle (A/B), one compiled active
    // template, one active context, Bridge-owned PowerPlan.
    CtTemplate     v2Ct;
    bool           v2CtValid = false;
    BsProfile      v2Profile;
    bool           v2BundleReady = false;
    V2DataSeq      v2DataSeq;
    V2ContextGen   v2CtxGen;
    uint64_t       v2LastAckAtMs = 0;
    uint8_t        v2DisplayState = 0;        // 0 none,1 displayed,2 pending,3 failed

    // A BLE data snapshot accepted during the window is rendered with the clock in
    // one frame after the radio is off (v2RendezvousRender).
    bool           v2WakeRenderPending = false;
    String lastUsage;
    String lastChannel = "-";
};
extern State state;

struct Retained {
    uint32_t rtcUsageHash = 0;
    uint32_t rtcUsageRev = 0;      // bridge usage_rev last seen
    char     rtcTplActiveId[17] = {0};
    char     rtcTplHash[9] = {0};
    V2DataCheckpoint v2DataCheckpoint = {};
};
extern Retained retained;

bool usageCacheLoad(String &out);
bool tplCacheLoad();
bool v2ActiveLoad();
bool v2SwitchActive(uint8_t index);
void nextTemplate();
void factoryReset();
void applyV2Data(const String &body);
void handleV2Data();
void applyV2Plan(const String &body);
void handleV2Plan();
void handleV2Activate();
void handleV2BundleBegin();
void handleV2BundleChunk();
void handleV2BundleCommit();

} // namespace firmware::platform
