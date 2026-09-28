#include "v2_sync.h"

#include <string.h>

namespace {
constexpr uint32_t MAGIC = 0x31594353u; // SCY1
constexpr uint16_t CAP = 4096;
constexpr uint8_t DUE = 1, KNOWN = 2, RETRY = 4;

uint8_t at(const SyncRtc &rtc, uint16_t pos) { return rtc.bytes[pos % CAP]; }
void put(SyncRtc &rtc, uint16_t pos, uint8_t value) { rtc.bytes[pos % CAP] = value; }
uint16_t sizeAt(const SyncRtc &rtc, uint16_t pos) {
    return (uint16_t)at(rtc, pos) | ((uint16_t)at(rtc, pos + 1) << 8);
}
uint32_t headerCrc(const SyncRtc &rtc) {
    return syncCrc32(reinterpret_cast<const uint8_t *>(&rtc.generation),
                     sizeof(SyncRtc) - offsetof(SyncRtc, generation));
}
void seal(SyncRtc &rtc) { rtc.headerCrc = headerCrc(rtc); }
void writeLe(uint8_t *out, uint64_t value, size_t count) {
    for (size_t i = 0; i < count; ++i) out[i] = (uint8_t)(value >> (8 * i));
}
uint64_t readLe(const uint8_t *in, size_t count) {
    uint64_t value = 0;
    for (size_t i = 0; i < count; ++i) value |= (uint64_t)in[i] << (8 * i);
    return value;
}
}

uint32_t syncCrc32(const uint8_t *bytes, size_t length) {
    uint32_t crc = 0xffffffffu;
    for (size_t i = 0; i < length; ++i) {
        crc ^= bytes[i];
        for (int bit = 0; bit < 8; ++bit)
            crc = (crc >> 1) ^ (0xedb88320u & -(int32_t)(crc & 1));
    }
    return ~crc;
}

void syncSha256(const uint8_t *bytes, size_t length, uint8_t digest[32]) {
    static const uint32_t k[64] = {
        0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
        0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
        0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
        0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
        0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
        0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
        0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
        0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2
    };
    uint32_t h[8] = {0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,
                     0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19};
    auto rotr = [](uint32_t x, unsigned n) { return (x >> n) | (x << (32 - n)); };
    auto block = [&](const uint8_t b[64]) {
        uint32_t w[64];
        for (int i = 0; i < 16; ++i)
            w[i] = ((uint32_t)b[4*i] << 24) | ((uint32_t)b[4*i+1] << 16)
                 | ((uint32_t)b[4*i+2] << 8) | b[4*i+3];
        for (int i = 16; i < 64; ++i) {
            uint32_t a = w[i-15], c = w[i-2];
            w[i] = w[i-16] + (rotr(a,7) ^ rotr(a,18) ^ (a >> 3)) + w[i-7]
                 + (rotr(c,17) ^ rotr(c,19) ^ (c >> 10));
        }
        uint32_t a=h[0], c=h[2], d=h[3], e=h[4], f=h[5], g=h[6], q=h[7], z=h[1];
        for (int i = 0; i < 64; ++i) {
            uint32_t t1=q+(rotr(e,6)^rotr(e,11)^rotr(e,25))+((e&f)^(~e&g))+k[i]+w[i];
            uint32_t t2=(rotr(a,2)^rotr(a,13)^rotr(a,22))+((a&z)^(a&c)^(z&c));
            q=g; g=f; f=e; e=d+t1; d=c; c=z; z=a; a=t1+t2;
        }
        h[0]+=a; h[1]+=z; h[2]+=c; h[3]+=d;
        h[4]+=e; h[5]+=f; h[6]+=g; h[7]+=q;
    };
    size_t offset = 0;
    while (length - offset >= 64) { block(bytes + offset); offset += 64; }
    uint8_t last[128] = {};
    size_t tail = length - offset;
    if (tail) memcpy(last, bytes + offset, tail);
    last[tail] = 0x80;
    uint64_t bits = (uint64_t)length * 8;
    size_t padded = tail < 56 ? 64 : 128;
    for (int i = 0; i < 8; ++i) last[padded-1-i] = (uint8_t)(bits >> (8*i));
    block(last);
    if (padded == 128) block(last + 64);
    for (int i = 0; i < 8; ++i)
        for (int j = 0; j < 4; ++j) digest[4*i+j] = (uint8_t)(h[i] >> (24-8*j));
}

void syncHex(const uint8_t *bytes, size_t length, char *output) {
    static const char digits[] = "0123456789abcdef";
    for (size_t i = 0; i < length; ++i) {
        output[2*i] = digits[bytes[i] >> 4];
        output[2*i+1] = digits[bytes[i] & 15];
    }
    output[2*length] = 0;
}

size_t syncBase64(const uint8_t *bytes, size_t length, char *output, size_t capacity) {
    static const char alphabet[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    size_t needed = ((length + 2) / 3) * 4;
    if (capacity <= needed) return 0;
    size_t at = 0;
    for (size_t i = 0; i < length; i += 3) {
        uint32_t triple = (uint32_t)bytes[i] << 16;
        if (i + 1 < length) triple |= (uint32_t)bytes[i+1] << 8;
        if (i + 2 < length) triple |= bytes[i+2];
        output[at++] = alphabet[(triple >> 18) & 63];
        output[at++] = alphabet[(triple >> 12) & 63];
        output[at++] = i + 1 < length ? alphabet[(triple >> 6) & 63] : '=';
        output[at++] = i + 2 < length ? alphabet[triple & 63] : '=';
    }
    output[at] = 0;
    return at;
}

size_t syncSanitizeText(const char *text, size_t length, char output[97], uint8_t &flags) {
    if (!text) return 0;
    if (length > 96) { length = 96; flags |= SYNC_TEXT_TRUNCATED; }
    size_t pos = 0, valid = 0;
    while (pos < length) {
        uint8_t first = (uint8_t)text[pos];
        size_t width = first < 0x80 ? 1 : (first & 0xe0) == 0xc0 ? 2
                     : (first & 0xf0) == 0xe0 ? 3 : (first & 0xf8) == 0xf0 ? 4 : 0;
        if (!width || pos + width > length) break;
        bool okay = true;
        for (size_t i = 1; i < width; ++i)
            if (((uint8_t)text[pos + i] & 0xc0) != 0x80) okay = false;
        if (!okay) break;
        pos += width;
        valid = pos;
    }
    memcpy(output, text, valid);
    output[valid] = 0;
    if (strstr(output, "pass=") || strstr(output, "password=") ||
        strstr(output, "token=") || strstr(output, "Authorization:")) {
        strcpy(output, "[redacted]");
        return 10;
    }
    return valid;
}

bool syncDiagnosticText(const char *text) {
    return text && (strstr(text, "fail") || strstr(text, "error") ||
        strstr(text, "rejected") || strstr(text, "unauthorized") ||
        strstr(text, "[sync]") || strstr(text, "[ota]") ||
        strstr(text, "[redacted]"));
}

bool syncRecover(SyncRtc &rtc, const uint8_t generation[16]) {
    bool corruptRecords = false;
    if (rtc.magic == MAGIC && rtc.head < CAP && rtc.used <= CAP &&
        rtc.nextSeq && rtc.headerCrc == headerCrc(rtc)) {
        bool valid = true;
        for (uint16_t offset = 0, next = 0; offset < rtc.used; offset = next) {
            SyncRecord record;
            if (!syncReadAt(rtc, offset, record, next) || next <= offset) {
                valid = false; break;
            }
        }
        if (valid) return true;
        corruptRecords = true;
    }
    memset(&rtc, 0, sizeof(rtc));
    rtc.magic = MAGIC;
    memcpy(rtc.generation, generation, 16);
    rtc.nextSeq = 1;
    rtc.rounds = 15;
    rtc.flags = DUE | (corruptRecords ? 8 : 0); // Bit 3 records a corrupt stream boundary.
    seal(rtc);
    return false;
}

bool syncReadAt(const SyncRtc &rtc, uint16_t offset, SyncRecord &record,
                uint16_t &nextOffset) {
    if (offset >= rtc.used) return false;
    const uint16_t start = (rtc.head + CAP - rtc.used + offset) % CAP;
    const uint16_t length = sizeAt(rtc, start);
    if (length < 24 || length > 120 || length > rtc.used - offset) return false;
    uint8_t raw[120];
    for (uint16_t i = 0; i < length; ++i) raw[i] = at(rtc, start + i);
    if (syncCrc32(raw, length - 4) != readLe(raw + length - 4, 4)) return false;
    if (raw[3] & ~(SYNC_TEXT_TRUNCATED | SYNC_TIME_UNKNOWN)) return false;
    record.kind = raw[2];
    record.flags = raw[3];
    record.seq = readLe(raw + 4, 8);
    record.wakeSeq = (uint32_t)readLe(raw + 12, 4);
    record.uptimeMs = (uint32_t)readLe(raw + 16, 4);
    record.payloadLen = (uint8_t)(length - 24);
    memcpy(record.payload, raw + 20, record.payloadLen);
    nextOffset = offset + length;
    return true;
}

bool syncAppend(SyncRtc &rtc, uint8_t kind, uint8_t flags, uint32_t wakeSeq,
                uint32_t uptimeMs, const uint8_t *payload, size_t length) {
    if (length > 96 || (length && !payload) || (flags & ~3u)) return false;
    uint8_t raw[120] = {};
    const uint16_t size = (uint16_t)(24 + length);
    writeLe(raw, size, 2);
    raw[2] = kind;
    raw[3] = flags;
    writeLe(raw + 4, rtc.nextSeq++, 8);
    writeLe(raw + 12, wakeSeq, 4);
    writeLe(raw + 16, uptimeMs, 4);
    if (length) memcpy(raw + 20, payload, length);
    writeLe(raw + size - 4, syncCrc32(raw, size - 4), 4);
    while (rtc.used + size > CAP) {
        const uint16_t tail = (rtc.head + CAP - rtc.used) % CAP;
        const uint16_t old = sizeAt(rtc, tail);
        if (old < 24 || old > 120 || old > rtc.used) {
            rtc.used = 0; // Corrupt old tail becomes a visible sequence gap.
            break;
        }
        rtc.used -= old;
    }
    for (uint16_t i = 0; i < size; ++i) put(rtc, rtc.head + i, raw[i]);
    rtc.head = (rtc.head + size) % CAP;
    rtc.used += size;
    seal(rtc);
    return true;
}

bool syncAppendEvent(SyncRtc &rtc, uint32_t wakeSeq, uint32_t uptimeMs,
                     uint16_t code, const int32_t *args, uint8_t argc) {
    if (code < 1 || code > 10 || argc > 4 || (argc && !args)) return false;
    uint8_t payload[20] = {};
    writeLe(payload, code, 2);
    payload[2] = argc;
    for (uint8_t i = 0; i < argc; ++i) writeLe(payload + 4 + 4*i, (uint32_t)args[i], 4);
    return syncAppend(rtc, SYNC_EVENT, 0, wakeSeq, uptimeMs, payload, 4 + 4*argc);
}

bool syncAppendResult(SyncRtc &rtc, uint32_t wakeSeq, uint32_t uptimeMs,
                      uint8_t result, uint8_t reasons, uint16_t error,
                      uint64_t batchSerial) {
    uint8_t payload[12] = {result, reasons};
    writeLe(payload + 2, error, 2);
    writeLe(payload + 4, batchSerial, 8);
    return syncAppend(rtc, SYNC_RESULT, 0, wakeSeq, uptimeMs, payload, sizeof(payload));
}

uint64_t syncEarliestSeq(const SyncRtc &rtc) {
    SyncRecord record;
    uint16_t next;
    return syncReadAt(rtc, 0, record, next) ? record.seq : rtc.nextSeq;
}

void syncCountDeepRendezvous(SyncRtc &rtc) {
    if (rtc.rounds < 15) ++rtc.rounds;
    if (rtc.rounds == 15) rtc.flags |= DUE;
    seal(rtc);
}

void syncAddReason(SyncRtc &rtc, uint8_t reasonBit) {
    rtc.reasonBits |= reasonBit & 0x1f;
    rtc.flags |= DUE;
    seal(rtc);
}

void syncComplete(SyncRtc &rtc, uint64_t serial, uint64_t throughSeq,
                  uint8_t completedReasons) {
    if (serial <= rtc.lastResetSerial) return;
    rtc.lastResetSerial = serial;
    if (throughSeq >= rtc.ackedSeq) rtc.ackedSeq = throughSeq;
    rtc.reasonBits &= ~completedReasons;
    rtc.rounds = rtc.retrySkip = rtc.failureCount = 0;
    rtc.flags = KNOWN | ((rtc.reasonBits & ~(1u << 3)) ? DUE : 0);
    seal(rtc);
}

void syncFail(SyncRtc &rtc) {
    if (rtc.failureCount < 255) ++rtc.failureCount;
    rtc.retrySkip = rtc.failureCount == 1 ? 1 : rtc.failureCount == 2 ? 2 : 4;
    rtc.flags |= DUE | RETRY;
    seal(rtc);
}

void syncConsumeSkip(SyncRtc &rtc) {
    if (rtc.retrySkip) --rtc.retrySkip;
    seal(rtc);
}
