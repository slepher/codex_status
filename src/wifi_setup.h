#pragma once
#include <Arduino.h>
#include <ArduinoJson.h>
#include "wifi_scheme.h"

bool wifiSchemesBegin();
bool wifiSchemesCredentials(String &ssid, String &password);
bool wifiSetupBusy();
// Authenticated diagnostic projection; no credentials or transaction hashes.
void wifiSchemesStorageStatus(JsonObject out);
void wifiSetupConnectionStarted(const String &ssid = String());
// Handles only wifi_*; callers still enforce encrypted bond and full MAC.
String wifiSetupCommand(JsonDocument &doc, const String &peer, uint32_t generation,
                        uint64_t now, uint64_t physicalUntil, uint64_t lightUntil,
                        bool selectAllowed, bool unified=false);
void wifiSetupPoll(uint64_t now, const String &peer, uint32_t generation, bool encrypted, bool startAllowed = true);
void wifiSetupAbort(const char *reason);
void wifiSetupRadioClosed(const char *reason);
String wifiSetupSelectingBridge();
String wifiSetupReadPage(JsonDocument &request, const String &peer, uint32_t generation,
                         const String &payload);

// Authenticated HTTP management. Permit is issued only by the encrypted BLE path.
String wifiManagementAuthorize(JsonDocument &request, const String &peer, uint64_t now,
                               uint64_t physicalUntil, uint64_t readUntil);
String wifiManagementCommand(JsonDocument &request, uint64_t now, const String &legacy);
void wifiManagementResetSession();

bool wifiSetupActivated();

// Explicit migration; input credentials stay inside the device. Caller retires
// old keys only after persisted=true (verified atomic blob write/readback).
String wifiSchemesMigrate(JsonDocument &request, const String &peer,
                          const WifiNetwork *slots, size_t count, const String &preferred,
                          uint64_t now, uint64_t physicalUntil, bool allowed);

// Trusted by adapters only; client-supplied peer/namespace/permit is ignored.
struct WifiControlContext {
    String principal, nameSpace;
    bool claimed=false;
    bool occupied=false;
    bool sessionValid=true;
    uint32_t generation=0;
    uint64_t until=0;
    uint64_t physicalUntil=0;
};
String wifiControlCommand(JsonDocument &request, const WifiControlContext &context,
                          uint64_t now, const String &legacy=String("[]"));

void wifiSetupConstrain(uint64_t now,uint64_t until,bool ownerAllowed);

uint64_t wifiManagementHighWater(const String &principal,const String &nameSpace);
