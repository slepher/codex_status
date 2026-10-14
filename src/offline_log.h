#pragma once
#include <ArduinoJson.h>
#include <stddef.h>
#include <stdint.h>
struct SyncRtc;

// Independent of sync receipts. Retention never changes the RTC stream or ACKs.
constexpr size_t OFFLINE_LOG_MAX_BYTES = 24 * 16384;
bool offlineLogBegin(uint64_t utc);
void offlineLogExpire(uint64_t utc);
bool offlineLogCapture(const SyncRtc &rtc, uint64_t savedUtc);
bool offlineLogAppend(uint64_t utc, const uint8_t generation[16],
                      const uint8_t *record, size_t length);
void offlineLogStatus(uint64_t utc, JsonObject out);
int offlineLogPage(uint64_t utc, uint64_t hour, uint32_t offset, JsonObject out,
                   uint64_t serial = 0);
