#pragma once
#include <Arduino.h>

// Explicit device occupancy (device-discovery task-4): the owner is only ever
// written/cleared by `POST /claim`; usage/template writes may refresh
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

void ownerBegin();                          // load persisted owner (NVS "owner")
bool ownerGet(OwnerRec &out);               // valid (non-expired) owner only
bool ownerAllows(const String &bridgeId);   // free or matching id (touches lease)
void ownerTouch();                          // refresh lastSeen in RAM
void ownerClaim(const OwnerRec &in, bool keepSince);
void ownerClear(bool persistNow);
String ownerJson();                         // `null` or the owner object
