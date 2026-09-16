#include "ble_bridge.h"
#include <NimBLEDevice.h>

static UsageJsonHandler    usageHandler    = nullptr;
static EndpointJsonHandler endpointHandler = nullptr;
static TemplateCtrlHandler tplCtrlHandler  = nullptr;
static TemplateDataHandler tplDataHandler  = nullptr;

static bool     connected      = false;
static String   peerAddress;
static uint32_t pairingUntil   = 0;
static String   usageBuf;
static String   endpointBuf;
static String   tplCtrlBuf;
static String   fwVersion      = "?";
static String   infoExtra;
static NimBLECharacteristic *statusChr  = nullptr;
static NimBLECharacteristic *infoChr    = nullptr;

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
            depth--;
            if (started && depth == 0) return true;
        }
    }
    return false;
}

static void refreshInfo() {
    if (!infoChr) return;
    String info = "{\"schema\":1,\"model\":\"ESP32-S3-ePaper-1.54G\",\"fw\":\"" + fwVersion +
                  "\",\"proto\":1";
    if (infoExtra.length()) info += "," + infoExtra;
    info += "}";
    infoChr->setValue(info.c_str());
}

class ServerCallbacks : public NimBLEServerCallbacks {
    void onConnect(NimBLEServer *server) override {
        connected = true;
    }
    void onConnect(NimBLEServer *server, ble_gap_conn_desc *desc) override {
        peerAddress = NimBLEAddress(desc->peer_id_addr).toString().c_str();
        Serial.printf("[ble] connected: %s bonded=%d\n", peerAddress.c_str(), desc->sec_state.bonded);
    }
    void onDisconnect(NimBLEServer *server) override {
        connected = false;
        Serial.println("[ble] disconnected");
    }
};

class EndpointCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c) override {
        std::string v = c->getValue();
        endpointBuf += v.c_str();
        if (endpointBuf.length() > 1024) endpointBuf = "";
        if (jsonComplete(endpointBuf)) {
            if (endpointHandler) endpointHandler(endpointBuf);
            endpointBuf = "";
        }
    }
};

class UsageCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c) override {
        std::string v = c->getValue();
        usageBuf += v.c_str();
        if (usageBuf.length() > 4096) usageBuf = "";
        if (jsonComplete(usageBuf)) {
            if (usageHandler) usageHandler(usageBuf);
            usageBuf = "";
        }
    }
};

class TplCtrlCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c) override {
        std::string v = c->getValue();
        tplCtrlBuf += v.c_str();
        if (tplCtrlBuf.length() > 512) tplCtrlBuf = "";
        if (jsonComplete(tplCtrlBuf)) {
            if (tplCtrlHandler) tplCtrlHandler(tplCtrlBuf);
            tplCtrlBuf = "";
        }
    }
};

class TplDataCallbacks : public NimBLECharacteristicCallbacks {
    void onWrite(NimBLECharacteristic *c) override {
        if (!tplDataHandler) return;
        std::string v = c->getValue();
        tplDataHandler((const uint8_t *)v.data(), v.size());
    }
};

void bleBegin(const String &deviceName, const String &fw) {
    fwVersion = fw;
    NimBLEDevice::init(deviceName.c_str());
    NimBLEDevice::setSecurityAuth(true, false, true);
    NimBLEDevice::setSecurityIOCap(BLE_HS_IO_NO_INPUT_OUTPUT);

    NimBLEServer *server = NimBLEDevice::createServer();
    server->setCallbacks(new ServerCallbacks());

    NimBLEService *svc = server->createService(BLE_SVC_UUID);

    infoChr = svc->createCharacteristic(BLE_CHR_INFO, NIMBLE_PROPERTY::READ);
    refreshInfo();

    NimBLECharacteristic *epChr = svc->createCharacteristic(BLE_CHR_ENDPT, NIMBLE_PROPERTY::WRITE);
    epChr->setCallbacks(new EndpointCallbacks());

    NimBLECharacteristic *usageChr = svc->createCharacteristic(BLE_CHR_USAGE, NIMBLE_PROPERTY::WRITE);
    usageChr->setCallbacks(new UsageCallbacks());

    NimBLECharacteristic *tplCtl = svc->createCharacteristic(BLE_CHR_TPLCTL, NIMBLE_PROPERTY::WRITE);
    tplCtl->setCallbacks(new TplCtrlCallbacks());

    NimBLECharacteristic *tplDat = svc->createCharacteristic(BLE_CHR_TPLDAT, NIMBLE_PROPERTY::WRITE);
    tplDat->setCallbacks(new TplDataCallbacks());

    statusChr = svc->createCharacteristic(BLE_CHR_STATUS, NIMBLE_PROPERTY::READ | NIMBLE_PROPERTY::NOTIFY);

    svc->start();

    NimBLEAdvertising *adv = NimBLEDevice::getAdvertising();
    adv->addServiceUUID(BLE_SVC_UUID);
    adv->setScanResponse(true);
    NimBLEDevice::startAdvertising();

    bleOpenPairingWindow(300000);
    Serial.printf("[ble] advertising as %s fw=%s\n", deviceName.c_str(), fw.c_str());
}

bool bleIsConnected() { return connected; }
String blePeerAddress() { return peerAddress; }
bool blePairingWindowOpen() { return millis() < pairingUntil; }
void bleOpenPairingWindow(uint32_t ms) { pairingUntil = millis() + ms; }

void bleSetHandlers(UsageJsonHandler onUsage, EndpointJsonHandler onEndpoint) {
    usageHandler = onUsage;
    endpointHandler = onEndpoint;
}

void bleSetTemplateHandlers(TemplateCtrlHandler onCtrl, TemplateDataHandler onData) {
    tplCtrlHandler = onCtrl;
    tplDataHandler = onData;
}

void bleSetInfoExtra(const String &json) {
    infoExtra = json;
    refreshInfo();
}

void bleNotifyStatus(const String &json) {
    if (!statusChr) return;
    statusChr->setValue(json.c_str());
    if (connected) statusChr->notify();
    Serial.printf("[ble] status: %s\n", json.c_str());
}

void bleClearBonds() {
    Serial.println("[ble] clearing all bonds");
    NimBLEDevice::deleteAllBonds();
    pairingUntil = 0;
}
