#include "v2_sync_protocol.h"
#include <LittleFS.h>
#include <filesystem>
#include <fstream>
#include <string>

// One Fake ROM process owns one LittleFS mount. This adapter supplies virtual
// RTC persistence and sampled snapshot data; protocol decisions live in the
// same C++ module called by the production HTTP handlers.
namespace {
struct SimSync {
    SyncRtc rtc{};
    std::filesystem::path rtcPath, configPath;
    std::string mac, owner;
    uint64_t seed = 0;
    uint32_t wakeSeq = 0;
    bool enabled = false;
};

bool saveRtc(const SimSync &sim) {
    std::ofstream out(sim.rtcPath, std::ios::binary | std::ios::trunc);
    if (!out.write((const char *)&sim.rtc, sizeof(sim.rtc))) return false;
    out.flush();
    return (bool)out;
}

bool saveConfig(const SimSync &sim) {
    JsonDocument doc;
    doc["owner"] = sim.owner;
    doc["enabled"] = sim.enabled;
    std::ofstream out(sim.configPath, std::ios::binary | std::ios::trunc);
    if (!out) return false;
    serializeJson(doc, out);
    out.flush();
    return (bool)out;
}

int copyJson(const JsonDocument &doc, char *out, int capacity) {
    if (!out || capacity < 2 || measureJson(doc) >= (size_t)capacity) return -1;
    serializeJson(doc, out, capacity);
    return (int)measureJson(doc);
}
}

extern "C" {
void *codex_sim_sync_new(const char *dataDir, const char *mac, const char *target,
                         unsigned long long seed, unsigned long long bootId,
                         const char *wakeCause) {
    if (!dataDir || !mac || !target || !wakeCause) return nullptr;
    LittleFS.useDirectory(dataDir);
    if (!syncStoreBegin()) return nullptr;
    auto *sim = new SimSync();
    sim->rtcPath = std::filesystem::path(dataDir) / "sim-sync-rtc.bin";
    sim->configPath = std::filesystem::path(dataDir) / "sim-sync-config.json";
    sim->mac = mac;
    sim->seed = seed;
    uint8_t generation[16] = {};
    uint64_t v = seed ^ (bootId << 1);
    for (int i = 0; i < 16; ++i) {
        v ^= v << 13; v ^= v >> 7; v ^= v << 17;
        generation[i] = (uint8_t)v;
    }
    if (strcmp(wakeCause, "cold")) {
        std::ifstream file(sim->rtcPath, std::ios::binary);
        if (!file.read((char *)&sim->rtc, sizeof(sim->rtc)) ||
            file.peek() != std::char_traits<char>::eof())
            memset(&sim->rtc, 0, sizeof(sim->rtc));
    }
    syncRecover(sim->rtc, generation);
    std::ifstream config(sim->configPath, std::ios::binary);
    if (config) {
        JsonDocument doc;
        if (deserializeJson(doc, config)) { delete sim; return nullptr; }
        sim->owner = doc["owner"].as<std::string>();
        sim->enabled = doc["enabled"] == true;
    }
    if (!saveRtc(*sim)) { delete sim; return nullptr; }
    return sim;
}

void codex_sim_sync_free(void *p) { delete (SimSync *)p; }

int codex_sim_sync_set_owner(void *p, const char *owner, int enabled) {
    if (!p || !owner || strlen(owner) > 64) return 0;
    auto &sim = *(SimSync *)p;
    bool changedOwner = sim.owner != owner;
    if (changedOwner && !syncStoreChangeOwner()) return 0;
    bool wasEnabled = sim.enabled;
    sim.owner = owner;
    if (enabled && (!wasEnabled || changedOwner)) syncAddReason(sim.rtc, 1);
    sim.enabled = enabled != 0;
    return saveRtc(sim) && saveConfig(sim) ? 1 : 0;
}

int codex_sim_sync_round(void *p, unsigned wakeSeq, unsigned uptimeMs) {
    if (!p) return 0;
    auto &sim = *(SimSync *)p;
    sim.wakeSeq = wakeSeq;
    syncCountDeepRendezvous(sim.rtc);
    int32_t args[] = {(int32_t)sim.rtc.rounds};
    syncAppendEvent(sim.rtc, wakeSeq, uptimeMs, 1, args, 1);
    return saveRtc(sim) ? 1 : 0;
}

int codex_sim_sync_reason(void *p, int bit, unsigned uptimeMs) {
    if (!p || bit < 0 || bit > 4) return 0;
    auto &sim = *(SimSync *)p;
    syncAddReason(sim.rtc, 1u << bit);
    int32_t args[] = {bit};
    syncAppendEvent(sim.rtc, sim.wakeSeq, uptimeMs, 8, args, 1);
    return saveRtc(sim) ? 1 : 0;
}

int codex_sim_sync_append_text(void *p, const char *text, unsigned uptimeMs) {
    if (!p || !text || strlen(text) > 8192) return 0;
    auto &sim = *(SimSync *)p;
    char clean[97]; uint8_t flags = 0;
    size_t length = syncSanitizeText(text, strlen(text), clean, flags);
    if (syncDiagnosticText(clean) && !syncAppend(sim.rtc, SYNC_TEXT, flags,
        sim.wakeSeq, uptimeMs, (const uint8_t *)clean, length)) return 0;
    return saveRtc(sim) ? 1 : 0;
}

int codex_sim_sync_corrupt(void *p, int header, unsigned long long seq) {
    if (!p) return 0;
    auto &sim = *(SimSync *)p;
    if (header) sim.rtc.headerCrc ^= 1;
    else {
        bool found = false;
        for (uint16_t offset = 0, next = 0; offset < sim.rtc.used; offset = next) {
            SyncRecord record;
            if (!syncReadAt(sim.rtc, offset, record, next)) return 0;
            if (record.seq != seq) continue;
            uint16_t tail = (sim.rtc.head + 4096 - sim.rtc.used) % 4096;
            sim.rtc.bytes[(tail + offset + 20) % 4096] ^= 1;
            found = true;
            break;
        }
        if (!found) return 0;
    }
    uint8_t generation[16];
    uint64_t v = sim.seed ^ sim.rtc.nextSeq ^ 0x9bc71c63a1ULL;
    for (int i = 0; i < 16; ++i) {
        v ^= v << 13; v ^= v >> 7; v ^= v << 17;
        generation[i] = (uint8_t)v;
    }
    syncRecover(sim.rtc, generation);
    return saveRtc(sim) ? 1 : 0;
}

int codex_sim_sync_fail(void *p, unsigned uptimeMs) {
    if (!p) return 0;
    auto &sim = *(SimSync *)p;
    syncFail(sim.rtc);
    syncAppendResult(sim.rtc, sim.wakeSeq, uptimeMs, 1,
                     (uint8_t)((sim.rtc.flags & 1 ? 1 : 0) | sim.rtc.reasonBits),
                     1, 0);
    return saveRtc(sim) ? 1 : 0;
}

int codex_sim_sync_consume_skip(void *p) {
    if (!p) return 0;
    auto &sim = *(SimSync *)p;
    syncConsumeSkip(sim.rtc);
    return saveRtc(sim) ? 1 : 0;
}

int codex_sim_sync_status(void *p, char *out, int cap) {
    if (!p) return -1;
    auto &sim = *(SimSync *)p;
    JsonDocument doc;
    doc["enabled"] = sim.enabled;
    doc["config_owner"] = sim.owner;
    doc["rounds"] = sim.rtc.rounds;
    doc["due"] = (sim.rtc.flags & 1) != 0;
    doc["retry_skip"] = sim.rtc.retrySkip;
    doc["baseline"] = (sim.rtc.flags & 2) ? "known" : "unknown";
    doc["diag_earliest_seq"] = std::to_string(syncEarliestSeq(sim.rtc));
    doc["diag_next_seq"] = std::to_string(sim.rtc.nextSeq);
    char generation[33];
    syncHex(sim.rtc.generation, 16, generation);
    doc["diag_generation"] = generation;
    doc["diag_used_bytes"] = sim.rtc.used;
    JsonArray reasons = doc["reasons"].to<JsonArray>();
    const char *names[] = {"periodic", "light_enter", "light_exit",
                           "ota_confirm", "bundle_confirm"};
    for (int bit = 1; bit < 5; ++bit)
        if ((sim.rtc.reasonBits & (1u << bit)) && (bit != 3 || (sim.rtc.flags & 1)))
            reasons.add(names[bit]);
    SyncStoreBatch batch;
    if (syncStoreActive(batch)) syncProtocolBatchFields(doc["pending_batch"].to<JsonObject>(), batch);
    else doc["pending_batch"] = nullptr;
    if (syncStoreReceipt(batch)) syncProtocolBatchFields(doc["last_completed"].to<JsonObject>(), batch);
    else doc["last_completed"] = nullptr;
    doc["last_error"] = syncStoreLost() ? "batch_lost" : nullptr;
    return copyJson(doc, out, cap);
}

int codex_sim_sync_command(void *p, const char *operation, const char *message,
                            const char *snapshotJson, char *out, int cap) {
    if (!p || !operation || !message || !snapshotJson) return -1;
    auto &sim = *(SimSync *)p;
    JsonDocument request, snapshot, response;
    if (deserializeJson(request, message) || deserializeJson(snapshot, snapshotJson)) return -1;
    const char *bridge = request["bridge_id"] | "";
    if (!sim.enabled || sim.owner != bridge) {
        response["result"] = "rejected";
        response["error"] = "disabled";
        response["http_status"] = 409;
    } else {
        // A deterministic suffix is sufficient for isolated replay; the ROM
        // passes 128 random bits into this same shared protocol entry point.
        uint8_t idBytes[16];
        uint64_t v = sim.seed ^ syncStoreHighestClientSerial() ^ 0xa17f321dcafeULL;
        for (int i = 0; i < 16; ++i) {
            v ^= v << 13; v ^= v >> 7; v ^= v << 17;
            idBytes[i] = (uint8_t)v;
        }
        char suffix[33];
        syncHex(idBytes, 16, suffix);
        SyncProtocolInput input{sim.rtc, sim.mac.c_str(), sim.owner.c_str(), suffix,
                                sim.wakeSeq, 0, false, snapshot.as<JsonVariantConst>()};
        SyncProtocolResult result = syncProtocolRun(input, operation, request, response);
        response["http_status"] = result.status;
        if (result.completed || !strcmp(operation, "sync_begin")) saveRtc(sim);
    }
    response["op"] = operation;
    response["request_id"] = request["request_id"] | "";
    response["device_mac"] = sim.mac;
    response["sync_version"] = 1;
    return copyJson(response, out, cap);
}
}
