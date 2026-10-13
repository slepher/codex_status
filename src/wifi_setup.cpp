#include "wifi_setup.h"
#include "wifi_scheme.h"
#include "ble_wire.h"
#ifdef CODEX_WIFI_SETUP_HOST_TEST
#include "../tools/wifi-setup-test-hal.h"
#else
#include <Preferences.h>
#include <WiFi.h>
#include <mbedtls/sha256.h>
#endif
#include <algorithm>
#include <memory>
#include <new>
#include <atomic>

static WifiStore wifiStore;
static WifiSchemes &schemes = wifiStore.schemes;
static bool trialOnly = false;
static bool storeReady = false;
// Only generations published in a pending commit are used for execution.
// A wake with no control transaction does not incur an extra Flash write.
static uint64_t wifiExecutionGeneration = 0;
static bool wifiStorageUncertain = false;
static bool wifiOldKeyPresent = false;
static WifiNetwork candidate;
static String transactionPeer, requestId, stage, error, resultIp;
static String oldSsid, oldPassword;
static bool running = false, persisted = false;
static bool deferredSelection = false;
static bool selectionStarted = false;
static String selectingBridge;
static uint64_t selectionVersion = 0;
static volatile uint16_t wifiDisconnectReason = 0;
// The event task is the sole writer and never calls DevLog. Preserve reason and sequence
// even if GOT_IP clears the transaction error before the main loop sees it.
static std::atomic<uint32_t> wifiDisconnectEvent{0};
uint32_t wifiSetupDisconnectEvent() { return wifiDisconnectEvent.load(std::memory_order_relaxed); }
static std::atomic<uint16_t> wifiAttemptFailure{0};
static std::atomic<uint32_t> wifiAssociatedEvent{0}, wifiIpEvent{0};
uint32_t wifiSetupAssociatedEvent() { return wifiAssociatedEvent.load(std::memory_order_relaxed); }
uint32_t wifiSetupIpEvent() { return wifiIpEvent.load(std::memory_order_relaxed); }
static String attemptedSsid;
void wifiSetupConnectionStarted(const String &ssid) { attemptedSsid = ssid; wifiDisconnectReason = 0; wifiAttemptFailure.store(0, std::memory_order_relaxed); if (!running) error = ""; }
static const char *wifiConnectionFailure(uint16_t reason) {
    switch(reason) {
        case 202: return "authentication_failed";
        case 15: case 204: return "handshake_timeout";
        case 201: return "network_not_found";
        case 210: case 211: return "security_mismatch";
        case 212: return "signal_weak";
        case 203: return "association_failed";
        default: return nullptr;
    }
}
uint16_t wifiSetupConnectionReason() {
    const uint16_t first = wifiAttemptFailure.load(std::memory_order_relaxed);
    return first ? first : wifiDisconnectReason;
}
const char *wifiSetupConnectionFailure() {
    // A later generic disconnect (e.g. local teardown reason 36) must not
    // erase the specific AP/auth failure observed in this association attempt.
    return wifiConnectionFailure(wifiSetupConnectionReason());
}
static uint64_t deadline = 0;
static String readSnapshot, readPeer, readOperation;
static uint32_t readGeneration = 0;
static uint32_t readCrc = 0;

String wifiSetupReadPage(JsonDocument &request, const String &peer, uint32_t generation,
                         const String &payload) {
    uint32_t offset = request["offset"] | 0u;
    if (!request["offset"].isNull() && !request["offset"].is<uint32_t>()) offset = UINT32_MAX;
    const String op = request["op"] | "";
    JsonDocument out;
    out["ack"] = "wifi"; out["request_id"] = request["request_id"];
    out["device_mac"] = request["device_mac"];
    if (!offset) {
        readSnapshot = payload; readPeer = peer; readGeneration = generation; readOperation = op;
        readCrc = v2Crc32((const uint8_t *)payload.c_str(), payload.length());
    }
    bool okay = readPeer == peer && readGeneration == generation && readOperation == op &&
        offset < readSnapshot.length() && readSnapshot.length() <= 4096 &&
        (!offset || (request["snapshot"] | 0u) == readCrc);
    out["ok"] = okay;
    if (okay) {
        uint32_t next = std::min((uint32_t)readSnapshot.length(), offset + 80);
        static const char hex[] = "0123456789abcdef";
        String chunk; chunk.reserve((next - offset) * 2);
        for (uint32_t i = offset; i < next; ++i) {
            uint8_t c = readSnapshot[i]; chunk += hex[c >> 4]; chunk += hex[c & 15];
        }
        out["chunk"] = chunk; out["snapshot"] = readCrc;
        out["next_offset"] = next; out["total"] = readSnapshot.length();
    } else out["error"] = "read_snapshot";
    String text; serializeJson(out, text); return text;
}

static void wipe(char *p, size_t size) {
    volatile char *v = p;
    while (size--) *v++ = 0;
}
static void wipeString(String &text) {
    if (text.length()) wipe(const_cast<char *>(text.c_str()), text.length());
    text = "";
}
static const char *wifiStorageFailure = "storage_unknown";
static uint8_t wifiStorageFormat=0;
static size_t wifiStorageFree = 0, wifiStorageBytes = 0, wifiVerifiedStorageBytes = 0;
static bool identifier(const char *, size_t);
static bool ssidValid(const char *);
static bool passwordValid(const char *);
#include "wifi_storage.inc"
static void wifiSetupDisconnected(uint16_t reason) {
    wifiDisconnectReason = reason;
    if (wifiConnectionFailure(reason)) {
        uint16_t expected = 0;
        wifiAttemptFailure.compare_exchange_strong(expected, reason, std::memory_order_relaxed);
    }
    const uint32_t next = (wifiDisconnectEvent.load(std::memory_order_relaxed) >> 16) + 1;
    wifiDisconnectEvent.store((next << 16) | reason, std::memory_order_relaxed);
}
static void wifiSetupGotIp() {
    wifiDisconnectReason = 0;
    wifiAttemptFailure.store(0, std::memory_order_relaxed);
    wifiIpEvent.fetch_add(1, std::memory_order_relaxed);
}

bool wifiSchemesBegin() {
#ifndef CODEX_WIFI_SETUP_HOST_TEST
    static bool eventsInstalled = false;
    if (!eventsInstalled) {
        WiFi.onEvent([](arduino_event_id_t, arduino_event_info_t info) {
            wifiSetupDisconnected(info.wifi_sta_disconnected.reason);
        }, ARDUINO_EVENT_WIFI_STA_DISCONNECTED);
        WiFi.onEvent([](arduino_event_id_t, arduino_event_info_t) {
            wifiAssociatedEvent.fetch_add(1, std::memory_order_relaxed);
        }, ARDUINO_EVENT_WIFI_STA_CONNECTED);
        WiFi.onEvent([](arduino_event_id_t, arduino_event_info_t) {
            wifiSetupGotIp();
        }, ARDUINO_EVENT_WIFI_STA_GOT_IP);
        eventsInstalled = true;
    }
#endif
    Preferences prefs;
    if (!prefs.begin("wifi-bonds", false)) { storeReady = false; return false; }
    wifiStorageFormat=0;wifiStorageSplit=false;wifiStorageCleanupPending=false;
    wifiConfigurationBytes=wifiTransactionBytes=wifiStorageBytes=0;
    if (!prefs.isKey("commit") && !prefs.isKey("store3") && !prefs.isKey("scheme") && !wifiPreparedKeys(prefs)) {
        wifiStore = WifiStore{}; storeReady = true;wifiStorageFormat=0;
    } else {
        std::unique_ptr<WifiStore> check(new(std::nothrow) WifiStore);
        storeReady = check && readWifiStore(prefs, *check);
        if (storeReady) wifiStore = std::move(*check);
    }
    wifiOldKeyPresent = prefs.isKey("scheme");
    if (storeReady) wifiVerifiedStorageBytes = wifiStorageBytes;
    prefs.end();
    if (storeReady && !wifiExecutionGeneration && wifiStore.executionGeneration != UINT64_MAX)
        wifiExecutionGeneration = wifiStore.executionGeneration + 1;
    if (storeReady) {
        bool interrupted = false;
        for (const auto &r : wifiStore.receipts) if (r.executionGeneration &&
            r.executionGeneration != wifiExecutionGeneration && !strcmp(r.error,"pending")) interrupted = true;
        if (interrupted) {
            std::unique_ptr<WifiStore> recovered(new(std::nothrow) WifiStore(wifiStore));
            storeReady = recovered && recovered->valid();
            if (storeReady) {
                for (auto &r : recovered->receipts) if (r.executionGeneration &&
                    r.executionGeneration != wifiExecutionGeneration && !strcmp(r.error,"pending")) r.error.set("interrupted");
                // Valid pending bytes prove activate never committed its new
                // active row: that change and its success receipt are atomic.
                storeReady = saveV2(*recovered);
            }
        }
    }
    if (storeReady) wifiStorageUncertain = false;
    return storeReady;
}
bool wifiSchemesCredentials(String &ssid, String &password) {
    if (running && deferredSelection) {
        ssid = candidate.ssid.c_str(); password = candidate.password.c_str(); return true;
    }
    if (!storeReady || !*wifiStore.active.id) return false;
    const auto *active = wifiStore.activeCredentials();
    if (!*active->ssid) return false;
    ssid = active->ssid.c_str(); password = active->password.c_str();
    return true;
}
void wifiSchemesStorageStatus(JsonObject out) {
    out["transaction_format"]=!storeReady?"unreadable":wifiStorageSplit?"WTX1":wifiStorageFormat?"embedded":"empty";
    out["configuration_bytes"]=wifiConfigurationBytes;out["transaction_bytes"]=wifiTransactionBytes;
    out["cleanup_pending"]=wifiStorageCleanupPending;
    out["format"] = !storeReady?"unreadable":wifiStorageFormat==4?"WFS4":"empty"; out["ready"] = storeReady; out["uncertain"] = wifiStorageUncertain;
    out["bytes"] = wifiVerifiedStorageBytes; out["old_key_present"] = wifiOldKeyPresent;
    out["networks"] = schemes.networks.size(); out["receipts"] = wifiStore.receipts.size();
    out["revision"]=wifiStore.revision;out["cursors"]=wifiStore.cursors.size();out["cursor_bytes"]=sizeof(WifiCursor);
    out["ram_requested_bytes"] = sizeof(wifiStore) + wifiStore.heapBytes();
    out["record_bytes"] = sizeof(WifiNetwork); out["receipt_bytes"] = sizeof(WifiReceipt);
}
bool wifiSetupBusy() { return running; }
bool wifiSetupConnecting() {
    return running && (!deferredSelection || selectionStarted) &&
        (WiFi.status() != WL_CONNECTED || WiFi.localIP() == IPAddress(0,0,0,0));
}
bool wifiSetupFailed() { return !running && stage == "failed"; }
bool wifiSetupActivated() { return persisted && !trialOnly; }
String wifiSetupSelectingBridge() { return running ? selectingBridge : String(); }

static bool identifier(const char *p, size_t limit) {
    size_t size = strlen(p);
    if (!size || size > limit) return false;
    for (; *p; ++p) if (!((*p >= 'a' && *p <= 'z') || (*p >= 'A' && *p <= 'Z') || (*p >= '0' && *p <= '9') || *p == '-' || *p == '_')) return false;
    return true;
}
static bool ssidValid(const char *p) { return *p && bleWireText(String(p), 32, false); }
static bool passwordValid(const char *p) {
    size_t size = strlen(p);
    if (!size || (size >= 8 && size <= 63)) return true;
    if (size != 64) return false;
    for (; *p; ++p) if (!isxdigit((unsigned char)*p)) return false;
    return true;
}
static String reply(JsonDocument &request, const char *failure = nullptr, bool includeResult = false) {
    JsonDocument out;
    out["ack"] = "wifi";
    out["op"] = "wifi";
    out["request_id"] = request["request_id"];
    out["device_mac"] = request["device_mac"];
    out["ok"] = !failure;
    out["stage"] = failure ? "rejected" : "ready";
    if (failure) out["error"] = failure;
    if (includeResult) {
        out["query_id"] = requestId;
        out["network_id"] = candidate.id.c_str();
        out["stage"] = stage;
        out["persisted"] = persisted;
        out["ip"] = resultIp;
        out["ok"] = !error.length();
        if (error.length()) out["error"] = error;
    }
    String text;
    serializeJson(out, text);
    return text;
}
static int wifiReceiptIndex(const String &bridge,const String &bond,const String &id) {
    for(size_t i=0;i<wifiStore.receipts.size();++i) {
        if(wifiStore.receipts[i].matches(bridge.c_str(),bond.c_str(),id.c_str()))return i;
    }
    return -1;
}
static bool wifiFinishActivation(const char *failure=nullptr) {
    int index=wifiReceiptIndex(selectingBridge,transactionPeer,requestId);if(index<0)return false;
    std::unique_ptr<WifiStore> next(new(std::nothrow) WifiStore(wifiStore));if(!next||!next->valid())return false;
    auto &r=next->receipts[index];
    if(failure) r.error.set(failure);
    else if (!trialOnly) {
        int slot=next->schemes.find(candidate.bond,candidate.id);
        if(slot<0 || next->schemes.networks[slot].version!=selectionVersion || next->revision==UINT64_MAX)return false;
        next->selectActive(slot);next->schemes.selectedBond=next->active.bond;next->schemes.selectedId=next->active.id;
        ++next->revision;r.revision=next->revision;r.error.clear();
    } else r.error.clear();
    return saveV2(*next);
}
void wifiSetupConstrain(uint64_t now,uint64_t until,bool ownerAllowed) {
    if(!running)return;
    deadline=std::min(deadline,until);
    if(!ownerAllowed || deadline<=now){deadline=0;wifiSetupAbort(ownerAllowed?"window_closed":"occupied");}
}
void wifiSetupAbort(const char *reason) {
    if (!running) return;
    running = false;
    error = reason;
    stage = "failed";
    if(!wifiStorageUncertain && !wifiFinishActivation(reason)) error="storage_unknown";
    WiFi.disconnect(false);
    if (oldSsid.length() && deadline > 0) WiFi.begin(oldSsid.c_str(), oldPassword.c_str());
    candidate.password.clear();
    wipeString(oldPassword);
}
void wifiSetupPoll(uint64_t now, bool startAllowed) {
    if (!running) return;
    if (now >= deadline) {
        const char *reported = wifiSetupConnectionFailure();
        const char *failure = WiFi.status() == WL_CONNECTED && WiFi.SSID() == String(candidate.ssid) ? "dhcp_timeout" :
            reported ? reported : "connect_timeout";
        deadline=0;
        wifiSetupAbort(failure); return;
    }
    if (deferredSelection && !selectionStarted) {
        if (!startAllowed) return;
        // The accepted transaction survives ordinary BLE release. Start RF
        // only when the main loop has ended the rendezvous BLE exchange.
        selectionStarted = true;
        stage = "connecting";
        if (WiFi.status() != WL_CONNECTED || WiFi.SSID() != String(candidate.ssid)) {
            WiFi.disconnect(false); WiFi.mode(WIFI_STA);
            wifiSetupConnectionStarted(String(candidate.ssid));
            WiFi.begin(candidate.ssid, candidate.password);
        }
    }
    if (WiFi.status() != WL_CONNECTED || WiFi.localIP() == IPAddress(0, 0, 0, 0)) return;
    // Do not accept a stale link/IP from the network before WiFi.begin.
    if (WiFi.SSID() != String(candidate.ssid)) return;
    if (trialOnly) {
        if(!wifiFinishActivation()) { wifiSetupAbort("storage_unknown"); return; }
        resultIp = WiFi.localIP().toString(); running = false; stage = "trial_complete"; persisted = false;
        WiFi.disconnect(false);
        if (oldSsid.length()) WiFi.begin(oldSsid.c_str(), oldPassword.c_str());
        candidate.password.clear(); wipeString(oldPassword); return;
    }
    if(!wifiFinishActivation()){wifiSetupAbort("storage_unknown");return;}
    resultIp=WiFi.localIP().toString();persisted=true;running=false;stage="persisted";deferredSelection=false;
    candidate.password.clear();wipeString(oldPassword);
}

// Same length-delimited metadata fingerprint as Bridge; no password bytes.
static String wifiMetadataKey(const WifiNetwork &network, const char *label, bool open) {
    uint8_t payload[132], hash[32]; size_t size = 0;
    for (const char *value : {network.host.c_str(), network.ssid.c_str(), label}) {
        size_t length = strlen(value); memcpy(payload+size,value,length+1); size += length+1;
    }
    payload[size++] = open ? 1 : 0;
    mbedtls_sha256(payload,size,hash,0);
    static const char hex[]="0123456789abcdef"; char text[65];
    for(size_t i=0;i<32;++i){text[2*i]=hex[hash[i]>>4];text[2*i+1]=hex[hash[i]&15];}text[64]=0;
    return String(text);
}
String wifiSetupCommand(JsonDocument &doc, const String &peer, uint32_t generation,
                        uint64_t now, uint64_t physicalUntil, uint64_t lightUntil,
                        bool selectAllowed) {
    const char *op = doc["op"] | "";
    const char *id = doc["request_id"] | "";
    if (!identifier(id, 64) || !peer.length() || peer.length() > 21) return reply(doc, "shape");
    for (const char *key : {"op", "request_id", "network_id", "query_id", "host_ssid", "ssid", "password", "label", "credential", "metadata_key", "credential_key"}) {
        if (doc[key].isNull()) continue;
        JsonString text = doc[key].as<JsonString>();
        if (!text || text.size() != strlen(text.c_str())) return reply(doc, "shape");
    }
    if (wifiStorageUncertain && !running && !wifiSchemesBegin()) return reply(doc, "storage");
    if (!storeReady) return reply(doc, "storage");
    if (!strcmp(op, "wifi_match")) {
        const char *networkId = doc["network_id"] | "";
        if (!identifier(networkId, 32)) return reply(doc, "shape");
        int slot = schemes.findShared(networkId);
        if (slot == -2) return reply(doc, "ambiguous_network_id");
        JsonDocument out;
        out["ack"] = "wifi"; out["op"] = "wifi"; out["request_id"] = id;
        out["device_mac"] = doc["device_mac"]; out["ok"] = true; out["network_id"] = networkId;
        out["version"] = slot < 0 ? 0 : schemes.networks[slot].version;
        if (!doc["open"].is<bool>() || String(doc["metadata_key"] | "").length()!=64) return reply(doc,"shape");
        bool open = doc["open"].as<bool>();
        bool matches = slot >= 0 && wifiMetadataKey(schemes.networks[slot],schemes.networks[slot].label.c_str(),open) == (doc["metadata_key"] | "") &&
            (!open || !*schemes.networks[slot].password);
        if(!doc["credential_key"].isNull()) {
            String key=doc["credential_key"] | "";
            if(key.length()!=64)return reply(doc,"shape");
            if(slot>=0) {
                uint8_t digest[32];char hex[65];const char *password=schemes.networks[slot].password;
                mbedtls_sha256((const uint8_t *)password,strlen(password),digest,0);
                static const char digits[]="0123456789abcdef";
                for(size_t i=0;i<32;++i){hex[2*i]=digits[digest[i]>>4];hex[2*i+1]=digits[digest[i]&15];}hex[64]=0;
                matches=matches&&key==hex;
            }
        }
        out["matches"] = matches;
        bool active = slot >= 0 && String(schemes.networks[slot].bond) == wifiStore.active.bond && String(networkId) == wifiStore.active.id &&
            wifiStore.activeVersion == schemes.networks[slot].version;
        bool connected = active && WiFi.status() == WL_CONNECTED && WiFi.SSID() == String(schemes.networks[slot].ssid) &&
            !(WiFi.localIP() == IPAddress(0,0,0,0));
        out["active"] = active; out["connected"] = connected;
        if (WiFi.status() == WL_CONNECTED) out["actual_ssid"] = WiFi.SSID();
        bool currentAttempt = slot >= 0 && transactionPeer == peer && String(networkId) == candidate.id &&
            selectionVersion == schemes.networks[slot].version && (running || error.length());
        const char *normalFailure = active && !connected && !currentAttempt && attemptedSsid == String(schemes.networks[slot].ssid) ?
            wifiSetupConnectionFailure() : nullptr;
        out["stage"] = connected ? "connected" : currentAttempt ? stage : normalFailure ? "failed" : slot < 0 ? "missing" : "saved";
        if (currentAttempt && error.length()) { out["error"] = error; out["reason"] = wifiSetupConnectionReason(); }
        else if (normalFailure) { out["error"] = normalFailure; out["reason"] = wifiSetupConnectionReason(); }
        if (connected) out["ip"] = WiFi.localIP().toString();
        String text; serializeJson(out,text); return text;
    }
    if (!strcmp(op, "wifi_active")) {
        JsonDocument out; out["ack"] = "wifi"; out["op"] = "wifi"; out["request_id"] = id;
        out["device_mac"] = doc["device_mac"]; out["ok"] = true;
        out["network_id"] = wifiStore.active.id.c_str(); out["version"] = wifiStore.activeVersion;
        out["revision"] = wifiStore.revision;
        String text; serializeJson(out, text); return text;
    }
    if (!strcmp(op, "wifi_scheme_get")) {
        JsonDocument out;
        out["ack"] = "wifi"; out["op"] = op; out["request_id"] = id;
        out["device_mac"] = doc["device_mac"]; out["ok"] = true;
        out["capacity"] = 12; out["wifi_setup_protocol"] = 1;
        out["physical_remaining_s"] = physicalUntil > now ? (physicalUntil - now) / 1000 : 0;
        out["selected_id"] = schemes.selectedId.c_str();
        JsonArray networks = out["networks"].to<JsonArray>();
        for (const auto &network : schemes.networks) {
            if (!*network.id) continue;
            JsonObject n = networks.add<JsonObject>();
            n["network_id"] = network.id.c_str(); n["host_ssid"] = network.host.c_str(); n["ssid"] = network.ssid.c_str();
            int index = schemes.find(network.bond, network.id);
            n["label"] = schemes.networks[index].label.c_str(); n["version"] = schemes.networks[index].version;
            n["password"] = String(network.password);
        }
        String text; serializeJson(out, text);
        return wifiSetupReadPage(doc, peer, generation, text);
    }
    if (!strcmp(op, "wifi_setup_status") || !strcmp(op, "wifi_setup_cancel")) {
        if (transactionPeer != peer || selectingBridge != (doc["bridge_id"] | "") || requestId != (doc["query_id"] | "")) return reply(doc, "not_found");
        if (!strcmp(op, "wifi_setup_cancel")) { wifiSetupConstrain(now,lightUntil,selectAllowed); wifiSetupAbort("cancelled"); }
        return reply(doc, nullptr, true);
    }
    bool trial = !strcmp(op, "wifi_trial"), activate = !strcmp(op, "wifi_activate");
    if (!trial && !activate) return reply(doc, "unsupported");
    // Bind idempotency to the whole payload without retaining transient secrets.
    JsonDocument business;
    for(const char *key:{"op","network_id","version","host_ssid","ssid","password","replace"}) if(!doc[key].isNull())business[key]=doc[key];
    String body; serializeJson(business, body);
    uint8_t hash[32];
    mbedtls_sha256((const unsigned char *)body.c_str(), body.length(), hash, 0);
    wipeString(body);
    if (running) return reply(doc, "busy");
    if (!selectAllowed) return reply(doc,"claim_required");
    const char *networkId = doc["network_id"] | "";
    if (!identifier(networkId, 32)) return reply(doc, "shape");
    int slot = schemes.findShared(networkId);
    if (slot == -2) return reply(doc, "ambiguous_network_id");
    WifiNetwork network;
    if (activate && (!doc["version"].is<uint64_t>() || doc["version"].as<uint64_t>() != (slot < 0 ? 0 : schemes.networks[slot].version))) return reply(doc, "revision_conflict");
    if (trial) {
        const char *host = doc["host_ssid"] | "", *ssid = doc["ssid"] | "";
        const char *password = doc["password"] | "";
        if (!doc["password"].is<const char *>() || !ssidValid(host) || !ssidValid(ssid) ||
            !passwordValid(password) || !doc["replace"].is<bool>()) return reply(doc, "shape");
        network.bond.set(slot >= 0 ? schemes.networks[slot].bond.c_str() : peer.c_str()); network.id.set(networkId);
        network.host.set(host); network.ssid.set(ssid); network.password.set(password);
        if (slot >= 0) { network.label = schemes.networks[slot].label; network.version = schemes.networks[slot].version; }
    } else {
        if (slot < 0) return reply(doc, "not_found");
        network = schemes.networks[slot];
    }
    uint64_t until = physicalUntil > now ? physicalUntil : lightUntil;
    if (!network.valid()) return reply(doc, "memory");
    if (until <= now) { network.password.clear(); return reply(doc, "window_required"); }
    {
        if(!wifiExecutionGeneration || wifiExecutionGeneration < wifiStore.executionGeneration) return reply(doc,"generation_exhausted");
        String bridge=doc["bridge_id"]|"";uint64_t seq=wifiRequestSequence(id);
        int existing=wifiReceiptIndex(bridge,peer,String(id));
        if(existing>=0) return reply(doc,"request_already_recorded");
        if(wifiStore.receipts.size()>=8)return reply(doc,"result_capacity");
        int cursor=-1;
        for(size_t i=0;i<wifiStore.cursors.size();++i)if(bridge==wifiStore.cursors[i].bridge && peer==wifiStore.cursors[i].bond)cursor=i;
        if(seq && cursor>=0 && seq<=wifiStore.cursors[cursor].high)return reply(doc,"request_retired");
        if(seq && cursor<0 && wifiStore.cursors.size()>=8)return reply(doc,"cursor_capacity");
        std::unique_ptr<WifiStore> next(new(std::nothrow) WifiStore(wifiStore));if(!next||!next->valid())return reply(doc,"memory");
        WifiReceipt receipt;receipt.bridge.set(bridge.c_str());receipt.bond.set(peer.c_str());receipt.request.set(id);receipt.error.set("pending");
        next->executionGeneration=wifiExecutionGeneration;receipt.executionGeneration=wifiExecutionGeneration;
        receipt.operation.set(trial?"trial":"activate");receipt.target.set(networkId);receipt.targetVersion=network.version;receipt.revision=next->revision;memcpy(receipt.hash,hash,32);
        if(!next->receipts.append(receipt))return reply(doc,"memory");
        if(seq){if(cursor<0){WifiCursor c;c.bridge.set(bridge.c_str());c.bond.set(peer.c_str());c.high=seq;if(!next->cursors.append(c))return reply(doc,"memory");}else next->cursors[cursor].high=seq;}
        if(!saveV2(*next))return reply(doc,"storage_unknown");
    }
    trialOnly = trial;
    transactionPeer = peer; requestId = id;
    candidate = network;
    if (!candidate.valid()) return reply(doc, "memory");
    network.password.clear();
    selectionVersion = network.version;
    wifiDisconnectReason = 0;
    error = ""; resultIp = ""; persisted = false;
    deferredSelection = activate && physicalUntil <= now;
    selectionStarted = false;
    selectingBridge = doc["bridge_id"] | "";
    stage = deferredSelection ? "accepted" : "connecting";
    oldSsid = WiFi.status() == WL_CONNECTED ? WiFi.SSID() : String();
    oldPassword = WiFi.status() == WL_CONNECTED ? WiFi.psk() : String();
    deadline = std::min(until, now + 25000);
    running = true;
    if (!deferredSelection && (WiFi.status() != WL_CONNECTED ||
        WiFi.SSID() != String(candidate.ssid) || WiFi.localIP() == IPAddress(0,0,0,0))) {
        WiFi.disconnect(false);
        WiFi.mode(WiFi.getMode() == WIFI_AP ? WIFI_AP_STA : WIFI_STA);
        wifiSetupConnectionStarted(String(candidate.ssid));
        WiFi.begin(candidate.ssid, candidate.password);
    }
    return reply(doc, nullptr, true);
}

#include "wifi_management.inc"
