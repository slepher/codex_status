// Profile-scoped device font store.
//
// Fonts live at `/fonts/<profile_id>/<font_id>.bin` on LittleFS. There is
// deliberately no cross-Profile sharing: the same font used by two Profiles
// exists twice, which keeps Profile replacement a directory-level operation with
// no reference counting, no LRU and no "in use" refusals.
//
// Fonts are immutable: an id is a content address, so an existing file is never
// rewritten, a duplicate push is a no-op, and an update is always a new id.
#pragma once
#include <Arduino.h>

#include "font_asset.h"

// Caps per Profile. Exceeding either is an explicit rejection (the bridge checks
// the same numbers before offering a push), never a silent trim.
#define FONT_STORE_MAX_PER_PROFILE 8
#define FONT_STORE_MAX_BYTES (1024u * 1024u)

// Mount LittleFS (idempotent) and select the Profile directory. Must be called
// before any other fontStore* function. An invalid/empty id leaves the store
// bound to no Profile, and every call then fails closed.
bool fontStoreBegin(const char *profileId);

// Currently selected Profile id ("" when unbound).
const char *fontStoreProfile();

// Installed fonts of the selected Profile (validated on read). Returns the count
// (<= cap); entries whose file is corrupt or is not a valid container are
// skipped and reported through `badOut` so the bridge can re-push them.
int fontStoreInventory(FontAssetInfo *out, int cap, int *badOut);

bool fontStoreHas(const char *fontId);

// Validate then store. Rejects: invalid container, profile mismatch of the
// pixel format, already-present id (no-op success, `stored=false`), and either
// Profile cap. The file appears under its final name only after a successful
// read-back verification.
bool fontStoreWrite(const uint8_t *bytes, size_t len, FontAssetInfo &info,
                    bool &stored, String &err);

// Load + validate one font. `buf` must hold at least FONT_ASSET_MAX_BYTES.
bool fontStoreLoad(const char *fontId, uint8_t *buf, size_t cap, size_t &len,
                   FontAssetInfo &info, String &err);

// Delete every font in the selected Profile that is not in `keepIds`. Returns the
// number of files removed, or -1 on error.
int fontStorePrune(const char *const *keepIds, int keepCount);

// Bytes and file count currently used by the selected Profile.
void fontStoreUsage(uint32_t &bytes, int &count);

// Remove the whole Profile directory (used when a Profile is replaced wholesale).
bool fontStoreClearProfile();
