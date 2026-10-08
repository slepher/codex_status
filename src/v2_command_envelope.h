#pragma once

#include <ArduinoJson.h>
#include <stdint.h>
#include <stddef.h>

// One current wire format. Explicit offsets keep this independent of C++ padding.
constexpr size_t COMPACT_FRAME_HEADER = 48;
constexpr uint8_t COMPACT_FRAME_MARKER = 0xC1;
constexpr uint16_t COMPACT_FRAME_MAX_OP = 27;
struct CompactFrameHeader {
    uint8_t kind = 0;
    uint16_t op = 0;
    uint16_t result = 0;
    uint16_t payloadLen = 0;
    uint8_t session[16] = {};
    uint64_t requestNo = 0;
    uint8_t context[16] = {};
};
bool compactFrameEncode(const CompactFrameHeader &header, uint8_t out[COMPACT_FRAME_HEADER]);
bool compactFrameDecode(const uint8_t *frame, size_t length, CompactFrameHeader &header,
                        size_t frameLimit);
uint16_t compactOperation(const char *name);

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
                  uint32_t acceptedRemainingS, const char *fwTarget, bool fullSeq = false);
