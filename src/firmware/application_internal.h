#pragma once

// Firmware integration only: state is owned here; never copy it as a snapshot.
#include "config.h"
#include "dependencies.h"

namespace firmware::application {

struct State {
    esp_sleep_wakeup_cause_t bootWakeCause = ESP_SLEEP_WAKEUP_UNDEFINED;
    const char *firmwareVersion = "unknown";
    const char *imageIdentity = "unknown";
};
extern State state;
bool captureStartupBoot();


} // namespace firmware::application
