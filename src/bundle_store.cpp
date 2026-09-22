#include "bundle_store.h"
#include "v2_state.h"
#include "dev_log.h"

#include <ArduinoJson.h>
#include <LittleFS.h>
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
};
#pragma pack(pop)

bool g_mounted = false;
bool g_configured = false;
BsProfile g_profile;
uint32_t g_seq = 0;
uint8_t g_slot = 0;
uint8_t g_metaNext = 0;
String g_lastError;

bool writeAll(File &f, const void *data, size_t len) {
    return f.write((const uint8_t *)data, len) == len;
}

bool slotCrc(const char *path, uint32_t &crc, uint32_t &len) {
    File f = LittleFS.open(path, "r");
    if (!f) return false;
    uint64_t total = f.size();
    if (total > BS_MAX_BUNDLE_BYTES + 4096) { f.close(); return false; }
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
    bool ok = f.read((uint8_t *)&r, sizeof(r)) == sizeof(r);
    f.close();
    return ok && r.magic == META_MAGIC;
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
    r.seq = ++g_seq;
    r.slot = slot;
    r.prevSlot = prev;
    r.configured = 1;
    r.fileLen = fileLen;
    r.fileCrc = fileCrc;
    strncpy(r.jobId, h.jobId, sizeof(r.jobId) - 1);
    strncpy(r.contextId, h.contextId, sizeof(r.contextId) - 1);
    strncpy(r.firmwareTarget, h.firmwareTarget, sizeof(r.firmwareTarget) - 1);
    strncpy(r.renderTarget, h.renderTarget, sizeof(r.renderTarget) - 1);
    // Alternating double copies: the older copy stays intact if power is lost.
    uint8_t target = g_metaNext;
    if (!writeMeta(target, r)) return false;
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
const char *bsLastError() { return g_lastError.c_str(); }
size_t bsFreeBytes() {
    if (!g_mounted) return 0;
    return LittleFS.totalBytes() - LittleFS.usedBytes();
}

bool bsInstall(const String &bundleJson, const char *expectedFirmware,
               const char *expectedRender, const char *newContextId, String &err) {
    if (!g_mounted) { err = "fs"; return false; }
    if (bundleJson.length() == 0 || bundleJson.length() > BS_MAX_BUNDLE_BYTES) {
        err = "size";
        return false;
    }
    // Narrow filter: the device compiles from sources and ignores the bridge's
    // compiled artifact/bindings, so they never allocate on the device.
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
    JsonDocument doc;
    DeserializationError de = deserializeJson(doc, bundleJson, DeserializationOption::Filter(filter));
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
    if (order.isNull() || templates.isNull() || order.size() == 0 ||
        order.size() > BS_MAX_TEMPLATES || order.size() != templates.size()) {
        err = "shape";
        return false;
    }
    const char *active = doc["profile"]["initial_active_id"] | "";
    int initial = 0;
    for (size_t i = 0; i < order.size(); i++) {
        if (strcmp(order[i] | "", active) == 0) initial = (int)i;
    }

    // Shape/target pre-check before any compile or write.
    uint32_t total = sizeof(SlotHeader);
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
    if (total > BS_MAX_BUNDLE_BYTES) { err = "size"; return false; }
    // Never delete the only valid copy: require room for a full extra slot.
    const size_t freeBytes = bsFreeBytes();
    DevLog.printf("[bundle] install total=%u free=%u fs_total=%u fs_used=%u\n",
                  (unsigned)total, (unsigned)freeBytes,
                  (unsigned)LittleFS.totalBytes(), (unsigned)LittleFS.usedBytes());
    if (g_configured && (uint64_t)freeBytes < (uint64_t)total + 4096) {
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
    uint8_t *blob = (uint8_t *)malloc(blobCap);
    if (!blob) { f.close(); err = "oom"; return false; }
    for (size_t i = 0; i < order.size() && ok; i++) {
        JsonObject tpl = templates[i].as<JsonObject>();
        String source;
        serializeJson(tpl["source"], source);
        String cerr;
        if (!tplCompile(source, compiled, cerr)) {
            err = "compile:" + cerr;
            ok = false;
            break;
        }
        size_t written = 0;
        if (!tplCtSerialize(compiled, blob, blobCap, written)) {
            err = "serialize";
            ok = false;
            break;
        }
        const char *id = order[i] | "";
        ok = f.write((const uint8_t *)id, BS_ID_LEN) == BS_ID_LEN;
        uint32_t len = (uint32_t)written;
        ok = ok && f.write((const uint8_t *)&len, 4) == 4;
        ok = ok && f.write(blob, written) == written;
    }
    free(blob);
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
    h.payloadCrc = v2Crc32((const uint8_t *)bundleJson.c_str(), bundleJson.length());
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

bool bsLoadCompiled(uint8_t index, CtTemplate &out, String &err) {
    if (!g_configured) { err = "unconfigured"; return false; }
    if (index >= g_profile.count) { err = "index"; return false; }
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
    return ok;
}

bool bsSetActive(uint8_t index, const char *newContextId, String &err) {
    if (!g_configured) { err = "unconfigured"; return false; }
    if (index >= g_profile.count) { err = "index"; return false; }
    SlotHeader h;
    if (!readSlotHeader(g_slot, h)) { err = "slot"; return false; }
    h.initial = index;
    strncpy(h.contextId, newContextId ? newContextId : "", sizeof(h.contextId) - 1);
    File f = LittleFS.open(SLOT_PATH[g_slot], "r+");
    if (!f) { err = "open"; return false; }
    bool ok = f.seek(0) && writeAll(f, &h, sizeof(h));
    f.close();
    if (!ok) { err = "write"; return false; }
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
    char id[BS_ID_LEN] = {0};
    uint32_t ctLen = 0, ctOffset = 0;
    return slotTemplateAt(g_slot, index, id, ctLen, ctOffset);
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
