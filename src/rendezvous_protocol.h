#pragma once
#include <stdint.h>

// Pure lifecycle. GPIO/radio/WebServer/PM operations belong to the adapter.
constexpr uint32_t RV_NETWORK_MS = 30000;
constexpr uint32_t RV_CONNECTED_MS = 6000;
constexpr uint32_t RV_ACK_GRACE_MS = 200;
enum class RendezvousPhase { Idle, Exchange, HttpStarting, HttpReady, AckGrace, Done };
class RendezvousProtocol {
public:
    void begin(uint64_t now, uint32_t advertiseMs);
    void connected(uint64_t now);
    void planReceived() { planReceived_ = true; }
    bool hasPlan() const { return planReceived_; }
    void awaitNetworkStart(uint64_t now, uint64_t legalUntil);
    void admitHttp(uint64_t now, uint64_t legalUntil);
    void httpReady();
    void finishExchange(uint64_t now, uint64_t historyUntil);
    bool tick(uint64_t now);
    void cancel();
    bool active() const { return phase_ != RendezvousPhase::Idle && phase_ != RendezvousPhase::Done; }
    bool needsHttp() const { return phase_ == RendezvousPhase::HttpStarting || phase_ == RendezvousPhase::HttpReady; }
    bool ready() const { return phase_ == RendezvousPhase::HttpReady; }
    uint64_t deadline() const { return deadline_; }
    RendezvousPhase phase() const { return phase_; }
private:
    RendezvousPhase phase_ = RendezvousPhase::Idle;
    uint64_t deadline_ = 0;
    bool connected_ = false, planReceived_ = false;
};
