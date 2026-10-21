#pragma once
#include "bundle_store_context.h"
#include "v2_bundle_command.h"
#include "v2_state.h"
#include <atomic>
#include <memory>

struct Ap01BusinessPort {
    void *context;
    // Success transfers frame ownership to the UI queue. A null frame retires
    // the content page. Only the UI executor acknowledges actual presentation.
    bool (*present)(void *, uint8_t *, uint32_t);
    bool (*retire)(void *) = nullptr;
    const char *(*displayReason)(void *) = nullptr;
};

class Ap01Business {
    PAINT paint{};
    Ap01BusinessPort ui;
    BundleFilesystem &fs;
    uint64_t boot, contextCounter = 0;
    std::unique_ptr<CtTemplate> compiled;
    BsProfile profile{};
    BsReceipt durable{};
    V2BundleServices services;
    V2BundleRx rx{};
    V2DataSeq sequence;
    String usage, syncTime, currentTime;
    String unknownOwner, unknownRequest;
    uint16_t unknownOperation = 0;
    uint32_t generation = 0, frameCrc = 0;
    std::atomic<uint32_t> displayedGeneration{0};
    std::atomic<uint32_t> failedGeneration{UINT32_MAX};
    bool configured = false, storageUnknown = false, replacing = false;
    void newContext(char (&out)[BS_CTX_LEN]);
    bool loadActive(uint64_t now);
    bool render();
    void displayFeedback(JsonDocument &out, const char *state);
    static bool beforeReplace(void *);
public:
    TemplateRenderer renderer;
    BundleStore store;
    Ap01Business(BundleFilesystem &filesystem, const Ap01BusinessPort &port, uint64_t bootId);
    bool begin(uint64_t now);
    void clock(const char *hhmm);
    bool redraw() { return configured && render(); }
    void displayed(uint32_t value) { displayedGeneration.store(value, std::memory_order_release); }
    void displayFailed(uint32_t value) {failedGeneration.store(value,std::memory_order_release);}
    uint8_t count() const { return configured ? profile.count : 0; }
    uint8_t active() const { return profile.initial; }
    const char *screenId(unsigned index) const { return configured && index < profile.count ? profile.ids[index] : ""; }
    const char *context() const { return profile.contextId; }
    bool localActivate(uint8_t index, uint64_t now);
    void status(JsonDocument &out);
    int command(uint16_t op, JsonDocument &request, const char *nonce, uint64_t now,
                const uint8_t *body, size_t bytes, const char *requestId,
                const char *offset, String &reply);
};
