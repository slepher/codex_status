// Included by wifi_setup.cpp: shares the production storage and Wi-Fi HAL.
#ifdef CODEX_WIFI_SETUP_HOST_TEST
static uint32_t wifiRandom() { static uint32_t n = 123; return ++n * 2654435761U; }
#else
#include <esp_random.h>
static uint32_t wifiRandom() { return esp_random(); }
#endif
struct WifiPermit { String id, peer, bridge; uint64_t readUntil = 0, writeUntil = 0; };
static WifiPermit wifiPermits[3];
static String fullSnapshot, fullSnapshotId, fullSnapshotBridge;
static uint64_t fullSnapshotUntil = 0;
static uint64_t fullSnapshotRevision = 0;
void wifiManagementResetSession() {
    for (auto &p : wifiPermits) p = WifiPermit{};
    fullSnapshot = fullSnapshotId = fullSnapshotBridge = "";
    fullSnapshotUntil = 0;
}
static String wifiRandomId() {
    char text[33];
    snprintf(text, sizeof(text), "%08lx%08lx%08lx%08lx", (unsigned long)wifiRandom(),
             (unsigned long)wifiRandom(), (unsigned long)wifiRandom(), (unsigned long)wifiRandom());
    return String(text);
}
static bool wifiManagementShape(JsonDocument &doc) {
    for (const char *key : {"bridge_id","request_id","query_id","device_mac","permit","snapshot_id"}) {
        if (doc[key].isNull()) continue;
        JsonString s = doc[key].as<JsonString>();
        if (!s || s.size() != strlen(s.c_str())) return false;
    }
    return true;
}
String wifiManagementAuthorize(JsonDocument &request, const String &peer, uint64_t now,
                               uint64_t physicalUntil, uint64_t readUntil) {
    if (!wifiManagementShape(request)) return reply(request,"shape");
    const char *bridge = request["bridge_id"] | "";
    if (!identifier(bridge, 64) || !peer.length()) return reply(request, "shape");
    WifiPermit *permit = nullptr;
    for (auto &p : wifiPermits) if (p.peer == peer && p.bridge == bridge) permit = &p;
    if (!permit) for (auto &p : wifiPermits) if (!p.id.length() || p.readUntil <= now) { permit = &p; break; }
    if (!permit) return reply(request, "capacity");
    if (permit->readUntil <= now) *permit = WifiPermit{};
    if (!permit->id.length()) { permit->id = wifiRandomId(); permit->peer = peer; permit->bridge = bridge; }
    // Routine writes borrow an already admitted window; this grants no RF lease.
    permit->writeUntil = std::max(physicalUntil, readUntil);
    permit->readUntil = std::max(physicalUntil, readUntil);
    if (permit->readUntil <= now) return reply(request, "window_required");
    JsonDocument out;
    out["ack"] = "wifi"; out["op"] = "wifi"; out["ok"] = true;
    out["request_id"] = request["request_id"]; out["device_mac"] = request["device_mac"];
    out["permit"] = permit->id; out["write_remaining_ms"] = permit->writeUntil > now ? permit->writeUntil - now : 0;
    out["wifi_management"] = 2;
    String text; serializeJson(out, text); return text;
}
static WifiPermit *findWifiPermit(JsonDocument &request, uint64_t now) {
    String id = request["permit"] | "", bridge = request["bridge_id"] | "";
    for (auto &p : wifiPermits) if (p.id.length() && p.id == id && p.bridge == bridge && p.readUntil > now) return &p;
    return nullptr;
}
static void projectWifiNetwork(JsonArray list, const WifiNetwork &n, const char *label,
                               uint64_t version, bool writable) {
    JsonObject item = list.add<JsonObject>();
    // Legacy rows are projected from a loop-local WifiNetwork. ArduinoJson
    // borrows const char arrays; own these strings until the snapshot freezes.
    item["bond"] = String(n.bond); item["writable"] = writable;
    JsonObject net = item["network"].to<JsonObject>();
    net["network_id"] = String(n.id); net["host_ssid"] = String(n.host); net["ssid"] = String(n.ssid);
    net["label"] = String(label); net["version"] = version; net["credential"] = "device";
    net["origin"] = String(writable ? "local" : n.bond); net["deleted"] = false;
    if(writable) net["password"] = String(n.password);
}
String wifiManagementCommand(JsonDocument &doc, uint64_t now, const String &legacy) {
    const String op = doc["op"] | "", bridge = doc["bridge_id"] | "";
    if (wifiStorageUncertain && !running && !wifiSchemesBegin()) return reply(doc, "storage");
    if (!storeReady) return reply(doc, "storage");
    if (!wifiManagementShape(doc)) return reply(doc,"shape");
    if (!identifier(bridge.c_str(), 64)) return reply(doc, "shape");
    WifiPermit *permit = findWifiPermit(doc, now);
    JsonDocument out;
    out["ok"] = true; out["device_mac"] = doc["device_mac"];
    if (op == "snapshot") {
        uint32_t offset = doc["offset"] | 0u;
        if (!doc["offset"].isNull() && !doc["offset"].is<uint32_t>()) return reply(doc, "shape");
        if (!offset) {
            JsonDocument snapshot;
            snapshot["schema"] = 2; snapshot["device_mac"] = doc["device_mac"];
            snapshot["revision"] = wifiStore.revision;
            snapshot["snapshot_id"] = wifiRandomId(); snapshot["complete"] = true;
            snapshot["observed_at"] = 0;
            snapshot["capacity"]["total"] = 12; snapshot["capacity"]["per_bond"] = 4;
            snapshot["active"]["bond"] = wifiStore.active.bond.c_str();
            snapshot["active"]["network_id"] = wifiStore.active.id.c_str();
            // Active version can differ from the saved version.
            snapshot["active"]["version"] = wifiStore.activeVersion;
            JsonArray list = snapshot["networks"].to<JsonArray>();
            for (size_t i = 0; i < schemes.networks.size(); ++i) if (*schemes.networks[i].id)
                projectWifiNetwork(list, schemes.networks[i], schemes.networks[i].label, schemes.networks[i].version,
                    permit && permit->peer == schemes.networks[i].bond);
            JsonDocument old;
            if (legacy.length() && !deserializeJson(old, legacy)) for (JsonObject n : old.as<JsonArray>()) {
                WifiNetwork network; network.bond.set("legacy");
                String id = n["network_id"] | "", ssid = n["ssid"] | "";
                if (!identifier(id.c_str(), 32) || !ssidValid(ssid.c_str())) continue;
                network.id.set(id.c_str()); network.host.set(ssid.c_str()); network.ssid.set(ssid.c_str());
                projectWifiNetwork(list, network, "USB/AP", 0, false);
            }
            fullSnapshot = ""; serializeJson(snapshot, fullSnapshot);
            fullSnapshotId = snapshot["snapshot_id"].as<String>(); fullSnapshotBridge = bridge;
            fullSnapshotUntil = now + 30000; fullSnapshotRevision = wifiStore.revision;
        } else if (now >= fullSnapshotUntil || bridge != fullSnapshotBridge ||
            String(doc["snapshot_id"] | "") != fullSnapshotId || fullSnapshotRevision != wifiStore.revision) return reply(doc, "snapshot_expired");
        if (offset >= fullSnapshot.length()) return reply(doc, "shape");
        uint32_t next = std::min((uint32_t)fullSnapshot.length(), offset + 768);
        while (next < fullSnapshot.length() && next > offset && (((uint8_t)fullSnapshot[next] & 0xc0) == 0x80)) --next;
        out["schema"] = 2; out["revision"] = fullSnapshotRevision; out["snapshot_id"] = fullSnapshotId;
        out["offset"] = offset; out["next_offset"] = next; out["complete"] = next == fullSnapshot.length();
        out["total"] = fullSnapshot.length(); out["chunk"] = fullSnapshot.substring(offset, next);
    } else if (op == "result") {
        if (!running && !wifiSchemesBegin()) return reply(doc, "storage");
        String query = doc["query_id"] | "";
        if (!identifier(query.c_str(), 64)) return reply(doc, "shape");
        out["outcome"] = "unknown";
        for (const auto &r : wifiStore.receipts) if (bridge == r.bridge && query == r.request && (doc["result_peer"].isNull() || String(doc["result_peer"] | "") == r.bond)) {
            out["outcome"] = *r.error ? "rejected" : "committed"; out["revision"] = r.revision; out["bond"] = String(r.bond);
            if (*r.error) out["error"] = r.error.c_str(); break;
        }
    } else if (op == "apply") {
        const char *id = doc["request_id"] | "";
        if (!identifier(id, 64) || !doc["changes"].is<JsonArray>() || !doc["expected_revision"].is<uint64_t>()) return reply(doc, "shape");
        // Hash only business content; nonce, token, permit and revalidated
        // expected_revision are transport envelopes, never transaction identity.
        JsonDocument intent; intent["changes"] = doc["changes"];
        String body; serializeJson(intent, body); uint8_t hash[32];
        mbedtls_sha256((const unsigned char *)body.c_str(), body.length(), hash, 0); wipeString(body);
        for (const auto &r : wifiStore.receipts) if (bridge == r.bridge && !strcmp(id, r.request)) {
            if ((permit && permit->peer != r.bond) || memcmp(hash, r.hash, sizeof(hash))) return reply(doc, "request_conflict");
            out["outcome"] = *r.error ? "rejected" : "committed"; out["revision"] = r.revision; out["bond"] = String(r.bond);
            if (*r.error) { out["ok"] = false; out["error"] = r.error.c_str(); }
            String text; serializeJson(out, text); return text;
        }
        if (!permit || permit->writeUntil <= now) return reply(doc, "window_required");
        if (doc["expected_revision"].as<uint64_t>() != wifiStore.revision) return reply(doc, "revision_conflict");
        if (running) return reply(doc, "busy");
        const size_t receipt = wifiStore.receipts.size();
        if (receipt >= 8) return reply(doc, "result_capacity");
        JsonArray changes = doc["changes"].as<JsonArray>();
        if (!changes.size() || changes.size() > 4 || wifiStore.revision == UINT64_MAX) return reply(doc, "capacity");
        std::unique_ptr<WifiStore> buffer(new(std::nothrow) WifiStore(wifiStore));
        if(!buffer || !buffer->valid())return reply(doc,"memory");
        buffer->preserveActive();
        auto &next=*buffer;
        const char *failure = nullptr;
        String used;
        for (JsonObject change : changes) {
            const char *networkId = change["network_id"] | "";
            if (!identifier(networkId, 32) || used.indexOf((String("|") + networkId + "|").c_str()) >= 0) { failure = "shape"; break; }
            used += String("|") + networkId + "|";
            int slot = next.schemes.find(permit->peer.c_str(), networkId);
            if (change["deleted"] == true) {
                if (slot < 0) { failure = "not_found"; break; }
                if (!next.schemes.erase(slot)) { failure = "memory"; break; } continue;
            }
            const char *host = change["host_ssid"] | "", *ssid = change["ssid"] | "";
            const char *label = change["label"] | "", *credential = change["credential"] | "";
            if (!ssidValid(host) || !ssidValid(ssid) || strlen(label) > 64) { failure = "shape"; break; }
            for (const char *key : {"network_id", "host_ssid", "ssid", "label", "password", "credential"}) {
                if (change[key].isNull()) continue;
                JsonString value = change[key].as<JsonString>();
                if (!value || value.size() != strlen(value.c_str())) failure = "shape";
            }
            if (failure) break;
            WifiNetwork network;
            network.bond.set(permit->peer.c_str()); network.id.set(networkId);
            network.host.set(host); network.ssid.set(ssid);
            if (!strcmp(credential, "keep") || !strcmp(credential, "device")) {
                if (slot < 0) { failure = "credentials_required"; break; }
                network.password.set(next.schemes.networks[slot].password);
            } else if (!strcmp(credential, "replace")) {
                const char *password = change["password"] | "";
                if (!change["password"].is<const char *>() || !*password || !passwordValid(password)) { failure = "credentials_required"; break; }
                network.password.set(password);
            } else if (strcmp(credential, "open")) { failure = "shape"; break; }
            // v1 put selected as a side effect; restore the saved active reference.
            WifiText bond = next.schemes.selectedBond, selected = next.schemes.selectedId;
            failure = next.schemes.put(network, slot >= 0);
            next.schemes.selectedBond.set(bond); next.schemes.selectedId.set(selected);
            network.password.clear();
            if (failure) break;
            slot = next.schemes.find(permit->peer.c_str(), networkId);
            next.schemes.networks[slot].label.set(label); next.schemes.networks[slot].version = wifiStore.revision + 1;
        }
        // Retire the saved active row only when another own-bond row remains.
        // The live connection/active copy survives until explicit activation.
        if (!failure && *next.active.id && !strcmp(next.active.bond, permit->peer.c_str()) &&
            next.schemes.find(next.active.bond, next.active.id) < 0) {
            bool replacement = false;
            for (const auto &n : next.schemes.networks) if (*n.id && !strcmp(n.bond, permit->peer.c_str())) replacement = true;
            if (!replacement) failure = "active_protected";
        }
        if (failure) {
            // Persist a deterministic refusal together with the unchanged
            // configuration; never persist the partially validated candidate.
            next = wifiStore;
        } else next.revision++;
        if (!next.valid() || !next.receipts.resize(receipt+1)) return reply(doc,"memory");
        auto &r = next.receipts[receipt]; r.bridge.set(bridge.c_str()); r.request.set(id);
        r.bond.set(permit->peer.c_str()); memcpy(r.hash, hash, sizeof(hash)); r.revision = next.revision;
        if (failure) r.error.set(failure);
        if (!saveV2(next)) failure = "storage_unknown";
        if (failure) return reply(doc, failure);
        out["outcome"] = "committed"; out["revision"] = wifiStore.revision; out["bond"] = permit->peer;
    } else return reply(doc, "unsupported");
    String text; serializeJson(out, text); return text;
}
