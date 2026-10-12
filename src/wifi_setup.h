#pragma once
#include <Arduino.h>
#include <ArduinoJson.h>
#include "wifi_scheme.h"

bool wifiSchemesBegin();
bool wifiSchemesCredentials(String &ssid, String &password);
bool wifiSetupBusy();
bool wifiSetupConnecting();
bool wifiSetupFailed();
// Authenticated diagnostic projection; no credentials or transaction hashes.
void wifiSchemesStorageStatus(JsonObject out);
void wifiSetupConnectionStarted(const String &ssid = String());
// High 16 bits: wrapping event sequence; low 16 bits: latest driver reason.
uint32_t wifiSetupDisconnectEvent();
uint32_t wifiSetupAssociatedEvent();
uint32_t wifiSetupIpEvent();
uint16_t wifiSetupConnectionReason();
const char *wifiSetupConnectionFailure();
// Handles only wifi_*; callers still enforce encrypted bond and full MAC.
String wifiSetupCommand(JsonDocument &doc, const String &peer, uint32_t generation,
                        uint64_t now, uint64_t physicalUntil, uint64_t lightUntil,
                        bool selectAllowed);
void wifiSetupPoll(uint64_t now, bool startAllowed = true);
void wifiSetupAbort(const char *reason);
String wifiSetupSelectingBridge();
String wifiSetupReadPage(JsonDocument &request, const String &peer, uint32_t generation,
                         const String &payload);

void wifiManagementResetSession();

bool wifiSetupActivated();

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
