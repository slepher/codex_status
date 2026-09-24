#pragma once

#include <ArduinoJson.h>
#include "bundle_store.h"

enum V2ActivateAction : uint8_t {
    V2_ACTIVATE_REJECT,
    V2_ACTIVATE_REPLAY,
    V2_ACTIVATE_SWITCH,
};

struct V2ActivateDecision {
    V2ActivateAction action = V2_ACTIVATE_REJECT;
    const char *result = "rejected";
    const char *display = "unchanged";
    const char *error = nullptr;
    const char *context = nullptr;
    String request;
    String owner;
    String templateId;
    String expected;
    int index = -1;
};

V2ActivateDecision v2DecideActivate(
    JsonDocument &doc, bool bundleReady, const BsProfile &profile,
    const char *previousRequest, const char *previousOwner,
    const char *previousTemplate, const char *previousExpected,
    const char *previousContext);
