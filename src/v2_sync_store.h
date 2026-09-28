#pragma once

#include <stddef.h>
#include <stdint.h>

// LittleFS A/B metadata and a bounded frozen copy of the RTC stream.
// No page read or ACK deletes the source; only a durable complete receipt does.
struct SyncStoreBatch {
    char batchId[64] = {};
    char owner[65] = {};
    uint64_t clientSerial = 0;
    uint64_t deviceSerial = 0;
    uint64_t fromSeq = 0;
    uint64_t throughSeq = 0;
    uint8_t generation[16] = {};
    uint8_t reasons = 0;
    uint32_t bytes = 0;
    uint8_t sha256[32] = {};
};

bool syncStoreBegin();
bool syncStoreAvailable();
bool syncStoreLost();
bool syncStoreActive(SyncStoreBatch &out);
bool syncStoreReceipt(SyncStoreBatch &out);
uint64_t syncStoreHighestClientSerial();
bool syncStoreChangeOwner();
bool syncStoreAdvanceSerial(uint64_t &serial);
bool syncStoreFreeze(const uint8_t *bytes, size_t length, const SyncStoreBatch &batch);
bool syncStorePage(uint32_t offset, uint16_t limit, uint8_t *out, size_t &read);
bool syncStoreAck(uint32_t offset, const uint8_t prefixSha256[32]);
bool syncStoreComplete(const SyncStoreBatch &expected);
uint32_t syncStoreAckedOffset();
