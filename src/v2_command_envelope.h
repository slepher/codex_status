#pragma once

#include <ArduinoJson.h>

const char *v2ParseCommand(const String &body, JsonDocument &doc);

struct V2CommandSessionDecision {
    bool accepted = false;
    bool needsNonce = false;
    const char *error = "session";
};

// A null nonce performs the pre-nonce checks only, so the device can keep its
// nonce generation lazy. Passing a nonce also validates session_nonce.
V2CommandSessionDecision v2CheckCommandSession(
    JsonDocument &doc, const String &currentMac, const String *sessionNonce);

String v2BuildAck(const char *op, const char *result, const char *display,
                  const char *retention, const char *error, int64_t seq,
                  uint64_t planId, const char *context,
                  uint32_t acceptedRemainingS, const char *fwTarget);
