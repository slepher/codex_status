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

static bool securePeer(const ble_gap_conn_desc *desc) {
    return desc && desc->sec_state.encrypted && desc->sec_state.bonded;
}

static bool writeAllowed(const ble_gap_conn_desc *desc) {
    if (securePeer(desc)) return true;
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
    void onRead(NimBLECharacteristic *c) override { refreshInfo(); }
    void onRead(NimBLECharacteristic *c, ble_gap_conn_desc *desc) override { refreshInfo(); }
};

class ServerCallbacks : public NimBLEServerCallbacks {
    void onConnect(NimBLEServer *server) override {
        connected = true;
        peerBonded = false;
        peerEncrypted = false;
        disconnecting = false;
        clearReceiveBuffers();
        refreshInfo();
    }
    void onConnect(NimBLEServer *server, ble_gap_conn_desc *desc) override {
        connected = true;
        peerConnHandle = desc->conn_handle;
        peerBonded = NimBLEDevice::isBonded(NimBLEAddress(desc->peer_id_addr));
        peerEncrypted = desc->sec_state.encrypted;
        disconnecting = false;
        clearReceiveBuffers();
        peerAddress = NimBLEAddress(desc->peer_id_addr).toString().c_str();
        DevLog.printf("[ble] connected: %s bonded=%d\n", peerAddress.c_str(), desc->sec_state.bonded);
        refreshInfo();
        if (!peerBonded && !blePairingWindowOpen()) {
            DevLog.println("[ble] unbonded peer outside pairing window; disconnecting");
            disconnecting = true;
            server->disconnect(desc->conn_handle);
        }
    }
    void onDisconnect(NimBLEServer *server) override {
        connected = false;
        peerBonded = false;
        peerEncrypted = false;
        peerConnHandle = BLE_HS_CONN_HANDLE_NONE;
        disconnecting = false;
        clearReceiveBuffers();
        refreshInfo();
        DevLog.println("[ble] disconnected");
    }
    void onDisconnect(NimBLEServer *server, ble_gap_conn_desc *desc) override {
        onDisconnect(server);
    }
    void onAuthenticationComplete(ble_gap_conn_desc *desc) override {
        peerEncrypted = desc && desc->sec_state.encrypted;
        peerBonded = desc && desc->sec_state.bonded;
        refreshInfo();
        if (!securePeer(desc)) {
            DevLog.println("[ble] authentication rejected: encryption and bond required");
            if (bleServer && desc) {
                disconnecting = true;
                bleServer->disconnect(desc->conn_handle);
            }
            return;
        }
        DevLog.println("[ble] authenticated bonded peer");
    }
};

class EndpointCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c, ble_gap_conn_desc *desc) override {
        if (!writeAllowed(desc)) return;
        std::string v = c->getValue();
        appendJson(endpointBuf, v, 1024, "endpoint",
                   [](const String &json) { if (endpointHandler) endpointHandler(json); });
    }
};

class UsageCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c, ble_gap_conn_desc *desc) override {
        if (!writeAllowed(desc)) return;
        std::string v = c->getValue();
        appendJson(usageBuf, v, 4096, "usage",
                   [](const String &json) { if (usageHandler) usageHandler(json); });
    }
};

class TplCtrlCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c, ble_gap_conn_desc *desc) override {
        if (!writeAllowed(desc)) return;
        std::string v = c->getValue();
        appendJson(tplCtrlBuf, v, 512, "template-control",
                   [](const String &json) { if (tplCtrlHandler) tplCtrlHandler(json); });
    }
};

class TplDataCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c, ble_gap_conn_desc *desc) override {
        if (!writeAllowed(desc)) return;
        if (!tplDataHandler) return;
        std::string v = c->getValue();
        tplDataHandler((const uint8_t *)v.data(), v.size());
    }
};

class AuthCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c, ble_gap_conn_desc *desc) override {
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
    adv->addServiceUUID(BLE_SVC_UUID);
    adv->setScanResponse(true);
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
