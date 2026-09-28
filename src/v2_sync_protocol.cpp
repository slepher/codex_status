#include "v2_sync_protocol.h"

#include <string.h>
#include <string>
#include <vector>

namespace {
std::string number(uint64_t value) { return std::to_string(value); }

bool parseNumber(const char *text, uint64_t &value) {
    if (!text || !*text) return false;
    value = 0;
    for (const char *p = text; *p; ++p) {
        if (*p < '0' || *p > '9' || value > (UINT64_MAX - (*p - '0')) / 10) return false;
        value = value * 10 + (*p - '0');
    }
    return true;
}

bool parseHash(const char *text, uint8_t out[32]) {
    if (!text || strlen(text) != 64) return false;
    for (int i = 0; i < 32; ++i) {
        auto digit = [](char c) -> int {
            if (c >= '0' && c <= '9') return c - '0';
            if (c >= 'a' && c <= 'f') return c - 'a' + 10;
            return -1;
        };
        int a = digit(text[2*i]), b = digit(text[2*i+1]);
        if (a < 0 || b < 0) return false;
        out[i] = (uint8_t)((a << 4) | b);
    }
    return true;
}

const char *reasonName(int bit) {
    static const char *names[] = {"periodic", "light_enter", "light_exit",
                                  "ota_confirm", "bundle_confirm"};
    return names[bit];
}

uint8_t reasonBits(JsonArrayConst values) {
    if (values.isNull()) return 0;
    uint8_t bits = 0;
    for (JsonVariantConst value : values) {
        if (!value.is<const char *>()) return 0;
        int bit = 0;
        while (bit < 5 && strcmp(value.as<const char *>(), reasonName(bit))) ++bit;
        if (bit == 5) return 0;
        bits |= (uint8_t)(1u << bit);
    }
    return bits;
}

SyncProtocolResult answer(JsonDocument &out, const char *result,
                          const char *error = nullptr, int status = 200) {
    out["result"] = result;
    if (error) out["error"] = error;
    return {status, false, false};
}

bool freeze(const SyncProtocolInput &input, uint8_t requested,
            uint64_t clientSerial, SyncStoreBatch &batch) {
    uint64_t deviceSerial;
    if (!syncStoreAdvanceSerial(deviceSerial)) return false;
    std::vector<uint8_t> raw;
    raw.reserve(input.rtc.used);
    const uint64_t high = input.rtc.nextSeq - 1;
    uint64_t first = 0;
    for (uint16_t offset = 0, next = 0; offset < input.rtc.used; offset = next) {
        SyncRecord record;
        if (!syncReadAt(input.rtc, offset, record, next)) return false;
        if (record.seq <= input.rtc.ackedSeq || record.seq > high) continue;
        if (!first) first = record.seq;
        const uint16_t tail = (input.rtc.head + 4096 - input.rtc.used) % 4096;
        for (uint16_t i = offset; i < next; ++i)
            raw.push_back(input.rtc.bytes[(tail + i) % 4096]);
    }
    std::vector<char> encoded(5465);
    if (raw.size() && !syncBase64(raw.data(), raw.size(), encoded.data(), encoded.size()))
        return false;
    if (raw.empty()) encoded[0] = 0;
    if (!first) first = high + 1;
    char generation[33];
    syncHex(input.rtc.generation, 16, generation);
    if (!input.batchSuffix || strlen(input.batchSuffix) != 32 || !input.owner ||
        strlen(input.owner) > 64) return false;
    memset(&batch, 0, sizeof(batch));
    std::string id = number(deviceSerial) + "-" + input.batchSuffix;
    if (id.size() >= sizeof(batch.batchId)) return false;
    strcpy(batch.batchId, id.c_str());
    strcpy(batch.owner, input.owner);
    batch.clientSerial = clientSerial;
    batch.deviceSerial = deviceSerial;
    batch.fromSeq = first;
    batch.throughSeq = high;
    memcpy(batch.generation, input.rtc.generation, 16);
    batch.reasons = requested;
    JsonDocument doc;
    doc["format"] = "device-sync-1";
    doc["device_mac"] = input.mac;
    doc["bridge_id"] = input.owner;
    doc["batch_id"] = batch.batchId;
    doc["client_serial"] = number(clientSerial);
    JsonArray reasons = doc["reasons"].to<JsonArray>();
    for (int bit = 0; bit < 5; ++bit)
        if (requested & (1u << bit)) reasons.add(reasonName(bit));
    doc["snapshot"] = input.snapshot;
    JsonObject diag = doc["diag"].to<JsonObject>();
    diag["generation"] = generation;
    diag["from_seq"] = number(first);
    diag["through_seq"] = number(high);
    diag["records_b64"] = encoded.data();
    JsonArray gaps = diag["gaps"].to<JsonArray>();
    if (!(input.rtc.flags & 2)) {
        JsonObject gap = gaps.add<JsonObject>();
        gap["reason"] = (input.rtc.flags & 8) ? "corrupt" : "previous_generation_lost";
    }
    if (first > input.rtc.ackedSeq + 1) {
        JsonObject gap = gaps.add<JsonObject>();
        gap["reason"] = "overwritten";
        gap["from_seq"] = number(input.rtc.ackedSeq + 1);
        gap["through_seq"] = number(first - 1);
    }
    std::string bytes;
    serializeJson(doc, bytes);
    if (bytes.size() > 16384) return false;
    batch.bytes = (uint32_t)bytes.size();
    syncSha256((const uint8_t *)bytes.data(), bytes.size(), batch.sha256);
    return syncStoreFreeze((const uint8_t *)bytes.data(), bytes.size(), batch);
}
}

void syncProtocolBatchFields(JsonObject out, const SyncStoreBatch &batch) {
    char hash[65], generation[33];
    syncHex(batch.sha256, 32, hash);
    syncHex(batch.generation, 16, generation);
    out["batch_id"] = std::string(batch.batchId);
    out["client_serial"] = number(batch.clientSerial);
    out["bytes"] = batch.bytes;
    out["sha256"] = std::string(hash);
    out["diag_generation"] = std::string(generation);
    out["from_seq"] = number(batch.fromSeq);
    out["through_seq"] = number(batch.throughSeq);
    out["acked_offset"] = syncStoreAckedOffset();
    JsonArray reasons = out["reasons"].to<JsonArray>();
    for (int bit = 0; bit < 5; ++bit)
        if (batch.reasons & (1u << bit)) reasons.add(reasonName(bit));
}

SyncProtocolResult syncProtocolRun(const SyncProtocolInput &input,
                                   const char *operation,
                                   JsonDocument &request,
                                   JsonDocument &response) {
    SyncStoreBatch batch;
    const char *bridge = request["bridge_id"] | "";
    const char *batchId = request["batch_id"] | "";
    if (!syncStoreAvailable()) return answer(response, "rejected", "storage", 503);
    if (!strcmp(operation, "sync_begin")) {
        uint64_t clientSerial;
        if (strlen(bridge) > 64 || !parseNumber(request["client_serial"] | "", clientSerial) ||
            !clientSerial) return answer(response, "rejected", "shape", 400);
        if (syncStoreActive(batch)) {
            if (strcmp(batch.owner, bridge))
                return answer(response, "rejected", "owner_changed", 409);
            syncProtocolBatchFields(response.to<JsonObject>(), batch);
            return answer(response, "applied");
        }
        if (syncStoreReceipt(batch) && batch.clientSerial == clientSerial &&
            !strcmp(batch.owner, bridge)) {
            syncProtocolBatchFields(response.to<JsonObject>(), batch);
            response["receipt"] = std::string(batch.batchId);
            return answer(response, "already_complete");
        }
        if (clientSerial <= syncStoreHighestClientSerial())
            return answer(response, "rejected", "stale_serial", 409);
        uint8_t bits = reasonBits(request["reasons"].as<JsonArrayConst>());
        if (!bits) return answer(response, "rejected", "shape", 400);
        if (bits & ~((input.rtc.flags & 1 ? 1 : 0) | input.rtc.reasonBits))
            return answer(response, "rejected", "range", 400);
        if (!freeze(input, bits, clientSerial, batch))
            return answer(response, "rejected", "capacity", 422);
        int32_t args[] = {0, 0};
        syncAppendEvent(input.rtc, input.wakeSeq, input.uptimeMs, 10, args, 2);
        syncProtocolBatchFields(response.to<JsonObject>(), batch);
        return answer(response, "applied");
    }
    if (!strcmp(operation, "sync_page") || !strcmp(operation, "sync_ack") ||
        !strcmp(operation, "sync_complete")) {
        bool active = syncStoreActive(batch);
        if (!active && !strcmp(operation, "sync_complete") &&
            syncStoreReceipt(batch) && !strcmp(batch.batchId, batchId)) {
            uint8_t hash[32];
            if (request["bytes"].is<uint32_t>() &&
                batch.bytes == request["bytes"].as<uint32_t>() &&
                parseHash(request["sha256"] | "", hash) &&
                !memcmp(batch.sha256, hash, 32)) {
                response["receipt"] = std::string(batch.batchId);
                return answer(response, "already_complete");
            }
            return answer(response, "rejected", "digest_mismatch", 409);
        }
        if (!active || strcmp(batch.batchId, batchId))
            return answer(response, "rejected", syncStoreLost() ? "batch_lost" : "batch_conflict",
                          syncStoreLost() ? 503 : 409);
        if (strcmp(batch.owner, bridge))
            return answer(response, "rejected", "owner_changed", 409);
        if (!strcmp(operation, "sync_page")) {
            if (!request["offset"].is<uint32_t>() ||
                (!request["limit"].isNull() && !request["limit"].is<uint16_t>()))
                return answer(response, "rejected", "shape", 400);
            uint32_t offset = request["offset"].as<uint32_t>();
            uint16_t limit = request["limit"] | 1024;
            if (!limit || limit > 1024 || offset > batch.bytes)
                return answer(response, "rejected", "range", 400);
            uint8_t bytes[1024]; size_t length = 0;
            if (!syncStorePage(offset, limit, bytes, length))
                return answer(response, "rejected", "storage", 503);
            char encoded[1369], hash[65]; uint8_t digest[32];
            syncBase64(bytes, length, encoded, sizeof(encoded));
            syncSha256(bytes, length, digest); syncHex(digest, 32, hash);
            response["batch_id"] = std::string(batch.batchId);
            response["offset"] = offset;
            response["next_offset"] = offset + length;
            response["data_b64"] = encoded;
            response["chunk_sha256"] = hash;
            response["more"] = offset + length < batch.bytes;
            return answer(response, "applied");
        }
        if (!strcmp(operation, "sync_ack")) {
            uint8_t hash[32];
            if (!request["offset"].is<uint32_t>() ||
                !parseHash(request["prefix_sha256"] | "", hash))
                return answer(response, "rejected", "shape", 400);
            uint32_t offset = request["offset"].as<uint32_t>();
            if (!syncStoreAck(offset, hash))
                return answer(response, "rejected", "digest_mismatch", 409);
            response["batch_id"] = std::string(batch.batchId);
            response["acked_offset"] = syncStoreAckedOffset();
            return answer(response, "applied");
        }
        uint8_t hash[32];
        if (!request["bytes"].is<uint32_t>() ||
            !parseHash(request["sha256"] | "", hash))
            return answer(response, "rejected", "shape", 400);
        if (batch.bytes != request["bytes"].as<uint32_t>() ||
            memcmp(batch.sha256, hash, 32))
            return answer(response, "rejected", "digest_mismatch", 409);
        if (!syncStoreComplete(batch)) return answer(response, "rejected", "storage", 503);
        syncAppendResult(input.rtc, input.wakeSeq, input.uptimeMs, 0,
                         (uint8_t)((input.rtc.flags & 1 ? 1 : 0) | input.rtc.reasonBits),
                         0, batch.deviceSerial);
        uint8_t completed = batch.reasons;
        if ((completed & (1u << 3)) && !input.imageVerified) completed &= ~(1u << 3);
        if (!memcmp(batch.generation, input.rtc.generation, 16))
            syncComplete(input.rtc, batch.deviceSerial, batch.throughSeq, completed);
        response["batch_id"] = std::string(batch.batchId);
        response["receipt"] = std::string(batch.batchId);
        SyncProtocolResult result = answer(response, "applied");
        result.completed = true;
        result.otaConfirmed = (batch.reasons & (1u << 3)) && input.imageVerified;
        return result;
    }
    return answer(response, "rejected", "unsupported", 404);
}
