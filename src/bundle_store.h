// v2 complete Bundle storage with A/B slots and tear-proof commit records.
//
// Layout on littlefs (`storage` partition, shared with the legacy template
// store which keeps its own files):
//   /bundle/a.bin, /bundle/b.bin   complete self-contained bundles
//   /bundle/m0.bin, /bundle/m1.bin commit records (double copies, alternating)
//
// Guarantees (docs/generic-display-platform-design-v2.md §9):
//  - the current slot always stays usable; a new bundle is written to the other
//    slot only;
//  - a slot is only committed after a full read-back + CRC check;
//  - power loss leaves either the complete old or the complete new bundle;
//  - insufficient space rejects the publish without deleting the valid copy;
//  - commit records are chosen by the highest sequence number;
//  - the previous complete bundle is recovery-only (no history UI).
#pragma once

#include <Arduino.h>
#include "template_engine.h"

#define BS_MAX_TEMPLATES 8
#define BS_MAX_BUNDLE_BYTES 262144
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

// Install a complete Bundle payload (canonical JSON) into the inactive slot.
// `expectedFirmware`/`expectedRender` guard against wrong-target images.
bool bsInstall(const String &bundleJson, const char *expectedFirmware,
               const char *expectedRender, const char *newContextId, String &err);

// Load one compiled template from the active slot (bounded, validated).
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
