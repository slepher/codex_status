#pragma once

// Firmware integration only: state is owned here; never copy it as a snapshot.
#include "config.h"
#include "dependencies.h"

namespace firmware::radio {

struct ReadyReportState {
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

    // task-10 B: the Wi-Fi icon blinks while a boot connect attempt is running.
    // `wifiConnActive` switches the template state to WIFI CONN (blink on) /
    // WIFI OFF (blink off); `rfnBlink` marks a blink tick so epdFlush keeps it a
    // partial waveform and does not consume the region ghost budget.
    bool     wifiConnActive = false;
    bool     wifiBlinkOn = false;
    uint16_t wifiBlinkMs = WIFI_BLINK_MS;
    uint32_t blinkTicks = 0;
    bool     wifiUp = false;           // STA associated
    bool     wifiLostHandled = false;  // WIFI OFF policy already applied
    String   wifiSsid, wifiPass;
    bool     bleOn = false;
    bool     bleUserOff = false;       // user switched BLE off while docked
    uint32_t lastPersistedSync = 0;

    // Plan C: while the rendezvous window is open the screen is not touched; the
    // single wake render happens after bleOff (clock window, or the light plan's
    // first frame in startNormalMode). Other BLE paths keep their immediate render.
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
const char *deviceStateText();
const char *templateStateText();
const char *wifiStateText();
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
uint64_t wifiControlUntil(uint64_t now);
WifiControlContext wifiControlContext(const EndpointRec &endpoint,JsonDocument &request);
void wifiDeviceStatus(JsonDocument &doc,const EndpointRec *endpoint=nullptr);
void handleWifiManagement(const char *op);
String wifiManagementRequest(JsonDocument &request, const char *op);
void v2RendezvousBegin(bool deep = false);
void v2RendezvousTick();
void startNormalMode();

} // namespace firmware::radio
