#pragma once

#include <stddef.h>
#include <stdint.h>

// One retained diagnostic stream. The caller places this object in RTC SLOW.
// A frozen Flash batch is an immutable copy of these bytes, not another log.
enum : uint8_t {
    SYNC_WAKE_SUMMARY = 1, SYNC_EVENT = 2, SYNC_TEXT = 3, SYNC_RESULT = 4,
};
enum : uint8_t { SYNC_TEXT_TRUNCATED = 1, SYNC_TIME_UNKNOWN = 2 };

struct SyncRecord {
    uint8_t kind = 0;
    uint8_t flags = 0;
    uint64_t seq = 0;
    uint32_t wakeSeq = 0;
    uint32_t uptimeMs = 0;
    uint8_t payload[96] = {};
    uint8_t payloadLen = 0;
};

struct SyncRtc {
    uint8_t bytes[4096] = {};
    uint32_t magic = 0;
    uint32_t headerCrc = 0;
    uint8_t generation[16] = {};
    uint64_t nextSeq = 1;
    uint64_t ackedSeq = 0;
    uint64_t lastResetSerial = 0;
    uint16_t head = 0;
    uint16_t used = 0;
    uint8_t rounds = 15;
    uint8_t retrySkip = 0;
    uint8_t failureCount = 0;
    uint8_t reasonBits = 0;
    uint8_t flags = 1; // bit 0 due; bit 1 baseline known; bit 2 retry pending
    uint8_t reserved[3] = {};
};
static_assert(sizeof(((SyncRtc*)nullptr)->bytes) == 4096, "sync RTC ring must be 4096 bytes");
static_assert(sizeof(SyncRtc) - 4096 <= 768, "sync RTC control exceeds 768 bytes");

uint32_t syncCrc32(const uint8_t *bytes, size_t length);
void syncSha256(const uint8_t *bytes, size_t length, uint8_t digest[32]);
void syncHex(const uint8_t *bytes, size_t length, char *output);
size_t syncBase64(const uint8_t *bytes, size_t length, char *output, size_t capacity);
size_t syncSanitizeText(const char *text, size_t length, char output[97], uint8_t &flags);
bool syncDiagnosticText(const char *text);
bool syncRecover(SyncRtc &rtc, const uint8_t generation[16]);
bool syncAppend(SyncRtc &rtc, uint8_t kind, uint8_t flags, uint32_t wakeSeq,
                uint32_t uptimeMs, const uint8_t *payload, size_t length);
bool syncAppendEvent(SyncRtc &rtc, uint32_t wakeSeq, uint32_t uptimeMs,
                     uint16_t code, const int32_t *args, uint8_t argc);
bool syncAppendResult(SyncRtc &rtc, uint32_t wakeSeq, uint32_t uptimeMs,
                      uint8_t result, uint8_t reasons, uint16_t error,
                      uint64_t batchSerial);
bool syncReadAt(const SyncRtc &rtc, uint16_t offset, SyncRecord &record,
                uint16_t &nextOffset);
uint64_t syncEarliestSeq(const SyncRtc &rtc);
void syncCountDeepRendezvous(SyncRtc &rtc);
void syncAddReason(SyncRtc &rtc, uint8_t reasonBit);
void syncComplete(SyncRtc &rtc, uint64_t serial, uint64_t throughSeq,
                  uint8_t completedReasons);
void syncFail(SyncRtc &rtc);
void syncConsumeSkip(SyncRtc &rtc);
