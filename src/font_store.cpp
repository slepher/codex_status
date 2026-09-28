#include "font_store.h"

#include <LittleFS.h>
#include <string.h>
#include <vector>

#include "dev_log.h"

static const char *FONT_ROOT = "/fonts";

// Path handling stays on the String subset the engine already relies on
// (length/indexOf/substring/c_str), so the same code runs on the device and in
// the host harness: the host String shim is deliberately minimal.
static int lastSlash(const String &s) {
    int found = -1;
    for (int i = 0; i < (int)s.length(); i++) {
        if (s[i] == '/') found = i;
    }
    return found;
}

static String baseName(const String &path) {
    int slash = lastSlash(path);
    return slash < 0 ? path : path.substring((size_t)slash + 1);
}

// Index of the last '.', or -1. Used to strip the ".bin"/".tmp" suffix.
static int lastDot(const String &s) {
    int found = -1;
    for (int i = 0; i < (int)s.length(); i++) {
        if (s[i] == '.') found = i;
    }
    return found;
}

static String withoutExtension(const String &s) {
    int dot = lastDot(s);
    return dot > 0 ? s.substring(0, (size_t)dot) : s;
}

static bool hasSuffix(const String &s, const char *suffix) {
    const size_t n = strlen(suffix);
    if (s.length() < n) return false;
    const size_t at = s.length() - n;
    for (size_t i = 0; i < n; i++) {
        if (s[at + i] != suffix[i]) return false;
    }
    return true;
}

static String sProfile;
static String sDir;

// Profile ids come from the bridge; keep them to the same safe alphabet the
// template store uses so a crafted id can never escape the /fonts directory.
static bool profileIdValid(const String &id) {
    if (id.length() == 0 || id.length() > 16) return false;
    for (size_t i = 0; i < id.length(); i++) {
        char c = id[i];
        if (!isalnum((unsigned char)c) && c != '_' && c != '-') return false;
    }
    return true;
}

static bool fontIdValid(const char *id) {
    if (!id || strlen(id) != 8) return false;
    for (int i = 0; i < 8; i++) {
        char c = id[i];
        if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'))) return false;
    }
    return true;
}

static String fontPath(const char *id) {
    return sDir + "/" + String(id) + ".bin";
}

bool fontStoreBegin(const char *profileId) {
    if (!LittleFS.begin(true, "/littlefs", 10, "storage")) {
        DevLog.println("[font] LittleFS mount failed");
        sProfile = "";
        sDir = "";
        return false;
    }
    if (!LittleFS.exists(FONT_ROOT)) LittleFS.mkdir(FONT_ROOT);
    String id = profileId ? String(profileId) : String("");
    if (!profileIdValid(id)) {
        DevLog.printf("[font] invalid profile id '%s'\n", id.c_str());
        sProfile = "";
        sDir = "";
        return false;
    }
    sProfile = id;
    sDir = String(FONT_ROOT) + "/" + id;
    if (!LittleFS.exists(sDir.c_str())) LittleFS.mkdir(sDir.c_str());
    uint32_t bytes = 0;
    int count = 0;
    fontStoreUsage(bytes, count);
    DevLog.printf("[font] profile %s: %d font(s), %u B\n", sProfile.c_str(), count,
                  (unsigned)bytes);
    return true;
}

const char *fontStoreProfile() { return sProfile.c_str(); }

static bool bound() { return sProfile.length() > 0; }

bool fontStoreHas(const char *fontId) {
    if (!bound() || !fontIdValid(fontId)) return false;
    return LittleFS.exists(fontPath(fontId).c_str());
}

// Read a whole container into `buf` and validate it. Fonts are small (a few KB,
// hard-capped at FONT_ASSET_MAX_BYTES) and a container can only be trusted after
// its CRC has been checked over the complete payload, so the read is whole-file.
static bool readValidate(const String &path, uint8_t *buf, size_t cap, size_t &len,
                         FontAssetInfo &info, String &err) {
    File f = LittleFS.open(path.c_str(), FILE_READ);
    if (!f) { err = "font_open"; return false; }
    const size_t size = f.size();
    if (size == 0 || size > cap || size > FONT_ASSET_MAX_BYTES) {
        f.close();
        err = "font_size";
        return false;
    }
    len = f.read(buf, size);
    f.close();
    if (len != size) { err = "font_read"; return false; }
    if (!fontAssetValidate(buf, len, info, err)) return false;
    if (!fontAssetMatchesTarget(info, err)) return false;
    return true;
}

int fontStoreInventory(FontAssetInfo *out, int cap, int *badOut) {
    if (badOut) *badOut = 0;
    if (!bound() || !out || cap <= 0) return 0;
    static std::vector<uint8_t> buf;
    buf.resize(FONT_ASSET_MAX_BYTES);
    int n = 0;
    File dir = LittleFS.open(sDir.c_str(), FILE_READ);
    if (!dir || !dir.isDirectory()) {
        if (dir) dir.close();
        return 0;
    }
    for (File f = dir.openNextFile(); f; f = dir.openNextFile()) {
        String name = baseName(String(f.name()));
        f.close();
        if (!hasSuffix(name, ".bin")) continue;
        String id = name.substring(0, name.length() - 4);
        if (!fontIdValid(id.c_str())) continue;
        size_t len = 0;
        FontAssetInfo info;
        String err;
        if (!readValidate(sDir + "/" + name, buf.data(), buf.size(), len, info, err)) {
            DevLog.printf("[font] inventory: %s unusable (%s)\n", name.c_str(), err.c_str());
            if (badOut) (*badOut)++;
            continue;
        }
        // The store is content-addressed, so the file name and the container's own
        // id must agree. A mis-named file is reported as bad instead of being
        // listed under an id it is not stored at: otherwise the bridge would treat
        // that id as installed and a later load by it would fail.
        if (strcmp(info.id, id.c_str()) != 0) {
            DevLog.printf("[font] inventory: %s carries id %s; unusable\n", name.c_str(),
                          info.id);
            if (badOut) (*badOut)++;
            continue;
        }
        if (n < cap) out[n++] = info;
    }
    dir.close();
    return n;
}

bool fontStoreWrite(const uint8_t *bytes, size_t len, FontAssetInfo &info,
                    bool &stored, String &err) {
    stored = false;
    if (!bound()) { err = "font_no_profile"; return false; }
    if (!fontAssetValidate(bytes, len, info, err)) return false;
    if (!fontAssetMatchesTarget(info, err)) return false;

    // Immutable content address: an existing id is already the right bytes, so
    // this is a successful no-op that must not rewrite the file (and therefore
    // transfers nothing on a repeated push).
    if (fontStoreHas(info.id)) {
        DevLog.printf("[font] %s already installed; no-op\n", info.id);
        return true;
    }

    uint32_t used = 0;
    int count = 0;
    fontStoreUsage(used, count);
    if (count >= FONT_STORE_MAX_PER_PROFILE) { err = "font_cap_count"; return false; }
    if (used + len > FONT_STORE_MAX_BYTES) { err = "font_cap_bytes"; return false; }
    if (LittleFS.totalBytes() - LittleFS.usedBytes() < len + 40960) {
        err = "font_space"; return false;
    }

    String tmp = fontPath(info.id) + ".tmp";
    String dst = fontPath(info.id);
    LittleFS.remove(tmp.c_str());
    File f = LittleFS.open(tmp.c_str(), FILE_WRITE);
    if (!f) { err = "font_create"; return false; }
    const size_t wrote = f.write(bytes, len);
    f.close();
    if (wrote != len) {
        LittleFS.remove(tmp.c_str());
        err = "font_write";
        return false;
    }
    // Read the staged file back and validate it: a torn write, a short write or
    // a flash hiccup must never become a visible font.
    {
        static std::vector<uint8_t> verify;
        verify.resize(len);
        size_t got = 0;
        FontAssetInfo vinfo;
        String verr;
        if (!readValidate(tmp, verify.data(), verify.size(), got, vinfo, verr) ||
            strcmp(vinfo.id, info.id) != 0) {
            LittleFS.remove(tmp.c_str());
            err = "font_verify";
            return false;
        }
    }
    LittleFS.remove(dst.c_str());
    if (!LittleFS.rename(tmp.c_str(), dst.c_str())) {
        LittleFS.remove(tmp.c_str());
        err = "font_rename";
        return false;
    }
    stored = true;
    DevLog.printf("[font] stored %s (%s, %u B)\n", info.id, info.name, (unsigned)len);
    return true;
}

bool fontStoreLoad(const char *fontId, uint8_t *buf, size_t cap, size_t &len,
                   FontAssetInfo &info, String &err) {
    if (!bound()) { err = "font_no_profile"; return false; }
    if (!fontIdValid(fontId)) { err = "font_id"; return false; }
    if (!buf || cap == 0) { err = "font_buffer"; return false; }
    String path = fontPath(fontId);
    if (!LittleFS.exists(path.c_str())) { err = "font_missing"; return false; }
    if (!readValidate(path, buf, cap, len, info, err)) return false;
    // A file stored under the wrong name must never be used silently.
    if (strcmp(info.id, fontId) != 0) { err = "font_id_mismatch"; return false; }
    return true;
}

int fontStorePrune(const char *const *keepIds, int keepCount) {
    if (!bound()) return -1;
    int removed = 0;
    File dir = LittleFS.open(sDir.c_str(), FILE_READ);
    if (!dir || !dir.isDirectory()) {
        if (dir) dir.close();
        return 0;
    }
    std::vector<String> victims;
    for (File f = dir.openNextFile(); f; f = dir.openNextFile()) {
        String name = baseName(String(f.name()));
        f.close();
        if (!hasSuffix(name, ".bin") && !hasSuffix(name, ".tmp")) continue;
        String base = withoutExtension(name);
        bool keep = false;
        for (int i = 0; i < keepCount && !keep; i++) {
            if (keepIds[i] && base == keepIds[i]) keep = true;
        }
        if (!keep) victims.push_back(name);
    }
    dir.close();
    for (size_t i = 0; i < victims.size(); i++) {
        if (LittleFS.remove((sDir + "/" + victims[i]).c_str())) {
            removed++;
            DevLog.printf("[font] pruned %s\n", victims[i].c_str());
        }
    }
    return removed;
}

void fontStoreUsage(uint32_t &bytes, int &count) {
    bytes = 0;
    count = 0;
    if (!bound()) return;
    File dir = LittleFS.open(sDir.c_str(), FILE_READ);
    if (!dir || !dir.isDirectory()) {
        if (dir) dir.close();
        return;
    }
    for (File f = dir.openNextFile(); f; f = dir.openNextFile()) {
        String name = baseName(String(f.name()));
        const size_t size = f.size();
        f.close();
        if (!hasSuffix(name, ".bin")) continue;
        bytes += (uint32_t)size;
        count++;
    }
    dir.close();
}

bool fontStoreClearProfile() {
    if (!bound()) return false;
    fontStorePrune(nullptr, 0);
    bool ok = LittleFS.rmdir(sDir.c_str());
    DevLog.printf("[font] cleared profile %s (%s)\n", sProfile.c_str(), ok ? "ok" : "partial");
    return ok;
}
