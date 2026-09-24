#include "v2_claim_command.h"

String v2ClaimText(const String &input, size_t maxChars) {
    String output;
    size_t chars = 0;
    for (size_t i = 0; i < input.length() && chars < maxChars;) {
        unsigned char c = (unsigned char)input[i];
        size_t sequence = c >= 0xF0 ? 4 : c >= 0xE0 ? 3 : c >= 0xC0 ? 2 : 1;
        if (i + sequence > input.length()) break;
        if (c < 0x20 || c == 0x7F) {
            output += '?';
            ++i;
        } else {
            for (size_t k = 0; k < sequence; ++k) output += input[i + k];
            i += sequence;
        }
        ++chars;
    }
    return output;
}

V2ClaimArgs v2PrepareClaim(const String &id, const String &name,
                           const String &host, const String &port,
                           const String &lease, bool hasLease,
                           bool force, bool release) {
    V2ClaimArgs args;
    args.request.id = v2ClaimText(id, 32);
    size_t first = 0, last = args.request.id.length();
    while (first < last && args.request.id[first] == ' ') ++first;
    while (last > first && args.request.id[last - 1] == ' ') --last;
    args.request.id = args.request.id.substring(first, last);
    args.validId = args.request.id.length() != 0;
    args.request.name = v2ClaimText(name, 16);
    args.request.host = v2ClaimText(host, 32);
    long portNumber = port.toInt();
    args.request.port = portNumber > 0 && portNumber <= 65535
        ? (uint16_t)portNumber : 0;
    long leaseSeconds = hasLease ? lease.toInt() : 300;
    if (leaseSeconds < 60) leaseSeconds = 60;
    if (leaseSeconds > 3600) leaseSeconds = 3600;
    args.request.lease = (uint32_t)leaseSeconds;
    args.force = force;
    args.release = release;
    return args;
}

V2ClaimDecision v2DecideClaim(const V2ClaimArgs &args, bool haveOwner,
                              const OwnerRec &current) {
    V2ClaimDecision decision;
    decision.request = args.request;
    if (args.release) {
        if (!haveOwner) decision.action = V2_CLAIM_RELEASE_EMPTY;
        else if (current.id != args.request.id && !args.force)
            decision.action = V2_CLAIM_OCCUPIED;
        else decision.action = V2_CLAIM_RELEASE;
        return decision;
    }
    if (haveOwner && current.id != args.request.id && !args.force) {
        decision.action = V2_CLAIM_OCCUPIED;
        return decision;
    }
    decision.action = V2_CLAIM_CLAIM;
    decision.keepSince = haveOwner && current.id == args.request.id;
    decision.newClaim = !decision.keepSince;
    return decision;
}
