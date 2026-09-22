// v2 platform state machines shared by firmware and host tests.
//
// Pure C++ (no Arduino/JSON dependencies): protocol idempotency for PowerPlan,
// the BOOT provisional 300 s window, data_seq rules and the light-deadline
// bookkeeping. The device is the final authority for its own safety limits.
#pragma once

#include <stdint.h>
#include <stddef.h>

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
            if (plan.mode != acceptedMode_ || plan.lightDurationS != acceptedDurationS_)
                return V2_PLAN_CONFLICT;
            // Returning the original result must not move the deadline.
            return V2_PLAN_ACCEPTED;
        }
        uint32_t duration = plan.lightDurationS;
        if (plan.mode == V2_PLAN_LIGHT) {
            uint32_t limit = maxLightS ? maxLightS : V2_MAX_LIGHT_S;
            if (duration > limit) duration = limit;   // device may shorten
            if (duration < V2_MIN_LIGHT_S) duration = V2_MIN_LIGHT_S;
        } else {
            duration = 0;
        }
        highId_ = plan.planId;
        haveAccepted_ = true;
        acceptedId_ = plan.planId;
        acceptedMode_ = plan.mode;
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
inline uint32_t v2Crc32(const uint8_t *data, size_t len) {
    uint32_t crc = 0xFFFFFFFFu;
    for (size_t i = 0; i < len; i++) {
        crc ^= data[i];
        for (int b = 0; b < 8; b++) {
            crc = (crc >> 1) ^ (0xEDB88320u & (uint32_t)(-(int32_t)(crc & 1)));
        }
    }
    return ~crc;
}

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
