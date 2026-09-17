#pragma once
#include <Arduino.h>

struct TplEnv {
    String channel;   // "WIFI" / "BLE"
    String ip;
    String syncHHMM;  // "--:--" when unknown
    int battery = -1; // percentage, or -1 when the device value is unknown
    bool idle = false;           // render mode: IDLE screen (true) / LIVE data (false)
    int offlineMins = -1;        // minutes since the last successful sync
    String idleReason;           // boot/wifi_lost/bridge_lost/env_switch
};

// Draw a template onto the current Paint image (caller has cleared the frame).
// Returns false if the template is invalid/unsupported (nothing is drawn in
// that case: validation runs before the draw pass).
bool tplDraw(const String &tmplJson, const String &usageJson, const TplEnv &env);

// Structural validation only (no usage needed): used at BLE receive time.
bool tplValidate(const String &tmplJson, String &err);
