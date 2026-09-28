#include "dev_log.h"
#include "v2_sync.h"

#include <stdarg.h>
#include <string.h>

extern SyncRtc syncRtc;
extern uint32_t syncLogWakeSeq;
extern bool syncLogReady;

namespace {
char line[97];
uint8_t length = 0;
bool truncated = false;

void flushLine() {
    char output[97];
    uint8_t flags = truncated ? SYNC_TEXT_TRUNCATED : 0;
    size_t count = syncSanitizeText(line, length, output, flags);
    if (syncLogReady && syncDiagnosticText(output))
        syncAppend(syncRtc, SYNC_TEXT, flags, syncLogWakeSeq, millis(),
                   (const uint8_t *)output, count);
    Serial.write((const uint8_t *)output, count);
    Serial.write((const uint8_t *)"\n", 1);
    length = 0;
    truncated = false;
}
}

DevLogger DevLog;

void DevLogger::append(const char *data, size_t count) {
    for (size_t i = 0; i < count; ++i) {
        if (data[i] == '\n') flushLine();
        else if (length < sizeof(line) - 1) line[length++] = data[i];
        else truncated = true;
    }
}

void DevLogger::printf(const char *fmt, ...) {
    char buf[200];
    va_list args;
    va_start(args, fmt);
    int n = vsnprintf(buf, sizeof(buf), fmt, args);
    va_end(args);
    if (n > 0) append(buf, (size_t)((n < (int)sizeof(buf)) ? n : (int)sizeof(buf) - 1));
}

void DevLogger::println(const char *s) {
    append(s, strlen(s));
    append("\n", 1);
}
void DevLogger::println() { append("\n", 1); }
void DevLogger::print(const char *s) { append(s, strlen(s)); }

String DevLogger::dump() {
    String out;
    out.reserve(syncRtc.used);
    for (uint16_t offset = 0, next = 0; offset < syncRtc.used; offset = next) {
        SyncRecord record;
        if (!syncReadAt(syncRtc, offset, record, next)) break;
        if (record.kind != SYNC_TEXT) continue;
        for (uint8_t i = 0; i < record.payloadLen; ++i) out += (char)record.payload[i];
        out += '\n';
    }
    return out;
}
