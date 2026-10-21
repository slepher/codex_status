#pragma once
#include "business.h"
#include "ap01_pairing.h"

struct Ap01ApplicationPort {
    void *context;
    bool (*random)(void *, uint8_t *, size_t);
    // Serializes only binding/UI/owner state, never Bundle rendering or I/O.
    bool (*lockBindings)(void *);
    void (*unlockBindings)(void *);
    // Firmware owns maintenance; App only observes its availability gate.
    bool (*systemBusy)(void *) = nullptr;
};
#include "native_http_0041.h"

// One business executor owns this object. Maintenance and UI may independently
// enter binding APIs while the business executor is rendering/installing.
class Ap01Application {
    struct Session;
    BundleFilesystem &fs;
    Ap01Business &business;
    Ap01ApplicationPort port;
    Ap01Bindings bindings{};
    std::unique_ptr<Session> sessions;
    char mac[13], nonce[33] = {};
    uint64_t boot;
    bool ready = false;
    std::atomic<bool> online{false};
    static int save(void *, const uint8_t *, size_t);
    Session *session(const char *principal);
    void dispatch(const Ap01HttpRequest &, const char *principal, uint16_t op,
                  uint64_t now, Ap01HttpReply &reply);
public:
    Ap01Application(BundleFilesystem &, Ap01Business &, const Ap01ApplicationPort &,
                    const char *wifiMac, uint64_t bootId);
    ~Ap01Application();
    bool begin(uint64_t now=0);
    void businessOnline(bool value) { online.store(value, std::memory_order_release); }
    bool maintenanceActive() const {return port.systemBusy && port.systemBusy(port.context);}
    bool businessAvailable() const {return online.load(std::memory_order_acquire) && !maintenanceActive();}
    Ap01HttpReply pairingHttp(const Ap01HttpRequest &, uint64_t now);
    Ap01HttpReply businessHttp(const Ap01HttpRequest &, uint64_t now);
    int bindingPrincipal(const char *token, char out[65]);
    bool discovery(const uint8_t *, size_t, String &reply);
    bool pairingUi(uint64_t now, String &reply);
    bool pairingDisplayed(const char *request, uint32_t revision, uint64_t event);
    bool button(bool pressed, uint64_t event, uint64_t now);
};
