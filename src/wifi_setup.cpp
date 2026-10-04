#include "wifi_setup.h"
#include "wifi_scheme.h"
#include "ble_wire.h"
static const char *wifiEncodeStage = "encode";
static int wifiEncodeStatus = 0;
#ifdef CODEX_WIFI_SETUP_HOST_TEST
#include "../tools/wifi-setup-test-hal.h"
#else
#include <Preferences.h>
#include <WiFi.h>
#include <mbedtls/sha256.h>
#include <miniz.h>
#include <esp_heap_caps.h>
static size_t wifiDeflate(void *out, size_t cap, const void *in, size_t size) {
    // ROM convenience wrappers may have no allocator. Supply the workspace;
    // keep the large dictionary off the Arduino loop stack and internal heap.
    auto *work = (tdefl_compressor *)heap_caps_malloc(sizeof(tdefl_compressor), MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    if (!work) work = (tdefl_compressor *)heap_caps_malloc(sizeof(tdefl_compressor), MALLOC_CAP_8BIT);
    wifiEncodeStage = "workspace"; wifiEncodeStatus = -1;
    if (!work) return 0;
    size_t consumed = size, written = cap;
    wifiEncodeStage = "initialize";
    wifiEncodeStatus = tdefl_init(work, nullptr, nullptr, TDEFL_WRITE_ZLIB_HEADER | TDEFL_DEFAULT_MAX_PROBES);
    bool okay = wifiEncodeStatus == TDEFL_STATUS_OKAY;
    if (okay) {
        wifiEncodeStage = "compress";
        wifiEncodeStatus = tdefl_compress(work, in, &consumed, out, &written, TDEFL_FINISH);
        okay = wifiEncodeStatus == TDEFL_STATUS_DONE && consumed == size;
    }
    volatile uint8_t *secret = (volatile uint8_t *)work;
    for (size_t i = 0; i < sizeof(*work); ++i) secret[i] = 0;
    heap_caps_free(work); return okay ? written : 0;
}
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

static WifiSchemes schemes;
static WifiStoreV2 wifiStore;
static bool trialOnly = false;
static bool storeReady = false;
static WifiNetwork candidate;
static String transactionPeer, requestId, operation, stage, error, resultIp;
static String oldSsid, oldPassword;
static bool running = false, persisted = false, replaceNetwork = false;
static bool deferredSelection = false;
static bool selectionStarted = false;
static String selectingBridge;
static uint64_t selectionVersion = 0;
static volatile uint16_t wifiDisconnectReason = 0;
static String attemptedSsid;
void wifiSetupConnectionStarted(const String &ssid) { attemptedSsid = ssid; wifiDisconnectReason = 0; if (!running) error = ""; }
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
static size_t wifiStorageFree = 0, wifiStorageBytes = 0;
static bool readWifiStore(Preferences &prefs, WifiStoreV2 &out) {
    size_t size = prefs.getBytesLength("scheme");
    if (!size || size > sizeof(WifiStoreV2) + 64) return false;
    std::unique_ptr<uint8_t[]> bytes(new(std::nothrow) uint8_t[size]);
    if (!bytes) return false;
    bool okay = prefs.getBytes("scheme", bytes.get(), size) == size;
    if (okay && size > 4 && !memcmp(bytes.get(), "WFZ2", 4)) {
        okay = wifiInflate(&out, sizeof(out), bytes.get()+4, size-4) == sizeof(out) && out.valid();
    } else if (okay && size == sizeof(WifiStoreV2)) {
        memcpy(&out, bytes.get(), sizeof(out)); okay = out.valid();
    } else if (okay && size == sizeof(WifiSchemes)) {
        // Construct defaults in place: a 5 KiB assignment temporary overflowed loopTask.
        new (&out) WifiStoreV2{}; memcpy(&out.schemes, bytes.get(), sizeof(WifiSchemes));
        okay = out.schemes.valid();
        if (okay) {
            int active = out.schemes.find(out.schemes.selectedBond, out.schemes.selectedId);
            if (active >= 0) out.active = out.schemes.networks[active];
        }
    } else okay = false;
    wipe((char *)bytes.get(), size); return okay;
}
static bool saveV2(WifiStoreV2 &next) {
    next.seal();
    // Fixed empty networks/receipts consumed scarce NVS pages. Keep the exact
    // CRC-protected structure, but store its zlib encoding as one atomic blob.
    const size_t capacity = sizeof(next) + 64;
    std::unique_ptr<uint8_t[]> bytes(new(std::nothrow) uint8_t[capacity]);
    if (!bytes) { wifiStorageFailure = "storage_encode"; return false; }
    memcpy(bytes.get(), "WFZ2", 4);
    size_t compressed = wifiDeflate(bytes.get()+4, capacity-4, &next, sizeof(next));
    Preferences prefs;
    bool opened = compressed && prefs.begin("wifi-bonds", false);
    bool okay = false;
    wifiStorageBytes = compressed ? compressed + 4 : 0;
    wifiStorageFailure = compressed ? "storage_open" : "storage_encode";
    std::unique_ptr<WifiStoreV2> check;
    if (opened) {
        wifiStorageFree = prefs.freeEntries();
        okay = prefs.putBytes("scheme", bytes.get(), wifiStorageBytes) == wifiStorageBytes;
        wifiStorageFailure = okay ? "storage_readback" : "storage_write";
        check.reset(new(std::nothrow) WifiStoreV2);
        okay = okay && check && readWifiStore(prefs, *check) && !memcmp(check.get(), &next, sizeof(next));
        prefs.end();
    }
    if (okay) { wifiStore = next; schemes = next.schemes; }
    if (check) wipe((char *)check.get(), sizeof(*check));
    wipe((char *)bytes.get(), capacity); return okay;
}
static bool save(WifiSchemes &next) {
    std::unique_ptr<WifiStoreV2> buffer(new(std::nothrow) WifiStoreV2(wifiStore));
    if(!buffer)return false;
    auto &stored=*buffer;
    stored.schemes = next;
    stored.revision++;
    int selected = next.find(next.selectedBond, next.selectedId);
    stored.active = selected >= 0 ? next.networks[selected] : WifiNetwork{};
    for (int i = 0; i < 12; ++i) {
        if (memcmp(&next.networks[i], &schemes.networks[i], sizeof(WifiNetwork))) stored.versions[i] = stored.revision;
    }
    stored.activeVersion = selected >= 0 ? stored.versions[selected] : 0;
    bool okay = saveV2(stored); wipe((char *)&stored, sizeof(stored)); return okay;
}

bool wifiSchemesBegin() {
#ifndef CODEX_WIFI_SETUP_HOST_TEST
    static bool eventsInstalled = false;
    if (!eventsInstalled) {
        WiFi.onEvent([](arduino_event_id_t, arduino_event_info_t info) {
            wifiDisconnectReason = info.wifi_sta_disconnected.reason;
        }, ARDUINO_EVENT_WIFI_STA_DISCONNECTED);
        WiFi.onEvent([](arduino_event_id_t, arduino_event_info_t) {
            wifiDisconnectReason = 0;
        }, ARDUINO_EVENT_WIFI_STA_GOT_IP);
        eventsInstalled = true;
    }
#endif
    Preferences prefs;
    if (!prefs.begin("wifi-bonds", false)) return false;
    size_t size = prefs.getBytesLength("scheme");
    storeReady = !size;
    if (!size) {
        // Avoid large aggregate temporaries in the startup call chain.
        new (&schemes) WifiSchemes{};
        new (&wifiStore) WifiStoreV2{};
    }
    else {
        std::unique_ptr<WifiStoreV2> check(new(std::nothrow) WifiStoreV2);
        storeReady = check && readWifiStore(prefs, *check);
        if (storeReady) { wifiStore = *check; schemes = check->schemes; }
        if (check) wipe((char *)check.get(), sizeof(*check));
    }
    prefs.end();
    return storeReady;
}
bool wifiSchemesCredentials(String &ssid, String &password) {
    if (running && deferredSelection) {
        ssid = candidate.ssid; password = candidate.password; return true;
    }
    int slot = schemes.find(schemes.selectedBond, schemes.selectedId);
    if (!storeReady || slot < 0) return false;
    ssid = *wifiStore.active.id ? wifiStore.active.ssid : schemes.networks[slot].ssid;
    password = *wifiStore.active.id ? wifiStore.active.password : schemes.networks[slot].password;
    return true;
}
bool wifiSetupBusy() { return running; }
bool wifiSetupActivated() { return persisted && !trialOnly; }
String wifiSetupSelectingBridge() { return running && (operation == "wifi_select" || operation == "wifi_activate") ? selectingBridge : String(); }

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
        out["network_id"] = candidate.id;
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
    std::unique_ptr<WifiStoreV2> next(new(std::nothrow) WifiStoreV2(wifiStore));
    if (!next) return reply(request, "storage");
    const char *failure = nullptr;
    bool changed = false;
    int selected = -1;
    for (size_t i = 0; i < count && !failure; ++i) {
        WifiNetwork n = slots[i];
        if (n.id[32] || n.ssid[32] || n.password[64] ||
            !identifier(n.id, 32) || !ssidValid(n.ssid) || !passwordValid(n.password)) {
            failure = "invalid_old_slot";
        } else {
            strcpy(n.bond, peer.c_str());
            strcpy(n.host, n.ssid);
            int index = next->schemes.find(n.bond, n.id);
            if (index >= 0) {
                if (memcmp(&next->schemes.networks[index], &n, sizeof(n))) failure = "migration_conflict";
            } else {
                failure = next->schemes.put(n, false);
                if (!failure) {
                    changed = true;
                    index = next->schemes.find(n.bond, n.id);
                    next->versions[index] = wifiStore.revision + 1;
                    snprintf(next->labels[index], sizeof(next->labels[index]), "Migrated %s", n.id);
                }
            }
            if (!failure && preferred == String(n.ssid)) {
                if (selected >= 0) failure = "active_ambiguous";
                selected = index;
            }
        }
        wipe((char *)&n, sizeof(n));
    }
    // put() selects the inserted row. Migration preserves the existing active
    // snapshot, or adopts precisely the old/current connection without WiFi.begin.
    strcpy(next->schemes.selectedBond, schemes.selectedBond);
    strcpy(next->schemes.selectedId, schemes.selectedId);
    if (!failure && !*wifiStore.active.id && count) {
        if (selected < 0) failure = "active_not_found";
        else {
            next->active = next->schemes.networks[selected];
            next->activeVersion = next->versions[selected];
            strcpy(next->schemes.selectedBond, next->active.bond);
            strcpy(next->schemes.selectedId, next->active.id);
            changed = true;
        }
    }
    if (!failure && changed && wifiStore.revision == UINT64_MAX) failure = "capacity";
    if (!failure && changed) {
        next->revision = wifiStore.revision + 1;
        if (!saveV2(*next)) failure = "storage_unknown";
    }
    wipe((char *)next.get(), sizeof(*next));
    if (failure) {
        JsonDocument out; deserializeJson(out, reply(request, failure));
        if (!strcmp(failure, "storage_unknown")) {
            out["storage_stage"] = wifiStorageFailure;
            out["storage_bytes"] = wifiStorageBytes;
            out["unencoded_bytes"] = sizeof(WifiStoreV2);
            if (!strcmp(wifiStorageFailure, "storage_encode")) {
                out["encode_stage"] = wifiEncodeStage; out["encode_status"] = wifiEncodeStatus;
            }
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

void wifiSetupAbort(const char *reason) {
    if (!running) return;
    running = false;
    error = reason;
    stage = "failed";
    WiFi.disconnect(false);
    if (oldSsid.length()) WiFi.begin(oldSsid.c_str(), oldPassword.c_str());
    wipe(candidate.password, sizeof(candidate.password));
    wipeString(oldPassword);
}
void wifiSetupRadioClosed(const char *reason) {
    // A token/owner-authorized selection uses existing stored credentials and
    // can finish in the formal light Wi-Fi window after normal BLE closes.
    if (!deferredSelection) wifiSetupAbort(reason);
}
void wifiSetupPoll(uint64_t now, const String &peer, uint32_t generation, bool encrypted) {
    if (!running) return;
    if (!deferredSelection && (!encrypted || peer != transactionPeer || generation != transactionGeneration)) {
        wifiSetupAbort("disconnected"); return;
    }
    if (now >= deadline) {
        const char *reported = wifiConnectionFailure(wifiDisconnectReason);
        const char *failure = WiFi.status() == WL_CONNECTED && WiFi.SSID() == String(candidate.ssid) ? "dhcp_timeout" :
            reported ? reported : "connect_timeout";
        wifiSetupAbort(failure); return;
    }
    if (deferredSelection && !selectionStarted) {
        // Wi-Fi must associate while BLE is alive so HTTP can prove readiness
        // before handoff closes BLE. The existing absolute deadline still wins.
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
        wipe(candidate.password, sizeof(candidate.password)); wipeString(oldPassword); return;
    }
    WifiSchemes next = schemes;
    const char *failure = nullptr;
    if (operation == "wifi_setup") failure = next.put(candidate, replaceNetwork);
    else {
        strcpy(next.selectedBond, candidate.bond);
        strcpy(next.selectedId, candidate.id);
    }
    if (!failure && !save(next)) failure = "storage";
    wipe((char *)&next, sizeof(next));
    if (failure) { wifiSetupAbort(failure); return; }
    resultIp = WiFi.localIP().toString();
    persisted = true;
    running = false;
    stage = "persisted";
    deferredSelection = false;
    wipe(candidate.password, sizeof(candidate.password));
    wipeString(oldPassword);
}

// Same length-delimited metadata fingerprint as Bridge; no password bytes.
static String wifiMetadataKey(const WifiNetwork &network, const char *label, bool open) {
    uint8_t payload[132], hash[32]; size_t size = 0;
    for (const char *value : {network.host, network.ssid, label}) {
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
    if (!storeReady) return reply(doc, "storage");
    if (!strcmp(op, "wifi_save") || !strcmp(op, "wifi_result")) {
        if (!strcmp(op, "wifi_save") && (!doc["changes"].is<JsonArray>() || !doc["changes"].size() || doc["changes"].size() > 4)) return reply(doc, "shape");
        JsonDocument request; request.set(doc);
        if (!strcmp(op, "wifi_save")) {
            if (!selectAllowed && physicalUntil <= now) return reply(doc, "occupied");
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
        int slot = schemes.find(peer.c_str(), networkId);
        JsonDocument out;
        out["ack"] = "wifi"; out["op"] = "wifi"; out["request_id"] = id;
        out["device_mac"] = doc["device_mac"]; out["ok"] = true; out["network_id"] = networkId;
        out["version"] = slot < 0 ? 0 : wifiStore.versions[slot];
        if (!doc["open"].is<bool>() || String(doc["metadata_key"] | "").length()!=64) return reply(doc,"shape");
        bool open = doc["open"].as<bool>();
        bool matches = slot >= 0 && wifiMetadataKey(schemes.networks[slot],wifiStore.labels[slot],open) == (doc["metadata_key"] | "") &&
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
        bool active = slot >= 0 && peer == wifiStore.active.bond && String(networkId) == wifiStore.active.id &&
            wifiStore.activeVersion == wifiStore.versions[slot];
        bool connected = active && WiFi.status() == WL_CONNECTED && WiFi.SSID() == String(schemes.networks[slot].ssid) &&
            !(WiFi.localIP() == IPAddress(0,0,0,0));
        out["active"] = active; out["connected"] = connected;
        if (WiFi.status() == WL_CONNECTED) out["actual_ssid"] = WiFi.SSID();
        bool currentAttempt = slot >= 0 && transactionPeer == peer && String(networkId) == candidate.id &&
            selectionVersion == wifiStore.versions[slot] && (running || error.length());
        const char *normalFailure = active && !connected && !currentAttempt && attemptedSsid == String(schemes.networks[slot].ssid) ?
            wifiConnectionFailure(wifiDisconnectReason) : nullptr;
        out["stage"] = connected ? "connected" : currentAttempt ? stage : normalFailure ? "failed" : slot < 0 ? "missing" : "saved";
        if (currentAttempt && error.length()) { out["error"] = error; out["reason"] = wifiDisconnectReason; }
        else if (normalFailure) { out["error"] = normalFailure; out["reason"] = wifiDisconnectReason; }
        if (connected) out["ip"] = WiFi.localIP().toString();
        String text; serializeJson(out,text); return text;
    }
    if (!strcmp(op, "wifi_active")) {
        JsonDocument out; out["ack"] = "wifi"; out["op"] = "wifi"; out["request_id"] = id;
        out["device_mac"] = doc["device_mac"]; out["ok"] = true;
        out["network_id"] = wifiStore.active.id; out["version"] = wifiStore.activeVersion;
        out["revision"] = wifiStore.revision;
        String text; serializeJson(out, text); return text;
    }
    if (!strcmp(op, "wifi_scheme_get")) {
        JsonDocument out;
        out["ack"] = "wifi"; out["op"] = op; out["request_id"] = id;
        out["device_mac"] = doc["device_mac"]; out["ok"] = true;
        out["capacity"] = 4; out["wifi_setup_protocol"] = 1;
        out["physical_remaining_s"] = physicalUntil > now ? (physicalUntil - now) / 1000 : 0;
        out["selected_id"] = peer == schemes.selectedBond ? schemes.selectedId : "";
        JsonArray networks = out["networks"].to<JsonArray>();
        for (const auto &network : schemes.networks) {
            if (!*network.id || peer != network.bond) continue;
            JsonObject n = networks.add<JsonObject>();
            n["network_id"] = network.id; n["host_ssid"] = network.host; n["ssid"] = network.ssid;
            int index = schemes.find(network.bond, network.id);
            n["label"] = wifiStore.labels[index]; n["version"] = wifiStore.versions[index];
            n["password"] = String(network.password);
        }
        String text; serializeJson(out, text);
        return wifiSetupReadPage(doc, peer, generation, text);
    }
    if (!strcmp(op, "wifi_setup_status") || !strcmp(op, "wifi_setup_cancel")) {
        if (transactionPeer != peer || requestId != (doc["query_id"] | "")) return reply(doc, "not_found");
        if (!strcmp(op, "wifi_setup_cancel")) wifiSetupAbort("cancelled");
        return reply(doc, nullptr, true);
    }
    bool trial = !strcmp(op, "wifi_trial"), activate = !strcmp(op, "wifi_activate");
    bool select = !strcmp(op, "wifi_select") || activate, setup = !strcmp(op, "wifi_setup") || trial;
    bool remove = !strcmp(op, "wifi_scheme_delete");
    if (!select && !setup && !remove) return reply(doc, "shape");
    // Bind idempotency to the whole payload without retaining transient secrets.
    String body; serializeJson(doc, body);
    uint8_t hash[32];
    mbedtls_sha256((const unsigned char *)body.c_str(), body.length(), hash, 0);
    wipeString(body);
    if (transactionPeer == peer && requestId == id) {
        return !memcmp(hash, fingerprint, sizeof(hash)) ? reply(doc, nullptr, true)
                                                        : reply(doc, "request_conflict");
    }
    if (running) return reply(doc, "busy");
    if ((setup || remove) && physicalUntil <= now) return reply(doc, "physical_required");
    if (select && !selectAllowed) return reply(doc, "occupied");
    const char *networkId = doc["network_id"] | "";
    if (!identifier(networkId, 32)) return reply(doc, "shape");
    int slot = schemes.find(peer.c_str(), networkId);
    WifiNetwork network;
    if (activate && (!doc["version"].is<uint64_t>() || doc["version"].as<uint64_t>() != wifiStore.versions[slot < 0 ? 0 : slot])) return reply(doc, "revision_conflict");
    if (setup) {
        const char *host = doc["host_ssid"] | "", *ssid = doc["ssid"] | "";
        const char *password = doc["password"] | "";
        if (!doc["password"].is<const char *>() || !ssidValid(host) || !ssidValid(ssid) ||
            !passwordValid(password) || !doc["replace"].is<bool>()) return reply(doc, "shape");
        strcpy(network.bond, peer.c_str()); strcpy(network.id, networkId);
        strcpy(network.host, host); strcpy(network.ssid, ssid); strcpy(network.password, password);
        WifiSchemes next = schemes;
        const char *failure = trial ? nullptr : next.put(network, doc["replace"].as<bool>());
        wipe((char *)&next, sizeof(next));
        if (failure) { wipe(network.password, sizeof(network.password)); return reply(doc, failure); }
    } else {
        if (slot < 0) return reply(doc, "not_found");
        network = schemes.networks[slot];
    }
    uint64_t until = physicalUntil > now ? physicalUntil : lightUntil;
    if (!remove && until <= now) { wipe(network.password, sizeof(network.password)); return reply(doc, "window_required"); }
    if (remove) {
        const bool selected = !strcmp(schemes.selectedBond, network.bond) &&
                              !strcmp(schemes.selectedId, network.id);
        WifiSchemes next = schemes;
        next.erase(slot);
        bool saved = save(next);
        wipe((char *)&next, sizeof(next));
        wipe(network.password, sizeof(network.password));
        if (!saved) return reply(doc, "storage");
        if (selected) WiFi.disconnect(false);
    }
    trialOnly = trial;
    transactionPeer = peer; requestId = id; operation = op; transactionGeneration = generation;
    candidate = network; wipe(network.password, sizeof(network.password));
    selectionVersion = slot < 0 ? 0 : wifiStore.versions[slot];
    wifiDisconnectReason = 0;
    memcpy(fingerprint, hash, sizeof(hash));
    error = ""; resultIp = ""; persisted = remove;
    deferredSelection = select && physicalUntil <= now;
    selectionStarted = false;
    selectingBridge = select ? (doc["bridge_id"] | "") : "";
    stage = remove ? "persisted" : deferredSelection ? "accepted" : "connecting";
    if (!remove) {
        oldSsid = WiFi.status() == WL_CONNECTED ? WiFi.SSID() : String();
        oldPassword = WiFi.status() == WL_CONNECTED ? WiFi.psk() : String();
        replaceNetwork = doc["replace"] | false;
        deadline = std::min(until, now + 25000);
        running = true;
        if (!deferredSelection) {
            WiFi.disconnect(false);
            WiFi.mode(WiFi.getMode() == WIFI_AP ? WIFI_AP_STA : WIFI_STA);
            wifiSetupConnectionStarted(String(candidate.ssid));
            WiFi.begin(candidate.ssid, candidate.password);
        }
    } else wipe(candidate.password, sizeof(candidate.password));
    return reply(doc, nullptr, true);
}

#include "wifi_management.inc"
