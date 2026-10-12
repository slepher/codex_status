#include "radio_internal.h"
#include "application_internal.h"
#include "clock_internal.h"
#include "diagnostics_internal.h"
#include "display_internal.h"
#include "platform_internal.h"
#include "power_internal.h"
#include "sync_internal.h"
#include "transport_internal.h"
#include "data_activity_led.h"
#include "data_activity_client.h"
#include "network_link_policy.h"

namespace firmware::radio {

State state;
RTC_DATA_ATTR Retained retained;

static WiFiUDP     announceUdp;

static uint32_t wifiLostSince = 0;

static uint32_t announcedIp = 0;

static uint32_t lastAnnounce = 0;

static byte hostMac[6] = {0};

static uint64_t v2RvStartMs = 0;

static uint8_t v2RvWifiPhase = 0;

static uint64_t       v2RfDeadlineMs = 0;

static void setActiveMac(const String &mac);
static String infoExtraJson();
static void handleConfigRoot();
static void handleConfigSave();
static void requestAnnounce(bool bleFlag);
static uint32_t retryDelaySec(uint8_t stage);
static uint64_t wifiPresenceUntil(uint64_t now);
static void servicePresence();
static void serviceReadyReport();
static void rendezvousWifiBegin();
static void networkFailed(const char *reason);
static bool startFallback();

bool automaticContactNeeded() {
    return !state.bridgeReachable || WiFi.status() != WL_CONNECTED;
}
static void rendezvousWifiTick();
static bool configureWifiPowerSave();

void loadHostMac() {
    esp_read_mac(hostMac, ESP_MAC_WIFI_STA);
}

String macSuffix() {
    // Read the base MAC straight from eFuse: WiFi.macAddress() needs the Wi-Fi
    // driver (and NVS) up, which is not true yet at BLE init / AP startup.
    char buf[8];
    snprintf(buf, sizeof(buf), "%02X%02X%02X", hostMac[3], hostMac[4], hostMac[5]);
    return String(buf);
}

String macText() {
    char buf[18];
    snprintf(buf, sizeof(buf), "%02X:%02X:%02X:%02X:%02X:%02X",
             hostMac[0], hostMac[1], hostMac[2], hostMac[3], hostMac[4], hostMac[5]);
    return String(buf);
}

WifiPhase wifiPhase() {
    // The final sleep frame describes the committed radio closure, just as
    // device.mode describes the sleep entered immediately after that frame.
    if (power::retained.rtcMode == MODE_DEEP && !state.v2InRendezvous) return WifiPhase::Idle;
    return wifiPhaseFor(state.wifiConnActive || wifiSetupConnecting(),
        WiFi.status() == WL_CONNECTED && WiFi.localIP() != IPAddress(0,0,0,0),
        wifiSetupBusy() || state.v2InRendezvous,
        state.wifiTerminal);
}

void serviceWifiMetadata() {
    static uint32_t seenAssociated = 0, seenIp = 0;
    const uint32_t associated = wifiSetupAssociatedEvent(), ip = wifiSetupIpEvent();
    if (associated != seenAssociated) {
        DevLog.printf("[net] stage=associated count=%u\n", (unsigned)(associated-seenAssociated));
        seenAssociated = associated;
    }
    if (ip != seenIp) {
        diagnostics::traceSetTransport(2); diagnostics::traceSetStage(TRACE_STAGE_WIFI_IP);
        DevLog.printf("[net] stage=wifi_ip count=%u\n", (unsigned)(ip-seenIp));
        seenIp = ip;
    }
    static uint32_t seenDisconnect = 0;
    const uint32_t event = wifiSetupDisconnectEvent();
    if (event != seenDisconnect) {
        // DevLog is loop-task owned. Multiple events may coalesce during EPD
        // blocking; report their count instead of inventing intermediate reasons.
        DevLog.printf("[wifi] link event reason=%u count=%u\n", event & 0xffff,
                      uint16_t((event >> 16) - (seenDisconnect >> 16)));
        seenDisconnect = event;
    }
    // A retained transaction error is not a fresh link failure after recovery.
    const bool failed = wifiSetupFailed();
    if (failed && !state.wifiFailureObserved) state.wifiTerminal = true;
    state.wifiFailureObserved = failed;
    if (wifiSetupBusy() || (WiFi.status() == WL_CONNECTED && WiFi.localIP() != IPAddress(0,0,0,0)))
        state.wifiTerminal = false;
    const bool connecting = wifiSetupConnecting() || (state.fallbackUntilMs &&
        (WiFi.status()!=WL_CONNECTED || WiFi.localIP()==IPAddress(0,0,0,0)));
    const uint64_t now = clock::v2NowMs();
    if (connecting != state.wifiConnActive) {
        state.wifiConnActive = connecting;
        state.wifiBlinkOn = connecting;
        state.wifiBlinkAtMs = now;
    } else if (connecting && now - state.wifiBlinkAtMs >= state.wifiBlinkMs) {
        state.wifiBlinkOn = !state.wifiBlinkOn;
        state.wifiBlinkAtMs = now;
    }
}

static void setActiveMac(const String &mac) {
    strncpy(retained.rtcActiveMac, mac.c_str(), sizeof(retained.rtcActiveMac) - 1);
    retained.rtcActiveMac[sizeof(retained.rtcActiveMac) - 1] = '\0';
}

void markSynced() {
    retained.rtcLastSyncEpoch = clock::timeKnown() ? (uint32_t)time(nullptr) : 0;
    power::retained.rtcRetryStage = 0;
    if (retained.rtcLastSyncEpoch == 0) return;
    // RTC data does not survive the OTA software reset on this board, so keep
    // the last-sync epoch in NVS too (throttled: at most one write per minute).
    if (state.lastPersistedSync && retained.rtcLastSyncEpoch - state.lastPersistedSync < 60) return;
    Preferences p;
    p.begin("sync", false);
    p.putUInt("last", retained.rtcLastSyncEpoch);
    p.end();
    state.lastPersistedSync = retained.rtcLastSyncEpoch;
}

String ipText() {
    if (state.configMode) return "192.168.4.1";
    if (WiFi.status() == WL_CONNECTED) return WiFi.localIP().toString();
    return "-";
}

static String infoExtraJson() {
    String items = "\"mac\":\"" + macText() + "\",\"ip\":\"" + ipText() +
                   "\",\"http_port\":80,\"rendezvous\":true,\"wifi_setup_protocol\":1,\"wifi_management\":3,\"wifi_connection\":1,\"ble_claim\":1" +
                   ",\"wake_generation\":" + String((unsigned long)diagnostics::retained.rtcWakeGeneration) +
                   ",\"wake_seq\":" + String((unsigned long)diagnostics::retained.rtcTraceCurrentSeq) +
                   ",\"wake_stage\":\"" +
                   (diagnostics::traceCurrent() ? diagnostics::traceStageName(diagnostics::traceCurrent()->furthest) : "none") +
                   "\",\"wake_cause\":\"" + diagnostics::wakeCauseName(application::state.bootWakeCause) + "\"" +
                   ",\"bundle_configured\":" + String(platform::state.v2BundleReady ? "true" : "false");
    const uint64_t now = clock::v2NowMs();
    const uint64_t until = v2RfDeadlineMs ? v2RfDeadlineMs : power::state.bleRecovery.until;
    if (until && !(power::state.plugged && !power::retained.rtcDeepOnUsb)) items += ",\"rf_remaining_ms\":" + String((unsigned long)(until > now ? until - now : 0));
    return items;
}

void updateInfoExtra() {
    bleSetInfoProvider(infoExtraJson);
    bleSetInfoExtra(infoExtraJson());
}

int countWifiSlots() {
    state.prefs.begin("wifi", true);
    int n = 0;
    for (int i = 0; i < MAX_SLOTS; i++) {
        if (state.prefs.getString(("s" + String(i)).c_str(), "").length()) n++;
    }
    state.prefs.end();
    return n;
}

void loadPreferredWifiCredentials() {
    if (wifiSchemesCredentials(state.wifiSsid, state.wifiPass)) return;
    state.prefs.begin("wifi", true);
    uint8_t slot = state.prefs.getUChar("last", 0);
    if (slot >= MAX_SLOTS || !state.prefs.getString(("s" + String(slot)).c_str(), "").length()) {
        slot = 0;
        for (int i = 0; i < MAX_SLOTS; ++i) {
            if (state.prefs.getString(("s" + String(i)).c_str(), "").length()) { slot = i; break; }
        }
    }
    state.wifiSsid = state.prefs.getString(("s" + String(slot)).c_str(), "");
    state.wifiPass = state.prefs.getString(("p" + String(slot)).c_str(), "");
    state.prefs.end();
}

bool hasWifiSlots() {
    String ssid, password;
    return countWifiSlots() > 0 || wifiSchemesCredentials(ssid, password);
}

// ---------------- 配网模式 ----------------
static void handleConfigRoot() {
    // AP is a recovery transport for the same bonded transaction protocol.
    // It cannot invent a bond, overwrite a slot, or bypass a BLE-issued permit.
    transport::state.server.send(200, "text/html", R"HTML(<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Wi-Fi recovery</title></head><body>
<h2>Wi-Fi recovery</h2><p>Use Bridge and the device BOOT button to bind or configure Wi-Fi over Bluetooth. Saving does not activate a network.</p>
<details><summary>Authenticated transaction</summary><p>Use an endpoint token, a live bonded permit, expected_revision and a stable request_id. If the response is lost, query the same transaction before retrying.</p>
<label>Endpoint token <input id="token" type="password" autocomplete="off"></label><br>
<label>Transaction JSON<br><textarea id="body" rows="10" cols="40"></textarea></label><br>
<button id="save">Save transaction</button><button id="result">Query result</button><pre id="output"></pre></details>
<script>for(const [id,url] of [['save','/api/wifi/apply'],['result','/api/wifi/result']])document.getElementById(id).onclick=async()=>{try{const r=await fetch(url,{method:'POST',headers:{'Authorization':'Bearer '+document.getElementById('token').value,'Content-Type':'application/json'},body:document.getElementById('body').value});document.getElementById('output').textContent=await r.text()}catch(e){document.getElementById('output').textContent='Response unknown. Query the original transaction.'}};</script></body></html>)HTML");
}

static void handleConfigSave() { handleWifiManagement("apply"); }

void startConfigMode() {
    if (state.v2InRendezvous) {
        state.v2Rv.cancel(); state.v2InRendezvous = false; v2RfDeadlineMs = 0;
        state.v2RvBleReleased = false;
    }
    state.configMode = true;
    state.configStartedAt = millis();
    WiFi.mode(WIFI_AP);
    state.apSsid = "CodexStatus-" + macSuffix();
    WiFi.softAP(state.apSsid.c_str(), AP_PASSWORD);
    DevLog.println("[config] setup AP ready at http://192.168.4.1");
    display::screen({"WIFI SETUP", "", "AP:   " + state.apSsid, "PASS: " AP_PASSWORD, "", "Open http://", "192.168.4.1"});
    transport::state.server.on("/", HTTP_GET, handleConfigRoot);
    transport::state.server.on("/save", HTTP_POST, handleConfigSave);
    transport::state.server.on("/api/wifi/apply", HTTP_POST, [] { transport::v2HttpRoute(15, [] { handleWifiManagement("apply"); }); }, transport::v2HttpRawHandler);
    transport::state.server.on("/api/wifi/result", HTTP_POST, [] { transport::v2HttpRoute(16, [] { handleWifiManagement("result"); }); }, transport::v2HttpRawHandler);
    transport::state.server.on("/api/wifi/result_ack", HTTP_POST, [] { transport::v2HttpRoute(17, [] { handleWifiManagement("result_ack"); }); }, transport::v2HttpRawHandler);
    transport::state.server.on("/api/wifi/command", HTTP_POST, [] { transport::v2HttpRoute(18, [] { handleWifiManagement("command"); }); }, transport::v2HttpRawHandler);
    transport::state.server.on("/clear", HTTP_GET, [] { transport::state.server.send(410, "text/plain", "Use authenticated Wi-Fi transactions; GET cannot clear configuration."); });
    const char *headers[] = {"Authorization", "Content-Type"}; transport::state.server.collectHeaders(headers, 2);
    transport::state.server.begin();
}

// UDP announce (docs/power-state.md §9): the device broadcasts its endpoint so
// the bridge can track DHCP changes; ble=1 asks for a one-shot BLE handshake.
void sendAnnounce(bool bleFlag) {
    if (!state.wifiUp || WiFi.status() != WL_CONNECTED) return;
    IPAddress ip = WiFi.localIP();
    String mac = macText();
    char payload[192];
    int n = snprintf(payload, sizeof(payload),
                     "{\"magic\":\"codex-status\",\"mac\":\"%s\",\"ip\":\"%u.%u.%u.%u\","
                     "\"port\":80,\"proto\":\"http\",\"ble\":%d,\"fw\":\"%s\"}",
                     mac.c_str(), ip[0], ip[1], ip[2], ip[3], bleFlag ? 1 : 0, application::state.firmwareVersion);
    announceUdp.beginPacket(IPAddress(255, 255, 255, 255), 8767);
    announceUdp.write((const uint8_t *)payload, n);
    if (announceUdp.endPacket()) dataLedActivity();
    announcedIp = (uint32_t)ip;
    lastAnnounce = millis();
    DevLog.printf("[udp] announce %s ble=%d\n", ip.toString().c_str(), bleFlag ? 1 : 0);
}

static void requestAnnounce(bool bleFlag) {
    sendAnnounce(bleFlag);
}

void serviceAnnounce() {
    servicePresence();
    if (!state.wifiUp) return;
    if (millis() - lastAnnounce > ANNOUNCE_MS) { sendAnnounce(state.bleOn); return; }
    // IP-change detection at 1 Hz: esp_netif_get_ip_info() is not free and
    // calling it every loop fragmented light sleep (see the 1 Hz housekeeping
    // tick in loop()).
    static uint32_t lastIpCheckMs = 0;
    if (lastIpCheckMs && (uint32_t)(millis() - lastIpCheckMs) < 1000) return;
    lastIpCheckMs = millis();
    if ((uint32_t)WiFi.localIP() != announcedIp) sendAnnounce(state.bleOn);
}

// ---------------- v0.12 state machine ----------------
static uint32_t retryDelaySec(uint8_t stage) {
    if (stage < 3) return 60;    // 1 min x3
    if (stage < 6) return 300;   // 5 min x3
    return 900;                  // 15 min forever
}

void enterBleOn(bool userInitiated) {
    if (!userInitiated && !state.v2InRendezvous) return;
    if (userInitiated) state.v2RvBleReleased = false;
    if (state.v2RvBleReleased && !userInitiated) return;
    if (!power::setBleAwake(true)) {
        DevLog.println("[pm] BLE refused: NO_LIGHT_SLEEP lock unavailable");
        return;
    }
    // An unconfigured device must still advertise on a timer wake so the
    // bridge can issue a formal light plan and install its first Bundle.
    if (!transport::state.v2BleQueue) transport::state.v2BleQueue = xQueueCreate(2, sizeof(transport::V2BleMessage *));
    if (!bleInitialized()) {
        bleSetInfoProvider(infoExtraJson);
        bleBegin("CodexStatus-" + macSuffix(), application::state.firmwareVersion);
        bleSetEndpointHandler(transport::handleBleEndpoint);
        bleSetAuthHandler(transport::handleBleAuth);
        bleSetCommandHandler(transport::handleBleV2Ctrl);
    }
    updateInfoExtra();
    if (!state.bleOn) {
        state.bleOn = true;
        state.bleUserOff = false;
        DevLog.println("[ble] session on");
    }
    diagnostics::bleRadioMark(true);
    diagnostics::traceSetTransport(1);
    diagnostics::traceSetStage(TRACE_STAGE_WIRELESS);
    bleAdvertiseStart();
    diagnostics::traceSetStage(TRACE_STAGE_BLE_ADV);
    const uint64_t now = clock::v2NowMs(), physicalUntil = power::state.bleRecovery.physicalUntil();
    bleOpenPairingWindow(physicalUntil > now ? physicalUntil - now : 0);
    requestAnnounce(true);
    if (!state.v2InRendezvous) display::renderCurrent();
}

void bleOff(const char *reason) {
    if (!state.bleOn) return;
    state.bleOn = false;
    diagnostics::traceSetStage(TRACE_STAGE_BLE_OFF);
    bleOpenPairingWindow(0);
    wifiSetupRadioClosed("session_expired");
    diagnostics::bleRadioMark(false);
    bleAdvertiseStop();
    bleDeinit();
    power::setBleAwake(false);
    if (WiFi.status() == WL_CONNECTED) configureWifiPowerSave();
    transport::V2BleMessage *pending = nullptr;
    while (transport::state.v2BleQueue && xQueueReceive(transport::state.v2BleQueue, &pending, 0) == pdTRUE) delete pending;
    DevLog.printf("[ble] session off (%s)\n", reason ? reason : "");
    requestAnnounce(false);
    if (!state.v2InRendezvous) display::renderCurrent();
}

// Only the physical BOOT preference keeps GATT open outside a rendezvous.
void serviceBleSession() {
    if (!state.bleOn || state.v2InRendezvous) return;
    if (!power::v2ManualBleHoldActive()) bleOff("open window ended");
}

static void networkFailed(const char *reason) {
    const bool disconnect = networkFailureDisconnects(reason,
        WiFi.status() == WL_CONNECTED && WiFi.localIP() != IPAddress(0,0,0,0));
    if (disconnect) WiFi.disconnect(false);
    state.wifiUp = !disconnect;
    state.wifiLostHandled = disconnect;
    state.bridgeReachable = false;
    state.networkError = reason;
    state.wifiTerminal = disconnect;
    state.fallbackUntilMs = 0;
    state.wifiConnActive = false;
    state.failureReportPending = true;
    state.nextBleContactMs = clock::v2NowMs();
    DevLog.printf("[wifi] attempt failed: %s; connected=%d boot_remaining=%u; BLE report next\n",
                  reason, !disconnect, power::state.bleRecovery.lightRemaining(clock::v2NowMs()));
}

static bool startFallback() {
    const uint64_t now = clock::v2NowMs(), until = wifiControlUntil(now);
    if (state.fallbackAttempted || until <= now || wifiSetupBusy() || WiFi.status() == WL_CONNECTED ||
        !wifiSchemesCredentials(state.wifiSsid, state.wifiPass)) return false;
    state.fallbackAttempted = true;
    state.fallbackUntilMs = std::min<uint64_t>(until, now + 15000);
    state.wifiConnActive = true;
    state.wifiTerminal = false;
    WiFi.mode(WIFI_STA);
    WiFi.setAutoReconnect(false);
    wifiSetupConnectionStarted(state.wifiSsid);
    DevLog.printf("[net] stage=association_start remaining=%ums\n", (unsigned)(state.fallbackUntilMs-now));
    WiFi.begin(state.wifiSsid.c_str(), state.wifiPass.c_str());
    return true;
}

void pollWifi() {
    if (wifiSetupBusy()) return;
    const bool nowUp = WiFi.status() == WL_CONNECTED;
    if (state.fallbackUntilMs) {
        if (nowUp && WiFi.localIP() != IPAddress(0,0,0,0)) {
            state.fallbackUntilMs = 0; state.wifiConnActive = false;
        } else if (clock::v2NowMs() >= state.fallbackUntilMs) {
            networkFailed(wifiSetupConnectionFailure() ? wifiSetupConnectionFailure() : "connect_timeout");
            return;
        } else return;
    }
    if (nowUp) {
        if (!state.wifiUp) {
            state.wifiUp = true;
            state.wifiTerminal = false;
            diagnostics::traceSetTransport(2); diagnostics::traceSetStage(TRACE_STAGE_WIFI_IP);
            state.wifiLostHandled = false; wifiLostSince = 0; power::retained.rtcRetryStage = 0;
            state.presenceAttemptAtMs = 0; state.presenceFailures = 0;
            configureWifiPowerSave();
            sendAnnounce(state.bleOn); display::renderCurrent();
        }
        return;
    }
    if (state.wifiUp) {
        state.wifiUp = false; wifiLostSince = millis();
        DevLog.println("[wifi] link lost; awaiting BLE contact");
        display::renderCurrent();
    }
    if (!wifiLostSince) wifiLostSince = millis();
    if (!state.wifiLostHandled && millis() - wifiLostSince >= WIFI_LOST_MS) {
        state.wifiLostHandled = true;
        WiFi.disconnect(false); // stop the old association; never retry credentials
        networkFailed("wifi_link_lost");
    }
}

void resumeLightNetwork() {
    state.startupContact = "http";
    WiFi.mode(WIFI_STA);
    state.hostname = "codex-status-" + macSuffix();
    WiFi.setHostname(state.hostname.c_str());
    startNormalMode();
    if (!startFallback()) {
        networkFailed("resume_network_unavailable");
        v2RendezvousBegin();
    }
}

// No markSynced, owner claim or PowerPlan mutation: these borrow a live window.
static uint64_t wifiPresenceUntil(uint64_t now) {
    if(power::state.plugged && !power::retained.rtcDeepOnUsb)return UINT64_MAX;
    uint64_t until=power::state.v2Plan.lightActive(now)?power::state.v2Plan.deadlineMs():0;
    if(sync::state.syncOpenThisWake)until=std::max(until,sync::state.syncOpenAnsweredAt+90000);
    if(power::state.v2Provisional)until=std::max(until,power::state.v2BootMs+300000);
    until=std::max(until,power::state.bleRecovery.lightUntil);return until>now?until:0;
}

uint64_t wifiControlUntil(uint64_t now) {
    if(transport::state.otaInProgress)return 0;
    uint64_t until=wifiPresenceUntil(now);
    if(until!=UINT64_MAX && state.v2Rv.active() && state.v2Rv.deadline())until=std::min(until,state.v2Rv.deadline());
    return until>now?until:0;
}

WifiControlContext wifiControlContext(const EndpointRec &endpoint,JsonDocument &request) {
    WifiControlContext context;context.principal=endpoint.principal;context.nameSpace=endpoint.nameSpace;
    OwnerRec owner;bool owned=ownerGet(owner);context.claimed=owned && owner.id==endpoint.principal;context.occupied=owned && owner.id!=endpoint.principal;
    context.until=wifiControlUntil(clock::v2NowMs());context.physicalUntil=power::state.bleRecovery.physicalUntil();
    context.sessionValid=String(request["session_nonce"]|"")==transport::v2Nonce();return context;
}

void wifiDeviceStatus(JsonDocument &doc,const EndpointRec *endpoint) {
    doc["power"]["plan_storage"] = power::state.planStore.status();
    doc["network"]["startup_contact"] = state.startupContact;
    const uint64_t now=clock::v2NowMs(),until=wifiPresenceUntil(now),controlUntil=wifiControlUntil(now);
    const bool continuous=until==UINT64_MAX;
    const bool httpReady=state.wifiUp && WiFi.status()==WL_CONNECTED && WiFi.localIP()!=IPAddress(0,0,0,0);
    OwnerRec owner;const bool owned=ownerGet(owner);
    String key=String(power::state.plugged)+"|"+String(power::retained.rtcDeepOnUsb)+"|"+String(httpReady)+"|"+String(state.bleOn)+"|"+String(until)+"|"+String(power::state.bleRecovery.until)+
        String("|")+String(power::state.v2Plan.deadlineMs())+String("|")+WiFi.localIP().toString()+"|"+(owned?owner.id:String())+"|"+String(wifiSetupBusy());
    wifiSchemesStorageStatus(doc["wifi_storage"].to<JsonObject>());
    key+="|"+String(doc["wifi_storage"]["revision"].as<uint64_t>())+"|"+String(doc["wifi_storage"]["receipts"].as<unsigned>())+"|"+String(controlUntil);
    key+="|"+String(state.bridgeReachable)+"|"+String(state.networkError ? state.networkError : "");
    static uint32_t sequence=0;static String previous;
    if(!sequence || key!=previous){++sequence;previous=key;}
    doc["boot_id"]=transport::bootId();doc["status_seq"]=sequence;doc["device_mac"]=macText();
    doc["wifi_management"]=3;
    doc["power"]["mode"]=power::retained.rtcMode==MODE_LIGHT?"light":"sleep";doc["power"]["plan_id"]=power::state.v2Plan.acceptedId();doc["power"]["remaining_s"]=power::state.v2Plan.remainingS(now);
    auto presence=doc["presence"].to<JsonObject>();presence["mode"]=continuous?"continuous":until>now?"window":"ending";
    if(continuous)presence["remaining_ms"]=nullptr;else presence["remaining_ms"]=until>now?until-now:0;
    presence["window_id"]=transport::v2Nonce()+String("_")+String(sequence);
    doc["power"]["manual_remaining_s"]=power::state.bleRecovery.lightRemaining(now);
    doc["network"]["bridge_reachable"]=state.bridgeReachable;
    doc["network"]["error"]=state.networkError;
    doc["network"]["automatic_ble"]=automaticContactNeeded();
    presence["maintenance_remaining_ms"]=power::state.bleRecovery.until>now?power::state.bleRecovery.until-now:0;
    auto reasons=presence["reasons"].to<JsonArray>();
    if(continuous)reasons.add("usb");if(power::state.bleRecovery.until>now)reasons.add("boot");
    if(power::state.v2Provisional)reasons.add("startup");if(power::state.v2Plan.lightActive(now))reasons.add("bridge");else if(!continuous && power::state.bleRecovery.until<=now)reasons.add("timer");
    auto http=doc["transports"]["http"].to<JsonObject>();http["ready"]=httpReady;http["ip"]=ipText();http["port"]=80;
    doc["transports"]["ble"]["ready"]=state.bleOn;
    if(endpoint){
        auto control=doc["wifi_control"].to<JsonObject>();bool bound=endpoint->principal.length()&&endpoint->nameSpace.length();
        bool claimed=owned && owner.id==endpoint->principal;
        control["mode"]=controlUntil==UINT64_MAX?"continuous":controlUntil>now?"window":"unavailable";
        control["read_allowed"]=bound && controlUntil>now;
        control["write_allowed"]=bound && claimed && controlUntil>now && !wifiSetupBusy();
        if(controlUntil==UINT64_MAX)control["remaining_ms"]=nullptr;else control["remaining_ms"]=controlUntil>now?controlUntil-now:0;
        control["principal"]=endpoint->principal;
        control["namespace"]=endpoint->nameSpace;
        control["transaction_high_watermark"]=wifiManagementHighWater(endpoint->principal,endpoint->nameSpace);
        const char *reason=!bound?"binding_required":transport::state.otaInProgress?"ota_busy":!controlUntil?"window_closed":!claimed?(owned?"occupied":"claim_required"):wifiSetupBusy()?"busy":nullptr;
        if(reason)control["reason"]=reason;else control["reason"]=nullptr;
    }
}

static __attribute__((noinline)) void servicePresence() {
    if (!state.wifiUp || WiFi.status() != WL_CONNECTED || wifiSetupBusy() || transport::state.otaInProgress ||
        state.v2InRendezvous || state.failureReportPending) return;
    const uint64_t now = clock::v2NowMs(), until = wifiControlUntil(now);
    if (until <= now || (until != UINT64_MAX && until-now < 2000) ||
        (state.presenceAttemptAtMs && now-state.presenceAttemptAtMs < 30000)) return;
    state.presenceAttemptAtMs = now;
    OwnerRec owner; const bool owned = ownerGet(owner);
    bool found = false, reached = false;
    for (int i=0; i<storeCount(); ++i) {
        EndpointRec endpoint;
        if (!storeGet(i,endpoint) || !endpoint.principal.length() ||
            (owned && endpoint.principal != owner.id)) continue;
        found = true;
        JsonDocument event; event["event"]="presence"; event["device_mac"]=macText();
        event["boot_id"]=transport::bootId(); event["ip"]=ipText();
        event["manual_remaining_s"]=power::state.bleRecovery.lightRemaining(now);
        DataActivityClient connection;
        HTTPClient client; client.setConnectTimeout(750); client.setTimeout(750);
        if (client.begin(connection, String("http://")+endpoint.host+":"+String(endpoint.port)+"/api/device/presence")) {
            String body; serializeJson(event,body);
            client.addHeader("Content-Type","application/json");
            client.addHeader("Authorization",String("Bearer ")+endpoint.token);
            const int code=client.POST(body);
            if (code==202 && client.getSize()>0 && client.getSize()<=256) {
                JsonDocument reply;
                reached=!deserializeJson(reply,client.getString()) &&
                    bridgePresenceAccepted(code, String(reply["bridge_id"]|"")==endpoint.principal,
                                           String(reply["result"]|"")=="accepted");
            }
            client.end();
        }
        // One endpoint per period; do not multiply the radio budget by peers.
        break;
    }
    if (reached) {
        state.bridgeReachable=true;
        state.presenceFailures=0; state.networkError=nullptr;
        updateInfoExtra();
    } else if (!found || ++state.presenceFailures >= 3) {
        networkFailed("bridge_http_unreachable");
        updateInfoExtra();
    }
}

static __attribute__((noinline)) void serviceReadyReport() {
    const uint64_t now = clock::v2NowMs();
    if (!state.readyReport.port || state.readyReport.received || state.readyReport.attempts >= 3 ||
        now < state.readyReport.nextAtMs || WiFi.status() != WL_CONNECTED ||
        !state.httpServicesStarted || !state.v2Rv.ready()) return;
    const uint64_t legalUntil = sync::httpUntil(now);
    const uint64_t deadline = std::min(state.v2Rv.deadline(), legalUntil);
    if (deadline <= now + 1000) return;
    auto ioTimeout = [deadline]() -> uint16_t {
        const uint64_t current = clock::v2NowMs();
        return current >= deadline ? 0 :
            (uint16_t)std::min<uint64_t>(1000, (deadline - current) / 2);
    };
    EndpointRec endpoint;
    if (state.readyReport.endpointIndex < 0 || !storeGet(state.readyReport.endpointIndex, endpoint) ||
        endpoint.principal != state.readyReport.principal || !endpoint.token.length()) return;
    ++state.readyReport.attempts;
    const String base = String("http://") + state.readyReport.bridge.toString() +
        ":" + String(state.readyReport.port) + "/api/device/";
    if (!state.readyReport.httpSession.length()) {
        JsonDocument hello;
        hello["device_mac"] = macText(); hello["boot_id"] = transport::bootId();
        hello["principal"] = state.readyReport.principal;
        hello["instance_id"] = state.readyReport.principal;
        String body; serializeJson(hello, body);
        const uint16_t limit = ioTimeout();
        if (!limit) return;
        DataActivityClient connection;
        HTTPClient client; client.setConnectTimeout(limit); client.setTimeout(limit);
        if (client.begin(connection, base + "session")) {
            client.addHeader("Content-Type", "application/json");
            client.addHeader("Authorization", String("Bearer ") + endpoint.token);
            const int code = client.POST(body);
            if (code == 200 && client.getSize() > 0 && client.getSize() <= 256) {
                JsonDocument reply;
                if (!deserializeJson(reply, client.getString()) && reply["result"] == "applied" &&
                    String(reply["instance_id"] | "") == state.readyReport.principal) {
                    const char *id = reply["session_id"] | "";
                    if (strlen(id) == 32) state.readyReport.httpSession = id;
                }
            }
            client.end();
        }
    }
    if (state.readyReport.httpSession.length()) {
        if (!state.readyReport.reportBody.length()) {
            JsonDocument report;
            report["device_mac"] = macText(); report["boot_id"] = transport::bootId();
            report["principal"] = state.readyReport.principal;
            report["instance_id"] = state.readyReport.principal;
            report["origin_session"] = state.readyReport.originSession;
            report["origin_request_no"] = state.readyReport.originRequestNo;
            report["business_id"] = sync::syncNumber(state.readyReport.planId);
            report["context_id"] = platform::state.v2BundleReady ? platform::state.v2Profile.contextId : "";
            report["ip"] = WiFi.localIP().toString(); report["port"] = 80;
            report["http_ready"] = true;
            report["plan_result"] = "applied";
            report["data_result"] = state.readyReport.dataResult.length() ? state.readyReport.dataResult : "none";
            report["network_result"] = "ready";
            report["remaining_ms"] = (uint32_t)std::min<uint64_t>(deadline - now, 90000);
            serializeJson(report, state.readyReport.reportBody);
        }
        const uint16_t limit = ioTimeout();
        if (!limit) return;
        DataActivityClient connection;
        HTTPClient client; client.setConnectTimeout(limit); client.setTimeout(limit);
        if (client.begin(connection, base + "report")) {
            CompactFrameHeader request;
            request.kind = 2;
            request.op = 20;
            request.requestNo = 1;
            request.payloadLen = state.readyReport.reportBody.length();
            if (!transport::compactSessionFromHex(state.readyReport.httpSession, request.session)) {
                client.end(); return;
            }
            if (platform::state.v2BundleReady && strlen(platform::state.v2Profile.contextId) == 16)
                memcpy(request.context, platform::state.v2Profile.contextId, 16);
            uint8_t header[COMPACT_FRAME_HEADER];
            if (!compactFrameEncode(request, header) ||
                COMPACT_FRAME_HEADER + state.readyReport.reportBody.length() > 4096) {
                client.end(); return;
            }
            String frame(reinterpret_cast<const char *>(header), COMPACT_FRAME_HEADER);
            frame.concat(state.readyReport.reportBody.c_str(), state.readyReport.reportBody.length());
            client.addHeader("Content-Type", "application/vnd.codex-status.frame");
            client.addHeader("Authorization", String("Bearer ") + endpoint.token);
            const int code = client.POST(reinterpret_cast<uint8_t *>(const_cast<char *>(frame.c_str())), frame.length());
            if (code == 202 && client.getSize() >= COMPACT_FRAME_HEADER && client.getSize() <= 256) {
                String response = client.getString();
                CompactFrameHeader ack;
                if (compactFrameDecode(reinterpret_cast<const uint8_t *>(response.c_str()),
                    response.length(), ack, 256) && ack.kind == 1 && ack.op == 20 &&
                    ack.result == 1 && ack.requestNo == request.requestNo &&
                    !memcmp(ack.session, request.session, 16)) {
                    JsonDocument reply;
                    if (!deserializeJson(reply, String(response.c_str() + COMPACT_FRAME_HEADER, ack.payloadLen)) &&
                        reply["result"] == "report_received") {
                        state.readyReport.received = true;
                        diagnostics::traceSetStage(TRACE_STAGE_HTTP);
                        DevLog.printf("[net] stage=ready_report_acked plan=%llu\n", (unsigned long long)state.readyReport.planId);
                    }
                }
            } else if (code == 409 || code == 401) {
                // A Bridge restart invalidates the hint session. The business
                // attempt and its original identifiers stay frozen.
                state.readyReport.httpSession = "";
                state.readyReport.reportBody = "";
            }
            client.end();
        }
    }
    if (!state.readyReport.received) {
        const uint32_t backoff = state.readyReport.attempts == 1 ? 250 : 1000;
        const uint64_t current = clock::v2NowMs();
        state.readyReport.nextAtMs = current + std::min<uint64_t>(backoff,
            deadline > current ? deadline - current : 0);
    }
}

// No owner/plan mutation. Auth is resolved independently of payload bridge_id.
void handleWifiManagement(const char *op) {
    String header=transport::state.server.header("Authorization");EndpointRec endpoint;
    if(!header.startsWith("Bearer ") || !storeAuthenticate(header.substring(7),endpoint)) {
        transport::v2Response(401,"{\"ok\":false,\"error\":\"unauthorized\"}");return;
    }
    JsonDocument request;const String body=transport::state.v2HttpFrameActive ? transport::state.v2HttpBody : transport::state.server.arg("plain");
    if(body.length()>8192 || deserializeJson(request,body) || String(request["device_mac"]|"")!=macText()) {
        transport::v2Response(400,"{\"ok\":false,\"error\":\"shape\"}");return;
    }
    if(strcmp(op,"command"))request["op"]=op;
    auto context=wifiControlContext(endpoint,request);
    String response=wifiControlCommand(request,context,clock::v2NowMs());
    JsonDocument outcome;deserializeJson(outcome,response);
    transport::v2Response(200,response);
}

String wifiManagementRequest(JsonDocument &request,const char *op) {
    EndpointRec endpoint;
    if(!storeAuthenticate(request["token"]|"",endpoint)) return "{\"ok\":false,\"error\":\"unauthorized\"}";
    request["op"]=op;
    auto context=wifiControlContext(endpoint,request);
    return wifiControlCommand(request,context,clock::v2NowMs());
}

void v2RendezvousBegin(bool deep) {
    state.fallbackAttempted = false;
    state.v2InRendezvous = true;
    state.v2RvBleReleased = false;
    v2RvStartMs = clock::v2NowMs();
    char identity[33];
    snprintf(identity, sizeof(identity), "%08lx%08lx%08lx%08lx",
             (unsigned long)esp_random(), (unsigned long)esp_random(),
             (unsigned long)esp_random(), (unsigned long)esp_random());
    state.v2RvId = identity;
    v2RvWifiPhase = 0;
    state.v2RvNetworkExpected = false;
    state.v2RvNetworkRequested = false;
    state.v2RvBleReleaseAtMs = 0;
    state.readyReport = {};
    if (deep && syncStoreAvailable()) {
        syncCountDeepRendezvous(sync::retained.syncRtc);
        sync::state.syncSkippedThisWake = sync::retained.syncRtc.retrySkip != 0;
        sync::state.syncVisibleSkip = sync::retained.syncRtc.retrySkip;
        if (sync::state.syncSkippedThisWake) syncConsumeSkip(sync::retained.syncRtc);
    }
    sync::state.v2HistorySyncMs = 0; sync::state.v2HistorySyncUntilMs = 0;
    state.v2Rv.begin(clock::v2NowMs(), V2_RENDEZVOUS_WINDOW_MS);
    v2RfDeadlineMs = state.v2Rv.deadline();
    enterBleOn(false);
    if (!state.bleOn) state.v2Rv.cancel();
}

static void rendezvousWifiBegin() {
    state.hostname = "codex-status-" + macSuffix();
    WiFi.setAutoReconnect(false);
    WiFi.mode(WIFI_STA); WiFi.setHostname(state.hostname.c_str());
    esp_wifi_set_ps(WIFI_PS_NONE);
    // A Plan admits HTTP, not an old-network connection. The Bridge must
    // resolve its live host SSID, provision if necessary, then activate over BLE.
    v2RvWifiPhase = 3;
    if (!state.httpServicesStarted) { transport::registerHttpRoutes(); transport::state.server.begin(); state.httpServicesStarted = true; }
    state.v2RvHttpStarted = true;
}

static void rendezvousWifiTick() {
    if (wifiSetupBusy()) return;
    if (!state.fallbackUntilMs && WiFi.status()!=WL_CONNECTED) startFallback();
    if (state.fallbackUntilMs && clock::v2NowMs() >= state.fallbackUntilMs &&
        (WiFi.status()!=WL_CONNECTED || WiFi.localIP()==IPAddress(0,0,0,0))) {
        networkFailed(wifiSetupConnectionFailure() ? wifiSetupConnectionFailure() : "connect_timeout");
        state.v2Rv.finishExchange(clock::v2NowMs(), 0);
        return;
    }
    if (WiFi.status() == WL_CONNECTED && !(WiFi.localIP() == IPAddress(0,0,0,0))) {
        state.wifiUp = true;
        state.wifiConnActive = false;
        state.wifiBlinkOn = false;
        if (!state.v2Rv.ready()) {
            diagnostics::traceSetTransport(2); diagnostics::traceSetStage(TRACE_STAGE_WIFI_IP);
            state.v2Rv.httpReady(); updateInfoExtra();
            DevLog.printf("[net] stage=http_listening plan=%llu\n", (unsigned long long)power::state.v2Plan.acceptedId());
        }
        serviceReadyReport();
        return;
    }
    // Configure before association completes; report handling does not start
    // another association after the accepted Plan window.
    wifi_config_t cfg = {};
    if (esp_wifi_get_config(WIFI_IF_STA, &cfg) == ESP_OK && cfg.sta.listen_interval != 10) {
        cfg.sta.listen_interval = 10; esp_wifi_set_config(WIFI_IF_STA, &cfg);
    }
    esp_wifi_set_ps(WIFI_PS_NONE);
}

void v2RendezvousTick() {
    const uint64_t now = clock::v2NowMs();
    if (bleIsConnected()) state.v2Rv.connected(now);
    const bool light = power::state.v2Plan.lightActive(now);
    const bool syncGuard = sync::state.syncEnabled && (sync::retained.syncRtc.flags & 1) && !sync::state.syncOpenThisWake && !sync::state.syncSkippedThisWake;
    if (state.v2RvNetworkRequested && ((state.v2Rv.hasPlan() && light) || sync::state.syncOpenThisWake)) {
        const uint64_t legalUntil = sync::httpUntil(now);
        state.v2Rv.admitHttp(now, legalUntil);
        if (state.v2Rv.needsHttp()) {
            if (state.bleOn && !power::v2ManualBleHoldActive()) {
                if (now < state.v2RvBleReleaseAtMs) return;
                state.v2RvBleReleased = true;
                bleOff("network accepted");
            }
            if (!v2RvWifiPhase) rendezvousWifiBegin();
            rendezvousWifiTick();
            if (state.readyReport.received || state.readyReport.attempts >= 3)
                state.v2Rv.finishExchange(now, sync::state.v2HistorySyncUntilMs);
        }
    } else if (state.v2Rv.hasPlan() && !state.v2RvNetworkExpected && !syncGuard) {
        state.v2Rv.finishExchange(now, sync::state.v2HistorySyncUntilMs);
    }
    v2RfDeadlineMs = state.v2Rv.deadline();
    if (state.v2Rv.active() && (state.bleOn || state.v2Rv.needsHttp() ||
         state.v2Rv.phase() == RendezvousPhase::AckGrace) &&
        !v2BatteryPowerOff(power::state.plugged, power::batteryPercent()) && !state.configMode) {
        if (!state.v2Rv.tick(now)) return;
    } else state.v2Rv.cancel();
    // Physical recovery has its own fixed deadline. Ordinary/USB auto BLE is
    // released for this window and cannot reopen behind a network task.
    if (!power::v2ManualBleHoldActive()) { state.v2RvBleReleased = true; bleOff("rendezvous deadline"); }
    const bool networkFailedThisRound = state.v2RvNetworkRequested &&
        (!state.readyReport.received || WiFi.status()!=WL_CONNECTED);
    state.v2InRendezvous = false; v2RfDeadlineMs = 0;
    if (state.readyReport.received) {
        state.bridgeReachable=true;
        state.networkError=nullptr; state.presenceFailures=0;
    }
    diagnostics::state.wakeResult = light ? WAKE_RV_LIGHT : WAKE_RV_SLEEP;
    diagnostics::retained.rtcNetCycles++;
    DevLog.printf("[platform] rendezvous network=%d light=%d elapsed=%ums\n", state.v2RvNetworkRequested, light, (unsigned)(now-v2RvStartMs));
    const bool reporting = state.failureReportPending;
    state.failureReportPending = false;
    state.nextBleContactMs = now + (uint64_t)V2_RENDEZVOUS_S * 1000ULL;
    if (!reporting && !state.v2RvNetworkRequested && !wifiSetupBusy() && WiFi.status()!=WL_CONNECTED &&
        power::state.bleRecovery.lightRemaining(now)) startFallback();
    if (networkFailedThisRound && !reporting) {
        networkFailed(WiFi.status()==WL_CONNECTED ? "bridge_http_unreachable" :
            wifiSetupConnectionFailure() ? wifiSetupConnectionFailure() : "connect_timeout");
    }
    const bool stayAwake = state.failureReportPending || power::state.bleRecovery.lightRemaining(now) || light || sync::state.syncOpenThisWake || power::v2ManualBleHoldActive() || (power::state.plugged && !power::retained.rtcDeepOnUsb);
    if (!stayAwake && power::retained.rtcMode != MODE_DEEP) {
        // Transition through the common mode/persistence/full sleep-frame path.
        // enterDeep renders the latest snapshot, so do not draw a Light frame first.
        platform::state.v2WakeRenderPending = false;
        power::enterDeep("rendezvous");
        return;
    }
    display::v2RendezvousRender(light);
    if (stayAwake) {
        startNormalMode();
        // A timed-out network attempt gets one ordinary bounded report round
        // before sleep; it cannot recursively retry networking in that round.
        if (state.failureReportPending) v2RendezvousBegin();
    }
    else {
        power::retained.rtcNextContactS = V2_RENDEZVOUS_S;
        power::retained.rtcNextNetAt = clock::timeKnown() ? (uint32_t)time(nullptr)+power::retained.rtcNextContactS : 0;
        power::sleepToNextEvent();
    }
}

// Wi-Fi power save: WIFI_PS_MAX_MODEM with listen_interval=10 wakes the modem
// every 10 beacons (~1 s) instead of every DTIM, so bridge pushes still wake
// the device from light sleep with acceptable latency (T9). Applying the
// listen interval needs one reassociation; while that runs pollWifi must not
// read the deliberate drop as a lost link.
static bool configureWifiPowerSave() {
    // Association belongs to an explicit Bridge Wi-Fi transaction. Never
    // disconnect/reconnect here or block the BLE/button loop for power save.
    esp_wifi_set_ps(state.bleOn ? WIFI_PS_NONE : WIFI_PS_MAX_MODEM);
    return WiFi.status() == WL_CONNECTED;
}

void startNormalMode() {
    state.configMode = false;
    diagnostics::traceSetTransport(2);
    diagnostics::traceSetStage(TRACE_STAGE_WIRELESS);
    state.hostname = "codex-status-" + macSuffix();
    // Keep the active TZ (NVS/bridge-provided); a hard-coded "CST-8" here used
    // to clobber a bridge-synced timezone on every light start.
    esp_sntp_set_time_sync_notification_cb(clock::ntpTimeSynced);
    configTzTime(clock::state.deviceTz, "pool.ntp.org");
    // Awaiting provisioning is idle, not a Wi-Fi association attempt.
    serviceWifiMetadata();

    // Deep wake: leave the sleep frame at once (Zzz gone) so BOOT has instant
    // feedback; the Wi-Fi cell starts visible and Bridge keeps its old glyph.
    // This first wake frame is a clean full baseline; a partial here can leave
    // a Zzz ghost that reads as "still asleep". The timer pull path already
    // drew that baseline inside deepNetworkCycle (wakeBaselineDrawn), so it
    // must not draw a second full frame here.
    if (!display::state.wakeBaselineDrawn && power::state.wokeFromDeep && power::retained.rtcMode == MODE_LIGHT && (diagnostics::retained.rtcDeepGlyph & 1)) {
        display::state.forceCleanRefresh = true;
        display::renderCurrent();
    }
    display::state.wakeBaselineDrawn = false;

    // task-10 A: cold boot with a cached usage snapshot shows the template at
    // once instead of blocking on association.
    const bool cachedColdBoot = !power::state.wokeFromDeep && platform::state.lastUsage.length() > 0;
    if (cachedColdBoot) display::renderCurrent();

    // Normal mode only serves an already admitted connection. Network choice
    // and asynchronous association belong to authenticated BLE provisioning.
    state.wifiUp = WiFi.status() == WL_CONNECTED;
    serviceWifiMetadata();
    if (!state.httpServicesStarted) { transport::registerHttpRoutes(); transport::state.server.begin(); state.httpServicesStarted = true; }

    if (state.wifiUp) {
        diagnostics::traceSetStage(TRACE_STAGE_WIFI_IP);
        state.wifiLostHandled = false;
        wifiLostSince = 0;
        power::state.lastBattCheck = millis();
        power::state.batteryPct = power::batteryPercent();
        configureWifiPowerSave();
        ArduinoOTA.setHostname(state.hostname.c_str());
        ArduinoOTA.onStart([]() {
            power::setOtaLock(true);
            display::screen({"ArduinoOTA", "updating..."});
        });
        ArduinoOTA.onProgress([](unsigned int p, unsigned int t) {
            if (p) dataLedActivity();
            DevLog.printf("[ota] %u%%\r", t ? p * 100 / t : 0);
        });
        ArduinoOTA.onEnd([]() {
            display::screen({"OTA OK", "rebooting..."});
            delay(800);
            power::setOtaLock(false);
        });
        ArduinoOTA.onError([](ota_error_t e) {
            power::setOtaLock(false);
            DevLog.printf("[ota] error %u\n", e);
        });
        // task-3: no mDNS responder. Name resolution (.local) is dropped on
        // purpose (the bridge discovers via UDP/ARP/BLE); the DHCP hostname
        // (option 12) set by WiFi.setHostname still shows in the router.
        ArduinoOTA.setMdnsEnabled(false);
        ArduinoOTA.begin();
        // Wi-Fi is up: render again so the Wi-Fi icon appears (the wake render
        // drew the template while the state was still WIFI OFF). rtcDeepGlyph
        // bit0 = the last deep entry actually drew the sleep glyph.
        if (power::state.wokeFromDeep && (diagnostics::retained.rtcDeepGlyph & 1)) display::renderCurrent();
        // Cold boot: never leave the boot/connecting page up when the first
        // pull failed. The cached usage (or the built-in status screen) is
        // rendered once the link is up, so the panel is never stale. An
        // identical frame costs no write (RFN_NONE). With a cached snapshot
        // this is also the single link-up partial that lights the Wi-Fi icon
        // (the template was already drawn with WIFI OFF before the connect).
        if (!power::state.wokeFromDeep) display::renderCurrent();
        power::noteActivity(power::state.deepWakePath ? "deep-light" : "boot");
        sendAnnounce(state.bleOn);
    } else {
        state.wifiLostHandled = true;
        wifiLostSince = millis() - WIFI_LOST_MS;   // already past loss detection
        DevLog.println("[wifi] awaiting Bridge-selected network over BLE");
    }

    DevLog.printf("[net] ready state=%s ip=%s host=%s.local endpoints=%d\n",
                  wifiPhaseText(wifiPhase()), ipText().c_str(), state.hostname.c_str(), storeCount());
}

} // namespace firmware::radio
