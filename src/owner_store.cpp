#include "owner_store.h"
#include "dev_log.h"
#include <Preferences.h>

static OwnerRec owner;
static bool haveOwner = false;

static String jsonEscape(const String &in) {
    String out;
    out.reserve(in.length());
    for (size_t i = 0; i < in.length(); i++) {
        char c = in[i];
        if (c == '"' || c == '\\') { out += '\\'; out += c; }
        else if ((unsigned char)c < 0x20) out += '?';
        else out += c;
    }
    return out;
}

static void persist(const OwnerRec &rec) {
    Preferences p;
    p.begin("owner", false);
    p.putString("id", rec.id);
    p.putString("name", rec.name);
    p.putString("host", rec.host);
    p.putUShort("port", rec.port);
    p.putUInt("since", rec.since);
    p.putUInt("seen", rec.lastSeen);
    p.putUInt("lease", rec.lease);
    p.end();
}

void ownerBegin() {
    Preferences p;
    p.begin("owner", true);
    owner.id = p.getString("id", "");
    owner.name = p.getString("name", "");
    owner.host = p.getString("host", "");
    owner.port = p.getUShort("port", 0);
    owner.since = p.getUInt("since", 0);
    owner.lastSeen = p.getUInt("seen", 0);
    owner.lease = p.getUInt("lease", 300);
    p.end();
    haveOwner = owner.id.length() > 0;
    if (!haveOwner) return;
    // Uptime restarts at 0 on boot: clamp stale values so the owner is not
    // immediately expired, and let the bridge renew within one lease window.
    uint32_t now = millis() / 1000;
    if (owner.since > now) owner.since = now;
    if (owner.lastSeen > now) owner.lastSeen = now;
    DevLog.printf("[owner] restored id=%s lease=%us seen=%us\n",
                  owner.id.c_str(), (unsigned)owner.lease, (unsigned)owner.lastSeen);
}

void ownerClaim(const OwnerRec &in, bool keepSince) {
    uint32_t now = millis() / 1000;
    owner.id = in.id;
    owner.name = in.name;
    owner.host = in.host;
    owner.port = in.port;
    owner.lease = in.lease ? in.lease : 300;
    owner.lastSeen = now;
    owner.since = (keepSince && haveOwner) ? owner.since : now;
    haveOwner = true;
    persist(owner);
}

void ownerClear(bool persistNow) {
    haveOwner = false;
    owner = OwnerRec();
    if (persistNow) {
        Preferences p;
        p.begin("owner", false);
        p.clear();
        p.end();
    }
}

void ownerTouch() {
    if (!haveOwner) return;
    owner.lastSeen = millis() / 1000;
}

bool ownerGet(OwnerRec &out) {
    if (!haveOwner) return false;
    uint32_t now = millis() / 1000;
    if (owner.lease == 0 || (now - owner.lastSeen) > owner.lease) {
        DevLog.printf("[owner] lease expired (seen %us ago, lease %us); clearing\n",
                      (unsigned)(now - owner.lastSeen), (unsigned)owner.lease);
        ownerClear(true);
        return false;
    }
    out = owner;
    return true;
}

bool ownerAllows(const String &bridgeId) {
    OwnerRec cur;
    if (!ownerGet(cur)) return true;   // free/expired: legacy behavior, no owner
    if (bridgeId.length() && owner.id == bridgeId) {
        ownerTouch();
        return true;
    }
    return false;
}

String ownerJson() {
    OwnerRec cur;
    if (!ownerGet(cur)) return "null";
    uint32_t now = millis() / 1000;
    uint32_t elapsed = now - cur.lastSeen;
    long remaining = (elapsed >= cur.lease) ? 0 : (long)(cur.lease - elapsed);
    String out = "{\"id\":\"" + jsonEscape(cur.id) + "\",\"name\":\"" + jsonEscape(cur.name) +
                 "\",\"host\":\"" + jsonEscape(cur.host) + "\",\"port\":" + String(cur.port) +
                 ",\"since_s\":" + String(cur.since) +
                 ",\"last_seen_s\":" + String(cur.lastSeen) +
                 ",\"lease_s\":" + String(cur.lease) +
                 ",\"expires_in_s\":" + String(remaining) + "}";
    return out;
}
