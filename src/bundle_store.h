// One committed content.bin (raw JSON or CSZ1 + compiled table), a small checked
// state.bin, and a temporary incoming.bin while downloading. Download failure
// keeps the current application; replacement failure uses built-in recovery.
// There is no previous-content slot or automatic rollback. Native ROM OTA is
// independent. See project-workflow/single-copy-updates/design.md.
#pragma once

#include <Arduino.h>
#include <LittleFS.h>
#include "template_engine.h"

#define BS_MAX_TEMPLATES 8
#define BS_CTX_LEN 33
#define BS_ID_LEN 17
#define BS_JOB_LEN 17

struct BsProfile {
    char ids[BS_MAX_TEMPLATES][BS_ID_LEN];
    uint8_t count = 0;
    uint8_t initial = 0;
    char jobId[BS_JOB_LEN] = {0};
    char contextId[BS_CTX_LEN] = {0};
    char firmwareTarget[32] = {0};
    char renderTarget[32] = {0};
    uint32_t bundleCrc = 0;
};

// Mount the filesystem and load the committed bundle header (if any).
bool bsBegin();

// Install a downloaded complete Bundle; replace only after preflight succeeds.
// `expectedFirmware`/`expectedRender` guard against wrong-target images.
bool bsInstall(const String &bundleJson, const char *expectedFirmware,
               const char *expectedRender, const char *newContextId, String &err);
bool bsInstall(File &bundleFile, uint32_t bundleLength, uint32_t bundleCrc,
               const char *expectedFirmware, const char *expectedRender,
               const char *newContextId, String &err, void (*beforeReplace)() = nullptr);

// Load one compiled template from the current content file (bounded, validated).
bool bsLoadCompiled(uint8_t index, CtTemplate &out, String &err);

// The committed profile (order + initial active + context/job ids).
bool bsProfile(BsProfile &out);
bool bsConfigured();
// Return the source payload CRC when the active committed slot has this job.
bool bsActiveJobPayload(const char *jobId, uint32_t &payloadCrc);

// Local active switch (BOOT key cycle): new initial template and new context.
bool bsSetActive(uint8_t index, const char *newContextId, String &err);

// Minimal recovery read: order + active + source availability, no side effects
// (no claim, no push, no active change, no JSON re-parse of inactive items).
bool bsRecoveryDigest(String &out);
bool bsHasSource(uint8_t index);

// Diagnostics.
uint32_t bsCommitSeq();
const char *bsLastError();
size_t bsFreeBytes();

// Temporary download file; current content keeps running during transfer.
const char *bsReceivePath();
size_t bsMaxBundleBytes();
uint32_t bsDecodedLimit();
uint32_t bsCompressionProtocol();
// Authenticated download admission. Does not stop or invalidate current content.
bool bsStartReceive(uint32_t length, String &err,
                    const char *owner = "", const char *request = "");
struct BsReceipt {
    char owner[65] = {}, request[65] = {}, context[BS_CTX_LEN] = {};
    uint32_t length = 0, crc = 0;
};
bool bsReceipt(BsReceipt &out);
