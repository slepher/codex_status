#include "wifi_setup.h"
#include "wifi_scheme.h"
#include "ble_wire.h"
#ifdef CODEX_WIFI_SETUP_HOST_TEST
#include "../tools/wifi-setup-test-hal.h"
#else
#include <Preferences.h>
#include <WiFi.h>
#include <mbedtls/sha256.h>
#include <miniz.h>
#include <esp_heap_caps.h>
static size_t wifiInflate(void *out, size_t cap, const void *in, size_t size) {
    auto *work = (tinfl_decompressor *)heap_caps_malloc(sizeof(tinfl_decompressor), MALLOC_CAP_8BIT);
    if (!work) return 0;
    tinfl_init(work);
    size_t consumed = size, written = cap;
    bool okay = tinfl_decompress(work, (const mz_uint8 *)in, &consumed, (mz_uint8 *)out, (mz_uint8 *)out, &written,
        TINFL_FLAG_PARSE_ZLIB_HEADER | TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF) == TINFL_STATUS_DONE && consumed == size;
    volatile uint8_t *secret = (volatile uint8_t *)work;
    for (size_t i = 0; i < sizeof(*work); ++i) secret[i] = 0;
    heap_caps_free(work); return okay ? written : 0;
}
#endif
#include <algorithm>
#include <memory>
#include <new>
#include <atomic>

static WifiStore wifiStore;
static WifiSchemes &schemes = wifiStore.schemes;
static bool trialOnly = false;
static bool transactionUnified = false;
static bool storeReady = false;
static bool wifiStorageUncertain = false;
static bool wifiOldKeyPresent = false;
static WifiNetwork candidate;
static String transactionPeer, requestId, operation, stage, error, resultIp;
static String oldSsid, oldPassword;
static bool running = false, persisted = false, replaceNetwork = false;
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
static uint8_t fingerprint[32] = {};
static uint32_t transactionGeneration = 0;
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
static bool save(WifiSchemes &next) {
    std::unique_ptr<WifiStore> stored(new(std::nothrow) WifiStore);
    if (!stored || !next.valid() || wifiStore.revision == UINT64_MAX) return false;
    stored->receipts = wifiStore.receipts;
    stored->cursors = wifiStore.cursors;
    stored->schemes = std::move(next);
    stored->revision = wifiStore.revision + 1;
    for (auto &n : stored->schemes.networks) {
        int previous = schemes.find(n.bond, n.id);
        if (previous < 0 || strcmp(n.host, schemes.networks[previous].host) || strcmp(n.ssid, schemes.networks[previous].ssid) || strcmp(n.password, schemes.networks[previous].password) || strcmp(n.label, schemes.networks[previous].label)) n.version = stored->revision;
        else n.version = schemes.networks[previous].version;
    }
    stored->selectActive(stored->schemes.find(stored->schemes.selectedBond, stored->schemes.selectedId));
    return saveV2(*stored);
}

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
    bool old = !prefs.getBytesLength("store3") && prefs.getBytesLength("scheme");
    if (!prefs.getBytesLength("store3") && !old) {
        wifiStore = WifiStore{}; storeReady = true;wifiStorageFormat=0;
    } else {
        std::unique_ptr<WifiStore> check(new(std::nothrow) WifiStore);
        storeReady = check && readWifiStore(prefs, *check);
        if (storeReady) wifiStore = std::move(*check);
    }
    wifiOldKeyPresent = prefs.getBytesLength("scheme") != 0;
    wifiStorageBytes = prefs.getBytesLength("store3");
    if (storeReady) wifiVerifiedStorageBytes = wifiStorageBytes;
    if (storeReady && !old && wifiOldKeyPresent) {
        prefs.remove("scheme");
        wifiOldKeyPresent = prefs.getBytesLength("scheme") != 0;
    }
    prefs.end();
    // Never remove the old key until new bytes have been written and verified.
    if (storeReady && old) storeReady = saveV2(wifiStore);
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
    out["format"] = wifiStorageFormat==4?"WFS4":wifiStorageFormat==3?"WFS3":"empty"; out["ready"] = storeReady; out["uncertain"] = wifiStorageUncertain;
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
String wifiSetupSelectingBridge() { return running && (transactionUnified || operation == "wifi_select" || operation == "wifi_activate") ? selectingBridge : String(); }

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
String wifiSchemesMigrate(JsonDocument &request, const String &peer,
                          const WifiNetwork *slots, size_t count, const String &preferred,
                          uint64_t now, uint64_t physicalUntil, bool allowed) {
    if (!allowed) return reply(request, "unauthorized_or_occupied");
    if (physicalUntil <= now) return reply(request, "physical_required");
    if (!storeReady) return reply(request, "storage");
    if (running) return reply(request, "busy");
    if (!peer.length() || peer.length() > 21 || count > 3) return reply(request, "shape");
    std::unique_ptr<WifiStore> next(new(std::nothrow) WifiStore(wifiStore));
    if (!next || !next->valid()) return reply(request, "memory");
    const char *failure = nullptr;
    bool changed = false;
    int selected = -1;
    for (size_t i = 0; i < count && !failure; ++i) {
        WifiNetwork n = slots[i];
        if (!identifier(n.id, 32) || !ssidValid(n.ssid) || !passwordValid(n.password)) {
            failure = "invalid_old_slot";
        } else {
            n.bond.set(peer.c_str());
            n.host.set(n.ssid);
            int index = next->schemes.find(n.bond, n.id);
            if (index >= 0) {
                if (strcmp(next->schemes.networks[index].ssid, n.ssid) || strcmp(next->schemes.networks[index].password, n.password) || strcmp(next->schemes.networks[index].host, n.host)) failure = "migration_conflict";
            } else {
                failure = next->schemes.put(n, false);
                if (!failure) {
                    changed = true;
                    index = next->schemes.find(n.bond, n.id);
                    next->schemes.networks[index].version = wifiStore.revision + 1;
                    next->schemes.networks[index].label.set((String("Migrated ") + n.id.c_str()).c_str());
                }
            }
            if (!failure && preferred == String(n.ssid)) {
                if (selected >= 0) failure = "active_ambiguous";
                selected = index;
            }
        }
    }
    // put() selects the inserted row. Migration preserves the existing active
    // snapshot, or adopts precisely the old/current connection without WiFi.begin.
    next->schemes.selectedBond.set(schemes.selectedBond);
    next->schemes.selectedId.set(schemes.selectedId);
    if (!failure && !*wifiStore.active.id && count) {
        if (selected < 0) failure = "active_not_found";
        else {
            next->selectActive(selected);
            next->schemes.selectedBond.set(next->active.bond);
            next->schemes.selectedId.set(next->active.id);
            changed = true;
        }
    }
    if (!failure && changed && wifiStore.revision == UINT64_MAX) failure = "capacity";
    if (!failure && changed) {
        next->revision = wifiStore.revision + 1;
        if (!saveV2(*next)) failure = "storage_unknown";
    }
    if (failure) {
        JsonDocument out; deserializeJson(out, reply(request, failure));
        if (!strcmp(failure, "storage_unknown")) {
            out["storage_stage"] = wifiStorageFailure;
            out["storage_bytes"] = wifiStorageBytes;
            out["unencoded_bytes"] = wifiStorageBytes;
            out["nvs_free_entries"] = wifiStorageFree;
        }
        String text; serializeJson(out, text); return text;
    }
    JsonDocument out;
    deserializeJson(out, reply(request));
    out["persisted"] = true; out["migrated"] = count;
    out["revision"] = wifiStore.revision;
    out["storage_bytes"] = wifiStorageBytes;
    out["nvs_free_entries_before"] = wifiStorageFree;
    out["active_network_id"] = String(wifiStore.active.id);
    String text; serializeJson(out, text); return text;
}

static int wifiReceiptIndex(const String &bridge,const String &bond,const String &id) {
    for(size_t i=0;i<wifiStore.receipts.size();++i) {
        const auto &r=wifiStore.receipts[i];if(bridge==r.bridge && bond==r.bond && id==r.request)return i;
    }
    return -1;
}
static bool wifiFinishActivation(const char *failure=nullptr) {
    int index=wifiReceiptIndex(selectingBridge,transactionPeer,requestId);if(index<0)return false;
    std::unique_ptr<WifiStore> next(new(std::nothrow) WifiStore(wifiStore));if(!next||!next->valid())return false;
    auto &r=next->receipts[index];
    if(failure) r.error.set(failure);
    else {
        int slot=next->schemes.find(candidate.bond,candidate.id);
        if(slot<0 || next->schemes.networks[slot].version!=selectionVersion || next->revision==UINT64_MAX)return false;
        next->selectActive(slot);next->schemes.selectedBond=next->active.bond;next->schemes.selectedId=next->active.id;
        ++next->revision;r.revision=next->revision;r.error.clear();
    }
    return saveV2(*next);
}
void wifiSetupConstrain(uint64_t now,uint64_t until,bool ownerAllowed) {
    if(!running || !transactionUnified)return;
    deadline=std::min(deadline,until);
    if(!ownerAllowed || deadline<=now){deadline=0;wifiSetupAbort(ownerAllowed?"window_closed":"occupied");}
}
void wifiSetupAbort(const char *reason) {
    if (!running) return;
    running = false;
    error = reason;
    stage = "failed";
    if(transactionUnified && operation=="wifi_activate" && !wifiStorageUncertain)wifiFinishActivation(reason);
    WiFi.disconnect(false);
    if (oldSsid.length() && (!transactionUnified || deadline > 0)) WiFi.begin(oldSsid.c_str(), oldPassword.c_str());
    candidate.password.clear();
    wipeString(oldPassword);
}
void wifiSetupRadioClosed(const char *reason) {
    // A token/owner-authorized selection uses existing stored credentials and
    // can finish in the formal light Wi-Fi window after normal BLE closes.
    if (!deferredSelection && !transactionUnified) wifiSetupAbort(reason);
}
void wifiSetupPoll(uint64_t now, const String &peer, uint32_t generation, bool encrypted, bool startAllowed) {
    if (!running) return;
    if (!transactionUnified && !deferredSelection && (!encrypted || peer != transactionPeer || generation != transactionGeneration)) {
        wifiSetupAbort("disconnected"); return;
    }
    if (now >= deadline) {
        const char *reported = wifiSetupConnectionFailure();
        const char *failure = WiFi.status() == WL_CONNECTED && WiFi.SSID() == String(candidate.ssid) ? "dhcp_timeout" :
            reported ? reported : "connect_timeout";
        if (transactionUnified) deadline=0;
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
        resultIp = WiFi.localIP().toString(); running = false; stage = "trial_complete"; persisted = false;
        WiFi.disconnect(false);
        if (oldSsid.length()) WiFi.begin(oldSsid.c_str(), oldPassword.c_str());
        candidate.password.clear(); wipeString(oldPassword); return;
    }
    if(transactionUnified && operation=="wifi_activate") {
        if(!wifiFinishActivation()){wifiSetupAbort("storage_unknown");return;}
        resultIp=WiFi.localIP().toString();persisted=true;running=false;stage="persisted";deferredSelection=false;
        candidate.password.clear();wipeString(oldPassword);return;
    }
    WifiSchemes next = schemes;
    const char *failure = nullptr;
    if (operation == "wifi_setup") failure = next.put(candidate, replaceNetwork);
    else {
        next.selectedBond.set(candidate.bond);
        next.selectedId.set(candidate.id);
    }
    if (!failure && !save(next)) failure = "storage";
    if (failure) { wifiSetupAbort(failure); return; }
    resultIp = WiFi.localIP().toString();
    persisted = true;
    running = false;
    stage = "persisted";
    deferredSelection = false;
    candidate.password.clear();
    wipeString(oldPassword);
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
                        bool selectAllowed, bool unified) {
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
    if (!strcmp(op, "wifi_save") || !strcmp(op, "wifi_result")) {
        if (!strcmp(op, "wifi_save") && (!doc["changes"].is<JsonArray>() || !doc["changes"].size() || doc["changes"].size() > 4)) return reply(doc, "shape");
        JsonDocument request; request.set(doc);
        if (!strcmp(op, "wifi_save")) {
            if (!selectAllowed) return reply(doc, "occupied");
            JsonDocument grant;
            if (deserializeJson(grant, wifiManagementAuthorize(doc,peer,now,physicalUntil,lightUntil)) || grant["ok"] != true)
                return reply(doc,"window_required");
            request["permit"] = grant["permit"]; request["op"] = "apply";
        } else { request["op"] = "result"; request["result_peer"] = peer; }
        JsonDocument out; deserializeJson(out,wifiManagementCommand(request,now,"[]"));
        out["ack"] = "wifi"; out["op"] = "wifi"; out["request_id"] = doc["request_id"];
        String text; serializeJson(out,text); return text;
    }
    if (!strcmp(op, "wifi_match")) {
        const char *networkId = doc["network_id"] | "";
        if (!identifier(networkId, 32)) return reply(doc, "shape");
        int slot = unified ? schemes.findShared(networkId) : schemes.find(peer.c_str(), networkId);
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
        out["capacity"] = unified ? 12 : 4; out["wifi_setup_protocol"] = 1;
        out["physical_remaining_s"] = physicalUntil > now ? (physicalUntil - now) / 1000 : 0;
        out["selected_id"] = unified || peer == schemes.selectedBond ? schemes.selectedId.c_str() : "";
        JsonArray networks = out["networks"].to<JsonArray>();
        for (const auto &network : schemes.networks) {
            if (!*network.id || (!unified && peer != network.bond)) continue;
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
        if (transactionPeer != peer || requestId != (doc["query_id"] | "")) return reply(doc, "not_found");
        if (!strcmp(op, "wifi_setup_cancel")) { wifiSetupConstrain(now,lightUntil,selectAllowed); wifiSetupAbort("cancelled"); }
        return reply(doc, nullptr, true);
    }
    bool trial = !strcmp(op, "wifi_trial"), activate = !strcmp(op, "wifi_activate");
    bool select = !strcmp(op, "wifi_select") || activate, setup = !strcmp(op, "wifi_setup") || trial;
    bool remove = !strcmp(op, "wifi_scheme_delete");
    if (!select && !setup && !remove) return reply(doc, "shape");
    // Bind idempotency to the whole payload without retaining transient secrets.
    JsonDocument business;
    if(unified) {for(const char *key:{"op","network_id","version","host_ssid","ssid","password","replace"}) if(!doc[key].isNull())business[key]=doc[key];}
    else business.set(doc);
    String body; serializeJson(business, body);
    uint8_t hash[32];
    mbedtls_sha256((const unsigned char *)body.c_str(), body.length(), hash, 0);
    wipeString(body);
    if (transactionPeer == peer && requestId == id) {
        return !memcmp(hash, fingerprint, sizeof(hash)) ? reply(doc, nullptr, true)
                                                        : reply(doc, "request_conflict");
    }
    if (running) return reply(doc, "busy");
    if (unified && !selectAllowed) return reply(doc,"claim_required");
    if (!unified && (setup || remove) && physicalUntil <= now) return reply(doc, "physical_required");
    if (select && !selectAllowed) return reply(doc, "occupied");
    const char *networkId = doc["network_id"] | "";
    if (!identifier(networkId, 32)) return reply(doc, "shape");
    int slot = unified ? schemes.findShared(networkId) : schemes.find(peer.c_str(), networkId);
    if (slot == -2) return reply(doc, "ambiguous_network_id");
    WifiNetwork network;
    if (activate && (!doc["version"].is<uint64_t>() || doc["version"].as<uint64_t>() != (slot < 0 ? 0 : schemes.networks[slot].version))) return reply(doc, "revision_conflict");
    if (setup) {
        const char *host = doc["host_ssid"] | "", *ssid = doc["ssid"] | "";
        const char *password = doc["password"] | "";
        if (!doc["password"].is<const char *>() || !ssidValid(host) || !ssidValid(ssid) ||
            !passwordValid(password) || !doc["replace"].is<bool>()) return reply(doc, "shape");
        network.bond.set(slot >= 0 ? schemes.networks[slot].bond.c_str() : peer.c_str()); network.id.set(networkId);
        network.host.set(host); network.ssid.set(ssid); network.password.set(password);
        if (slot >= 0) { network.label = schemes.networks[slot].label; network.version = schemes.networks[slot].version; }
        WifiSchemes next = schemes;
        const char *failure = trial ? nullptr : next.put(network, doc["replace"].as<bool>());
        if (failure) { network.password.clear(); return reply(doc, failure); }
    } else {
        if (slot < 0) return reply(doc, "not_found");
        network = schemes.networks[slot];
    }
    uint64_t until = physicalUntil > now ? physicalUntil : lightUntil;
    if (!network.valid()) return reply(doc, "memory");
    if (!remove && until <= now) { network.password.clear(); return reply(doc, "window_required"); }
    if (remove) {
        const bool selected = !strcmp(schemes.selectedBond, network.bond) &&
                              !strcmp(schemes.selectedId, network.id);
        WifiSchemes next = schemes;
        if (!next.erase(slot)) return reply(doc, "memory");
        bool saved = save(next);
        network.password.clear();
        if (!saved) return reply(doc, "storage");
        if (selected) WiFi.disconnect(false);
    }
    if(unified && (trial || activate)) {
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
        receipt.operation.set(trial?"trial":"activate");receipt.target.set(networkId);receipt.targetVersion=network.version;receipt.revision=next->revision;memcpy(receipt.hash,hash,32);
        if(!next->receipts.append(receipt))return reply(doc,"memory");
        if(seq){if(cursor<0){WifiCursor c;c.bridge.set(bridge.c_str());c.bond.set(peer.c_str());c.high=seq;if(!next->cursors.append(c))return reply(doc,"memory");}else next->cursors[cursor].high=seq;}
        if(!saveV2(*next))return reply(doc,"storage_unknown");
    }
    transactionUnified = unified;
    trialOnly = trial;
    transactionPeer = peer; requestId = id; operation = op; transactionGeneration = generation;
    candidate = network;
    if (!candidate.valid()) return reply(doc, "memory");
    network.password.clear();
    selectionVersion = network.version;
    wifiDisconnectReason = 0;
    memcpy(fingerprint, hash, sizeof(hash));
    error = ""; resultIp = ""; persisted = remove;
    deferredSelection = select && physicalUntil <= now;
    selectionStarted = false;
    selectingBridge = (select || unified) ? (doc["bridge_id"] | "") : "";
    stage = remove ? "persisted" : deferredSelection ? "accepted" : "connecting";
    if (!remove) {
        oldSsid = WiFi.status() == WL_CONNECTED ? WiFi.SSID() : String();
        oldPassword = WiFi.status() == WL_CONNECTED ? WiFi.psk() : String();
        replaceNetwork = doc["replace"] | false;
        deadline = std::min(until, now + 25000);
        running = true;
        if (!deferredSelection && (WiFi.status() != WL_CONNECTED ||
            WiFi.SSID() != String(candidate.ssid) || WiFi.localIP() == IPAddress(0,0,0,0))) {
            WiFi.disconnect(false);
            WiFi.mode(WiFi.getMode() == WIFI_AP ? WIFI_AP_STA : WIFI_STA);
            wifiSetupConnectionStarted(String(candidate.ssid));
            WiFi.begin(candidate.ssid, candidate.password);
        }
    } else candidate.password.clear();
    return reply(doc, nullptr, true);
}

#include "wifi_management.inc"
