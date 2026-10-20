#pragma once

// Firmware integration only: state is owned here; never copy it as a snapshot.
#include "config.h"
#include "dependencies.h"
#include "../network_display_state.h"
#include "../plan_activation.h"

namespace firmware::radio {

struct ReadyReportState {
    PlanActivation activation;
    IPAddress bridge;
    uint16_t port = 0;
    int8_t endpointIndex = -1;
    String principal;
    String originSession;
    uint64_t originRequestNo = 0;
    uint64_t planId = 0;
    String dataResult;
    String httpSession;
    String reportBody;
    uint8_t attempts = 0;
    uint64_t nextAtMs = 0;
    bool received = false;
};

struct State {

    // Loop-task radio state. Accepted HTTP work survives ordinary BLE closure.
    Preferences prefs;
    bool        configMode = false;
    String      hostname;
    String      apSsid;
    const char *AP_REASON_NAMES[3] = {"none", "no_slots", "boot_hold"};
    uint32_t configStartedAt = 0;

    // Local association and blink facts; templates choose their presentation.
    bool     wifiConnActive = false;
    bool     wifiBlinkOn = false;
    uint16_t wifiBlinkMs = WIFI_BLINK_MS;
    uint32_t blinkTicks = 0;
    bool     wifiUp = false;           // STA associated
    bool     wifiLostHandled = false;  // WIFI OFF policy already applied
    bool     wifiTerminal = false;     // completed loss/close, not initial idle
    bool     wifiFailureObserved = false;
    uint64_t wifiBlinkAtMs = 0;
    String   wifiSsid, wifiPass;
    bool     bleOn = false;
    bool     bleUserOff = false;       // user switched BLE off while docked
    uint32_t lastPersistedSync = 0;

    // Plan C defers the deep wake frame until exchange completion. In Light,
    // local Wi-Fi metadata can invalidate the template during association.
    bool           v2InRendezvous = false;
    RendezvousProtocol v2Rv;
    String v2RvId;
    bool v2RvNetworkExpected = false;
    bool v2RvNetworkRequested = false;
    uint64_t v2RvBleReleaseAtMs = 0;
    ReadyReportState readyReport;
    bool v2RvHttpStarted = false, v2RvBleReleased = false;
    uint64_t nextBleContactMs = 0;
    bool httpServicesStarted = false;
    uint64_t fallbackUntilMs = 0;
    bool fallbackAttempted = false;
    uint64_t presenceAttemptAtMs = 0;
    uint8_t presenceFailures = 0;
    bool bridgeReachable = false;
    bool failureReportPending = false;
    const char *networkError = nullptr;
    const char *startupContact = "ble";
};
extern State state;

struct Retained {
    uint32_t rtcActiveAt = 0;
    char     rtcActiveMac[20] = {0};
    uint32_t rtcLastSyncEpoch = 0;

    // Why AP config mode was entered last (0 none, 1 no saved slots, 2 BOOT held
    // 15 s). Kept in RTC memory for post-mortem /status.json reads.
    uint8_t  rtcApReason = 0;
    uint32_t rtcApChannel = 0;     // cached AP for fast connect
    char     rtcApBssid[20] = {0};
    uint8_t  rtcApSlot = 0xFF;
};
extern Retained retained;

void loadHostMac();
String macSuffix();
String macText();
WifiPhase wifiPhase();
void serviceWifiMetadata();
void markSynced();
String ipText();
void updateInfoExtra();
int countWifiSlots();
void loadPreferredWifiCredentials();
bool hasWifiSlots();
void startConfigMode();
void sendAnnounce(bool bleFlag);
void serviceAnnounce();
void enterBleOn(bool userInitiated);
void bleOff(const char *reason);
void serviceBleSession();
void pollWifi();
bool automaticContactNeeded();
uint64_t wifiControlUntil(uint64_t now);
WifiControlContext wifiControlContext(const EndpointRec &endpoint);
void wifiDeviceStatus(JsonDocument &doc,const EndpointRec *endpoint=nullptr);
void handleWifiManagement(const char *op);
void v2RendezvousBegin(bool deep = false);
void v2RendezvousTick();
void startNormalMode();
void resumeLightNetwork();

} // namespace firmware::radio
