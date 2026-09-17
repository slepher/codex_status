#pragma once
#include <Arduino.h>

struct EndpointRec {
    String   mac;
    String   host;
    uint16_t port = 0;
    String   token;
    String   bssid;   // BSSID seen when this endpoint last synced
    uint32_t mru  = 0;
};

int  storeCount();
bool storeGet(int i, EndpointRec &rec);
void storeUpsert(const String &mac, const String &host, uint16_t port, const String &token);
void storeTouch(const String &mac);
void storeSetBssid(const String &mac, const String &bssid);
void storeClear();
