#pragma once

#include "owner_store.h"

struct V2ClaimArgs {
    OwnerRec request;
    bool validId = false;
    bool force = false;
    bool release = false;
};

enum V2ClaimAction : uint8_t {
    V2_CLAIM_RELEASE_EMPTY,
    V2_CLAIM_OCCUPIED,
    V2_CLAIM_RELEASE,
    V2_CLAIM_CLAIM,
};

struct V2ClaimDecision {
    V2ClaimAction action = V2_CLAIM_OCCUPIED;
    OwnerRec request;
    bool keepSince = false;
    bool newClaim = false;
};

String v2ClaimText(const String &input, size_t maxChars);
V2ClaimArgs v2PrepareClaim(const String &id, const String &name,
                           const String &host, const String &port,
                           const String &lease, bool hasLease,
                           bool force, bool release);
V2ClaimDecision v2DecideClaim(const V2ClaimArgs &args, bool haveOwner,
                              const OwnerRec &current);
