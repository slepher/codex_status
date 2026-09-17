#include "ble_bridge.h"
#include "dev_log.h"
#include <NimBLEDevice.h>
#include <ArduinoJson.h>

static UsageJsonHandler    usageHandler    = nullptr;
static EndpointJsonHandler endpointHandler = nullptr;
static TemplateCtrlHandler tplCtrlHandler  = nullptr;
static TemplateDataHandler tplDataHandler  = nullptr;
static TemplateResetHandler tplResetHandler = nullptr;
static AuthJsonHandler     authHandler     = nullptr;

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
        appendJson(tplCtrlBuf, v, 512, "template-control",
                   [](const String &json) { if (tplCtrlHandler) tplCtrlHandler(json); });
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
    if (advertising) return;
    NimBLEDevice::startAdvertising();
    advertising = true;
    DevLog.println("[ble] advertising start");
}

void bleAdvertiseStop() {
    if (!advertising) return;
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
    NimBLEDevice::deleteAllBonds();
    pairingUntil = 0;
}
