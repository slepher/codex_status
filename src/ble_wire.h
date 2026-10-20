#pragma once
#include <ArduinoJson.h>

constexpr size_t BLE_WIRE_MAX = 512;
// v1 only changes the BLE projection, not the HTTP/device business protocol.
struct BleInfoSnapshot {
    String mac, ip, firmware;
    uint32_t generation = 0, wakeSeq = 0;
    uint32_t rfRemainingMs = UINT32_MAX; // absent means the ROM reports no bound
    bool pairing = false, bonded = false, encrypted = false, configured = false;
    uint8_t wifiProtocol = 0;
    uint8_t wifiManagement = 0;
    uint8_t wifiConnection = 0;
    uint8_t bleClaim = 0;
    uint8_t planActivation = 0;
};
struct BleStatusSnapshot {
    String mac, nonce, context, activeTemplate, job;
    uint64_t appliedSeq = 0, planId = 0;
    uint32_t remainingS = 0, provisionalS = 0, commitSeq = 0;
    bool light = false, syncAvailable = false, syncEnabled = false;
    uint8_t rounds = 0, retrySkip = 0;
    bool due = false, pending = false;
    uint8_t rendezvousVersion = 0;
    bool automaticContact = true;
    const char *networkError = nullptr;
};
bool bleWireText(const String &value, size_t limit, bool allowEmpty = true);
bool bleWireFits(const String &bytes);
String bleBuildAuth(bool okay, const String &token = String());
String bleBuildEndpoint(const String &request, const String &mac, const String &principal,
                        const char *error = "", const char *stage = "persisted");
String bleBuildInfo(const BleInfoSnapshot &snapshot);
String bleBuildStatus(const BleStatusSnapshot &snapshot);
// Adds the final correlation envelope, encodes byte-bounded identifiers and
// trims history against the FINAL frame. Oversize after possible execution is
// unknown, never rejected. Invalid correlation produces no new ACK.
String bleBuildReply(JsonDocument &body, const String &request,
                     uint32_t generation, uint32_t wakeSeq);
