#include "application_internal.h"
#include "clock_internal.h"
#include "diagnostics_internal.h"
#include "display_internal.h"
#include "platform_internal.h"
#include "power_internal.h"
#include "radio_internal.h"
#include "sync_internal.h"
#include "transport_internal.h"
#include "application.h"

namespace firmware::application {

State state;

// A target whose panel combination is not hardware-verified refuses normal
// operation: the ROM builds and is host-tested, but it must not pretend the
// panel contract is proven. Set CODEX_GRAY4_PANEL_CONFIRMED only after the
// hardware facts (panel model, controller datasheet, pins, waveform) exist.
static bool targetUnverified = false;


void initialize(const char *firmwareVersion, const char *imageIdentity) {
    state.firmwareVersion = firmwareVersion;
    state.imageIdentity = imageIdentity;
    esp_sleep_wakeup_cause_t cause = esp_sleep_get_wakeup_cause();
    state.bootWakeCause = cause;
    diagnostics::traceBegin(cause);
    diagnostics::retained.rtcLastWake = (uint8_t)cause;
    diagnostics::retained.rtcStage = 1;
    if (cause != ESP_SLEEP_WAKEUP_TIMER) {
        power::retained.rtcDeepOnUsb = 0;      // reboot/reset clears the test switches
        diagnostics::retained.rtcFrameCapture = 0;
    }
    {
        Preferences p;
        p.begin("pm", true);
        diagnostics::state.nvsStageAtBoot = p.getUChar("stg", 0xFF);
#if defined(CODEX_TARGET_NOTE4)
        display::state.note4KeepPanelPower = p.getUChar("panel_pwr", 1) != 0;
#endif
        power::state.bootPostOtaS = p.getUShort("post_ota_s", 0);
        String tz = p.getString("tz", "");
        if (tz.length() && tz.length() < (int)sizeof(clock::state.deviceTz)) {
            strncpy(clock::state.deviceTz, tz.c_str(), sizeof(clock::state.deviceTz) - 1);
            clock::state.deviceTz[sizeof(clock::state.deviceTz) - 1] = '\0';
        }
        p.end();
    }
#if defined(CODEX_TARGET_NOTE4)
    EPD_SSD2683_SetKeepPower(display::state.note4KeepPanelPower);
#endif
    clock::applyTimezone();
    diagnostics::state.nvsStageLast = 0xFF;
    diagnostics::nvsStageMark(1);      // wake reached setup (diagnostic sessions only)
    bool woke = (cause == ESP_SLEEP_WAKEUP_TIMER || cause == ESP_SLEEP_WAKEUP_EXT1);
    radio::loadHostMac();
    radio::state.hostname = "codex-status-" + radio::macSuffix();
    power::releaseWakeHolds();   // clear deep-sleep GPIO holds from the previous cycle
    diagnostics::nvsStageMark(4);      // holds released without losing the VBAT latch
    power::state.plugged = usb_serial_jtag_is_connected();
    power::state.batteryPct = power::batteryPercent();
    if (!clock::readExternalRtc(true)) {
        DevLog.printf("[clock] %s unavailable: %s; internal RTC fallback\n",
                      clock::state.externalRtc.model(), clock::state.externalRtc.status());
        if (power::retained.rtcMagic == 0xC0DE0001) clock::restoreTimeFromRtc();
        if (clock::timeKnown()) clock::state.timeSource = TIME_RTC;
    }

    // A deep timer wake before the next contact first tries the retained clock
    // window. If the panel lacks a usable baseline, setup falls through to the
    // local full-render fallback below. Buttons and USB use the normal path.
    bool deepTimerBoot = (cause == ESP_SLEEP_WAKEUP_TIMER && power::retained.rtcMode == MODE_DEEP &&
                          (!power::state.plugged || power::retained.rtcDeepOnUsb));
    bool deepClockOnlyBoot = deepTimerBoot && !power::deepNetDue();
    diagnostics::nvsStageMark(5);      // wake classified (deepTimerBoot known)
    diagnostics::histAdd(HIST_BOOT, (uint16_t)cause);
    diagnostics::state.wakeResult = deepTimerBoot ? WAKE_NET : WAKE_LIGHT;
    if (deepClockOnlyBoot) {
        diagnostics::state.wakeResult = WAKE_THIN;
        diagnostics::traceMarkThin();
        if (power::deepThinWake()) power::sleepToNextEvent();
    }

    display::epdBegin(!woke);
#if defined(CODEX_TARGET_NOTE4)
    if (woke && power::retained.rtcMagic == 0xC0DE0001) display::note4RestoreFrameBaseline(false);
#endif
    if (!woke) display::screen({"CODEX STATUS", application::state.firmwareVersion, "booting..."});
    if (power::retained.rtcMagic != 0xC0DE0001) {
        power::retained.rtcMagic = 0xC0DE0001;
        radio::retained.rtcActiveAt = 0;
        radio::retained.rtcActiveMac[0] = 0;
        radio::retained.rtcLastSyncEpoch = 0;
        diagnostics::retained.rtcDeepGlyph = 0;
        power::retained.rtcRetryStage = 0;
        platform::retained.rtcUsageHash = 0;
        power::retained.rtcMode = MODE_LIGHT;
        power::retained.rtcNextContactS = DEEP_CONTACT_DEFAULT_S;
        power::retained.rtcNextNetAt = 0;
        platform::retained.rtcUsageRev = 0;
        clock::retained.rtcEpochUsAtSleep = 0;
        clock::retained.rtcClkUsAtSleep = 0;
        radio::retained.rtcApChannel = 0;
        radio::retained.rtcApBssid[0] = 0;
        radio::retained.rtcApSlot = 0xFF;
        display::retained.rtcClkPartials = 0;
        display::retained.rtcEpdBusyFails = 0;
        platform::retained.rtcTplActiveId[0] = 0;
        platform::retained.rtcTplHash[0] = 0;
        display::retained.rtcClkContextId[0] = 0;
        diagnostics::retained.rtcAccCycles = 0;
        diagnostics::retained.rtcAccAwakeMs = 0;
        diagnostics::retained.rtcAccBleMs = 0;
        diagnostics::retained.rtcAccRenderMs = 0;
        display::retained.clkR.valid = false;
        display::retained.clkPixelsValid = false;
        DevLog.println("[pm] RTC state initialized");
    }
    // The last successful sync must survive OTA/software resets (RTC data is
    // not reliable across them on this board), so it is also kept in NVS.
    if (radio::retained.rtcLastSyncEpoch == 0) {
        Preferences p;
        p.begin("sync", true);
        radio::retained.rtcLastSyncEpoch = p.getUInt("last", 0);
        p.end();
    }
    radio::state.lastPersistedSync = radio::retained.rtcLastSyncEpoch;
    // v2: the BOOT provisional window starts at the physical wake instant; a
    // timer wake never gets this fallback (docs/generic-display-platform-design §7).
    power::state.v2BootMs = clock::v2NowMs();
    power::state.v2Provisional = (cause == ESP_SLEEP_WAKEUP_EXT1);
    if (cause == ESP_SLEEP_WAKEUP_EXT1) {
        // Manual wake (BOOT/PWR): leave deep and behave as light (docs §13.3).
        power::retained.rtcMode = MODE_LIGHT;
        power::persistMode();
        power::retained.rtcRetryStage = 0;   // button wake replays the retry cadence from the top
        diagnostics::histAdd(HIST_TO_LIGHT, 1);   // aux=1: button wake
        DevLog.printf("[pm] button wake: light mode, provisional %us\n",
                      (unsigned)V2_BOOT_PROVISIONAL_S);
    }
    if (power::state.plugged && !power::retained.rtcDeepOnUsb) power::retained.rtcMode = MODE_LIGHT;
    if (power::state.bootPostOtaS) {
        // Fresh firmware after an OTA: stay light for the minimum window so the
        // bridge can reach HTTP and install/refresh the formal light plan.
        power::retained.rtcMode = MODE_LIGHT;
        power::persistMode();
        power::state.postOtaHoldUntilMs = millis() + (uint32_t)power::state.bootPostOtaS * 1000UL;
        DevLog.printf("[ota] post-OTA light window %us (bridge may extend)\n",
                      (unsigned)power::state.bootPostOtaS);
        Preferences p;
        p.begin("pm", false);
        p.remove("post_ota_s");
        p.end();
        power::state.bootPostOtaS = 0;
    }
    power::state.deepWakePath = deepTimerBoot;
    // Any wake from deep wakes up on top of the sleep glyph: connect silently
    // and replace it as soon as the wake render runs (docs/ble-rendezvous-power-design §3).
    power::state.wokeFromDeep = woke;

#ifdef CODEX_TARGET_UNVERIFIED
    targetUnverified = true;
    display::state.epdPartialReady = false;
    display::state.epdBaselineTrusted = false;
    DevLog.printf("[target] %s (%s) unverified: compile/host-tested only; "
                  "hardware facts missing (blocked_by_hardware_arrival)\n",
                  FW_TARGET_ID, RENDER_TARGET_ID);
#endif
    const esp_partition_t *running = esp_ota_get_running_partition();
    DevLog.printf("\n[codex-status] v%s mac=%s reset=%s slot=%s wake=%d(%s) usb=%d deepusb=%d mode=%s\n",
                  application::state.firmwareVersion, radio::macText().c_str(), diagnostics::resetReasonName(),
                  running ? running->label : "?", (int)cause,
                  diagnostics::wakeCauseName(cause), power::state.plugged ? 1 : 0, (unsigned)power::retained.rtcDeepOnUsb,
                  power::retained.rtcMode == MODE_DEEP ? "deep" : "light");
    { Preferences p; p.begin("brg", false); p.end(); }
    ownerBegin();

    tplStoreBegin();
    tplXferBegin(application::state.firmwareVersion, []() { platform::state.pendingTplChanged = true; });
    // v2 Bundle store: a committed A/B bundle takes over the active template
    // path; without one the legacy store continues to serve templates.
    platform::state.v2CtxGen.seed(esp_random());
    platform::state.v2BundleReady = bsBegin();
    if (!syncStoreBegin()) DevLog.println("[sync] frozen store unavailable");
    // Old ROMs persisted an OTA arm that could block recovery after A/B rollback.
    Preferences oldOta;
    if (oldOta.begin("sync-ota", false)) {
        if (oldOta.isKey("arm")) oldOta.remove("arm");
        oldOta.end();
    }
    sync::retained.syncRtc.reasonBits &= ~(1u << 3);
    {
        Preferences p;
        if (p.begin("sync-v1", true)) {
            sync::state.syncEnabled = p.getBool("enabled", false);
            sync::state.syncOwner = p.getString("owner", "");
            p.end();
        }
    }
    if (platform::state.v2BundleReady) {
        bsProfile(platform::state.v2Profile);
        DevLog.printf("[platform] bundle job=%s active=%s templates=%u ctx=%s commit=%u\n",
                      platform::state.v2Profile.jobId, platform::state.v2Profile.ids[platform::state.v2Profile.initial],
                      (unsigned)platform::state.v2Profile.count, platform::state.v2Profile.contextId,
                      (unsigned)bsCommitSeq());
        platform::v2ActiveLoad();
        if (!deepClockOnlyBoot) {
            platform::state.v2DataSeq.beginContext(clock::v2NowMs(), 1);
            if (esp_reset_reason() == ESP_RST_DEEPSLEEP) {
                platform::retained.v2DataCheckpoint.restore(platform::state.v2Profile.contextId, platform::state.v2DataSeq);
            }
            if (!platform::state.v2DataSeq.haveApplied()) {
                // Unknown retention (cold reset, corrupt RTC, or new context): rotate
                // context so an in-flight old snapshot cannot be mistaken for new.
                if (!platform::v2SwitchActive(platform::state.v2Profile.initial)) {
                    platform::state.v2CtValid = false;
                    DevLog.println("[platform] cannot rotate unknown-retention context; data disabled");
                }
            }
        }
    } else {
        DevLog.println("[platform] no committed bundle; awaiting initial publish");
    }
    if (!deepClockOnlyBoot) {
        power::state.v2SafetyDeadlineMs = clock::v2NowMs() + (uint64_t)V2_MAX_LIGHT_S * 1000ULL;
    }
    String cached;
    if (platform::usageCacheLoad(cached)) platform::state.lastUsage = cached;

    // Clock-only wakes never initialize data sequence state or a safety plan.
    // If the retained window failed, use the local Bundle and usage cache to
    // rebuild the full frame, then keep the existing rendezvous schedule.
    if (deepClockOnlyBoot) {
        bool clockTemplateReady = platform::state.v2BundleReady
            ? platform::state.v2CtValid && platform::state.activeTplHasNow
            : platform::tplCacheLoad() && platform::state.activeTplHasNow;
        if (clockTemplateReady && platform::state.lastUsage.length()) {
            display::renderActiveUsage(platform::state.lastUsage, "RTC");
        }
        power::sleepToNextEvent();   // no network, owner claim, or data-sequence change
    }

    // All association is owned by bounded, explicit Wi-Fi transactions.
    WiFi.setAutoReconnect(false);
    transport::loadOrIssueAuthToken();
    // Initialize once for this wake, before BLE and button handling begin.
    pinMode(0, INPUT_PULLUP); pinMode(18, INPUT_PULLUP); pinMode(3, OUTPUT);
    digitalWrite(3, HIGH);
    power::configurePowerManagement(); power::setupOtaPmLock();
    if (cause == ESP_SLEEP_WAKEUP_EXT1 &&
        (esp_sleep_get_ext1_wakeup_status() & (1ULL << 0))) {
        // GPIO0 itself woke the device: do not require a second click.
        power::state.bleRecovery.open(power::state.v2BootMs);
        radio::enterBleOn(true);
    }
    wifiSchemesBegin();
    if (cause != ESP_SLEEP_WAKEUP_TIMER) power::state.offlineBootGraceUntilMs = clock::v2NowMs() + 120000;
    if (!radio::hasWifiSlots() && !platform::state.lastUsage.length())
        display::screen({"CODEX STATUS", application::state.firmwareVersion, "BLE Wi-Fi setup", "Press BOOT to pair"});
    // Every ordinary contact starts with bounded BLE, including cold startup.
    // No saved-network scan/association may run before a Bridge is present.
    radio::v2RendezvousBegin(power::state.deepWakePath);
}

void tick() {
    clock::flushNtpRtcSync();
    const uint64_t clockPollMs = (uint64_t)(esp_timer_get_time() / 1000ULL);
    if (clockPollMs - clock::state.externalRtcLastPollMs >= 60000) clock::readExternalRtc(false);
    if (transport::state.otaRebootPending && (int32_t)(millis() - transport::state.otaRebootAt) >= 0) ESP.restart();
    // OTA stall watchdog: no upload chunk for 20 s -> abort and restore the UI
    // (the WebServer also reports UPLOAD_FILE_ABORTED; this is the backstop).
    if (transport::state.otaInProgress && transport::state.otaLastDataMs && (int32_t)(millis() - transport::state.otaLastDataMs) > 20000) {
        transport::otaUploadCleanup("stalled");
    }
    diagnostics::handleSerialCli();
    transport::state.server.handleClient();
    if (targetUnverified) {
        // Unverified panel combination: stay recoverable over USB serial and
        // never drive data, templates, radios or the panel runtime.
        static uint32_t lastNote = 0;
        if (millis() - lastNote > 10000UL) {
            lastNote = millis();
            DevLog.printf("[target] %s unverified; normal operation refused\n", FW_TARGET_ID);
        }
        delay(50);
        return;
    }
    blePoll();
    diagnostics::tracePollConnection();
    transport::serviceV2Ble();

    static uint32_t pwrDownAt = 0;
    static bool pwrHandled = false;
    static uint32_t pwrEnableAt = 0;
    if (!pwrEnableAt) pwrEnableAt = millis() + 5000;   // boot grace: PWR just powered the board on
    if (digitalRead(18) == LOW) {
        if (!pwrDownAt) pwrDownAt = millis();
        else if (!pwrHandled && millis() - pwrDownAt > 3000 &&
                 (int32_t)(millis() - pwrEnableAt) >= 0) {
            pwrHandled = true;
            power::powerOff();
        }
    } else {
        pwrDownAt = 0;
        pwrHandled = false;
    }

    static uint32_t bootDownAt = 0;
    static int      bootStage = 0;
    // Arm the button only after GPIO0 has read HIGH once: a strapping/mux
    // glitch that made it read LOW for the first seconds of boot otherwise
    // looked like a long hold.
    static bool     bootArmed = false;
    bool bootLow = (digitalRead(0) == LOW);
    if (!bootArmed && !bootLow) bootArmed = true;
    if (bootArmed && bootLow) {
        if (!bootDownAt) bootDownAt = millis();
        uint32_t held = millis() - bootDownAt;
        if (bootStage < 1 && held > 2000) {
            bootStage = 1;
            platform::nextTemplate();     // 2 s: next local template (no BLE session)
        }
        if (bootStage < 2 && held > 15000) {
            bootStage = 2;
            radio::retained.rtcApReason = 2;
            DevLog.println("[config] BOOT held 15s; entering AP mode");
            radio::startConfigMode();
            return;
        }
        if (bootStage < 3 && held > 30000) {
            bootStage = 3;
            platform::factoryReset();
        }
    } else {
        if (bootDownAt) {
            uint32_t held = millis() - bootDownAt;
            if (bootStage == 0 && held > 50 && held < 2000) power::handleBootClick();
            bootDownAt = 0;
            bootStage = 0;
        }
    }

    power::pollPlug();
    radio::serviceBleSession();
    power::serviceLed();
    OwnerRec wifiOwner;
    const String selectingBridge = wifiSetupSelectingBridge();
    if(wifiSetupBusy()) {
        OwnerRec owner;String principal=wifiSetupSelectingBridge();
        wifiSetupConstrain(clock::v2NowMs(),radio::wifiControlUntil(clock::v2NowMs()),ownerGet(owner) && owner.id==principal);
    }
    if (wifiSetupBusy() && selectingBridge.length() && ownerGet(wifiOwner) && wifiOwner.id != selectingBridge)
        wifiSetupAbort("occupied");
    if (!(radio::state.v2InRendezvous && radio::state.bleOn && radio::state.v2RvNetworkExpected))
        wifiSetupPoll(clock::v2NowMs(), blePeerIdentity(), bleConnectionGeneration(),
                      bleIsConnected() && blePeerIsBonded() && blePeerIsEncrypted());
    static bool wasSettingUp = false;
    if (wasSettingUp && !wifiSetupBusy() && WiFi.status() == WL_CONNECTED) {
        wifiSchemesCredentials(radio::state.wifiSsid, radio::state.wifiPass);
        if (radio::state.configMode && wifiSetupActivated()) {
            radio::state.configMode = false;
            transport::state.server.stop(); radio::state.httpServicesStarted = false;
            WiFi.mode(WIFI_STA);
            radio::startNormalMode();
        }
        radio::updateInfoExtra();
    }
    wasSettingUp = wifiSetupBusy();
    if (radio::state.configMode) {
        if (!power::state.plugged && millis() - radio::state.configStartedAt > CONFIG_IDLE_MS &&
            !power::v2ManualBleHoldActive()) {
            DevLog.println("[config] AP idle timeout (battery): deep sleep");
            power::deepSleepFor(0);
        }
        delay(5);
        return;
    }
    if (radio::state.v2InRendezvous) {
        radio::v2RendezvousTick();
        power::serviceLed();
        delay(5);
        return;
    }
    ArduinoOTA.handle();

    if (platform::state.pendingTplChanged) {
        platform::state.pendingTplChanged = false;
        platform::state.activeTplId = "";
        radio::updateInfoExtra();
        display::renderCurrent();
    }
    radio::pollWifi();
    if (!radio::state.wifiUp && !wifiSetupBusy() && !power::v2ManualBleHoldActive() &&
        radio::state.nextBleContactMs && clock::v2NowMs() >= radio::state.nextBleContactMs) {
        radio::v2RendezvousBegin();
        return;
    }
    power::pollPlug();
    radio::serviceBleSession();
    power::checkBattery();
    if (sync::state.syncOpenThisWake) {
        if (sync::state.syncExitAtMs && (int32_t)(millis() - sync::state.syncExitAtMs) >= 0) {
            sync::state.syncOpenThisWake = false;
            sync::state.syncExitAtMs = 0;
            if (!power::state.v2Plan.lightActive(clock::v2NowMs())) power::enterDeep("sync complete");
        } else if (clock::v2NowMs() >= sync::state.syncOpenAnsweredAt + 90000 ||
                   (sync::state.syncLastProgressMs && (uint32_t)(millis() - sync::state.syncLastProgressMs) >= 90000)) {
            sync::syncRecordOutcome(1, 3);
            syncFail(sync::retained.syncRtc);
            sync::state.syncOpenThisWake = false;
            DevLog.println("[sync] 90s window exhausted");
            if (!power::state.v2Plan.lightActive(clock::v2NowMs())) power::enterDeep("sync incomplete");
        }
    }
    radio::serviceAnnounce();
    power::serviceLed();

    // v2: only a new formal PowerPlan moves the light deadline; reads, data,
    // claims and status polls never do. The BOOT provisional 300 s closes the
    // radio when the Bridge stays unreachable (v2 §7/§12).
    if (power::retained.rtcMode == MODE_LIGHT && !sync::state.syncOpenThisWake && (!power::state.plugged || power::retained.rtcDeepOnUsb)) {
        switch (v2PowerSleepDecision(true, true, power::state.plugged, power::retained.rtcDeepOnUsb,
                                     power::v2ManualBleHoldActive(), power::state.v2Plan, power::state.v2Provisional,
                                     power::state.v2BootMs, power::state.v2SafetyDeadlineMs, clock::v2NowMs())) {
        case V2_POWER_PLAN_ENDED:
            DevLog.printf("[platform] formal light window ended (%s)\n", power::state.v2PlanReason.c_str());
            if (sync::state.syncEnabled && syncStoreAvailable()) {
                syncAddReason(sync::retained.syncRtc, 1u << 2);
                sync::state.syncOpenThisWake = true;
                sync::state.syncLastProgressMs = millis();
                sync::state.syncOpenAnsweredAt = clock::v2NowMs();
                radio::sendAnnounce(radio::state.bleOn);
            } else power::enterDeep("plan");
            break;
        case V2_POWER_BOOT_ENDED:
            DevLog.println("[platform] boot provisional 300s expired without a formal plan");
            if (sync::state.syncEnabled && syncStoreAvailable()) {
                syncAddReason(sync::retained.syncRtc, 1u << 2);
                sync::state.syncOpenThisWake = true;
                sync::state.syncLastProgressMs = millis();
                sync::state.syncOpenAnsweredAt = clock::v2NowMs();
                radio::sendAnnounce(radio::state.bleOn);
            } else power::enterDeep("provisional");
            break;
        case V2_POWER_SAFETY_ENDED:
            DevLog.println("[platform] no formal plan within the max light lease; sleeping");
            power::enterDeep("safety");
            break;
        default: break;
        }
    }

    // 1 Hz housekeeping tick: read the clock once per second and share it
    // between the offline-minutes row and the `device.now` clock bind. Both
    // used to call timeKnown()/time() every loop; 0.13.7 only gated the clock
    // block, and the remaining per-loop clock read still cut into light sleep.
    //
    // Offline-minutes row: once contact is lost (>= BRIDGE_LOST_MIN minutes
    // since the last sync) the template shows `OFF <n>M`, so redraw when the
    // integer minute changes. No periodic refresh while the bridge is
    // heartbeating.
    static int lastOfflineMinute = -1;
    static long lastClockMinute = -1;
    static uint32_t lastClockCheckMs = 0;
    uint32_t clockMs = millis();
    if ((platform::state.activeTplHasNow || radio::retained.rtcLastSyncEpoch > 1600000000) &&
        (lastClockCheckMs == 0 || (uint32_t)(clockMs - lastClockCheckMs) >= 1000)) {
        lastClockCheckMs = clockMs;
        if (clock::timeKnown()) {
            time_t nowSec = time(nullptr);
            if (radio::retained.rtcLastSyncEpoch > 1600000000) {
                long mins = ((long)nowSec - (long)radio::retained.rtcLastSyncEpoch) / 60;
                int offlineMinute = (mins > BRIDGE_LOST_MIN) ? (int)mins : -1;
                if (offlineMinute != lastOfflineMinute) {
                    bool wasShown = lastOfflineMinute > 0;
                    lastOfflineMinute = offlineMinute;
                    if (offlineMinute > 0 || wasShown) display::renderCurrent();
                }
            }
            // Clock bind: while the active template shows `device.now`, redraw
            // when the local minute changes (partial refresh, ~1/min; the
            // bridge only pushes every 5 min, so server_time looks frozen).
            if (platform::state.activeTplHasNow) {
                long minute = (long)nowSec / 60;
                if (lastClockMinute < 0) {
                    lastClockMinute = minute;
                    if (display::retained.clkR.valid) display::clkCaptureFromFramebuffer();
                } else if (minute != lastClockMinute) {
                    lastClockMinute = minute;
#ifdef CODEX_CLK_WINDOW_TEST
                    display::clkTestTick();
#else
                    // Direct window write (v0.14): ~796 ms vs ~864 ms for a full
                    // re-render; a full re-render every 30 partials clears
                    // ghosting, and the clean request bypasses the identical-
                    // frame early return (design §8.4).
                    if (display::state.epdPartialCount >= 30) {
                        display::state.forceCleanRefresh = true;
                        display::renderCurrent();
                    } else if (!display::clockTickWake()) {
                        display::renderCurrent();
                    }
#endif
                }
            }
        }
    }

    delay(power::loopDelayForNow());
}

} // namespace firmware::application
