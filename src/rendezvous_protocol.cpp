#include "rendezvous_protocol.h"
#include <algorithm>
void RendezvousProtocol::begin(uint64_t now, uint32_t advertiseMs) {
    phase_ = RendezvousPhase::Exchange; deadline_ = now + advertiseMs;
    connected_ = handedOff_ = planReceived_ = false;
}
void RendezvousProtocol::connected(uint64_t now) {
    if (!connected_ && phase_ == RendezvousPhase::Exchange) {
        connected_ = true; deadline_ = now + RV_CONNECTED_MS;
    }
}
void RendezvousProtocol::admitHttp(uint64_t now, uint64_t legalUntil) {
    if (needsHttp()) { deadline_ = std::min(deadline_, legalUntil); return; }
    if (phase_ != RendezvousPhase::Exchange || now >= deadline_ || legalUntil <= now) return;
    phase_ = RendezvousPhase::HttpStarting;
    deadline_ = std::min(now + RV_HANDOFF_MS, legalUntil);
}
void RendezvousProtocol::httpReady() {
    if (phase_ == RendezvousPhase::HttpStarting) phase_ = RendezvousPhase::WaitHandoff;
}
bool RendezvousProtocol::handoff(uint64_t now, bool proofMatches) {
    if (!proofMatches) return false;
    if (handedOff_) return true; // query/replay never renews either deadline
    if (phase_ != RendezvousPhase::WaitHandoff || now >= deadline_) return false;
    handedOff_ = true; phase_ = RendezvousPhase::AckGrace;
    deadline_ = std::min(deadline_, now + RV_ACK_GRACE_MS);
    return true;
}
void RendezvousProtocol::finishExchange(uint64_t now, uint64_t historyUntil) {
    if (phase_ != RendezvousPhase::Exchange && !needsHttp()) return;
    phase_ = RendezvousPhase::AckGrace;
    deadline_ = std::min(deadline_, std::max(now + RV_ACK_GRACE_MS, historyUntil));
}
bool RendezvousProtocol::tick(uint64_t now) {
    if (!active() || now < deadline_) return false;
    phase_ = RendezvousPhase::Done; return true;
}
void RendezvousProtocol::cancel() { phase_ = RendezvousPhase::Done; }

#include <cstring>
bool rendezvousProofMatches(const RendezvousHandoffProof &p) {
    auto matches = [](const char *actual, const char *expected) {
        return actual && expected && *expected && !std::strcmp(actual, expected);
    };
    return p.sessionAccepted && p.ownerAccepted && matches(p.bridge,p.expectedBridge) &&
        matches(p.id,p.expectedId) && matches(p.challenge,p.expectedChallenge) &&
        p.planId == p.acceptedPlanId && p.generation == p.expectedGeneration && p.wakeSeq == p.expectedWakeSeq;
}
