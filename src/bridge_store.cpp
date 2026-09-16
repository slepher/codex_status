#include "bridge_store.h"
#include "dev_log.h"
#include <Preferences.h>

#define STORE_MAX 8

static Preferences prefs;

static String k(const char *prefix, int i) {
    return String(prefix) + String(i);
}

int storeCount() {
    prefs.begin("brg", true);
    int n = prefs.getUChar("n", 0);
    prefs.end();
    return n > STORE_MAX ? STORE_MAX : n;
}

bool storeGet(int i, EndpointRec &rec) {
    if (i < 0 || i >= STORE_MAX) return false;
    prefs.begin("brg", true);
    int n = prefs.getUChar("n", 0);
    if (i >= n) { prefs.end(); return false; }
    rec.mac   = prefs.getString(k("m", i).c_str(), "");
    rec.host  = prefs.getString(k("h", i).c_str(), "");
    rec.port  = prefs.getUShort(k("p", i).c_str(), 0);
    rec.token = prefs.getString(k("t", i).c_str(), "");
    rec.mru   = prefs.getULong(k("r", i).c_str(), 0);
    prefs.end();
    return rec.host.length() > 0;
}

void storeUpsert(const String &mac, const String &host, uint16_t port, const String &token) {
    prefs.begin("brg", false);
    int n = prefs.getUChar("n", 0);
    uint32_t ctr = prefs.getULong("ctr", 0) + 1;
    int slot = -1;
    for (int i = 0; i < n && i < STORE_MAX; i++) {
        if (prefs.getString(k("m", i).c_str(), "") == mac) { slot = i; break; }
    }
    if (slot < 0) {
        if (n < STORE_MAX) {
            slot = n;
            n++;
            prefs.putUChar("n", n);
        } else {
            int minI = 0;
            uint32_t minMru = 0xFFFFFFFF;
            for (int i = 0; i < STORE_MAX; i++) {
                uint32_t r = prefs.getULong(k("r", i).c_str(), 0);
                if (r < minMru) { minMru = r; minI = i; }
            }
            slot = minI;
        }
    }
    prefs.putString(k("m", slot).c_str(), mac);
    prefs.putString(k("h", slot).c_str(), host);
    prefs.putUShort(k("p", slot).c_str(), port);
    prefs.putString(k("t", slot).c_str(), token);
    prefs.putULong(k("r", slot).c_str(), ctr);
    prefs.putULong("ctr", ctr);
    prefs.end();
    DevLog.printf("[store] upsert slot=%d mac=%s %s:%u\n", slot, mac.c_str(), host.c_str(), port);
}

void storeTouch(const String &mac) {
    prefs.begin("brg", false);
    int n = prefs.getUChar("n", 0);
    uint32_t ctr = prefs.getULong("ctr", 0) + 1;
    for (int i = 0; i < n && i < STORE_MAX; i++) {
        if (prefs.getString(k("m", i).c_str(), "") == mac) {
            prefs.putULong(k("r", i).c_str(), ctr);
            break;
        }
    }
    prefs.putULong("ctr", ctr);
    prefs.end();
}

void storeClear() {
    prefs.begin("brg", false);
    prefs.clear();
    prefs.end();
}
