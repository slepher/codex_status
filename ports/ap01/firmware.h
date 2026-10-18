#pragma once
#include "LittleFS.h"
#include <memory>
#include <stdint.h>

struct Ap01HttpRequest;
struct Ap01HttpReply;
struct Ap01SlotImage {
    uint32_t length=0, crc=0;
    char build[33]={};
};
struct Ap01FirmwarePort {
    void *context = nullptr;
    bool (*busy)(void *) = nullptr;
    bool (*open)(void *, uint32_t) = nullptr;
    bool (*write)(void *, uint32_t, const uint8_t *, size_t) = nullptr;
    bool (*verify)(void *, uint32_t) = nullptr;
    int (*commit)(void *) = nullptr; // 1 confirmed; -1 result unknown.
    void (*close)(void *) = nullptr;
    const uint8_t *referenceHeader = nullptr;
    const char *build = nullptr;
    void (*reboot)(void *) = nullptr;
    int (*runningSlot)(void *) = nullptr;
    bool (*inspectSlot)(void *, unsigned, Ap01SlotImage &) = nullptr;
    int (*selectSlot)(void *, unsigned) = nullptr;
    bool (*bootHealthy)(void *) = nullptr;
};

// One maintenance executor owns the transaction. Only its availability gate
// crosses threads; neither UI nor business code reads this mutable state.
class Ap01Firmware {
    struct Record;
    struct Stable;
    LittleFSClass &fs;
    Ap01FirmwarePort port;
    std::unique_ptr<Record> record;
    std::unique_ptr<Stable> stable;
    char mac[13];
    bool corrupt = false, opened = false, booted = false, commitConfirmed = false, rebootPending = false;
    enum BootSafety { Unconfigured, StableRunning, Armed, Healthy, RetainedStable, SafetyUnknown } safety=Unconfigured;
    uint64_t armedAt=0, healthySince=0;
    bool healthyObserved=false;
    void armBoot(uint64_t now);
    const char *safetyName() const;
    bool save();
    bool load();
    bool loadStable();
    bool saveStable(const Stable &);
    Ap01HttpReply slots(const Ap01HttpRequest &,const char *,uint64_t);
    void close();
    void failed(const char *reason);
public:
    Ap01Firmware(LittleFSClass &, const Ap01FirmwarePort &, const char *mac);
    ~Ap01Firmware();
    bool available() const;
    bool begin(uint64_t now=0);
    bool blocksBusiness() const;
    void tick(uint64_t now);
    void afterResponse(bool sent);
    Ap01HttpReply handle(const Ap01HttpRequest &, const char *principal, uint64_t now);
};
