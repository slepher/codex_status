#include "bundle_store.h"
#include "v2_state.h"
#include "dev_log.h"
#include "font_asset.h"

#include <ArduinoJson.h>
#include <LittleFS.h>
#include <mbedtls/base64.h>
#include <string.h>

// Bounded diagnostics: bundle-store decisions are rare and must be visible in
// the device log when a publish fails.
static void DevLogFromBundle(const char *message) {
    DevLog.printf("[bundle] %s\n", message);
}

namespace {

const char *SLOT_PATH[2] = {"/bundle/a.bin", "/bundle/b.bin"};
const char *META_PATH[2] = {"/bundle/m0.bin", "/bundle/m1.bin"};
const uint32_t SLOT_MAGIC = 0x31424143u;   // "CAB1"
const uint32_t META_MAGIC = 0x314D4253u;   // "SBM1"

#pragma pack(push, 1)
struct SlotHeader {
    uint32_t magic;
    uint16_t count;
    uint8_t initial;
    uint8_t flags;
    char jobId[BS_JOB_LEN];
    char contextId[BS_CTX_LEN];
    char firmwareTarget[32];
    char renderTarget[32];
    uint32_t payloadCrc;   // CRC of the bundle payload that produced this slot
    uint32_t reserved;
};

struct MetaRecord {
    uint32_t magic;
    uint32_t seq;
    uint8_t slot;
    uint8_t prevSlot;
    uint8_t configured;
    uint8_t reserved;
    uint32_t fileLen;
    uint32_t fileCrc;
    char jobId[BS_JOB_LEN];
    char contextId[BS_CTX_LEN];
    char firmwareTarget[32];
    char renderTarget[32];
    uint32_t recordCrc;
};
#pragma pack(pop)

bool g_mounted = false;
bool g_configured = false;
BsProfile g_profile;
uint32_t g_seq = 0;
uint8_t g_slot = 0;
uint8_t g_metaNext = 0;
String g_lastError;
uint8_t *g_fontBytes[8] = {};
uint8_t g_fontCount = 0;

bool decodeBundleFont(JsonObject entry, uint8_t *&bytes, size_t &len, String &err) {
    bytes = nullptr;
    len = 0;
    const char *name = entry["name"] | "";
    const char *id = entry["font_id"] | "";
    const char *encoded = entry["data"] | "";
    size_t chars = strlen(encoded);
    if (!*name || !*id || chars == 0 || chars > ((FONT_ASSET_MAX_BYTES + 2) / 3) * 4) {
        err = "font_shape"; return false;
    }
    size_t cap = chars / 4 * 3 + 3;
    bytes = (uint8_t *)malloc(cap);
    if (!bytes) { err = "font_oom"; return false; }
    if (mbedtls_base64_decode(bytes, cap, &len,
                              (const unsigned char *)encoded, chars) != 0) {
        free(bytes); bytes = nullptr; err = "font_base64"; return false;
    }
    FontAssetInfo info;
    if (!fontAssetValidate(bytes, len, info, err) ||
        !fontAssetMatchesTarget(info, err) ||
        strcmp(info.name, name) || strcmp(info.id, id) ||
        tplFontIndexByName(name) < tplFontFixedCount()) {
        free(bytes); bytes = nullptr;
        if (err.length() == 0) err = "font_identity";
        return false;
    }
    return true;
}

void clearActiveFonts() {
    tplFontClearAssets();
    for (uint8_t i = 0; i < g_fontCount; i++) free(g_fontBytes[i]);
    memset(g_fontBytes, 0, sizeof(g_fontBytes));
    g_fontCount = 0;
}

bool writeAll(File &f, const void *data, size_t len) {
    return f.write((const uint8_t *)data, len) == len;
}

bool slotCrc(const char *path, uint32_t &crc, uint32_t &len) {
    File f = LittleFS.open(path, "r");
    if (!f) return false;
    uint64_t total = f.size();
    if (total > BS_MAX_BUNDLE_BYTES * 2 + 4096) { f.close(); return false; }
    uint8_t buf[256];
    uint32_t c = 0xFFFFFFFFu;
    uint32_t n = 0;
    while (f.available()) {
        size_t got = f.read(buf, sizeof(buf));
        if (got == 0) break;
        n += (uint32_t)got;
        for (size_t i = 0; i < got; i++) {
            c ^= buf[i];
            for (int b = 0; b < 8; b++) {
                c = (c >> 1) ^ (0xEDB88320u & (uint32_t)(-(int32_t)(c & 1)));
            }
        }
    }
    f.close();
    crc = ~c;
    len = n;
    return true;
}

bool readSlotHeader(uint8_t slot, SlotHeader &h) {
    File f = LittleFS.open(SLOT_PATH[slot], "r");
    if (!f) return false;
    bool ok = f.read((uint8_t *)&h, sizeof(h)) == sizeof(h);
    f.close();
    return ok && h.magic == SLOT_MAGIC && h.count <= BS_MAX_TEMPLATES;
}

bool readMeta(uint8_t copy, MetaRecord &r) {
    File f = LittleFS.open(META_PATH[copy], "r");
    if (!f) return false;
    const size_t oldSize = offsetof(MetaRecord, recordCrc);
    const size_t size = f.size();
    memset(&r, 0, sizeof(r));
    bool ok = (size == sizeof(r) || size == oldSize) &&
              f.read((uint8_t *)&r, size) == size;
    f.close();
    if (size == sizeof(r)) ok = ok && r.recordCrc == v2Crc32((const uint8_t *)&r, oldSize);
    else ok = ok && !(r.reserved & 0x80); // only old records may lack a checksum
    return ok && r.magic == META_MAGIC && r.slot < 2;
}

bool writeMeta(uint8_t copy, const MetaRecord &r) {
    File f = LittleFS.open(META_PATH[copy], "w");
    if (!f) return false;
    bool ok = writeAll(f, &r, sizeof(r));
    f.close();
    return ok;
}

bool commitSlot(uint8_t slot, uint8_t prev, uint32_t fileLen, uint32_t fileCrc,
                const SlotHeader &h) {
    MetaRecord r;
    memset(&r, 0, sizeof(r));
    r.magic = META_MAGIC;
    r.seq = g_seq + 1;
    r.slot = slot;
    r.prevSlot = prev;
    r.configured = 1;
    r.reserved = 0x80 | h.initial; // independent activation, never patch the slot
    r.fileLen = fileLen;
    r.fileCrc = fileCrc;
    strncpy(r.jobId, h.jobId, sizeof(r.jobId) - 1);
    strncpy(r.contextId, h.contextId, sizeof(r.contextId) - 1);
    strncpy(r.firmwareTarget, h.firmwareTarget, sizeof(r.firmwareTarget) - 1);
    strncpy(r.renderTarget, h.renderTarget, sizeof(r.renderTarget) - 1);
    r.recordCrc = v2Crc32((const uint8_t *)&r, offsetof(MetaRecord, recordCrc));
    // Alternating double copies: the older copy stays intact if power is lost.
    uint8_t target = g_metaNext;
    if (!writeMeta(target, r)) return false;
    MetaRecord check;
    if (!readMeta(target, check) || memcmp(&r, &check, sizeof(r))) return false;
    g_metaNext = (uint8_t)(target ^ 1);
    g_seq = r.seq;
    g_slot = slot;
    return true;
}

void fillProfile(const SlotHeader &h) {
    memset(&g_profile, 0, sizeof(g_profile));
    strncpy(g_profile.jobId, h.jobId, sizeof(g_profile.jobId) - 1);
    strncpy(g_profile.contextId, h.contextId, sizeof(g_profile.contextId) - 1);
    strncpy(g_profile.firmwareTarget, h.firmwareTarget, sizeof(g_profile.firmwareTarget) - 1);
    strncpy(g_profile.renderTarget, h.renderTarget, sizeof(g_profile.renderTarget) - 1);
    g_profile.initial = h.initial;
    g_profile.count = 0;
}

// Read the per-template table from the active slot (id + compiled blob).
bool slotTemplateAt(uint8_t slot, uint8_t index, char *idOut, uint32_t &ctLen,
                    uint32_t &ctOffset) {
    File f = LittleFS.open(SLOT_PATH[slot], "r");
    if (!f) return false;
    SlotHeader h;
    if (f.read((uint8_t *)&h, sizeof(h)) != sizeof(h) || h.magic != SLOT_MAGIC) {
        f.close();
        return false;
    }
    if (index >= h.count) { f.close(); return false; }
    uint32_t offset = sizeof(SlotHeader);
    for (uint8_t i = 0; i < h.count; i++) {
        char id[BS_ID_LEN] = {0};
        uint32_t len = 0;
        if (f.read((uint8_t *)id, BS_ID_LEN) != BS_ID_LEN) { f.close(); return false; }
        if (f.read((uint8_t *)&len, 4) != 4) { f.close(); return false; }
        offset += BS_ID_LEN + 4;
        if (i == index) {
            if (idOut) strncpy(idOut, id, BS_ID_LEN - 1);
            ctLen = len;
            ctOffset = offset;
            f.close();
            return len > 12 && len <= 64 * 1024;
        }
        offset += len;
        if (!f.seek(offset)) { f.close(); return false; }
    }
    f.close();
    return false;
}

}  // namespace

bool bsBegin() {
    if (!g_mounted) {
        // Same base path/label as the template store: a repeat begin() with the
        // default label would clobber the mount label and make totalBytes()/used
        // report 0 (free-space checks then cannot work).
        g_mounted = LittleFS.begin(true, "/littlefs", 10, "storage");
        if (!g_mounted) {
            // Another module may already own the mount; probe instead of failing.
            g_mounted = LittleFS.exists("/tpl") || LittleFS.exists("/");
        }
    }
    if (!g_mounted) {
        g_lastError = "fs";
        DevLogFromBundle("fs mount failed");
        return false;
    }
    if (!LittleFS.exists("/bundle")) {
        bool made = LittleFS.mkdir("/bundle");
        DevLogFromBundle(made ? "created /bundle" : "mkdir /bundle failed");
    }
    DevLogFromBundle("fs ready");
    g_configured = false;
    g_seq = 0;
    g_slot = 0;
    g_metaNext = 0;
    MetaRecord best;
    memset(&best, 0, sizeof(best));
    for (uint8_t copy = 0; copy < 2; copy++) {
        MetaRecord r;
        if (!readMeta(copy, r) || !r.configured) continue;
        if (g_seq == 0 || r.seq > g_seq) {
            // Verify the referenced slot before trusting the record.
            uint32_t crc = 0, len = 0;
            if (!slotCrc(SLOT_PATH[r.slot & 1], crc, len)) continue;
            if (len != r.fileLen || crc != r.fileCrc) continue;
            g_seq = r.seq;
            g_slot = r.slot & 1;
            g_metaNext = (uint8_t)(copy ^ 1);
            best = r;
            g_configured = true;
        }
    }
    if (g_configured) {
        SlotHeader h;
        if (!readSlotHeader(g_slot, h)) {
            g_configured = false;
            g_lastError = "slot";
            return false;
        }
        fillProfile(h);
        if (best.reserved & 0x80) {
            g_profile.initial = best.reserved & 0x7f;
            strncpy(g_profile.contextId, best.contextId, sizeof(g_profile.contextId) - 1);
        }
        g_profile.bundleCrc = best.fileCrc;
        // Template order lives inside the slot file; read it once here.
        File f = LittleFS.open(SLOT_PATH[g_slot], "r");
        if (!f) { g_configured = false; return false; }
        f.read((uint8_t *)&h, sizeof(h));
        for (uint8_t i = 0; i < h.count; i++) {
            char id[BS_ID_LEN] = {0};
            uint32_t len = 0;
            if (f.read((uint8_t *)id, BS_ID_LEN) != BS_ID_LEN) break;
            if (f.read((uint8_t *)&len, 4) != 4) break;
            strncpy(g_profile.ids[g_profile.count], id, BS_ID_LEN - 1);
            g_profile.count++;
            if (!f.seek(f.position() + len)) break;
        }
        f.close();
        if (g_profile.count == 0) g_configured = false;
        if (g_profile.initial >= g_profile.count) g_profile.initial = 0;
    }
    return g_configured;
}

bool bsConfigured() { return g_configured; }

bool bsProfile(BsProfile &out) {
    if (!g_configured) return false;
    out = g_profile;
    return true;
}

uint32_t bsCommitSeq() { return g_seq; }
bool bsActiveJobPayload(const char *jobId, uint32_t &payloadCrc) {
    if (!g_configured || !jobId || !*jobId) return false;
    SlotHeader h;
    if (!readSlotHeader(g_slot, h) || strcmp(h.jobId, jobId)) return false;
    payloadCrc = h.payloadCrc;
    return true;
}
const char *bsLastError() { return g_lastError.c_str(); }
size_t bsFreeBytes() {
    if (!g_mounted) return 0;
    const size_t free = LittleFS.totalBytes() - LittleFS.usedBytes();
    return free > 40960 ? free - 40960 : 0; // sync-v1 frozen blob and A/B metadata reserve
}

namespace {

struct StringBundleInput {
    const String &value;
    size_t offset = 0;

    size_t size() const { return value.length(); }
    bool rewind() { offset = 0; return true; }
    size_t read(uint8_t *out, size_t len) {
        size_t remaining = value.length() - offset;
        if (len > remaining) len = remaining;
        if (len) memcpy(out, value.c_str() + offset, len);
        offset += len;
        return len;
    }
};

struct FileBundleInput {
    File &file;

    size_t size() const { return file.size(); }
    bool rewind() { return file.seek(0); }
    size_t read(uint8_t *out, size_t len) { return file.read(out, len); }
};

static DeserializationError parseBundle(StringBundleInput &source,
                                        JsonDocument &doc, JsonDocument &filter) {
    return deserializeJson(doc, source.value, DeserializationOption::Filter(filter));
}

static DeserializationError parseBundle(FileBundleInput &source,
                                        JsonDocument &doc, JsonDocument &filter) {
    if (!source.rewind()) return DeserializationError::EmptyInput;
#ifdef ARDUINO
    return deserializeJson(doc, source.file, DeserializationOption::Filter(filter));
#else
    String body;
    uint8_t buf[256];
    while (source.file.available()) {
        size_t got = source.file.read(buf, sizeof(buf));
        if (!got) break;
        if (!body.concat((const char *)buf, got)) return DeserializationError::NoMemory;
    }
    return deserializeJson(doc, body, DeserializationOption::Filter(filter));
#endif
}

static uint32_t crcUpdate(uint32_t crc, const uint8_t *data, size_t len) {
    for (size_t i = 0; i < len; i++) {
        crc ^= data[i];
        for (int b = 0; b < 8; b++) {
            crc = (crc >> 1) ^ (0xEDB88320u & (uint32_t)(-(int32_t)(crc & 1)));
        }
    }
    return crc;
}

template <typename Input>
static bool appendBundle(Input &source, File &out, size_t len, uint32_t &crc) {
    if (!source.rewind()) return false;
    uint8_t buf[256];
    uint32_t state = 0xFFFFFFFFu;
    size_t copied = 0;
    while (copied < len) {
        size_t want = len - copied;
        if (want > sizeof(buf)) want = sizeof(buf);
        size_t got = source.read(buf, want);
        if (!got || !writeAll(out, buf, got)) return false;
        state = crcUpdate(state, buf, got);
        copied += got;
    }
    crc = ~state;
    return copied == len;
}

template <typename Input>
static bool bsInstallSource(Input &source, uint32_t expectedPayloadCrc,
                            const char *expectedFirmware, const char *expectedRender,
                            const char *newContextId, String &err) {
    const size_t bundleLength = source.size();
    if (!g_mounted) { err = "fs"; return false; }
    if (bundleLength == 0 || bundleLength > BS_MAX_BUNDLE_BYTES) {
        err = "size";
        return false;
    }
    // Keep only the fields used during installation in RAM. The exact complete
    // Bundle is retained in the slot, including the Bridge-only binding contract.
    JsonDocument filter;
    filter["job_id"] = true;
    filter["firmware_target"] = true;
    filter["render_target"] = true;
    filter["compiler_abi"] = true;
    filter["crc"] = true;
    filter["profile"]["template_ids"] = true;
    filter["profile"]["initial_active_id"] = true;
    filter["templates"][0]["key"]["template_id"] = true;
    filter["templates"][0]["key"]["render_target"] = true;
    filter["templates"][0]["source"] = true;
    filter["fonts"][0]["name"] = true;
    filter["fonts"][0]["font_id"] = true;
    filter["fonts"][0]["data"] = true;
    JsonDocument doc;
    DeserializationError de = parseBundle(source, doc, filter);
    if (de) { err = "json"; return false; }

    const char *fw = doc["firmware_target"] | "";
    const char *rt = doc["render_target"] | "";
    if (expectedFirmware && *expectedFirmware && strcmp(fw, expectedFirmware) != 0) {
        err = "target";
        return false;
    }
    if (expectedRender && *expectedRender && strcmp(rt, expectedRender) != 0) {
        err = "target";
        return false;
    }
    if ((doc["compiler_abi"] | 0) != CT_ABI) { err = "abi"; return false; }
    JsonArray order = doc["profile"]["template_ids"].as<JsonArray>();
    JsonArray templates = doc["templates"].as<JsonArray>();
    JsonArray fonts = doc["fonts"].as<JsonArray>();
    if (order.isNull() || templates.isNull() || order.size() == 0 ||
        order.size() > BS_MAX_TEMPLATES || order.size() != templates.size()) {
        err = "shape";
        return false;
    }
    if (!fonts.isNull() && fonts.size() > 8) { err = "font_count"; return false; }
    for (size_t i = 0; !fonts.isNull() && i < fonts.size(); i++) {
        const char *name = fonts[i]["name"] | "";
        for (size_t j = 0; j < i; j++) {
            if (!strcmp(name, fonts[j]["name"] | "")) {
                err = "font_duplicate"; return false;
            }
        }
        uint8_t *bytes = nullptr;
        size_t len = 0;
        if (!decodeBundleFont(fonts[i].as<JsonObject>(), bytes, len, err)) return false;
        free(bytes);
    }
    const char *active = doc["profile"]["initial_active_id"] | "";
    int initial = 0;
    for (size_t i = 0; i < order.size(); i++) {
        if (strcmp(order[i] | "", active) == 0) initial = (int)i;
    }

    // Shape/target pre-check before any compile or write.
    uint32_t total = sizeof(SlotHeader) + bundleLength;
    for (size_t i = 0; i < order.size(); i++) {
        const char *id = order[i] | "";
        if (!strlen(id) || strlen(id) >= BS_ID_LEN) { err = "id"; return false; }
        JsonObject tpl = templates[i].as<JsonObject>();
        if (tpl.isNull() || tpl["source"].isNull()) { err = "template"; return false; }
        if (strcmp(tpl["key"]["template_id"] | "", id) != 0) { err = "order"; return false; }
        if (strcmp(tpl["key"]["render_target"] | "", rt) != 0) { err = "target"; return false; }
        // 12-byte serialize header + fixed record, matching tplCtSerialize.
        total += BS_ID_LEN + 4 + 12 + (uint32_t)tplCtSize();
    }
    if (total > BS_MAX_BUNDLE_BYTES * 2) { err = "size"; return false; }
    // Never delete the only valid copy: require room for a full extra slot.
    const size_t freeBytes = bsFreeBytes();
    DevLog.printf("[bundle] install total=%u free=%u fs_total=%u fs_used=%u\n",
                  (unsigned)total, (unsigned)freeBytes,
                  (unsigned)LittleFS.totalBytes(), (unsigned)LittleFS.usedBytes());
    if ((uint64_t)freeBytes < (uint64_t)total + 4096) {
        err = "space";
        return false;
    }

    uint8_t prev = g_slot;
    uint8_t slot = g_configured ? (uint8_t)(prev ^ 1) : 0;

    // Write the inactive slot completely: header placeholder, then each
    // template compiled on the fly. A failure leaves an uncommitted partial
    // file that the commit record never references.
    File f = LittleFS.open(SLOT_PATH[slot], "w");
    if (!f) { err = "open"; return false; }
    bool ok = true;
    {
        SlotHeader blank;
        memset(&blank, 0, sizeof(blank));
        ok = writeAll(f, &blank, sizeof(blank));
    }
    // ~9 KB compiled record: never on the 8 KB loop-task stack.
    static CtTemplate compiled;
    const size_t blobCap = tplCtSize() + 16;
    for (size_t i = 0; i < order.size() && ok; i++) {
        JsonObject tpl = templates[i].as<JsonObject>();
        String source;
        serializeJson(tpl["source"], source);
        String cerr;
        size_t written = 0;
        // The compiled hex is a cache and can be larger than the free heap on
        // BLE-enabled 1.54. Rebuild from the retained source instead.
        if (!tplCompile(source, compiled, cerr)) {
            err = "compile:" + cerr; ok = false; break;
        }
        // Keep this allocation out of the source compiler's peak heap usage.
        uint8_t *blob = (uint8_t *)malloc(blobCap);
        if (!blob) { err = "oom"; ok = false; break; }
        if (!tplCtSerialize(compiled, blob, blobCap, written)) {
            free(blob);
            err = "compile:" + cerr; ok = false; break;
        }
        char id[BS_ID_LEN] = {};
        strncpy(id, order[i] | "", sizeof(id) - 1);
        ok = f.write((const uint8_t *)id, sizeof(id)) == sizeof(id);
        uint32_t len = (uint32_t)written;
        ok = ok && f.write((const uint8_t *)&len, 4) == 4;
        ok = ok && f.write(blob, written) == written;
        free(blob);
    }
    // Retain the exact frozen contract, including source/bindings/resources and
    // the Bridge artifact. The per-template compiled table is only a cache.
    uint32_t payloadCrc = 0;
    if (ok) ok = appendBundle(source, f, bundleLength, payloadCrc);
    if (ok && payloadCrc != expectedPayloadCrc) { err = "crc"; ok = false; }
    f.close();
    if (!ok) {
        LittleFS.remove(SLOT_PATH[slot]);
        return false;
    }

    // Read back + CRC before committing.
    uint32_t fileCrc = 0, fileLen = 0;
    if (!slotCrc(SLOT_PATH[slot], fileCrc, fileLen)) { err = "readback"; return false; }
    if (fileLen != total) { err = "length"; return false; }

    SlotHeader h;
    memset(&h, 0, sizeof(h));
    h.magic = SLOT_MAGIC;
    h.count = (uint16_t)order.size();
    h.initial = (uint8_t)initial;
    h.flags = 1; // complete source Bundle appended after the compiled table
    h.reserved = bundleLength;
    h.payloadCrc = payloadCrc;
    strncpy(h.jobId, doc["job_id"] | "", sizeof(h.jobId) - 1);
    strncpy(h.contextId, newContextId ? newContextId : "", sizeof(h.contextId) - 1);
    strncpy(h.firmwareTarget, fw, sizeof(h.firmwareTarget) - 1);
    strncpy(h.renderTarget, rt, sizeof(h.renderTarget) - 1);
    // Header is written first in the slot; patch it in place after the payload
    // is complete so a torn write can never look committed.
    File hf = LittleFS.open(SLOT_PATH[slot], "r+");
    if (!hf) { err = "header"; return false; }
    bool headerOk = hf.seek(0) && writeAll(hf, &h, sizeof(h));
    hf.close();
    if (!headerOk) { err = "header"; return false; }

    // Recompute the CRC with the final header, then write the commit record.
    if (!slotCrc(SLOT_PATH[slot], fileCrc, fileLen)) { err = "readback2"; return false; }
    if (!commitSlot(slot, prev, fileLen, fileCrc, h)) { err = "commit"; return false; }

    // Only now switch the runtime view.
    SlotHeader check;
    if (!readSlotHeader(slot, check)) { err = "verify"; return false; }
    g_slot = slot;
    g_configured = true;
    fillProfile(check);
    g_profile.bundleCrc = fileCrc;
    for (size_t i = 0; i < order.size(); i++) {
        strncpy(g_profile.ids[i], order[i] | "", BS_ID_LEN - 1);
    }
    g_profile.count = (uint8_t)order.size();
    return true;
}

}  // namespace

bool bsInstall(const String &bundleJson, const char *expectedFirmware,
               const char *expectedRender, const char *newContextId, String &err) {
    StringBundleInput source{bundleJson};
    return bsInstallSource(source, v2Crc32((const uint8_t *)bundleJson.c_str(), bundleJson.length()),
                           expectedFirmware, expectedRender, newContextId, err);
}

bool bsInstall(File &bundleFile, uint32_t bundleLength, uint32_t bundleCrc,
               const char *expectedFirmware, const char *expectedRender,
               const char *newContextId, String &err) {
    FileBundleInput source{bundleFile};
    if (source.size() != bundleLength) { err = "length"; return false; }
    return bsInstallSource(source, bundleCrc, expectedFirmware, expectedRender,
                           newContextId, err);
}

static bool loadActiveFonts(String &err) {
    SlotHeader h;
    if (!readSlotHeader(g_slot, h) || !(h.flags & 1) ||
        h.reserved == 0 || h.reserved > BS_MAX_BUNDLE_BYTES) {
        err = "font_source"; return false;
    }
    File f = LittleFS.open(SLOT_PATH[g_slot], "r");
    if (!f || f.size() < h.reserved + sizeof(h) ||
        !f.seek(f.size() - h.reserved)) {
        err = "font_read"; return false;
    }
    JsonDocument filter, doc;
    filter["fonts"][0]["name"] = true;
    filter["fonts"][0]["font_id"] = true;
    filter["fonts"][0]["data"] = true;
#ifdef ARDUINO
    DeserializationError de = deserializeJson(doc, f, DeserializationOption::Filter(filter));
#else
    String payload;
    while (f.available()) payload += (char)f.read();
    DeserializationError de = deserializeJson(doc, payload, DeserializationOption::Filter(filter));
#endif
    f.close();
    if (de) { err = "font_json"; return false; }
    JsonArray fonts = doc["fonts"].as<JsonArray>();
    if (!fonts.isNull() && fonts.size() > 8) { err = "font_count"; return false; }
    uint8_t *next[8] = {};
    size_t lengths[8] = {};
    uint8_t count = fonts.isNull() ? 0 : (uint8_t)fonts.size();
    for (uint8_t i = 0; i < count; i++) {
        if (!decodeBundleFont(fonts[i].as<JsonObject>(), next[i], lengths[i], err)) {
            for (uint8_t j = 0; j < i; j++) free(next[j]);
            return false;
        }
    }
    clearActiveFonts();
    for (uint8_t i = 0; i < count; i++) {
        if (!tplFontBindAsset(next[i], lengths[i])) {
            err = "font_bind";
            for (uint8_t j = i; j < count; j++) free(next[j]);
            clearActiveFonts();
            return false;
        }
        g_fontBytes[g_fontCount++] = next[i];
    }
    return true;
}

bool bsLoadCompiled(uint8_t index, CtTemplate &out, String &err) {
    if (!g_configured) { err = "unconfigured"; return false; }
    if (index >= g_profile.count) { err = "index"; return false; }
    if (!loadActiveFonts(err)) return false;
    char id[BS_ID_LEN] = {0};
    uint32_t ctLen = 0, ctOffset = 0;
    if (!slotTemplateAt(g_slot, index, id, ctLen, ctOffset)) { err = "slot"; return false; }
    if (ctLen > 64 * 1024) { err = "size"; return false; }
    uint8_t *buf = (uint8_t *)malloc(ctLen);
    if (!buf) { err = "oom"; return false; }
    File f = LittleFS.open(SLOT_PATH[g_slot], "r");
    bool readOk = f && f.seek(ctOffset) && f.read(buf, ctLen) == ctLen;
    if (f) f.close();
    bool ok = readOk && tplCtDeserialize(buf, ctLen, out, err);
    if (!readOk) err = "read";
    free(buf);
    if (!ok && bsHasSource(index)) {
        // ABI/cache recovery uses the source retained in this same slot. The
        // complete payload CRC is checked independently of the compiled cache.
        SlotHeader h;
        if (!readSlotHeader(g_slot, h)) return false;
        File sourceFile = LittleFS.open(SLOT_PATH[g_slot], "r");
        if (!sourceFile || sourceFile.size() < h.reserved + sizeof(h) ||
            !sourceFile.seek(sourceFile.size() - h.reserved)) return false;
        String payload;
        payload.reserve(h.reserved);
        while (sourceFile.available()) payload += (char)sourceFile.read();
        sourceFile.close();
        if (v2Crc32((const uint8_t *)payload.c_str(), payload.length()) != h.payloadCrc) {
            err = "source_crc"; return false;
        }
        JsonDocument filter, doc;
        filter["templates"][0]["source"] = true;
        if (deserializeJson(doc, payload, DeserializationOption::Filter(filter))) {
            err = "source_json"; return false;
        }
        String source;
        serializeJson(doc["templates"][index]["source"], source);
        ok = tplCompile(source, out, err) && !strcmp(out.id, g_profile.ids[index]);
    }
    return ok;
}

bool bsSetActive(uint8_t index, const char *newContextId, String &err) {
    if (!g_configured) { err = "unconfigured"; return false; }
    if (index >= g_profile.count) { err = "index"; return false; }
    SlotHeader h;
    if (!readSlotHeader(g_slot, h)) { err = "slot"; return false; }
    h.initial = index;
    strncpy(h.contextId, newContextId ? newContextId : "", sizeof(h.contextId) - 1);
    uint32_t crc = 0, len = 0;
    if (!slotCrc(SLOT_PATH[g_slot], crc, len)) { err = "crc"; return false; }
    if (!commitSlot(g_slot, g_slot, len, crc, h)) { err = "commit"; return false; }
    g_profile.initial = index;
    strncpy(g_profile.contextId, h.contextId, sizeof(g_profile.contextId) - 1);
    g_profile.bundleCrc = crc;
    return true;
}

bool bsHasSource(uint8_t index) {
    if (!g_configured || index >= g_profile.count) return false;
    SlotHeader h;
    return readSlotHeader(g_slot, h) && (h.flags & 1) && h.reserved > 0 &&
           h.reserved <= BS_MAX_BUNDLE_BYTES;
}

bool bsRecoveryDigest(String &out) {
    if (!g_configured) { out = "{\"configured\":false}"; return false; }
    out = "{\"configured\":true,\"active_template_id\":\"";
    out += g_profile.ids[g_profile.initial];
    out += "\",\"template_ids\":[";
    for (uint8_t i = 0; i < g_profile.count; i++) {
        if (i) out += ",";
        out += "\"";
        out += g_profile.ids[i];
        out += "\"";
    }
    out += "],\"context_id\":\"";
    out += g_profile.contextId;
    out += "\",\"job_id\":\"";
    out += g_profile.jobId;
    out += "\",\"commit_seq\":";
    out += String(g_seq);
    out += ",\"sources\":[";
    for (uint8_t i = 0; i < g_profile.count; i++) {
        if (i) out += ",";
        out += bsHasSource(i) ? "true" : "false";
    }
    out += "]}";
    return true;
}
