#pragma once
#include <Arduino.h>

// Explicit device occupancy (device-discovery task-4): the owner is only ever
// written by explicit authenticated HTTP/BLE claim; expiry clears it. Writes may refresh
// `lastSeen` for a matching id but never create or transfer ownership.
struct OwnerRec {
    String   id;
    String   name;
    String   host;
    uint16_t port = 0;
    uint32_t since = 0;      // device uptime seconds when ownership started
    uint32_t lastSeen = 0;   // device uptime seconds of the last renewal/touch
    uint32_t lease = 300;    // seconds
};

void ownerBegin();                          // restore only verifiable deep-sleep lease
bool ownerGet(OwnerRec &out);               // valid (non-expired) owner only
bool ownerAllows(const String &bridgeId);   // free or matching id (touches lease)
uint32_t ownerRemainingS();                 // RTC deadline, sleep included
void ownerTouch();                          // refresh lastSeen in RAM
bool ownerClaim(const OwnerRec &in, bool keepSince);
struct V2ClaimArgs;
struct OwnerClaimResult { int status = 400; String body; bool newClaim = false; };
OwnerClaimResult ownerApplyClaim(const V2ClaimArgs &args, bool authorized);
void ownerClear(bool persistNow);
String ownerJson();                         // `null` or the owner object
