// Included by wifi_setup.cpp: shares the production storage and Wi-Fi HAL.
#ifdef CODEX_WIFI_SETUP_HOST_TEST
static uint32_t wifiRandom() { static uint32_t n = 123; return ++n * 2654435761U; }
#else
#include <esp_random.h>
static uint32_t wifiRandom() { return esp_random(); }
#endif
static String fullSnapshot, fullSnapshotId, fullSnapshotBridge;
static uint64_t fullSnapshotUntil = 0;
static uint64_t fullSnapshotRevision = 0;
static bool fullSnapshotOwnerRead = false;
void wifiManagementResetSession() {
    fullSnapshot = fullSnapshotId = fullSnapshotBridge = "";
    fullSnapshotUntil = 0; fullSnapshotOwnerRead = false;
}
static String wifiRandomId() {
    char text[33];
    snprintf(text, sizeof(text), "%08lx%08lx%08lx%08lx", (unsigned long)wifiRandom(),
             (unsigned long)wifiRandom(), (unsigned long)wifiRandom(), (unsigned long)wifiRandom());
    return String(text);
}
static bool wifiManagementShape(JsonDocument &doc) {
    for (const char *key : {"bridge_id","request_id","query_id","device_mac","snapshot_id","op","command","session_nonce"}) {
        if (doc[key].isNull()) continue;
        JsonString s = doc[key].as<JsonString>();
        if (!s || s.size() != strlen(s.c_str())) return false;
    }
    return true;
}
static void projectWifiNetwork(JsonArray list, const WifiNetwork &n, const char *label,
                               uint64_t version, bool writable, bool ownerRead = false) {
    JsonObject item = list.add<JsonObject>();
    // Legacy rows are projected from a loop-local WifiNetwork. ArduinoJson
    // borrows const char arrays; own these strings until the snapshot freezes.
    item["bond"] = String(n.bond); item["writable"] = writable;
    JsonObject net = item["network"].to<JsonObject>();
    net["network_id"] = String(n.id); net["host_ssid"] = String(n.host); net["ssid"] = String(n.ssid);
    net["label"] = String(label); net["version"] = version; net["credential"] = "device";
    net["origin"] = String(writable ? "local" : n.bond); net["deleted"] = false;
    if(writable || ownerRead) net["password"] = String(n.password);
}
static String wifiManagementWithContext(JsonDocument &doc, uint64_t now, const String &legacy, const WifiControlContext &context) {
    const String op = doc["op"] | "", bridge = doc["bridge_id"] | "";
    if (wifiStorageUncertain && !running && !wifiSchemesBegin()) return reply(doc, "storage");
    if (!storeReady) return reply(doc, "storage");
    if (!wifiManagementShape(doc)) return reply(doc,"shape");
    if (!identifier(bridge.c_str(), 64)) return reply(doc, "shape");
    JsonDocument out;
    out["ok"] = true; out["device_mac"] = doc["device_mac"];
    if (op == "snapshot") {
        if(context.until<=now) return reply(doc,"window_required");
        uint32_t offset = doc["offset"] | 0u;
        if (!doc["offset"].isNull() && !doc["offset"].is<uint32_t>()) return reply(doc, "shape");
        if (!offset) {
            JsonDocument snapshot;
            snapshot["schema"] = 2; snapshot["device_mac"] = doc["device_mac"];
            snapshot["revision"] = wifiStore.revision;
            snapshot["snapshot_id"] = wifiRandomId(); snapshot["complete"] = true;
            snapshot["observed_at"] = 0;
            snapshot["capacity"]["total"] = 12;
            snapshot["active"]["bond"] = wifiStore.active.bond.c_str();
            snapshot["active"]["network_id"] = wifiStore.active.id.c_str();
            // Active version can differ from the saved version.
            snapshot["active"]["version"] = wifiStore.activeVersion;
            JsonArray list = snapshot["networks"].to<JsonArray>();
            for (size_t i = 0; i < schemes.networks.size(); ++i) if (*schemes.networks[i].id)
                projectWifiNetwork(list, schemes.networks[i], schemes.networks[i].label, schemes.networks[i].version,
                    context.sessionValid && context.claimed,
                    context.sessionValid && context.claimed);
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
            fullSnapshotOwnerRead = context.sessionValid && context.claimed;
            fullSnapshotUntil = now + 30000; fullSnapshotRevision = wifiStore.revision;
        } else if (now >= fullSnapshotUntil || bridge != fullSnapshotBridge ||
            String(doc["snapshot_id"] | "") != fullSnapshotId || fullSnapshotRevision != wifiStore.revision) return reply(doc, "snapshot_expired");
        if (fullSnapshotOwnerRead && !(context.sessionValid && context.claimed)) return reply(doc, "claim_required");
        if (offset >= fullSnapshot.length()) return reply(doc, "shape");
        uint32_t next = std::min((uint32_t)fullSnapshot.length(), offset + (context.generation ? 80 : 768));
        while (next < fullSnapshot.length() && next > offset && (((uint8_t)fullSnapshot[next] & 0xc0) == 0x80)) --next;
        out["schema"] = 2; out["revision"] = fullSnapshotRevision; out["snapshot_id"] = fullSnapshotId;
        out["offset"] = offset; out["next_offset"] = next; out["complete"] = next == fullSnapshot.length();
        out["total"] = fullSnapshot.length(); out["chunk"] = fullSnapshot.substring(offset, next);
    } else if (op == "result" || op == "result_ack") {
        if (!running && !wifiSchemesBegin()) return reply(doc, "storage");
        String query = doc["query_id"] | "";
        if (!identifier(query.c_str(), 64)) return reply(doc, "shape");
        out["outcome"] = "unknown";
        uint64_t sequence=wifiRequestSequence(query.c_str());
        for (const auto &c:wifiStore.cursors) if (bridge==c.bridge && sequence && sequence<=c.high &&
            context.nameSpace==c.bond) out["outcome"]="retired";
        for (const auto &r : wifiStore.receipts) if (r.matches(context.principal.c_str(),context.nameSpace.c_str(),query.c_str())) {
            out["outcome"] = *r.error ? "rejected" : !strcmp(r.operation,"trial") ? "completed" : "committed"; out["revision"] = r.revision; out["bond"] = String(r.bond);
            if(*r.operation){out["operation"]=String(r.operation);out["network_id"]=String(r.target);out["version"]=r.targetVersion;}
            if(String(r.error)=="pending") {
                const bool live=transactionPeer==r.bond && selectingBridge==r.bridge && requestId==r.request;
                out["outcome"]=live ? (running?"accepted":r.executionGeneration?"unknown":trialOnly && !error.length()?"completed":"rejected") : "unknown";
                out["stage"]=live?stage:String("result_unknown");
                if(live && error.length())out["error"]=error;
                if(op!="result_ack" || r.executionGeneration || !live || running || !trialOnly)break;
            }
            if (op=="result_ack") {
                if (!sequence) return reply(doc,"legacy_receipt_retained");
                if (String(r.error)=="pending" && !(transactionPeer==r.bond && selectingBridge==r.bridge && requestId==r.request && trialOnly && !running)) return reply(doc,"result_pending");
                if (String(r.error)=="result_unknown") return reply(doc,"result_pending");
                std::unique_ptr<WifiStore> next(new(std::nothrow) WifiStore(wifiStore));
                if (!next || !next->valid() || !next->receipts.erase(&r-wifiStore.receipts.begin())) return reply(doc,"memory");
                if (!saveV2(*next)) return reply(doc,"storage_unknown");
                out["outcome"]="retired"; break;
            }
            if (*r.error) out["error"] = r.error.c_str(); break;
        }
    } else if (op == "apply") {
        const char *id = doc["request_id"] | "";
        if (!identifier(id, 64) || !doc["changes"].is<JsonArray>() || !doc["expected_revision"].is<uint64_t>()) return reply(doc, "shape");
        // Hash only business content; nonce, token and revalidated
        // expected_revision are transport envelopes, never transaction identity.
        JsonDocument intent; intent["changes"] = doc["changes"];
        String body; serializeJson(intent, body); uint8_t hash[32];
        mbedtls_sha256((const unsigned char *)body.c_str(), body.length(), hash, 0); wipeString(body);
        for (const auto &r : wifiStore.receipts) if (r.matches(context.principal.c_str(),context.nameSpace.c_str(),id)) {
            if (memcmp(hash, r.hash, sizeof(hash))) return reply(doc, "request_conflict");
            out["outcome"] = *r.error ? "rejected" : !strcmp(r.operation,"trial") ? "completed" : "committed"; out["revision"] = r.revision; out["bond"] = String(r.bond);
            if(*r.operation){out["operation"]=String(r.operation);out["network_id"]=String(r.target);out["version"]=r.targetVersion;}
            if(String(r.error)=="pending") {
                const bool live=transactionPeer==r.bond && selectingBridge==r.bridge && requestId==r.request;
                out["outcome"]=live ? (running?"accepted":r.executionGeneration?"unknown":trialOnly?"completed":"rejected") : "unknown";
                out["stage"]=live?stage:String("result_unknown");
                if(live && error.length())out["error"]=error;
                break;
            }
            if (*r.error) { out["ok"] = false; out["error"] = r.error.c_str(); }
            String text; serializeJson(out, text); return text;
        }
        if(wifiRequestSequence(id)<=wifiManagementHighWater(context.principal,context.nameSpace) && wifiRequestSequence(id))return reply(doc,"request_retired");
        if(String(id).startsWith("w3_") && !wifiRequestSequence(id))return reply(doc,"shape");
        if (!context.sessionValid) return reply(doc,"session_expired");
        if (!context.claimed) return reply(doc,context.occupied?"occupied":"claim_required");
        if (context.until <= now) return reply(doc, "window_required");
        uint64_t sequence=wifiRequestSequence(id);
        int cursor=-1;
        for (size_t i=0;i<wifiStore.cursors.size();++i) if (bridge==wifiStore.cursors[i].bridge && context.nameSpace==wifiStore.cursors[i].bond) cursor=i;
        if (sequence && cursor>=0 && sequence<=wifiStore.cursors[cursor].high) return reply(doc,"request_retired");
        if (sequence && cursor<0 && wifiStore.cursors.size()>=8) return reply(doc,"cursor_capacity");
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
            int slot = next.schemes.findShared(networkId);
            if (slot == -2) { failure = "ambiguous_network_id"; break; }
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
            network.bond.set(slot >= 0 ? next.schemes.networks[slot].bond.c_str() : context.nameSpace.c_str()); network.id.set(networkId);
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
            slot = next.schemes.findShared(networkId);
            next.schemes.networks[slot].label.set(label); next.schemes.networks[slot].version = wifiStore.revision + 1;
        }
        // Retire the saved active row only when another device configuration remains.
        // The live connection/active copy survives until explicit activation.
        if (!failure && *next.active.id &&
            next.schemes.find(next.active.bond, next.active.id) < 0) {
            bool replacement = false;
            for (const auto &n : next.schemes.networks) if (*n.id) replacement = true;
            if (!replacement) failure = "active_protected";
        }
        if (failure) {
            // Persist a deterministic refusal together with the unchanged
            // configuration; never persist the partially validated candidate.
            next = wifiStore;
        } else next.revision++;
        if (!next.valid() || !next.receipts.resize(receipt+1)) return reply(doc,"memory");
        auto &r = next.receipts[receipt]; r.bridge.set(bridge.c_str()); r.request.set(id);
        r.bond.set(context.nameSpace.c_str()); memcpy(r.hash, hash, sizeof(hash)); r.revision = next.revision;
        if (failure) r.error.set(failure);
        if (sequence) {
            if (cursor<0) {
                WifiCursor c; c.bridge.set(bridge.c_str()); c.bond.set(context.nameSpace.c_str()); c.high=sequence;
                if (!next.cursors.append(c)) return reply(doc,"memory");
            } else next.cursors[cursor].high=sequence;
        }
        if (!saveV2(next)) failure = "storage_unknown";
        if (failure) return reply(doc, failure);
        out["outcome"] = "committed"; out["revision"] = wifiStore.revision; out["bond"] = context.nameSpace;
    } else return reply(doc, "unsupported");
    String text; serializeJson(out, text); return text;
}

String wifiControlCommand(JsonDocument &doc,const WifiControlContext &ctx,uint64_t now,const String &legacy) {
    if(!wifiManagementShape(doc))return reply(doc,"shape");
    const char *requestedId=doc["request_id"]|"";
    if(!strncmp(requestedId,"w3_",3) && !wifiRequestSequence(requestedId))return reply(doc,"shape");
    if (!ctx.principal.length() || !ctx.nameSpace.length()) return reply(doc,"binding_required");
    if (String(doc["bridge_id"]|"")!=ctx.principal) return reply(doc,"principal_mismatch");
    String op=doc["op"]|"";
    if (op.startsWith("wifi_")) op=op.substring(5);
    if (op=="snapshot" || op=="apply" || op=="result" || op=="result_ack") {
        JsonDocument request; request.set(doc);request["op"]=op;request["result_peer"]=ctx.nameSpace;
        return wifiManagementWithContext(request,now,legacy,ctx);
    }
    if (op=="match" || op=="active" || op=="trial" || op=="activate" || op=="setup_status" || op=="setup_cancel" || op=="scheme_get") {
        JsonDocument request; request.set(doc);request["op"]=String("wifi_")+op;
        if(op=="trial" || op=="activate") {
            int index=wifiReceiptIndex(ctx.principal,ctx.nameSpace,doc["request_id"]|"");
            if(index>=0){
                JsonDocument business;business["op"]=String("wifi_")+op;
                for(const char *key:{"network_id","version","host_ssid","ssid","password","replace"})if(!doc[key].isNull())business[key]=doc[key];
                String text;serializeJson(business,text);uint8_t hash[32];mbedtls_sha256((const uint8_t *)text.c_str(),text.length(),hash,0);wipeString(text);
                if(memcmp(hash,wifiStore.receipts[index].hash,32))return reply(doc,"request_conflict");
                JsonDocument query;query.set(doc);query["op"]="result";query["query_id"]=doc["request_id"];query["result_peer"]=ctx.nameSpace;
                return wifiManagementWithContext(query,now,legacy,ctx);
            }
        }
        if ((op=="trial" || op=="activate") && wifiRequestSequence(requestedId) && wifiRequestSequence(requestedId)<=wifiManagementHighWater(ctx.principal,ctx.nameSpace))return reply(doc,"request_retired");
        if ((op=="trial" || op=="activate") && !ctx.sessionValid) return reply(doc,"session_expired");
        if ((op=="trial" || op=="activate") && !ctx.claimed) return reply(doc,ctx.occupied?"occupied":"claim_required");
        if(op=="scheme_get" && !ctx.sessionValid)return reply(doc,"session_expired");
        if(op=="scheme_get" && !ctx.claimed)return reply(doc,ctx.occupied?"occupied":"claim_required");
        if(op=="scheme_get" && ctx.until<=now)return reply(doc,"window_required");
        return wifiSetupCommand(request,ctx.nameSpace,ctx.generation,now,ctx.physicalUntil,ctx.until,ctx.claimed);
    }
    return reply(doc,"unsupported");
}

uint64_t wifiManagementHighWater(const String &principal,const String &nameSpace) {
    for(const auto &c:wifiStore.cursors)if(principal==c.bridge && nameSpace==c.bond)return c.high;
    return 0;
}
