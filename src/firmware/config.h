#pragma once

#include <stdint.h>
#include "platform_target.h"

#if defined(CODEX_NOTE4_ROM_B) || (!defined(CODEX_TARGET_NOTE4) && !defined(CODEX_TARGET_GRAY4))
#define CODEX_BLE_B46 1
#endif
#if !TARGET_VERIFIED && !defined(CODEX_GRAY4_PANEL_CONFIRMED)
#define CODEX_TARGET_UNVERIFIED 1
#endif

#define AP_PASSWORD   "codex1234"

#define MAX_SLOTS     3

#define ACTIVE_HOLD_S    600

#define CONFIG_IDLE_MS   (5UL * 60UL * 1000UL)   // AP idle sleep (battery)

#define WIFI_BLINK_MS    1000UL                  // Wi-Fi icon phase while connecting

#define WIFI_LOST_MS     30000UL                 // link-loss declaration

#define BLE_GRACE_MS     120000UL                // BLE keep-alive after last use

#define BATT_CHECK_MS    (5UL * 60UL * 1000UL)

#define ANNOUNCE_MS      (5UL * 60UL * 1000UL)   // UDP announce heartbeat

#define BRIDGE_LOST_MIN  6                       // bridge heartbeat 5 min + margin

#define BLE_AUTO_PCT     20

#define STORE_MAX_LOCAL  8

#define MODE_DEEP              0

#define MODE_LIGHT             1

#define IDLE_DEEP_DEFAULT_S    600    // light -> deep after this much quiet

#define DEEP_CONTACT_DEFAULT_S 60     // deep network period (bridge overrides)

#define DEEP_CONTACT_MIN_S     30

#define DEEP_CONTACT_MAX_S     3600

#define DEEP_PENDING_WINDOW_MS 180000UL  // stay awake when pull reports pending

#define CLK_GHOST_LIMIT        90      // deep clock partials before a full redraw

#if TARGET_WIDTH > 200
#define CLK_MAX_BYTES          512
#else
#define CLK_MAX_BYTES          64
#endif

#ifdef CODEX_BLE_B46
#define V2_RENDEZVOUS_WINDOW_MS    4000
#else
#define V2_RENDEZVOUS_WINDOW_MS    3000
#endif

#define V2_RENDEZVOUS_CONNECTED_MS 6000

#define V2_RENDEZVOUS_ACK_GRACE_MS 200

#define NVS_STAGE_MAX_WRITES 40

#define HIST_CAP 64

#define TRACE_FORMAT 2

#define TRACE_MAGIC 0x57545232u // "WTR2"

#define TRACE_VALID_START 0xA5

#define TRACE_VALID_END 0x5A

#define TRACE_STAGE_COUNT 9

#ifdef CODEX_CLK_WINDOW_TEST
#define CLK_TEST_TICKS 10
#endif
static const int EPD_W = TARGET_WIDTH;

static const int EPD_H = TARGET_HEIGHT;

static const int EPD_FB_BYTES = (EPD_W / 8) * EPD_H;

enum : uint8_t {
    HIST_BOOT = 1,        // wake/boot classified, aux = esp_sleep_wakeup_cause_t
    HIST_ENTER_DEEP = 2,  // light -> deep transition, aux = next_contact_s
    HIST_THIN = 3,        // minute clock wake, aux = 1 when the clock was drawn
    HIST_NET_OK = 4,      // network window success, aux = HTTP code (200)
    HIST_NET_FAIL = 5,    // network window failure, aux = HTTP code (0 unknown)
    HIST_TO_LIGHT = 6,    // returned/switched to light
    HIST_WAKE = 7,        // wake summary at deep entry, aux = wake result (below)
};

enum : uint8_t {
    WAKE_LIGHT = 0,       // light session (idle/plugged/HTTP)
    WAKE_THIN = 1,        // thin deep clock wake
    WAKE_RV_SLEEP = 2,    // v2 rendezvous answered with a sleep plan
    WAKE_RV_LIGHT = 3,    // v2 rendezvous answered with a light plan
    WAKE_NET = 4,         // legacy deep network window
};

enum : uint8_t {
    TIME_NONE = 0,
    TIME_BLE = 1,
    TIME_RTC = 2,         // internal RTC fallback (legacy value preserved)
    TIME_EXTERNAL_RTC = 3,
    TIME_NTP = 4,
};

enum : uint8_t {
    TRACE_WAKE_BOOT = 0, TRACE_WAKE_TIMER = 1, TRACE_WAKE_BUTTON = 2,
    TRACE_WAKE_THIN = 3, TRACE_WAKE_OTHER = 4,
};

enum : uint8_t {
    TRACE_STAGE_WIRELESS = 1, TRACE_STAGE_BLE_ADV = 2,
    TRACE_STAGE_BLE_CONNECTED = 3, TRACE_STAGE_COMMAND = 4,
    TRACE_STAGE_REPLY = 5, TRACE_STAGE_BLE_OFF = 6,
    TRACE_STAGE_WIFI_IP = 7, TRACE_STAGE_HTTP = 8, TRACE_STAGE_SLEEP = 9,
};

enum : uint8_t {
    TRACE_RESULT_ACTIVE = 0, TRACE_RESULT_THIN = 1, TRACE_RESULT_ANSWERED = 2,
    TRACE_RESULT_NO_CONNECTION = 3, TRACE_RESULT_NO_COMMAND = 4,
    TRACE_RESULT_WIFI_NO_IP = 5, TRACE_RESULT_HTTP_FAILED = 6,
    TRACE_RESULT_INTERRUPTED = 7,
};

enum : uint8_t {
    TRACE_ERR_NONE = 0, TRACE_ERR_WIFI = 1, TRACE_ERR_ENDPOINT = 2,
    TRACE_ERR_HTTP = 3, TRACE_ERR_JSON = 4, TRACE_ERR_RESET = 5,
};
