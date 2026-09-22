#include "v2_runtime.h"
#include "v2_state.h"

#include <ArduinoJson.h>
#include <string.h>

// Bounded usage document: only the active template's required fields exist here.
static const size_t V2_USAGE_CAP = 6144;

uint32_t v2CrcOf(const String &text) {
    return v2Crc32((const uint8_t *)text.c_str(), text.length());
}

static void appendFieldKey(String &acc, JsonObjectConst entry) {
    acc += String((int)(entry["i"] | -1));
    acc += ':';
    acc += (const char *)(entry["k"] | "");
    acc += ':';
    JsonVariantConst v = entry["v"];
    if (v.isNull()) {
        acc += '~';
    } else {
        String tmp;
        serializeJson(v, tmp);
        acc += tmp;
    }
    acc += ':';
    acc += (const char *)(entry["q"] | "missing");
    acc += ';';
}

uint32_t v2DataFieldsCrc(JsonArrayConst fields) {
    String acc;
    acc.reserve(256);
    for (JsonObjectConst entry : fields) {
        appendFieldKey(acc, entry);
    }
    return v2CrcOf(acc);
}

// Minimal usage nodes for each bind kind. Windows are created per
// (bucket, winMode, winIndex) group with a class sentinel `windowMins` so the
// engine's findWindow resolves them exactly like the bridge's envelope did.
static JsonObject bucketFor(JsonDocument &doc, const char *id) {
    JsonArray buckets = doc["buckets"].as<JsonArray>();
    if (buckets.isNull()) buckets = doc["buckets"].to<JsonArray>();
    for (JsonObject b : buckets) {
        if (!strcmp(b["id"] | "", id)) return b;
    }
    JsonObject b = buckets.add<JsonObject>();
    b["id"] = id;
    b["windows"].to<JsonArray>();
    return b;
}

static int sentinelMins(uint8_t winMode) {
    switch (winMode) {
    case 0: return 10080;   // weekly
    case 1: return 300;     // 5h
    case 3: return 43200;   // monthly
    default: return 0;      // explicit index: findWindow uses the index
    }
}

static JsonObject windowFor(JsonDocument &doc, const CtReq &req) {
    JsonObject bucket = bucketFor(doc, req.bucket);
    JsonArray wins = bucket["windows"].as<JsonArray>();
    if (wins.isNull()) wins = bucket["windows"].to<JsonArray>();
    if (req.winMode == 2) {
        while ((int)wins.size() <= req.winIndex) {
            wins.add<JsonObject>();
        }
        return wins[(size_t)req.winIndex].as<JsonObject>();
    }
    int mins = sentinelMins(req.winMode);
    for (JsonObject w : wins) {
        int wm = w["windowMins"] | 0;
        if (mins == 300 ? wm == 300 : (mins >= 10080 ? wm >= mins : wm >= 43200)) return w;
    }
    JsonObject w = wins.add<JsonObject>();
    if (mins > 0) w["windowMins"] = mins;
    return w;
}

static bool writeField(JsonDocument &doc, const CtReq &req, JsonVariantConst value) {
    switch (req.kind) {
    case 0: doc["account"]["plan"] = value; return true;                 // B_PLAN
    case 1: doc["bridge"]["label"] = value; return true;                 // B_LABEL
    case 2: doc["bridge"]["hostId"] = value; return true;                // B_HOSTID
    case 3: doc["server_time"] = value; return true;                     // B_SERVER_TIME
    case 4: doc["resetCredits"]["availableCount"] = value; return true;  // B_RESET_COUNT
    case 5: doc["resetCredits"]["nextExpiresAt"] = value; return true;   // B_RESET_EXPIRES
    case 6: windowFor(doc, req)["usedPercent"] = value; return true;     // B_BUCKET_USED
    case 7: {
        // remaining is stored as usedPercent so the engine's `remaining`
        // resolution stays identical (100 - used).
        int used = 100 - (value.as<int>());
        windowFor(doc, req)["usedPercent"] = used;
        return true;
    }
    case 8: windowFor(doc, req)["resetsAt"] = value; return true;        // B_BUCKET_RESET
    case 9: windowFor(doc, req)["windowMins"] = value; return true;      // B_BUCKET_WINMINS
    default: return false;                                               // device.*
    }
}

static V2DataAck applyEntries(const CtTemplate &ct, JsonArrayConst fields,
                              JsonDocument &doc, String &err, uint32_t expectedCrc) {
    uint32_t crc = v2DataFieldsCrc(fields);
    if (crc != expectedCrc) { err = "crc"; return V2_DATA_REJECTED; }
    // The Bridge sends exactly the remote requirements (device-local binds like
    // device.now are never transmitted, and are not renumbered away): a complete
    // snapshot is one entry per remote requirement at its compiled index.
    uint8_t remoteCount = 0;
    for (uint8_t i = 0; i < ct.reqCount; i++) {
        if (ct.reqs[i].kind <= 9) remoteCount++;
    }
    if (fields.size() != remoteCount) { err = "incomplete"; return V2_DATA_REJECTED; }
    int lastIndex = -1;
    for (JsonObjectConst entry : fields) {
        int i = entry["i"] | -1;
        const char *k = entry["k"] | "";
        if (i < 0 || i >= ct.reqCount) { err = "index"; return V2_DATA_REJECTED; }
        if (strcmp(k, ct.reqs[i].path) != 0) { err = "field"; return V2_DATA_REJECTED; }
        if (ct.reqs[i].kind > 9) { err = "local"; return V2_DATA_REJECTED; }
        if (i <= lastIndex) { err = "order"; return V2_DATA_REJECTED; }
        lastIndex = i;
        JsonVariantConst v = entry["v"];
        if (v.isNull()) {
            // Missing/expired: leave the node absent so the template's
            // `when.exists` branches render exactly like the bridge preview.
            continue;
        }
        if (!writeField(doc, ct.reqs[i], v)) { err = "kind"; return V2_DATA_REJECTED; }
    }
    return V2_DATA_APPLIED;
}

V2DataAck v2ApplyData(const CtTemplate &ct, const String &messageJson,
                      const char *currentContext, String &usageOut, String &errorOut) {
    JsonDocument in;
    if (deserializeJson(in, messageJson)) { errorOut = "json"; return V2_DATA_REJECTED; }
    const char *context = in["active_context_id"] | "";
    if (!*context) context = in["context_id"] | "";
    if (!currentContext || strcmp(context, currentContext) != 0) {
        errorOut = "context";
        return V2_DATA_CONTEXT_MISMATCH;
    }
    JsonArrayConst fields = in["fields"].as<JsonArrayConst>();
    if (fields.isNull()) { errorOut = "fields"; return V2_DATA_REJECTED; }
    JsonDocument doc;
    doc["schema"] = 1;
    uint32_t crc = 0;
    const char *crcText = in["crc"] | "";
    if (!v2ParseCrc(crcText, crc)) { errorOut = "crc"; return V2_DATA_REJECTED; }
    if (!in["seq"].is<uint64_t>() || in["seq"].as<uint64_t>() == 0) {
        errorOut = "seq";
        return V2_DATA_REJECTED;
    }
    V2DataAck rc = applyEntries(ct, fields, doc, errorOut, crc);
    if (rc != V2_DATA_APPLIED) return rc;
    usageOut = "";
    serializeJson(doc, usageOut);
    if (usageOut.length() > V2_USAGE_CAP) {
        errorOut = "usage_size";
        return V2_DATA_REJECTED;
    }
    return rc;
}

V2DataAck v2AcceptData(const CtTemplate &ct, const String &messageJson,
                     const char *currentContext, V2DataSeq &state,
                     String &usageOut, String &errorOut) {
    V2DataAck rc = v2ApplyData(ct, messageJson, currentContext, usageOut, errorOut);
    if (rc != V2_DATA_APPLIED) return rc;
    JsonDocument in;
    if (deserializeJson(in, messageJson)) { errorOut = "json"; return V2_DATA_REJECTED; }
    uint64_t seq = in["seq"].as<uint64_t>();
    uint32_t crc = v2DataFieldsCrc(in["fields"].as<JsonArrayConst>());
    rc = state.observe(seq, crc);
    if (rc == V2_DATA_APPLIED) state.noteApplied(seq, crc);
    return rc;
}

bool v2UsageFromFields(const CtTemplate &ct, const String &storedFieldsJson,
                       String &usageOut, String &errorOut) {
    JsonDocument in;
    if (deserializeJson(in, storedFieldsJson)) { errorOut = "json"; return false; }
    JsonArrayConst fields = in.as<JsonArrayConst>();
    if (fields.isNull()) { errorOut = "fields"; return false; }
    JsonDocument doc;
    doc["schema"] = 1;
    V2DataAck rc = applyEntries(ct, fields, doc, errorOut, v2DataFieldsCrc(fields));
    if (rc != V2_DATA_APPLIED) return false;
    usageOut = "";
    serializeJson(doc, usageOut);
    return usageOut.length() <= V2_USAGE_CAP;
}
