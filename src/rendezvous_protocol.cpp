#include "rendezvous_protocol.h"
#include <algorithm>
void RendezvousProtocol::begin(uint64_t now, uint32_t advertiseMs) {
    phase_ = RendezvousPhase::Exchange; deadline_ = now + advertiseMs;
    connected_ = planReceived_ = false;
}
void RendezvousProtocol::connected(uint64_t now) {
    if (!connected_ && phase_ == RendezvousPhase::Exchange) {
        connected_ = true; deadline_ = now + RV_CONNECTED_MS;
    }
}
void RendezvousProtocol::awaitNetworkStart(uint64_t now, uint64_t legalUntil) {
    if (phase_ == RendezvousPhase::Exchange && legalUntil > now)
        deadline_ = std::min(now + RV_NETWORK_MS, legalUntil);
}
void RendezvousProtocol::admitHttp(uint64_t now, uint64_t legalUntil) {
    if (needsHttp()) { deadline_ = std::min(deadline_, legalUntil); return; }
    if (phase_ != RendezvousPhase::Exchange || now >= deadline_ || legalUntil <= now) return;
    phase_ = RendezvousPhase::HttpStarting;
    deadline_ = std::min(deadline_, legalUntil);
}
void RendezvousProtocol::httpReady() {
    if (phase_ == RendezvousPhase::HttpStarting) phase_ = RendezvousPhase::HttpReady;
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
