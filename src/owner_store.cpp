#include "owner_store.h"
#include "v2_claim_command.h"
#ifdef CODEX_OWNER_TEST
#include "../tools/owner-test-hal.h"
#else
#include "dev_log.h"
#include <Preferences.h>
#include <esp_attr.h>
#include <esp_system.h>
#include <esp_rtc_time.h>
#endif

static OwnerRec owner;
static bool haveOwner = false;

// RTC is the only clock here: wall-clock synchronization cannot extend a lease.
// Retained across deep sleep, discarded on resets without verified continuity.
struct OwnerDeadline {
    uint64_t renewedUs;
    uint64_t untilUs;
    uint32_t identity;
    uint32_t checksum;
};
static_assert(sizeof(OwnerDeadline) == 24, "owner RTC budget");
RTC_DATA_ATTR static OwnerDeadline ownerDeadline = {};

static uint32_t ownerIdentity() {
    uint32_t hash = 2166136261u;
    for (size_t i = 0; i < owner.id.length(); ++i)
        hash = (hash ^ (uint8_t)owner.id[i]) * 16777619u;
    return hash;
}
static uint32_t deadlineChecksum() {
    uint32_t crc = 0x4f574c31u; // OWL1, explicit LE fields; never native padding.
    auto word = [&crc](uint64_t value, unsigned count) {
        for (unsigned i = 0; i < count; ++i) {
            crc ^= (uint8_t)(value >> (i * 8));
            for (unsigned bit = 0; bit < 8; ++bit)
                crc = (crc >> 1) ^ (0xedb88320u & (0u - (crc & 1u)));
        }
    };
    word(ownerDeadline.renewedUs, 8); word(ownerDeadline.untilUs, 8);
    word(ownerDeadline.identity, 4);
    return crc;
}
static bool deadlineValid(uint64_t now) {
    return owner.lease > 0 && owner.lease <= 3600 &&
        ownerDeadline.identity == ownerIdentity() &&
        ownerDeadline.checksum == deadlineChecksum() &&
        ownerDeadline.untilUs >= ownerDeadline.renewedUs &&
        ownerDeadline.untilUs - ownerDeadline.renewedUs == (uint64_t)owner.lease * 1000000ULL &&
        now >= ownerDeadline.renewedUs && now < ownerDeadline.untilUs;
}
static void renewDeadline() {
    const uint64_t now = esp_rtc_get_time_us();
    ownerDeadline.renewedUs = now;
    ownerDeadline.untilUs = now + (uint64_t)owner.lease * 1000000ULL;
    ownerDeadline.identity = ownerIdentity();
    ownerDeadline.checksum = deadlineChecksum();
}
uint32_t ownerRemainingS() {
    const uint64_t now = esp_rtc_get_time_us();
    if (!haveOwner || !deadlineValid(now)) return 0;
    return (uint32_t)((ownerDeadline.untilUs - now + 999999ULL) / 1000000ULL);
}

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

static bool persist(const OwnerRec &rec) {
    Preferences p;
    if (!p.begin("owner", false)) return false;
    bool saved = p.putString("id", rec.id) == rec.id.length() &&
        p.putString("name", rec.name) == rec.name.length() &&
        p.putString("host", rec.host) == rec.host.length() &&
        p.putUShort("port", rec.port) == sizeof(uint16_t) &&
        p.putUInt("since", rec.since) == sizeof(uint32_t) &&
        p.putUInt("seen", rec.lastSeen) == sizeof(uint32_t) &&
        p.putUInt("lease", rec.lease) == sizeof(uint32_t);
    p.end();
    if (!saved || !p.begin("owner", true)) return false;
    saved = p.getString("id", "") == rec.id && p.getString("name", "") == rec.name &&
        p.getString("host", "") == rec.host && p.getUShort("port", 0) == rec.port &&
        p.getUInt("since", UINT32_MAX) == rec.since &&
        p.getUInt("seen", UINT32_MAX) == rec.lastSeen &&
        p.getUInt("lease", 0) == rec.lease;
    p.end();
    return saved;
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
    if (!haveOwner) { ownerDeadline = {}; return; }
    const uint64_t nowRtc = esp_rtc_get_time_us();
    if (esp_reset_reason() != ESP_RST_DEEPSLEEP || !deadlineValid(nowRtc)) {
        DevLog.println("[owner] expired/unverifiable restored lease; clearing");
        ownerClear(true);
        return;
    }
    // Legacy uptime fields are presentation only; the retained RTC deadline
    // carries sleep elapsed time and is never rebased by startup.
    const uint32_t now = millis() / 1000;
    const uint64_t elapsed = (nowRtc - ownerDeadline.renewedUs) / 1000000ULL;
    owner.lastSeen = elapsed < now ? now - (uint32_t)elapsed : 0;
    if (owner.since > now) owner.since = now;
    DevLog.printf("[owner] restored id=%s remaining=%us\n",
                  owner.id.c_str(), (unsigned)ownerRemainingS());
}

bool ownerClaim(const OwnerRec &in, bool keepSince) {
    OwnerRec next = in;
    const uint32_t now = millis() / 1000;
    next.lease = in.lease ? in.lease : 300;
    next.lastSeen = now;
    next.since = (keepSince && haveOwner) ? owner.since : now;
    if (!persist(next)) return false;
    owner = next;
    haveOwner = true;
    renewDeadline();
    return true;
}

OwnerClaimResult ownerApplyClaim(const V2ClaimArgs &args, bool authorized) {
    OwnerClaimResult reply;
    if (!authorized) { reply.status = 401; reply.body = "{\"error\":\"unauthorized\"}"; return reply; }
    if (!args.validId) { reply.body = "{\"error\":\"args\"}"; return reply; }
    OwnerRec current;
    const bool have = ownerGet(current);
    const V2ClaimDecision decision = v2DecideClaim(args, have, current);
    if (decision.action == V2_CLAIM_OCCUPIED) {
        reply.status = 409;
        reply.body = String("{\"error\":\"occupied\",\"owner\":") + ownerJson() + "}";
        return reply;
    }
    reply.status = 200;
    if (decision.action == V2_CLAIM_RELEASE_EMPTY || decision.action == V2_CLAIM_RELEASE) {
        if (decision.action == V2_CLAIM_RELEASE) ownerClear(true);
        reply.body = decision.action == V2_CLAIM_RELEASE ?
            "{\"owner\":null,\"released\":true}" : "{\"owner\":null,\"released\":false}";
        return reply;
    }
    if (!ownerClaim(decision.request, decision.keepSince)) {
        reply.status = 500; reply.body = "{\"error\":\"storage\",\"stage\":\"claim\"}";
        return reply;
    }
    reply.newClaim = decision.newClaim;
    reply.body = String("{\"owner\":") + ownerJson() + ",\"renew\":" +
        (decision.keepSince ? "true" : "false") + "}";
    DevLog.printf("[owner] %s id=%s lease=%us\n", decision.keepSince ? "renew" : "claim",
                  decision.request.id.c_str(), (unsigned)decision.request.lease);
    return reply;
}

void ownerClear(bool persistNow) {
    haveOwner = false;
    ownerDeadline = {};
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
    if (!deadlineValid(esp_rtc_get_time_us())) { ownerClear(true); return; }
    owner.lastSeen = millis() / 1000;
    renewDeadline();
}

bool ownerGet(OwnerRec &out) {
    if (!haveOwner) return false;
    if (!deadlineValid(esp_rtc_get_time_us())) {
        DevLog.println("[owner] lease expired/clock invalid; clearing");
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
    uint32_t remaining = ownerRemainingS();
    String out = "{\"id\":\"" + jsonEscape(cur.id) + "\",\"name\":\"" + jsonEscape(cur.name) +
                 "\",\"host\":\"" + jsonEscape(cur.host) + "\",\"port\":" + String(cur.port) +
                 ",\"since_s\":" + String(cur.since) +
                 ",\"last_seen_s\":" + String(cur.lastSeen) +
                 ",\"lease_s\":" + String(cur.lease) +
                 ",\"expires_in_s\":" + String(remaining) + "}";
    return out;
}
