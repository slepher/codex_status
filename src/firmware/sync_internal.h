#pragma once

// Firmware integration only: state is owned here; never copy it as a snapshot.
#include "config.h"
#include "dependencies.h"

namespace firmware::sync {

struct State {
    uint32_t syncLogWakeSeq = 0;
    bool syncLogReady = false;
    uint32_t lastBridgeContactMs = 0;
    uint32_t       v2HistorySyncMs = 0;
    uint64_t       v2HistorySyncUntilMs = 0;
    bool syncEnabled = false;
    String syncOwner;
    bool syncOpenThisWake = false;
    bool syncDataPending = false;
    uint64_t syncDataSeq = 0; // admitted periodic HTTP Data, not a formal Light grant
    uint32_t syncLastProgressMs = 0;
    uint32_t syncExitAtMs = 0;
    uint64_t syncOpenAnsweredAt = 0;
    String syncOpenId, syncOpenReason;
    bool syncSkippedThisWake = false;
    uint8_t syncVisibleSkip = 0;
    uint16_t syncBlockedError = 0;
    const char *lastFailure = nullptr; // RAM only; durable reason also enters unified diagnostic text.
};
extern State state;

struct Retained {

    // Packed on purpose: this is the RTC contract shared with Bridge history
    // export. Nine uint16 timings use 100 ms units, covering about 109 minutes;
    // JSON expands them back to milliseconds. awake_ms remains uint32.
    SyncRtc syncRtc;
};
extern Retained retained;

void syncRecordOutcome(uint8_t result, uint16_t error, uint64_t serial = 0);
String syncNumber(uint64_t value);
void syncReply(const char *op, const char *result, const char *error,
                      int httpStatus, JsonDocument *extra = nullptr);
void syncBatchFields(JsonDocument &doc, const SyncStoreBatch &batch);
uint64_t httpUntil(uint64_t now);
void dataApplied(uint64_t seq);
void handleSyncBegin();
void handleSyncPage();
void handleSyncAck();
void handleSyncComplete();
void beginOfflineLog();
void noteLogContact();
void expireOfflineLog();
void handleOfflineLog();

} // namespace firmware::sync
