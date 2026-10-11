// Arduino glue: build identity and the cooperative firmware entry points.
#include <Arduino.h>
#include "platform_target.h"
#include "firmware/application.h"

#if defined(CODEX_CLK_WINDOW_TEST)
#define FW_VERSION    "0.13.9-clkwin"
#elif defined(CODEX_TARGET_NOTE4)
#define FW_VERSION    "0.18.78-note4"
#else
#define FW_VERSION    "0.18.35-bw-rtc1"
#endif

// Keep version and OTA marker together: Bridge validates this exact identity.
static const char OTA_IMAGE_IDENTITY[] __attribute__((used)) =
    "codex-status-ota-v1|" FW_TARGET_ID "|" FW_VERSION;

size_t getArduinoLoopTaskStackSize() { return 12288; }

void setup() { firmware::application::initialize(FW_VERSION, OTA_IMAGE_IDENTITY); }
void loop() { firmware::application::tick(); }
