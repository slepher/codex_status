// v2 platform state machines shared by firmware and host tests.
//
// Pure C++ (no Arduino/JSON dependencies): protocol idempotency for PowerPlan,
// the BOOT provisional 300 s window, data_seq rules and the light-deadline
// bookkeeping. The device is the final authority for its own safety limits.
#pragma once

#include <stdint.h>
#include <stddef.h>
#include <string.h>

inline bool v2ParseCrc(const char *text, uint32_t &out) {
    if (!text || strlen(text) != 8) return false;
    uint32_t value = 0;
    for (unsigned i = 0; i < 8; ++i) {
        char c = text[i];
        unsigned digit = c >= '0' && c <= '9' ? c - '0'
                       : c >= 'a' && c <= 'f' ? c - 'a' + 10
                       : c >= 'A' && c <= 'F' ? c - 'A' + 10 : 16;
        if (digit > 15) return false;
        value = (value << 4) | digit;
    }
    out = value;
    return true;
}

// v2 design defaults (`docs/generic-display-platform-design-v2.md` §7).
static const uint32_t V2_BOOT_PROVISIONAL_S = 300;
static const uint32_t V2_MAX_LIGHT_S = 600;
static const uint32_t V2_MIN_LIGHT_S = 30;
static const uint32_t V2_RENDEZVOUS_S = 60;

enum V2PlanMode : uint8_t { V2_PLAN_SLEEP = 0, V2_PLAN_LIGHT = 1 };

enum V2PlanAck : uint8_t {
    V2_PLAN_ACCEPTED = 0,
    V2_PLAN_STALE_ID = 1,
    V2_PLAN_CONFLICT = 2,
    V2_PLAN_REJECTED_LIMIT = 3,
};

struct V2PowerPlan {
    uint64_t planId;
    uint8_t mode;
    uint32_t lightDurationS;
    uint32_t rendezvousPeriodS;
};

// Formal plan state: Bridge-generated ids, monotonic within the session.
// Duplicate id+content is idempotent; duplicate id with different content is a
// conflict; an older id is stale. A repeat never restarts a deadline.
class V2PlanState {
  public:
    void reset() {
        highId_ = 0;
        haveAccepted_ = false;
        acceptedId_ = 0;
        acceptedMode_ = V2_PLAN_SLEEP;
        acceptedDurationS_ = 0;
        acceptedAtMs_ = 0;
        sourceBoot_ = false;
    }

    // Validate an incoming plan against the accepted high water mark.
    V2PlanAck accept(const V2PowerPlan &plan, uint64_t nowMs, bool provisional,
                     uint32_t maxLightS) {
        if (plan.planId < highId_) return V2_PLAN_STALE_ID;
        if (plan.planId == highId_ && haveAccepted_) {
            // Same id: identical content is idempotent, different content conflicts.
            if (plan.mode != acceptedMode_ || plan.lightDurationS != requestedDurationS_ ||
                plan.rendezvousPeriodS != requestedPeriodS_)
                return V2_PLAN_CONFLICT;
            // Returning the original result must not move the deadline.
            return V2_PLAN_ACCEPTED;
        }
        uint32_t duration = plan.lightDurationS;
        if (plan.mode == V2_PLAN_LIGHT) {
            uint32_t limit = maxLightS ? maxLightS : V2_MAX_LIGHT_S;
            if (duration > limit) duration = limit;   // device may shorten
            const uint32_t minimum = limit < V2_MIN_LIGHT_S ? limit : V2_MIN_LIGHT_S;
            if (duration < minimum) duration = minimum;
        } else {
            duration = 0;
        }
        highId_ = plan.planId;
        haveAccepted_ = true;
        acceptedId_ = plan.planId;
        acceptedMode_ = plan.mode;
        requestedDurationS_ = plan.lightDurationS;
        requestedPeriodS_ = plan.rendezvousPeriodS;
        acceptedDurationS_ = duration;
        acceptedAtMs_ = nowMs;
        sourceBoot_ = provisional;
        return V2_PLAN_ACCEPTED;
    }

    bool lightActive(uint64_t nowMs) const {
        return haveAccepted_ && acceptedMode_ == V2_PLAN_LIGHT &&
               remainingS(nowMs) > 0;
    }

    uint32_t remainingS(uint64_t nowMs) const {
        if (!haveAccepted_ || acceptedMode_ != V2_PLAN_LIGHT) return 0;
        uint64_t elapsedS = (nowMs - acceptedAtMs_) / 1000ULL;
        if (elapsedS >= acceptedDurationS_) return 0;
        return (uint32_t)(acceptedDurationS_ - elapsedS);
    }

    uint32_t grantedS() const { return acceptedDurationS_; }
    uint64_t deadlineMs() const { return acceptedAtMs_ + (uint64_t)acceptedDurationS_ * 1000ULL; }
    uint64_t acceptedId() const { return acceptedId_; }
    uint64_t highId() const { return highId_; }
    bool accepted() const { return haveAccepted_; }
    bool fromBoot() const { return sourceBoot_; }

    // Timer wake never gets the BOOT fallback; only a real physical wake does.
    static uint32_t bootProvisionalRemaining(uint64_t tBootMs, uint64_t nowMs) {
        uint64_t elapsedS = (nowMs - tBootMs) / 1000ULL;
        if (elapsedS >= V2_BOOT_PROVISIONAL_S) return 0;
        return (uint32_t)(V2_BOOT_PROVISIONAL_S - elapsedS);
    }

  private:
    uint64_t highId_ = 0;
    bool haveAccepted_ = false;
    uint64_t acceptedId_ = 0;
    uint8_t acceptedMode_ = V2_PLAN_SLEEP;
    uint32_t acceptedDurationS_ = 0;
    uint32_t requestedDurationS_ = 0;
    uint32_t requestedPeriodS_ = 0;
    uint64_t acceptedAtMs_ = 0;
    bool sourceBoot_ = false;
};

enum V2DataAck : uint8_t {
    V2_DATA_APPLIED = 0,
    V2_DATA_UNCHANGED = 1,   // same seq + same content: idempotent replay
    V2_DATA_CONFLICT = 2,    // same seq, different content
    V2_DATA_STALE = 3,       // older seq
    V2_DATA_CONTEXT_MISMATCH = 4,
    V2_DATA_REJECTED = 5,    // validation failed
};

// One active context per device. `data_seq` is monotonic inside the context;
// gaps are allowed, replays are idempotent, older values are rejected.
class V2DataSeq {
  public:
    void beginContext(uint64_t nowMs, uint32_t keepNextSeq = 1) {
        seq_ = keepNextSeq ? keepNextSeq : 1;
        haveApplied_ = false;
        appliedSeq_ = 0;
        appliedCrc_ = 0;
        contextStartMs_ = nowMs;
    }

    V2DataAck observe(uint64_t seq, uint32_t contentCrc) {
        if (haveApplied_ && seq < appliedSeq_) return V2_DATA_STALE;
        if (haveApplied_ && seq == appliedSeq_) {
            if (contentCrc == appliedCrc_) return V2_DATA_UNCHANGED;
            return V2_DATA_CONFLICT;
        }
        return V2_DATA_APPLIED;
    }

    void noteApplied(uint64_t seq, uint32_t contentCrc) {
        haveApplied_ = true;
        appliedSeq_ = seq;
        appliedCrc_ = contentCrc;
        if (seq >= seq_) seq_ = seq + 1;
    }

    uint64_t nextSeq() const { return seq_; }
    bool haveApplied() const { return haveApplied_; }
    uint64_t appliedSeq() const { return appliedSeq_; }
    uint32_t appliedCrc() const { return appliedCrc_; }
    uint64_t contextStartMs() const { return contextStartMs_; }

  private:
    uint64_t seq_ = 1;
    bool haveApplied_ = false;
    uint64_t appliedSeq_ = 0;
    uint32_t appliedCrc_ = 0;
    uint64_t contextStartMs_ = 0;
};

// CRC32/IEEE (poly 0xEDB88320) matching the bridge/Python/device template hash.
// Split into start/update/finish so one implementation serves both the
// one-shot callers and the streaming ones (bundle commit hashes the received
// payload in bounded chunks instead of materializing it in RAM).
inline uint32_t v2Crc32Start() { return 0xFFFFFFFFu; }

inline uint32_t v2Crc32Update(uint32_t crc, const uint8_t *data, size_t len) {
    for (size_t i = 0; i < len; i++) {
        crc ^= data[i];
        for (int b = 0; b < 8; b++) {
            crc = (crc >> 1) ^ (0xEDB88320u & (uint32_t)(-(int32_t)(crc & 1)));
        }
    }
    return crc;
}

inline uint32_t v2Crc32Finish(uint32_t crc) { return ~crc; }

inline uint32_t v2Crc32(const uint8_t *data, size_t len) {
    return v2Crc32Finish(v2Crc32Update(v2Crc32Start(), data, len));
}

// RTC checkpoint is bound to the committed activation context. It contains no
// pointers and is never trusted after a cold reset or a checksum failure.
struct V2DataCheckpoint {
    char context[33];
    uint64_t seq;
    uint32_t contentCrc;
    uint32_t checksum;

    void save(const char *id, const V2DataSeq &state) {
        memset(this, 0, sizeof(*this));
        if (!id || strlen(id) >= sizeof(context) || !state.haveApplied()) return;
        strcpy(context, id);
        seq = state.appliedSeq();
        contentCrc = state.appliedCrc();
        checksum = v2Crc32((const uint8_t *)this, offsetof(V2DataCheckpoint, checksum));
    }
    bool restore(const char *id, V2DataSeq &state) const {
        if (!id || !seq || context[sizeof(context) - 1] || strcmp(context, id) ||
            checksum != v2Crc32((const uint8_t *)this, offsetof(V2DataCheckpoint, checksum))) return false;
        state.noteApplied(seq, contentCrc);
        return true;
    }
};

// Context ids are non-reusable: a rolling counter plus an entropy word.
class V2ContextGen {
  public:
    void seed(uint32_t entropy) { counter_ = entropy | 1u; }
    uint32_t next() {
        counter_ = counter_ * 1664525u + 1013904223u;
        return counter_;
    }

  private:
    uint32_t counter_ = 1;
};

// One bounded Bundle receive transaction. Transport authentication is checked
// separately on every operation; these fields prevent mixing authenticated jobs.
struct V2BundleRx {
    char owner[65] = {};
    char request[65] = {};
    char nonce[65] = {};
    uint32_t length = 0;
    uint32_t crc = 0;
    uint32_t offset = 0;
    uint64_t deadline = 0;

    bool matches(const char *bridge, const char *id, const char *session) const {
        return !strcmp(owner, bridge) && !strcmp(request, id) && !strcmp(nonce, session);
    }
    bool live(uint64_t now) const { return deadline && now < deadline; }
    bool begin(const char *bridge, const char *id, const char *session,
               uint32_t len, uint32_t sum, uint64_t now) {
        if (!*bridge || !*id || !*session || strlen(bridge) >= sizeof(owner) ||
            strlen(id) >= sizeof(request) || strlen(session) >= sizeof(nonce) ||
            !len || len > 262144) return false;
        strcpy(owner, bridge); strcpy(request, id); strcpy(nonce, session);
        length = len; crc = sum; offset = 0; deadline = now + 120000;
        return true;
    }
    bool append(uint32_t at, uint32_t size, uint64_t now) const {
        return live(now) && at == offset && size && size <= 16384 &&
               offset <= length && size <= length - offset;
    }
    bool complete(uint32_t len, uint32_t sum, uint64_t now) const {
        return live(now) && offset == length && len == length && sum == crc;
    }
};

// Exit-path bookkeeping: every radio/PM resource must be released on all paths.
struct V2RadioLocks {
    bool ble = false;
    bool wifi = false;
    bool pm = false;
    bool ota = false;

    void acquireBle() { ble = true; }
    void acquireWifi() { wifi = true; }
    void acquirePm() { pm = true; }
    void acquireOta() { ota = true; }
    void releaseAll() { ble = wifi = pm = ota = false; }
    bool clean() const { return !ble && !wifi && !pm && !ota; }
};
