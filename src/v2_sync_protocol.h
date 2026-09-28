#pragma once

#include "v2_sync.h"
#include "v2_sync_store.h"
#include <ArduinoJson.h>

// Auth, owner lease, HTTP and hardware sampling stay with each adapter. These
// decisions and the frozen byte format are shared by the ROM and Fake ROM.
struct SyncProtocolInput {
    SyncRtc &rtc;
    const char *mac;
    const char *owner;
    const char *batchSuffix; // 32 lowercase hex characters, used only for a new batch
    uint32_t wakeSeq;
    uint32_t uptimeMs;
    bool imageVerified;
    JsonVariantConst snapshot;
};

struct SyncProtocolResult {
    int status = 200;
    bool completed = false;
    bool otaConfirmed = false;
};

void syncProtocolBatchFields(JsonObject out, const SyncStoreBatch &batch);
SyncProtocolResult syncProtocolRun(const SyncProtocolInput &input,
                                   const char *operation,
                                   JsonDocument &request,
                                   JsonDocument &response);
