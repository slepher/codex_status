#pragma once
#include <stdint.h>

// Pure lifecycle. GPIO/radio/WebServer/PM operations belong to the adapter.
constexpr uint32_t RV_HANDOFF_MS = 30000;
constexpr uint32_t RV_CONNECTED_MS = 6000;
constexpr uint32_t RV_ACK_GRACE_MS = 200;
enum class RendezvousPhase { Idle, Exchange, HttpStarting, WaitHandoff, AckGrace, Done };
class RendezvousProtocol {
public:
    void begin(uint64_t now, uint32_t advertiseMs);
    void connected(uint64_t now);
    void planReceived() { planReceived_ = true; }
    bool hasPlan() const { return planReceived_; }
    void admitHttp(uint64_t now, uint64_t legalUntil);
    void httpReady();
    bool handoff(uint64_t now, bool proofMatches);
    void finishExchange(uint64_t now, uint64_t historyUntil);
    bool tick(uint64_t now);
    void cancel();
    bool active() const { return phase_ != RendezvousPhase::Idle && phase_ != RendezvousPhase::Done; }
    bool needsHttp() const { return phase_ == RendezvousPhase::HttpStarting || phase_ == RendezvousPhase::WaitHandoff; }
    bool ready() const { return phase_ == RendezvousPhase::WaitHandoff || handedOff_; }
    bool handedOff() const { return handedOff_; }
    uint64_t deadline() const { return deadline_; }
    RendezvousPhase phase() const { return phase_; }
private:
    RendezvousPhase phase_ = RendezvousPhase::Idle;
    uint64_t deadline_ = 0;
    bool connected_ = false, handedOff_ = false, planReceived_ = false;
};

// Envelope authentication and owner lookup are adapters. The protocol binds
// the proof to the current accepted plan, bridge, wake and HTTP-only challenge.
struct RendezvousHandoffProof {
    bool sessionAccepted = false, ownerAccepted = false;
    const char *bridge = "", *expectedBridge = "";
    const char *id = "", *expectedId = "", *challenge = "", *expectedChallenge = "";
    uint64_t planId = 0, acceptedPlanId = 0;
    uint32_t generation = 0, expectedGeneration = 0, wakeSeq = 0, expectedWakeSeq = 0;
};
bool rendezvousProofMatches(const RendezvousHandoffProof &proof);
