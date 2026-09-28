#include "v2_sync_store.h"
#include "v2_sync.h"

#include <LittleFS.h>
#include <stdlib.h>
#include <string.h>

namespace {
constexpr uint32_t MAGIC = 0x31534d53u; // SMS1
constexpr size_t MAX_BLOB = 16384;
constexpr size_t RESERVE = 40960;
const char *BLOB[2] = {"/sync/b0.bin", "/sync/b1.bin"};
const char *META[2] = {"/sync/m0.bin", "/sync/m1.bin"};

struct Meta {
    uint32_t magic = MAGIC;
    uint32_t revision = 0;
    uint64_t syncSerial = 0;
    uint64_t highestClientSerial = 0;
    uint8_t active = 0;
    uint8_t blob = 0;
    uint8_t receiptValid = 0;
    uint8_t reserved = 0;
    uint32_t ackedOffset = 0;
    SyncStoreBatch current;
    SyncStoreBatch receipt;
    uint32_t crc = 0;
};
static_assert(sizeof(Meta) <= 2048, "sync metadata exceeds 2048 bytes");
Meta state;
uint8_t currentCopy = 0;
bool mounted = false;
bool lost = false;

bool readMeta(uint8_t copy, Meta &out) {
    File f = LittleFS.open(META[copy], "r");
    if (!f || f.size() != sizeof(out)) { if (f) f.close(); return false; }
    bool ok = f.read((uint8_t *)&out, sizeof(out)) == sizeof(out);
    f.close();
    return ok && out.magic == MAGIC && out.blob < 2 && out.active <= 1 &&
           out.receiptValid <= 1 && out.crc ==
               syncCrc32((const uint8_t *)&out, offsetof(Meta, crc));
}

bool writeMeta(Meta next) {
    next.magic = MAGIC;
    next.revision = state.revision + 1;
    next.crc = syncCrc32((const uint8_t *)&next, offsetof(Meta, crc));
    const uint8_t target = currentCopy ^ 1;
    File f = LittleFS.open(META[target], "w");
    if (!f) return false;
    bool ok = f.write((const uint8_t *)&next, sizeof(next)) == sizeof(next);
    f.close();
    Meta verified;
    if (!ok || !readMeta(target, verified) || verified.revision != next.revision)
        return false;
    state = verified;
    currentCopy = target;
    return true;
}

bool readBlob(uint8_t slot, uint8_t *out, size_t &length) {
    File f = LittleFS.open(BLOB[slot], "r");
    if (!f || f.size() > MAX_BLOB) { if (f) f.close(); return false; }
    length = f.size();
    bool ok = f.read(out, length) == length;
    f.close();
    return ok;
}

bool verifyBlob(const SyncStoreBatch &batch, uint8_t slot) {
    if (!batch.bytes || batch.bytes > MAX_BLOB) return false;
    uint8_t *contents = (uint8_t *)malloc(batch.bytes);
    if (!contents) return false;
    size_t length = 0;
    bool ok = readBlob(slot, contents, length) && length == batch.bytes;
    if (ok) {
        uint8_t digest[32];
        syncSha256(contents, length, digest);
        ok = !memcmp(digest, batch.sha256, sizeof(digest));
    }
    free(contents);
    return ok;
}
}

bool syncStoreBegin() {
    mounted = false;
    lost = false;
    if (!LittleFS.exists("/sync") && !LittleFS.mkdir("/sync")) return false;
    Meta a, b;
    bool va = readMeta(0, a), vb = readMeta(1, b);
    if (va || vb) {
        currentCopy = vb && (!va || b.revision > a.revision) ? 1 : 0;
        state = currentCopy ? b : a;
    } else {
        state = Meta{};
        currentCopy = 1;
        if (!writeMeta(state)) return false;
    }
    if (state.active && (!verifyBlob(state.current, state.blob) ||
                         state.ackedOffset > state.current.bytes)) lost = true;
    if (state.active) LittleFS.remove(BLOB[state.blob ^ 1]);
    else { LittleFS.remove(BLOB[0]); LittleFS.remove(BLOB[1]); }
    mounted = true;
    return true;
}

bool syncStoreAvailable() { return mounted; }
bool syncStoreLost() { return lost; }
bool syncStoreActive(SyncStoreBatch &out) {
    if (!mounted || !state.active) return false;
    out = state.current;
    return true;
}
bool syncStoreReceipt(SyncStoreBatch &out) {
    if (!mounted || !state.receiptValid) return false;
    out = state.receipt;
    return true;
}
uint64_t syncStoreHighestClientSerial() { return state.highestClientSerial; }
bool syncStoreChangeOwner() {
    if (!mounted) return false;
    const bool oldActive = state.active;
    const uint8_t oldSlot = state.blob;
    Meta next = state;
    next.active = 0;
    next.receiptValid = 0;
    next.highestClientSerial = 0;
    next.ackedOffset = 0;
    if (!writeMeta(next)) return false;
    if (oldActive) LittleFS.remove(BLOB[oldSlot]);
    lost = false;
    return true;
}
bool syncStoreAdvanceSerial(uint64_t &serial) {
    if (!mounted || state.active || state.syncSerial == UINT64_MAX) return false;
    Meta next = state;
    serial = ++next.syncSerial;
    return writeMeta(next);
}

bool syncStoreFreeze(const uint8_t *bytes, size_t length, const SyncStoreBatch &batch) {
    if (!mounted || lost || state.active || !bytes || !length || length > MAX_BLOB ||
        batch.bytes != length || batch.deviceSerial != state.syncSerial ||
        batch.clientSerial <= state.highestClientSerial ||
        !batch.batchId[0] || !batch.owner[0]) return false;
    uint8_t digest[32];
    syncSha256(bytes, length, digest);
    if (memcmp(digest, batch.sha256, 32)) return false;
    // Other users of LittleFS leave 40 KiB available; this path may spend it.
    if (LittleFS.totalBytes() < RESERVE ||
        LittleFS.totalBytes() - LittleFS.usedBytes() < length + 4096) return false;
    const uint8_t slot = state.blob ^ 1;
    File f = LittleFS.open(BLOB[slot], "w");
    if (!f) return false;
    bool wrote = f.write(bytes, length) == length;
    f.close();
    if (!wrote || !verifyBlob(batch, slot)) return false;
    Meta next = state;
    next.current = batch;
    next.blob = slot;
    next.active = 1;
    next.highestClientSerial = batch.clientSerial;
    next.ackedOffset = 0;
    if (!writeMeta(next)) return false;
    return true;
}

bool syncStorePage(uint32_t offset, uint16_t limit, uint8_t *out, size_t &read) {
    read = 0;
    if (!mounted || lost || !state.active || !out || !limit || limit > 1024 ||
        offset > state.current.bytes) return false;
    File f = LittleFS.open(BLOB[state.blob], "r");
    if (!f || f.size() != state.current.bytes || !f.seek(offset)) {
        if (f) f.close();
        return false;
    }
    read = state.current.bytes - offset;
    if (read > limit) read = limit;
    bool ok = f.read(out, read) == read;
    f.close();
    return ok;
}

bool syncStoreAck(uint32_t offset, const uint8_t prefixSha256[32]) {
    if (!mounted || lost || !state.active || offset > state.current.bytes || !prefixSha256)
        return false;
    uint8_t *contents = (uint8_t *)malloc(offset ? offset : 1);
    if (!contents) return false;
    File f = LittleFS.open(BLOB[state.blob], "r");
    bool ok = f && f.read(contents, offset) == offset;
    if (f) f.close();
    if (ok) {
        uint8_t digest[32];
        syncSha256(contents, offset, digest);
        ok = !memcmp(digest, prefixSha256, 32);
    }
    free(contents);
    if (ok && offset > state.ackedOffset) {
        Meta next = state;
        next.ackedOffset = offset;
        ok = writeMeta(next);
    }
    return ok;
}

bool syncStoreComplete(const SyncStoreBatch &expected) {
    if (!mounted || lost || !state.active || strcmp(expected.batchId, state.current.batchId) ||
        expected.bytes != state.current.bytes ||
        memcmp(expected.sha256, state.current.sha256, 32)) return false;
    Meta next = state;
    next.receipt = state.current;
    next.receiptValid = 1;
    next.active = 0;
    next.ackedOffset = 0;
    if (!writeMeta(next)) return false;
    LittleFS.remove(BLOB[state.blob]); // Receipt is already durable.
    return true;
}

uint32_t syncStoreAckedOffset() { return state.ackedOffset; }
