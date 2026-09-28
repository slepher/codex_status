#include "ble_bridge.h"
#include "dev_log.h"
#include <NimBLEDevice.h>
#include <ArduinoJson.h>
#include <Preferences.h>
#include <WiFi.h>
#include <esp_wifi.h>
#include <algorithm>
#include <atomic>

static UsageJsonHandler    usageHandler    = nullptr;
static EndpointJsonHandler endpointHandler = nullptr;
static TemplateCtrlHandler tplCtrlHandler  = nullptr;
static TemplateDataHandler tplDataHandler  = nullptr;
static TemplateResetHandler tplResetHandler = nullptr;
static AuthJsonHandler     authHandler     = nullptr;
static V2CtrlHandler       v2CtrlHandler   = nullptr;

static bool     connected      = false;
static bool     peerBonded     = false;
static bool     peerEncrypted  = false;
static bool     disconnecting  = false;
static uint16_t peerConnHandle = BLE_HS_CONN_HANDLE_NONE;
static String   peerAddress;
static uint32_t pairingUntil   = 0;
static bool     advertising    = false;
static String   usageBuf;
static String   endpointBuf;
static String   tplCtrlBuf;
static String   authBuf;
static String   fwVersion      = "?";
static String   infoExtra;
static NimBLECharacteristic *statusChr  = nullptr;
static NimBLECharacteristic *infoChr    = nullptr;
static NimBLEServer        *bleServer  = nullptr;

static bool jsonComplete(const String &s) {
    int depth = 0;
    bool inStr = false;
    bool esc = false;
    bool started = false;
    for (size_t i = 0; i < s.length(); i++) {
        char c = s[i];
        if (inStr) {
            if (esc) esc = false;
            else if (c == '\\') esc = true;
            else if (c == '"') inStr = false;
            continue;
        }
        if (c == '"') inStr = true;
        else if (c == '{' || c == '[') { depth++; started = true; }
        else if (c == '}' || c == ']') {
            if (!started || depth <= 0) return false;
            depth--;
            if (started && depth == 0) return true;
        }
    }
    return false;
}

static void clearReceiveBuffers() {
    usageBuf = "";
    endpointBuf = "";
    tplCtrlBuf = "";
    authBuf = "";
    if (tplResetHandler) tplResetHandler();
}

// NimBLE-Arduino 1.x exposes connection info as ble_gap_conn_desc*; 2.x as
// NimBLEConnInfo&. CODEX_NIMBLE_V2 is set by the pm env so both cores build
// from this same source.
#if CODEX_NIMBLE_V2
class PeerRef {
  public:
    PeerRef(NimBLEConnInfo &info) : info_(&info) {}
    bool     secure() const { return info_->isEncrypted() && info_->isBonded(); }
    bool     encrypted() const { return info_->isEncrypted(); }
    bool     bonded() const { return info_->isBonded(); }
    uint16_t handle() const { return info_->getConnHandle(); }
    String   address() const { return info_->getAddress().toString().c_str(); }

  private:
    NimBLEConnInfo *info_;
};
#define PEER_ARG NimBLEConnInfo &desc
#else
class PeerRef {
  public:
    PeerRef(ble_gap_conn_desc *desc) : desc_(desc) {}
    bool secure() const { return desc_ && desc_->sec_state.encrypted && desc_->sec_state.bonded; }
    bool encrypted() const { return desc_ && desc_->sec_state.encrypted; }
    bool bonded() const { return desc_ && desc_->sec_state.bonded; }
    uint16_t handle() const { return desc_ ? desc_->conn_handle : BLE_HS_CONN_HANDLE_NONE; }
    String address() const {
        return desc_ ? NimBLEAddress(desc_->peer_id_addr).toString().c_str() : String();
    }

  private:
    ble_gap_conn_desc *desc_;
};
#define PEER_ARG ble_gap_conn_desc *desc
#endif

static bool writeAllowed(PeerRef peer) {
    if (peer.secure()) return true;
    DevLog.println("[ble] rejected unencrypted or unbonded write");
    return false;
}

// v2 compatibility split (design §5.1): only a control JSON that explicitly
// declares `rv>=2` goes to the rendezvous handler; everything else keeps the
// legacy template transfer path byte-for-byte. With no v2 handler registered
// (stage 1/2) the frame gets a bounded NACK instead of an old-path parse.
static void dispatchTemplateCtrl(const String &json) {
    JsonDocument doc;
    if (deserializeJson(doc, json) == DeserializationError::Ok) {
        int rv = doc["rv"] | 0;
        if (rv >= 2) {
            if (v2CtrlHandler) {
                v2CtrlHandler(json);
            } else {
                DevLog.println("[ble] v2 control rejected: handler not ready");
                bleNotifyStatusQuiet("{\"ack\":\"v2\",\"ok\":false,\"err\":\"not_ready\"}");
            }
            return;
        }
    }
    if (tplCtrlHandler) tplCtrlHandler(json);
}

static bool appendJson(String &buf, const std::string &value, size_t limit,
                       const char *name, void (*handler)(const String &)) {
    if (buf.length() + value.size() > limit) {
        DevLog.printf("[ble] %s JSON overflow, buffer cleared\n", name);
        buf = "";
        if (!strcmp(name, "template-control") && tplResetHandler) tplResetHandler();
        return false;
    }
    for (size_t i = 0; i < value.size(); i++) buf += value[i];
    if (!jsonComplete(buf)) return false;
    JsonDocument doc;
    if (deserializeJson(doc, buf)) {
        DevLog.printf("[ble] malformed %s JSON, buffer cleared\n", name);
        buf = "";
        if (!strcmp(name, "template-control") && tplResetHandler) tplResetHandler();
        return false;
    }
    String complete = buf;
    buf = "";
    if (handler) handler(complete);
    return true;
}

static void refreshInfo() {
    if (!infoChr) return;
    String info = "{\"schema\":1,\"model\":\"ESP32-S3-ePaper-1.54-BW\",\"fw\":\"" + fwVersion +
                  "\",\"proto\":1";
    if (infoExtra.length()) info += "," + infoExtra;
    info += ",\"pairingWindow\":" + String(blePairingWindowOpen() ? "true" : "false") +
            ",\"peerBonded\":" + String(peerBonded ? "true" : "false") +
            ",\"peerEncrypted\":" + String(peerEncrypted ? "true" : "false");
    info += "}";
    infoChr->setValue(reinterpret_cast<const uint8_t *>(info.c_str()), info.length());
}

class InfoCallbacks : public NimBLECharacteristicCallbacks {
#if CODEX_NIMBLE_V2
    void onRead(NimBLECharacteristic *c, PEER_ARG) override { refreshInfo(); }
#else
    void onRead(NimBLECharacteristic *c) override { refreshInfo(); }
    void onRead(NimBLECharacteristic *c, PEER_ARG) override { refreshInfo(); }
#endif
};

class ServerCallbacks : public NimBLEServerCallbacks {
  private:
    void handleDisconnect() {
        connected = false;
        peerBonded = false;
        peerEncrypted = false;
        peerConnHandle = BLE_HS_CONN_HANDLE_NONE;
        disconnecting = false;
        clearReceiveBuffers();
        refreshInfo();
        DevLog.println("[ble] disconnected");
        // NimBLE 2.x does not resume advertising after a connection ends.
        // Restart it while the window still wants the radio on; LIVE and sleep
        // entry set `advertising` false explicitly.
        if (advertising) {
            NimBLEDevice::startAdvertising();
            DevLog.println("[ble] advertising restarted after disconnect");
        }
    }

  public:
#if !CODEX_NIMBLE_V2
    void onConnect(NimBLEServer *server) override {
        connected = true;
        peerBonded = false;
        peerEncrypted = false;
        disconnecting = false;
        clearReceiveBuffers();
        refreshInfo();
    }
#endif
    void onConnect(NimBLEServer *server, PEER_ARG) override {
        connected = true;
        PeerRef peer(desc);
        peerConnHandle = peer.handle();
        peerBonded = peer.bonded();
        peerEncrypted = peer.encrypted();
        disconnecting = false;
        clearReceiveBuffers();
        peerAddress = peer.address();
        DevLog.printf("[ble] connected: %s bonded=%d\n", peerAddress.c_str(), peerBonded ? 1 : 0);
        refreshInfo();
        if (!peerBonded && !blePairingWindowOpen()) {
            DevLog.println("[ble] unbonded peer outside pairing window; disconnecting");
            disconnecting = true;
            server->disconnect(peer.handle());
        }
    }
#if CODEX_NIMBLE_V2
    void onDisconnect(NimBLEServer *server, PEER_ARG, int reason) override { handleDisconnect(); }
#else
    void onDisconnect(NimBLEServer *server) override { handleDisconnect(); }
    void onDisconnect(NimBLEServer *server, PEER_ARG) override { handleDisconnect(); }
#endif
    void onAuthenticationComplete(PEER_ARG) override {
        PeerRef peer(desc);
        peerEncrypted = peer.encrypted();
        peerBonded = peer.bonded();
        refreshInfo();
        if (!peer.secure()) {
            DevLog.println("[ble] authentication rejected: encryption and bond required");
            if (bleServer) {
                disconnecting = true;
                bleServer->disconnect(peer.handle());
            }
            return;
        }
        DevLog.println("[ble] authenticated bonded peer");
    }
};

class EndpointCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c, PEER_ARG) override {
        if (!writeAllowed(desc)) return;
        std::string v = c->getValue();
        appendJson(endpointBuf, v, 1024, "endpoint",
                   [](const String &json) { if (endpointHandler) endpointHandler(json); });
    }
};

class UsageCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c, PEER_ARG) override {
        if (!writeAllowed(desc)) return;
        std::string v = c->getValue();
        appendJson(usageBuf, v, 4096, "usage",
                   [](const String &json) { if (usageHandler) usageHandler(json); });
    }
};

class TplCtrlCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c, PEER_ARG) override {
        if (!writeAllowed(desc)) return;
        std::string v = c->getValue();
        appendJson(tplCtrlBuf, v, 8192, "template-control",
                   [](const String &json) { dispatchTemplateCtrl(json); });
    }
};

class TplDataCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c, PEER_ARG) override {
        if (!writeAllowed(desc)) return;
        if (!tplDataHandler) return;
        std::string v = c->getValue();
        tplDataHandler((const uint8_t *)v.data(), v.size());
    }
};

class AuthCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c, PEER_ARG) override {
        if (!writeAllowed(desc)) return;
        std::string v = c->getValue();
        appendJson(authBuf, v, 256, "auth",
                   [](const String &json) { if (authHandler) authHandler(json); });
    }
};

void bleBegin(const String &deviceName, const String &fw) {
    if (NimBLEDevice::isInitialized()) return;
    fwVersion = fw;
    NimBLEDevice::init(deviceName.c_str());
    NimBLEDevice::setSecurityAuth(true, false, true);
    NimBLEDevice::setSecurityIOCap(BLE_HS_IO_NO_INPUT_OUTPUT);

    bleServer = NimBLEDevice::createServer();
    bleServer->setCallbacks(new ServerCallbacks());

    NimBLEService *svc = bleServer->createService(BLE_SVC_UUID);

    infoChr = svc->createCharacteristic(BLE_CHR_INFO, NIMBLE_PROPERTY::READ);
    infoChr->setCallbacks(new InfoCallbacks());
    refreshInfo();

    NimBLECharacteristic *epChr = svc->createCharacteristic(BLE_CHR_ENDPT,
        NIMBLE_PROPERTY::WRITE | NIMBLE_PROPERTY::WRITE_ENC);
    epChr->setCallbacks(new EndpointCallbacks());

    NimBLECharacteristic *usageChr = svc->createCharacteristic(BLE_CHR_USAGE,
        NIMBLE_PROPERTY::WRITE | NIMBLE_PROPERTY::WRITE_ENC);
    usageChr->setCallbacks(new UsageCallbacks());

    NimBLECharacteristic *tplCtl = svc->createCharacteristic(BLE_CHR_TPLCTL,
        NIMBLE_PROPERTY::WRITE | NIMBLE_PROPERTY::WRITE_ENC);
    tplCtl->setCallbacks(new TplCtrlCallbacks());

    NimBLECharacteristic *tplDat = svc->createCharacteristic(BLE_CHR_TPLDAT,
        NIMBLE_PROPERTY::WRITE | NIMBLE_PROPERTY::WRITE_ENC);
    tplDat->setCallbacks(new TplDataCallbacks());

    NimBLECharacteristic *authChr = svc->createCharacteristic(BLE_CHR_AUTH,
        NIMBLE_PROPERTY::WRITE | NIMBLE_PROPERTY::WRITE_ENC);
    authChr->setCallbacks(new AuthCallbacks());

    statusChr = svc->createCharacteristic(BLE_CHR_STATUS, NIMBLE_PROPERTY::READ | NIMBLE_PROPERTY::NOTIFY);

    svc->start();

    NimBLEAdvertising *adv = NimBLEDevice::getAdvertising();
    // Plan C: fast advertising explicitly (30-60 ms, units of 0.625 ms) instead
    // of relying on the NimBLE default; the rendezvous window is a hard 3 s.
    adv->setMinInterval(0x30);
    adv->setMaxInterval(0x60);
#if CODEX_NIMBLE_V2
    // NimBLE 2.x advertises no name unless set explicitly, and the scan
    // response must be enabled explicitly as well; bridges scan by the
    // CodexStatus- prefix. The 128-bit service UUID does not fit next to the
    // name in the 31-byte legacy payload (Data length exceeded), and nothing
    // filters on it, so it is left out of the advertisement.
    adv->setName(deviceName.c_str());
    adv->enableScanResponse(true);
#else
    adv->addServiceUUID(BLE_SVC_UUID);
    adv->setScanResponse(true);
#endif
    NimBLEDevice::startAdvertising();
    advertising = true;

    DevLog.printf("[ble] advertising as %s fw=%s\n", deviceName.c_str(), fw.c_str());
}

bool bleInitialized() { return NimBLEDevice::isInitialized(); }

// v0.12: the BLE session is the only reason the BT controller exists. Leaving
// the session deinitializes NimBLE so the controller releases its
// ESP_PM_NO_LIGHT_SLEEP lock and automatic light sleep can apply.
void bleDeinit() {
    if (!NimBLEDevice::isInitialized()) return;
    advertising = false;
    connected = false;
    peerBonded = false;
    peerEncrypted = false;
    peerConnHandle = BLE_HS_CONN_HANDLE_NONE;
    disconnecting = false;
    statusChr = nullptr;
    infoChr = nullptr;
    bleServer = nullptr;
    clearReceiveBuffers();
    NimBLEDevice::deinit(true);
    DevLog.println("[ble] deinitialized (controller released)");
}

bool bleIsConnected() { return connected; }
bool blePeerIsBonded() { return peerBonded; }
bool blePeerIsEncrypted() { return peerEncrypted; }
String blePeerAddress() { return peerAddress; }
bool blePairingWindowOpen() {
    return pairingUntil != 0 && (int32_t)(millis() - pairingUntil) < 0;
}
void bleOpenPairingWindow(uint32_t ms) {
    pairingUntil = ms ? millis() + ms : 0;
    refreshInfo();
}

// BLE advertises only while the device has an active window (boot/OTA/pairing).
// Sleep entry stops advertising so the radio is quiet outside the window.
void bleAdvertiseStart() {
    if (advertising || !NimBLEDevice::isInitialized()) return;
    NimBLEDevice::startAdvertising();
    advertising = true;
    DevLog.println("[ble] advertising start");
}

void bleAdvertiseStop() {
    if (!advertising || !NimBLEDevice::isInitialized()) { advertising = false; return; }
    NimBLEDevice::stopAdvertising();
    advertising = false;
    DevLog.println("[ble] advertising stop");
}

void bleSetHandlers(UsageJsonHandler onUsage, EndpointJsonHandler onEndpoint) {
    usageHandler = onUsage;
    endpointHandler = onEndpoint;
}

void bleSetTemplateHandlers(TemplateCtrlHandler onCtrl, TemplateDataHandler onData,
                            TemplateResetHandler onReset) {
    tplCtrlHandler = onCtrl;
    tplDataHandler = onData;
    tplResetHandler = onReset;
}

void bleSetAuthHandler(AuthJsonHandler onAuth) {
    authHandler = onAuth;
}

void bleSetV2Handler(V2CtrlHandler onV2) {
    v2CtrlHandler = onV2;
}

void bleSetInfoExtra(const String &json) {
    infoExtra = json;
    refreshInfo();
}

void bleNotifyStatus(const String &json) {
    if (!statusChr) return;
    statusChr->setValue(reinterpret_cast<const uint8_t *>(json.c_str()), json.length());
    if (connected && peerEncrypted && peerBonded) statusChr->notify();
    DevLog.printf("[ble] status: %s\n", json.c_str());
}

// Like bleNotifyStatus but without serial logging; used for secrets (auth token).
void bleNotifyStatusQuiet(const String &json) {
    if (!statusChr) return;
    statusChr->setValue(reinterpret_cast<const uint8_t *>(json.c_str()), json.length());
    if (connected && peerEncrypted && peerBonded) statusChr->notify();
}

void blePoll() {
    if (connected && !peerBonded && !blePairingWindowOpen() && !disconnecting && bleServer) {
        DevLog.println("[ble] pairing window expired; disconnecting unbonded peer");
        disconnecting = true;
        bleServer->disconnect(peerConnHandle);
    }
}

void bleClearBonds() {
    DevLog.println("[ble] clearing all bonds");
    if (NimBLEDevice::isInitialized()) {
        NimBLEDevice::deleteAllBonds();
    } else {
        // The controller is down (BLE OFF); erase the bond namespace directly.
        Preferences p;
        p.begin("nimble_bond", false);
        p.clear();
        p.end();
    }
    pairingUntil = 0;
}

// ---------------------------------------------------------------------------
// Diagnostic scan (Plan C task-6 §1.1)
// ---------------------------------------------------------------------------

static String hexBytes(const uint8_t *data, size_t len) {
    static const char digits[] = "0123456789abcdef";
    String out;
    out.reserve(len * 2);
    for (size_t i = 0; i < len; i++) {
        out += digits[(data[i] >> 4) & 0x0F];
        out += digits[data[i] & 0x0F];
    }
    return out;
}

class DiagScanCallbacks : public NimBLEScanCallbacks {
  public:
    // Root stays an object; per-record data lives in `records` and every
    // observed manufacturer company id (bounded) is counted in `companies`,
    // so an empty match list can be told apart from a dead scan.
    JsonDocument doc;
    JsonArray records;
    JsonArray companies;
    uint32_t t0 = 0;
    uint32_t total = 0;
    uint32_t matched = 0;
    uint32_t firstMatchMs = 0;
    bool haveFirst = false;
    int scanEndReason = -1;

    DiagScanCallbacks(uint16_t companyFilter, uint8_t maxRecords)
        : filter_(companyFilter), maxRecords_(maxRecords) {
        records = doc["records"].to<JsonArray>();
        companies = doc["companies"].to<JsonArray>();
    }

    void onScanEnd(const NimBLEScanResults &results, int reason) override {
        (void)results;
        scanEndReason = reason;
    }

    void onResult(const NimBLEAdvertisedDevice *dev) override {
        total++;
        const uint32_t tMs = (uint32_t)(millis() - t0);
        bool hasMfr = dev->haveManufacturerData();
        uint16_t company = 0;
        std::string mfr;
        if (hasMfr) {
            mfr = dev->getManufacturerData();
            if (mfr.size() >= 2) {
                company = (uint8_t)mfr[0] | ((uint16_t)(uint8_t)mfr[1] << 8);
            }
            // Bounded census of observed company ids: an empty match list with
            // a non-empty census means the scan works but the beacon is absent.
            bool counted = false;
            for (JsonObject c : companies) {
                if (c["id"].as<uint16_t>() == company) {
                    c["n"] = c["n"].as<uint32_t>() + 1;
                    counted = true;
                    break;
                }
            }
            if (!counted && companies.size() < 8) {
                JsonObject c = companies.add<JsonObject>();
                c["id"] = company;
                c["n"] = 1;
            }
        }
        const bool match = hasMfr && (!filter_ || company == filter_);
        if (match) {
            matched++;
            if (!haveFirst) {
                haveFirst = true;
                firstMatchMs = tMs;
            }
        } else if (filter_) {
            return;   // filtered mode records only the target company
        }
        if ((uint8_t)records.size() >= maxRecords_) return;
        JsonObject o = records.add<JsonObject>();
        o["t_ms"] = tMs;
        o["addr"] = dev->getAddress().toString().c_str();
        o["rssi"] = dev->getRSSI();
        o["conn"] = dev->isConnectable();
        o["len"] = dev->getAdvLength();
        if (dev->haveName()) o["name"] = dev->getName();
        if (hasMfr) {
            o["company"] = company;
            const size_t skip = mfr.size() >= 2 ? 2 : 0;
            o["mfr"] = hexBytes(reinterpret_cast<const uint8_t *>(mfr.data()) + skip,
                                mfr.size() - skip);
        }
        const std::vector<uint8_t> &raw = dev->getPayload();
        if (filter_ && raw.size()) {
            o["raw"] = hexBytes(raw.data(), raw.size());
        }
    }

  private:
    uint16_t filter_;
    uint8_t maxRecords_;
};

String bleScanJson(uint32_t seconds, uint16_t companyFilter, uint8_t maxRecords) {
    if (seconds < 1) seconds = 1;
    if (seconds > 30) seconds = 30;
    if (maxRecords < 1) maxRecords = 1;
    bool initHere = false;
    if (!NimBLEDevice::isInitialized()) {
        NimBLEDevice::init("CodexStatus-diag");
        initHere = true;
    }
    const bool wasAdvertising = advertising;
    if (wasAdvertising) bleAdvertiseStop();
    // Wi-Fi power save starves the shared-radio BLE scan (coex time-slicing):
    // keep the modem awake for the diagnostic window and restore afterwards.
    esp_wifi_set_ps(WIFI_PS_NONE);

    NimBLEScan *scan = NimBLEDevice::getScan();
    scan->setActiveScan(true);
    scan->setInterval(100);
    scan->setWindow(80);
    scan->setDuplicateFilter(0);
    scan->clearResults();
    DiagScanCallbacks cb(companyFilter, maxRecords);
    cb.t0 = millis();
    scan->setScanCallbacks(&cb, true);
    // NimBLEScan::start() takes milliseconds and returns immediately: keep the
    // callbacks registered and wait for the controller to finish (bounded).
    const bool ok = scan->start(seconds * 1000);
    if (ok) {
        const uint32_t deadline = millis() + seconds * 1000 + 2000;
        while (scan->isScanning() && (int32_t)(millis() - deadline) < 0) {
            delay(20);
        }
        if (scan->isScanning()) scan->stop();
    }
    scan->setScanCallbacks(nullptr);

    cb.doc["scan_s"] = seconds;
    cb.doc["ok"] = ok;
    cb.doc["init"] = NimBLEDevice::isInitialized();
    cb.doc["scan_end"] = cb.scanEndReason;
    cb.doc["total"] = cb.total;
    cb.doc["matched"] = cb.matched;
    if (cb.haveFirst) cb.doc["first_match_ms"] = cb.firstMatchMs;
    String out;
    serializeJson(cb.doc, out);

    esp_wifi_set_ps(WIFI_PS_MAX_MODEM);
    if (wasAdvertising) bleAdvertiseStart();
    if (initHere) bleDeinit();
    return out;
}

// RF-only experiment. A/C first listen for the PC's AVAILABLE packet; B starts
// with a CHALLENGE. Every path then waits for an OFFER. There are no keys,
// schedule writes, GATT connections, or business effects in this diagnostic.
namespace {
constexpr uint16_t kRecoveryTestCompany = 0xFFFF;
constexpr uint32_t kRecoveryTestDeadlineMs = 6000;

class RecoveryScanCallbacks : public NimBLEScanCallbacks {
  public:
    RecoveryScanCallbacks(uint32_t run, uint8_t type) : run_(run), type_(type) {}
    uint32_t started = 0;
    uint32_t seen = 0;
    uint32_t firstMs = 0;
    int firstRssi = 0;
    std::atomic<bool> found{false};

    void onResult(const NimBLEAdvertisedDevice *dev) override {
        seen++;
        if (!dev->haveManufacturerData()) return;
        const std::string data = dev->getManufacturerData();
        if (data.size() < 8 || (uint8_t)data[0] != 0xFF || (uint8_t)data[1] != 0xFF ||
            (uint8_t)data[2] != 0xE0 || (uint8_t)data[3] != type_) return;
        uint32_t run = ((uint32_t)(uint8_t)data[4] << 24) |
                       ((uint32_t)(uint8_t)data[5] << 16) |
                       ((uint32_t)(uint8_t)data[6] << 8) | (uint8_t)data[7];
        if (run != run_ || found.load(std::memory_order_relaxed)) return;
        firstMs = millis() - started;
        firstRssi = dev->getRSSI();
        found.store(true, std::memory_order_release);
    }

  private:
    uint32_t run_;
    uint8_t type_;
};

uint32_t recoveryRemaining(uint32_t start) {
    uint32_t elapsed = millis() - start;
    return elapsed >= kRecoveryTestDeadlineMs - 250 ? 0 :
           kRecoveryTestDeadlineMs - 250 - elapsed;
}

bool recoveryListen(uint32_t run, uint8_t type, uint32_t requestedMs,
                    uint32_t start, JsonObject out) {
    uint32_t duration = std::min(requestedMs, recoveryRemaining(start));
    if (!duration) return false;
    NimBLEScan *scan = NimBLEDevice::getScan();
    scan->setActiveScan(false);
    scan->setInterval(100);
    scan->setWindow(80);
    scan->setDuplicateFilter(0);
    scan->clearResults();
    RecoveryScanCallbacks cb(run, type);
    cb.started = millis();
    scan->setScanCallbacks(&cb, true);
    bool ok = scan->start(duration);
    uint32_t end = cb.started + duration + 100;
    while (ok && scan->isScanning() && !cb.found.load(std::memory_order_acquire) &&
           (int32_t)(millis() - end) < 0) delay(10);
    if (scan->isScanning()) scan->stop();
    scan->setScanCallbacks(nullptr);
    out["ok"] = ok;
    out["duration_ms"] = millis() - cb.started;
    out["seen"] = cb.seen;
    const bool hit = cb.found.load(std::memory_order_acquire);
    out["hit"] = hit;
    if (hit) {
        out["first_ms"] = cb.firstMs;
        out["rssi"] = cb.firstRssi;
    }
    return ok && hit;
}

bool recoveryTransmit(uint32_t run, uint8_t type, uint32_t requestedMs,
                      uint32_t start, JsonObject out) {
    uint32_t duration = std::min(requestedMs, recoveryRemaining(start));
    if (!duration) return false;
    uint8_t bytes[26] = {0xFF, 0xFF, 0xE0, type,
                         (uint8_t)(run >> 24), (uint8_t)(run >> 16),
                         (uint8_t)(run >> 8), (uint8_t)run};
    NimBLEAdvertisementData data;
    if (!data.setManufacturerData(bytes, sizeof(bytes))) return false;
    NimBLEAdvertising *adv = NimBLEDevice::getAdvertising();
    adv->reset();
    adv->setConnectableMode(BLE_GAP_CONN_MODE_NON);
    adv->setDiscoverableMode(BLE_GAP_DISC_MODE_NON);
    adv->enableScanResponse(false);
    adv->setMinInterval(0x30);
    adv->setMaxInterval(0x60);
    if (!adv->setAdvertisementData(data)) return false;
    uint32_t begun = millis();
    bool ok = adv->start();
    if (ok) delay(duration);
    adv->stop();
    out["ok"] = ok;
    out["duration_ms"] = millis() - begun;
    return ok;
}
}  // namespace

String bleRecoveryTrialJson(char variant, uint32_t runId) {
    JsonDocument doc;
    doc["rf_only"] = true;
    doc["run_id"] = runId;
    doc["variant"] = String(variant);
    if ((variant != 'a' && variant != 'b' && variant != 'c') ||
        NimBLEDevice::isInitialized()) {
        doc["error"] = "invalid_variant_or_ble_busy";
    } else {
        wifi_ps_type_t previousPs = WIFI_PS_MAX_MODEM;
        esp_wifi_get_ps(&previousPs);
        esp_wifi_set_ps(WIFI_PS_NONE);
        uint32_t start = millis();
        NimBLEDevice::init("CodexStatus-rf-test");
        bool ready = true;
        if (variant != 'b') {
            ready = recoveryListen(runId, 0xA0, 1500, start,
                                   doc["available"].to<JsonObject>());
        }
        if (ready) {
            ready = recoveryTransmit(runId, 0xB1, 800, start,
                                     doc["challenge"].to<JsonObject>());
        }
        if (ready) {
            recoveryListen(runId, 0xB2, 2500, start,
                           doc["offer"].to<JsonObject>());
        }
        doc["radio_ms"] = millis() - start;
        doc["deadline_ok"] = (millis() - start) <= kRecoveryTestDeadlineMs;
        bleDeinit();
        esp_wifi_set_ps(previousPs);
    }
    String out;
    serializeJson(doc, out);
    return out;
}
