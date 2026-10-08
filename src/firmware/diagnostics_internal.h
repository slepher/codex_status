#pragma once

// Firmware integration only: state is owned here; never copy it as a snapshot.
#include "config.h"
#include "dependencies.h"

namespace firmware::diagnostics {

struct State {
    uint8_t nvsStageLast = 0xFF;
    uint8_t nvsStageAtBoot = 0xFF;

    // ---- Plan C wake telemetry (task-3) ----
    // millis() is boot-relative on every deep wake, so `awake_ms` is live until the
    // next deep sleep. deepSleepRaw() snapshots the final values into RTC for
    // /status.json (`deep.last_*`) and /history (`HIST_WAKE`).
    uint32_t bootRenderCount = 0;      // waveforms written on this wake
    uint32_t wakeRenderMs = 0;         // waveform ms accumulated this wake
    uint8_t  wakeResult = WAKE_LIGHT;  // what this wake turned out to be
};
extern State state;

struct Retained {
    uint32_t rtcDeepCycles = 0;    // thin clock wakes
    uint32_t rtcNetCycles = 0;     // deep network windows

    // Deep-cycle trace: last stage code reached (survives deep sleep; read via
    // /status.json after a manual wake). 1 setup, 2 thin, 3 thin-done, 10 net,
    // 11 wifi, 12 pull-ok, 13 pull-fail, 14 light, 15 pending, 20 enter-deep,
    // 90 raw-sleep, 99 normal boot.
    uint8_t  rtcStage = 0;
    uint8_t  rtcLastWake = 0xFF;

    // P1 diagnosis: did the light -> deep glyph render run? bit0 set = the
    // `device.mode` template flag was set, bit1 = it was clear, bit2 = render
    // returned. Read via /status.json `deep.glyph`.
    uint8_t  rtcDeepGlyph = 0; // bit 3: last Bridge OFF glyph

    // Debug capture gate: when enabled (see captureFrameToFs), the framebuffer is
    // saved after a pre-sleep refresh; fixed files, overwritten, off by default.
    uint8_t  rtcFrameCapture = 0;
    uint32_t rtcWakeGeneration = 0;
    uint32_t rtcTraceCurrentSeq = 0;
    uint32_t rtcLastAwakeMs = 0;
    uint32_t rtcLastBleMs = 0;
    uint32_t rtcLastRenders = 0;
    uint8_t  rtcLastTimeSource = TIME_NONE;
    uint8_t  rtcLastWakeResult = WAKE_LIGHT;

    // Plan C power estimate: cumulative totals over completed deep cycles only
    // (light sessions are excluded), so /status.json can report per-cycle averages
    // without a current meter. See task-3 "功耗预估" and tools/estimate-power.mjs.
    uint32_t rtcAccCycles = 0;
    uint32_t rtcAccAwakeMs = 0;
    uint32_t rtcAccBleMs = 0;
    uint32_t rtcAccRenderMs = 0;
};
extern Retained retained;

void nvsStageMark(uint8_t code);
void setStage(uint8_t code);
uint32_t fnv1a(const String &s);
WakeTraceRec *traceCurrent();
const char *traceStageName(uint8_t stage);
void traceSetStage(uint8_t stage);
void traceSetTransport(uint8_t transport);
void traceSetRequest(const char *request);
void traceMarkHttp(bool ok, int code);
void traceMarkThin();
void traceBegin(esp_sleep_wakeup_cause_t cause);
void traceFinishSleep();
void tracePollConnection();
void expireStatusRead();
void histAdd(uint8_t ev, uint16_t aux);
void bleRadioMark(bool on);
uint32_t bleRadioMs();
const char *resetReasonName();
const char *wakeCauseName(esp_sleep_wakeup_cause_t cause);
void handleStatus();
void handleStatusJson();
void handleLog();
String traceHistoryJson(uint32_t since, uint8_t limit, size_t maxBytes,
                               bool &more);
void handleHistory();
void handlePmStats();
void captureFrameToFs(const char *tag);
void handleFrame();
void handleDiag();
void handleV2Status();
void handleSerialCli();

} // namespace firmware::diagnostics
