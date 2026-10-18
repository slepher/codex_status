#include "bundle_store_context.h"
#include "bundle_payload.h"
#include "v2_state.h"
#include "font_asset.h"
#include <ArduinoJson.h>
#include <mbedtls/base64.h>
#include <string.h>
#include <algorithm>
#include <cstddef>
#include <memory>
#include <new>

namespace {
constexpr const char *CONTENT = "/bundle/content.bin";
constexpr const char *INCOMING = "/bundle/incoming.bin";
constexpr const char *META = "/bundle/state.bin";
constexpr const char *META_PENDING = "/bundle/state.tmp";
// Leave room for sync receipts, small commit records and filesystem blocks.
constexpr size_t RESERVE = 40960 + 4096 + 4096;
constexpr size_t META_LIMIT = 2048;
constexpr size_t TEMPLATE_LIMIT = 32768;
const char *const LEGACY[] = {"/bundle/a.bin", "/bundle/b.bin",
                        "/bundle/m0.bin", "/bundle/m1.bin", "/bundle/rx.bin"};
class JsonBudget : public ArduinoJson::Allocator {
    struct alignas(std::max_align_t) Header { size_t size; };
    size_t used = 0, limit;
public:
    explicit JsonBudget(size_t bytes) : limit(bytes) {}
    void *allocate(size_t n) override {
        if (n > limit - used || n > SIZE_MAX - sizeof(Header)) return nullptr;
        auto *h = (Header *)malloc(sizeof(Header) + n);
        if (!h) return nullptr;
        h->size = n; used += n; return h + 1;
    }
    void deallocate(void *p) override {
        if (!p) return;
        auto *h = (Header *)p - 1; used -= h->size; free(h);
    }
    void *reallocate(void *p, size_t n) override {
        if (!p) return allocate(n);
        auto *h = (Header *)p - 1;
        size_t old = h->size;
        if (n > limit - used + old || n > SIZE_MAX - sizeof(Header)) return nullptr;
        h = (Header *)realloc(h, sizeof(Header) + n);
        if (!h) return nullptr;
        h->size = n; used = used - old + n; return h + 1;
    }
};

uint32_t le32(const uint8_t *p) {
    return uint32_t(p[0]) | uint32_t(p[1]) << 8 | uint32_t(p[2]) << 16 | uint32_t(p[3]) << 24;
}
void put32(uint8_t *p, uint32_t v) {
    for (int i = 0; i < 4; ++i) p[i] = uint8_t(v >> (8 * i));
}
bool write(File &f, const void *p, size_t n) { return f.write((const uint8_t *)p, n) == n; }



bool crcRange(File &f, uint32_t start, uint32_t length, uint32_t &crc) {
    if (uint64_t(start) + length > f.size() || !f.seek(start)) return false;
    uint8_t buf[256];
    uint32_t state = v2Crc32Start();
    while (length) {
        size_t want = length > sizeof(buf) ? sizeof(buf) : length;
        size_t got = f.read(buf, want);
        if (got != want) return false;
        state = v2Crc32Update(state, buf, got);
        length -= got;
    }
    crc = v2Crc32Finish(state);
    return true;
}
// All JSON reads use a bounded view, including the host tests. No whole-file String.
template<class Input> struct Reader {
    Input &file;
    uint32_t left;
    Reader(Input &f, uint32_t at, uint32_t n) : file(f), left(f.seek(at) ? n : 0) {}
    int read() { if (!left) return -1; int c = file.read(); if (c >= 0) --left; return c; }
    size_t readBytes(char *p, size_t n) {
        if (n > left) n = left;
        size_t got = file.read((uint8_t *)p, n); left -= got; return got;
    }
};
bool textField(JsonVariantConst v, char *out, size_t cap, bool empty = false) {
    if (!v.is<const char *>()) return false;
    JsonString s = v.as<JsonString>();
    if ((!empty && !s.size()) || s.size() >= cap || strlen(s.c_str()) != s.size()) return false;
    memcpy(out, s.c_str(), s.size() + 1); return true;
}



// Legacy data is moved, never copied. Once content.bin exists we never consult
// legacy slots again, even after a failed update. No fallback to the older slot.


struct Range { uint32_t at, length; };
struct Ranges {
    Range entries[8];
    size_t count = 0;
    void clear() { count = 0; }
    size_t size() const { return count; }
    void push_back(Range r) { entries[count++] = r; }
    const Range &operator[](size_t i) const { return entries[i]; }
    const Range *begin() const { return entries; }
    const Range *end() const { return entries + count; }
};
// A small cursor locates array elements after the full filtered JSON validation.
// It skips strings/escapes without ever storing compiled hex or unrelated assets.
template<class Input> struct Cursor {
    Input &f; uint32_t end, pos;
    uint8_t buffer[256]; size_t used = 0, count = 0;
    Cursor(Input &file, uint32_t until) : f(file), end(until), pos(file.position()) {}
    uint32_t position() const { return pos; }
    void seek(uint32_t at) { pos = at; used = count = 0; }
    int peek() {
        if (pos >= end) return -1;
        if (used == count) {
            if (!f.seek(pos)) return -1;
            count = f.read(buffer, std::min<size_t>(sizeof(buffer), end - pos)); used = 0;
            if (!count) return -1;
        }
        return buffer[used];
    }
    int get() { int c = peek(); if (c >= 0) { ++used; ++pos; } return c; }
    void space() { while (peek() == ' ' || peek() == '\n' || peek() == '\r' || peek() == '\t') get(); }
    bool value(unsigned depth = 0) {
        space(); int c = get();
        if (depth > 32 || c < 0) return false;
        if (c == '"') {
            while ((c = get()) >= 0) {
                if (c == '"') return true;
                if (c == '\\' && get() < 0) return false;
            }
            return false;
        }
        if (c == '[' || c == '{') {
            int close = c == '[' ? ']' : '}';
            space(); if (peek() == close) { get(); return true; }
            while (true) {
                if (!value(depth + 1)) return false;
                space(); c = get();
                if (c == close) return true;
                if (c != ',' && c != ':') return false;
            }
        }
        while ((c = peek()) >= 0 && c != ',' && c != ']' && c != '}' &&
               c != ' ' && c != '\r' && c != '\n' && c != '\t') get();
        return true;
    }
};
template<class Input> bool ranges(Input &f, uint32_t at, uint32_t length, const char *key,
            size_t maxEntry, Ranges &out) {
    out.clear(); if (!f.seek(at)) return false;
    Cursor c{f, at + length}; c.space(); if (c.get() != '{') return false;
    bool found = false;
    while (true) {
        c.space(); if (c.peek() == '}') { c.get(); return true; }
        uint32_t begin = c.position();
        if (c.peek() != '"' || !c.value()) return false;
        uint32_t finish = c.position();
        if (finish - begin > 128) return false;
        JsonBudget keyBudget(8192); JsonDocument kd(&keyBudget); Reader r(f, begin, finish - begin);
        if (deserializeJson(kd, r) || !kd.is<const char *>()) return false;
        bool wanted = !strcmp(kd.as<const char *>(), key);
        c.seek(finish); c.space(); if (c.get() != ':') return false; c.space();
        if (wanted) {
            if (found || c.get() != '[') return false;
            found = true; c.space();
            if (c.peek() == ']') c.get();
            else while (true) {
                c.space(); begin = c.position();
                if (!c.value() || c.position() - begin > maxEntry || out.size() >= 8) return false;
                out.push_back({begin, c.position() - begin});
                c.space(); int sep = c.get();
                if (sep == ']') break;
                if (sep != ',') return false;
            }
        } else if (!c.value()) return false;
        c.space(); int sep = c.get();
        if (sep == '}') return true;
        if (sep != ',') return false;
    }
}


} // namespace





















void BundleStore::clearFonts() {
    renderer.tplFontClearAssets();
    for (uint8_t i = 0; i < fontCount; ++i) free(fontBytes[i]);
    memset(fontBytes, 0, sizeof(fontBytes)); fontCount = 0;
    fontsLoaded = false;
}

size_t BundleStore::fileSize(const char *path) {
    File f = fs.open(path, "r"); size_t n = f ? f.size() : 0; f.close(); return n;
}

bool BundleStore::removeIfPresent(const char *path) { return !fs.exists(path) || fs.remove(path); }

bool BundleStore::readMeta(const char *path, JsonDocument &doc) {
    File f = fs.open(path, "r");
    uint8_t h[12];
    if (!f || f.size() < sizeof(h) || f.read(h, sizeof(h)) != sizeof(h) ||
        memcmp(h, "BCS2", 4) || le32(h + 4) > META_LIMIT ||
        f.size() != sizeof(h) + le32(h + 4)) return false;
    uint32_t crc;
    if (!crcRange(f, 12, le32(h + 4), crc) || crc != le32(h + 8)) return false;
    Reader reader(f, 12, le32(h + 4));
    return !deserializeJson(doc, reader) && doc["version"] == 2;
}

bool BundleStore::writeMeta() {
    JsonBudget docBudget(8192); JsonDocument doc(&docBudget);
    doc["version"] = 2; doc["seq"] = sequence;
    doc["length"] = contentLength; doc["crc"] = contentCrc;
    doc["payload_offset"] = payloadOffset; doc["payload_length"] = payloadLength;
    doc["payload_crc"] = payloadCrc; doc["cache_offset"] = cacheOffset;
    doc["initial"] = profile.initial; doc["job"] = profile.jobId;
    doc["context"] = profile.contextId; doc["firmware"] = profile.firmwareTarget;
    doc["render"] = profile.renderTarget;
    doc["owner"] = receipt.owner; doc["request"] = receipt.request;
    doc["receipt_context"] = receipt.context;
    if (activation) {
        JsonObject a = doc["activation"].to<JsonObject>();
        a["owner"] = activation->owner; a["request"] = activation->request;
        a["template"] = activation->templateId; a["expected"] = activation->expected;
        a["context"] = activation->context;
    }
    for (uint8_t i = 0; i < profile.count; ++i) doc["ids"].add(profile.ids[i]);
    if (doc.overflowed()) return false;
    size_t expected = measureJson(doc);
    if (expected > META_LIMIT) return false;
    String body;
    if (serializeJson(doc, body) != expected || body.length() != expected) return false;
    uint8_t h[12]; memcpy(h, "BCS2", 4); put32(h + 4, body.length());
    put32(h + 8, v2Crc32((const uint8_t *)body.c_str(), body.length()));
    File f = fs.open(META_PENDING, "w");
    if (!f) return false;
    bool ok = write(f, h, sizeof(h)) && write(f, body.c_str(), body.length());
    f.close();
    JsonBudget checkBudget(8192); JsonDocument check(&checkBudget);
    // This small atomic activation/receipt is not another content copy.
    if (!ok || !readMeta(META_PENDING, check) || !fs.rename(META_PENDING, META)) return false;
    check.clear();
    if (!readMeta(META, check)) return false;
    String actual;
    return serializeJson(check, actual) == body.length() && actual == body;
}

bool BundleStore::loadMeta(JsonDocument &d) {
    JsonArray ids = d["ids"].as<JsonArray>();
    if (ids.isNull() || ids.size() < 1 || ids.size() > BS_MAX_TEMPLATES ||
        !d["initial"].is<uint8_t>() || d["initial"].as<unsigned>() >= ids.size()) return false;
    profile = BsProfile{};
    profile.count = ids.size(); profile.initial = d["initial"];
    if (!textField(d["job"], profile.jobId, sizeof(profile.jobId)) ||
        !textField(d["context"], profile.contextId, sizeof(profile.contextId)) ||
        !textField(d["firmware"], profile.firmwareTarget, sizeof(profile.firmwareTarget)) ||
        !textField(d["render"], profile.renderTarget, sizeof(profile.renderTarget))) return false;
    for (size_t i = 0; i < ids.size(); ++i)
        if (!textField(ids[i], profile.ids[i], BS_ID_LEN)) return false;
    for (const char *key : {"seq", "length", "crc", "payload_offset", "payload_length", "payload_crc", "cache_offset"})
        if (!d[key].is<uint32_t>()) return false;
    sequence = d["seq"]; contentLength = d["length"]; contentCrc = d["crc"];
    payloadOffset = d["payload_offset"]; payloadLength = d["payload_length"];
    payloadCrc = d["payload_crc"]; cacheOffset = d["cache_offset"];
    receipt = BsReceipt{};
    if (!textField(d["owner"], receipt.owner, sizeof(receipt.owner), true) ||
        !textField(d["request"], receipt.request, sizeof(receipt.request), true) ||
        !textField(d["receipt_context"], receipt.context, sizeof(receipt.context), true)) return false;
    receipt.length = payloadLength; receipt.crc = payloadCrc;
    activation.reset();
    if (!d["activation"].isNull()) {
        activation.reset(new (std::nothrow) BsActivationReceipt);
        if (!activation) return false;
        JsonObject a = d["activation"].as<JsonObject>();
        if (!textField(a["owner"], activation->owner, sizeof(activation->owner)) ||
            !textField(a["request"], activation->request, sizeof(activation->request)) ||
            !textField(a["template"], activation->templateId, sizeof(activation->templateId)) ||
            !textField(a["expected"], activation->expected, sizeof(activation->expected)) ||
            !textField(a["context"], activation->context, sizeof(activation->context))) return false;
    }
    if (!payloadLength || uint64_t(payloadOffset) + payloadLength > contentLength ||
        uint64_t(cacheOffset) + profile.count * (BS_ID_LEN + 4 + 12 + renderer.tplCtSize()) > contentLength) return false;
    profile.bundleCrc = contentCrc;
    File f = fs.open(CONTENT, "r"); uint32_t crc;
    return f && f.size() == contentLength && crcRange(f, 0, contentLength, crc) && crc == contentCrc;
}

bool BundleStore::migrateLegacy() {
    uint8_t best[138] = {}; bool found = false;
    for (int i = 2; i < 4; ++i) {
        File f = fs.open(LEGACY[i], "r"); uint8_t r[138] = {};
        if (!f || f.size() != sizeof(r) || f.read(r, sizeof(r)) != sizeof(r) ||
            le32(r) != 0x314d4253 || r[8] > 1 || !r[10] ||
            le32(r + 134) != v2Crc32(r, 134)) continue;
        if (!found || le32(r + 4) > le32(best + 4)) { memcpy(best, r, sizeof(r)); found = true; }
    }
    if (!found) return false;
    File f = fs.open(LEGACY[best[8]], "r"); uint8_t h[130]; uint32_t crc;
    if (!f || f.size() != le32(best + 12) || !crcRange(f, 0, f.size(), crc) ||
        crc != le32(best + 16) || !f.seek(0) || f.read(h, sizeof(h)) != sizeof(h) ||
        le32(h) != 0x31424143 || h[5] || !h[4] || h[4] > BS_MAX_TEMPLATES ||
        !(h[7] & 1)) return false;
    profile = BsProfile{};
    profile.count = h[4]; profile.initial = best[11] & 0x7f;
    if (profile.initial >= profile.count || !memchr(h + 8, 0, 17) ||
        !memchr(best + 37, 0, 33) || !memchr(h + 58, 0, 32) || !memchr(h + 90, 0, 32)) return false;
    memcpy(profile.jobId, h + 8, 17); memcpy(profile.contextId, best + 37, 33);
    memcpy(profile.firmwareTarget, h + 58, 32); memcpy(profile.renderTarget, h + 90, 32);
    cacheOffset = 130;
    for (uint8_t i = 0; i < profile.count; ++i) {
        uint8_t len[4];
        if (f.read((uint8_t *)profile.ids[i], BS_ID_LEN) != BS_ID_LEN ||
            !memchr(profile.ids[i], 0, BS_ID_LEN) || f.read(len, 4) != 4 ||
            le32(len) != 12 + renderer.tplCtSize() || !f.seek(f.position() + le32(len))) return false;
    }
    contentLength = f.size(); contentCrc = crc;
    payloadLength = le32(h + 126); payloadCrc = le32(h + 122);
    payloadOffset = f.position(); sequence = le32(best + 4);
    if (!payloadLength || uint64_t(payloadOffset) + payloadLength != contentLength) return false;
    f.close();
    if (!fs.rename(LEGACY[best[8]], CONTENT) || !writeMeta()) return false;
    for (auto path : LEGACY) removeIfPresent(path);
    return true;
}

bool BundleStore::decodeFont(JsonObject entry, uint8_t *&bytes, size_t &len, String &err) {
    bytes = nullptr; len = 0;
    const char *name = entry["name"] | "", *id = entry["font_id"] | "", *data = entry["data"] | "";
    size_t chars = strlen(data);
    if (!*name || !*id || !chars || chars > ((FONT_ASSET_MAX_BYTES + 2) / 3) * 4) {
        err = "font_shape"; return false;
    }
    size_t cap = chars / 4 * 3 + 3; bytes = (uint8_t *)malloc(cap);
    if (!bytes) { err = "font_oom"; return false; }
    FontAssetInfo info;
    if (mbedtls_base64_decode(bytes, cap, &len, (const unsigned char *)data, chars) ||
        !fontAssetValidate(bytes, len, info, err) || !fontAssetMatchesTarget(info, err) ||
        strcmp(info.name, name) || strcmp(info.id, id) || renderer.tplFontIndexByName(name) < renderer.tplFontFixedCount()) {
        free(bytes); bytes = nullptr; if (!err.length()) err = "font_identity"; return false;
    }
    return true;
}

bool BundleStore::loadFonts(String &err) {
    if (fontsLoaded) return true;
    clearFonts();
    File file = fs.open(CONTENT, "r"); Ranges entries;
    BundlePayload f(file);
    if (!file || !f.begin(payloadOffset, payloadLength, fs.totalBytes()) ||
        !ranges(f, 0, f.size(), "fonts", 65536 + 1024, entries)) {
        err = "font_shape"; return false;
    }
    int names[8] = {};
    for (const auto &entry : entries) {
        JsonBudget budget(80 * 1024);
        JsonDocument doc(&budget); Reader reader(f, entry.at, entry.length);
        if (deserializeJson(doc, reader)) { err = "font_json"; clearFonts(); return false; }
        int name = renderer.tplFontIndexByName(doc["name"] | "");
        for (uint8_t i = 0; i < fontCount; ++i)
            if (names[i] == name) { err = "font_duplicate"; clearFonts(); return false; }
        uint8_t *bytes; size_t len;
        if (!decodeFont(doc.as<JsonObject>(), bytes, len, err)) { clearFonts(); return false; }
        doc.clear(); // release base64 before binding the next asset
        if (!renderer.tplFontBindAsset(bytes, len)) {
            free(bytes); clearFonts(); err = "font_bind"; return false;
        }
        names[fontCount] = name; fontBytes[fontCount++] = bytes;
    }
    fontsLoaded = true; return true;
}

const char *BundleStore::bsReceivePath() { return INCOMING; }

uint32_t BundleStore::bsDecodedLimit() { return std::min<uint64_t>(fs.totalBytes(), UINT32_MAX / 2); }

uint32_t BundleStore::bsCompressionProtocol() {
#if defined(CODEX_TARGET_NOTE4) || defined(CODEX_BUNDLE_HOST)
    return 1;
#else
    return 0;
#endif
}

bool BundleStore::bsBegin() {
    activation.reset();
    clearFonts(); configured = false; profile = BsProfile{};
    receipt = BsReceipt{}; incomingReceipt = BsReceipt{}; sequence = 0;
    if (!mounted) mounted = fs.begin(false, "/littlefs", 10, "storage");
    if (!mounted) { lastError = "fs"; return false; }
    if (!fs.exists("/bundle") && !fs.mkdir("/bundle")) { lastError = "mkdir"; return false; }
    if (!fs.exists(CONTENT)) migrateLegacy();
    JsonBudget docBudget(8192); JsonDocument doc(&docBudget);
    configured = readMeta(META, doc) && loadMeta(doc);
    if (!configured) {
        profile = BsProfile{};
        lastError = fs.exists(META) || fs.exists(CONTENT) ? "restore_failed" : "unconfigured";
    }
    return configured;
}

size_t BundleStore::bsFreeBytes() {
    size_t total = fs.totalBytes(), used = fs.usedBytes();
    return mounted && used <= total && total - used > RESERVE ? total - used - RESERVE : 0;
}

size_t BundleStore::bsMaxBundleBytes() {
    if (!mounted) return 0;
    // Only abandoned download bytes may be reclaimed before transfer. Current
    // content stays live. Reserve cache space too: no logical-size claim about
    // how many physical blocks deleting the current file will free.
    uint64_t reclaim = fileSize(INCOMING);
    uint64_t total = fs.totalBytes(), used = fs.usedBytes();
    // Advisory; begin checks actual free space after retiring the old download.
    uint64_t overhead = RESERVE + BS_MAX_TEMPLATES * (BS_ID_LEN + 4 + 12 + renderer.tplCtSize());
    uint64_t available = total > used ? total - used : 0;
    available += reclaim;
    return available > overhead ? size_t(std::min<uint64_t>(available - overhead, UINT32_MAX / 2)) : 0;
}

bool BundleStore::bsStartReceive(uint32_t length, String &err, const char *owner, const char *request) {
    if (!mounted) { err = "fs"; return false; }
    if (!length || length > bsMaxBundleBytes()) { err = "size"; return false; }
    if (!owner || !request || strlen(owner) >= sizeof(receipt.owner) ||
        strlen(request) >= sizeof(receipt.request)) { err = "identity"; return false; }
    if (!removeIfPresent(INCOMING)) { err = "reclaim"; return false; }
    File f = fs.open(INCOMING, "w");
    if (!f) { err = "open"; return false; }
    f.close();
    if (uint64_t(length) + BS_MAX_TEMPLATES * (BS_ID_LEN + 4 + 12 + renderer.tplCtSize()) > bsFreeBytes()) {
        err = "space"; return false;
    }
    incomingReceipt = BsReceipt{};
    strcpy(incomingReceipt.owner, owner); strcpy(incomingReceipt.request, request);
    return true;
}

bool BundleStore::bsInstall(File &f, uint32_t length, uint32_t crc, const char *fw,
               const char *rt, const char *context, String &err, void (*beforeReplace)()) {
    if (!f || f.size() != length || fileSize(INCOMING) != length) { err = "receive"; return false; }
    uint32_t actual;
    if (!crcRange(f, 0, length, actual) || actual != crc) { err = "crc"; return false; }
    BundlePayload payload(f);
    if (!payload.begin(0, length, fs.totalBytes()) || !payload.verify()) { err = "compression"; return false; }
    JsonBudget metadataBudget(8192);
    JsonBudget filterBudget(8192); JsonDocument filter(&filterBudget), doc(&metadataBudget);
    for (const char *key : {"job_id", "firmware_target", "render_target", "compiler_abi"}) filter[key] = true;
    filter["profile"]["template_ids"] = true; filter["profile"]["initial_active_id"] = true;
    Reader reader(payload, 0, payload.size());
    if (deserializeJson(doc, reader, DeserializationOption::Filter(filter))) { err = "json"; return false; }
    BsProfile next{};
    if (!textField(doc["job_id"], next.jobId, sizeof(next.jobId)) ||
        !textField(doc["firmware_target"], next.firmwareTarget, sizeof(next.firmwareTarget)) ||
        !textField(doc["render_target"], next.renderTarget, sizeof(next.renderTarget)) ||
        !context || !*context || strlen(context) >= sizeof(next.contextId)) { err = "identity"; return false; }
    strcpy(next.contextId, context);
    if ((fw && *fw && strcmp(fw, next.firmwareTarget)) ||
        (rt && *rt && strcmp(rt, next.renderTarget))) { err = "target"; return false; }
    if (doc["compiler_abi"] != CT_ABI) { err = "abi"; return false; }
    JsonArray order = doc["profile"]["template_ids"].as<JsonArray>();
    const char *initial = doc["profile"]["initial_active_id"] | "";
    if (order.isNull() || order.size() < 1 || order.size() > BS_MAX_TEMPLATES) { err = "shape"; return false; }
    bool activeFound = false; next.count = order.size();
    for (size_t i = 0; i < order.size(); ++i) {
        if (!textField(order[i], next.ids[i], BS_ID_LEN)) { err = "id"; return false; }
        for (size_t j = 0; j < i; ++j)
            if (!strcmp(next.ids[j], next.ids[i])) { err = "duplicate"; return false; }
        if (!strcmp(initial, next.ids[i])) { next.initial = i; activeFound = true; }
    }
    if (!activeFound) { err = "initial"; return false; }
    doc.clear();
    Ranges entries;
    // Template includes a fixed compiled hex cache plus a bounded source.
    if (!ranges(payload, 0, payload.size(), "templates", TEMPLATE_LIMIT + 2 * (renderer.tplCtSize() + 12) + 4096, entries) ||
        entries.size() != next.count) { err = "shape"; return false; }
    size_t cacheSize = next.count * (BS_ID_LEN + 4 + 12 + renderer.tplCtSize());
    if (cacheSize > bsFreeBytes()) { err = "space"; return false; }
    payload.release();
    // Transfer, CRC, target and shape checks have completed. Only NOW stop the
    // application. Rename the download into place; never copy the whole body.
    if (replaceGuard && !replaceGuard(replaceContext)) { err = "ui_busy"; return false; }
    if (beforeReplace) beforeReplace();
    if (!removeIfPresent(META) || !removeIfPresent(META_PENDING)) { err = "invalidate"; return false; }
    configured = false; clearFonts(); profile = BsProfile{};
    for (auto path : LEGACY) if (!removeIfPresent(path)) { err = "reclaim"; return false; }
    f.close();
    if (!removeIfPresent(CONTENT) || !fs.rename(INCOMING, CONTENT)) { err = "replace"; return false; }
    f = fs.open(CONTENT, "r");
    if (!f) { err = "open"; return false; }
    receipt = incomingReceipt;
    payloadOffset = 0; payloadLength = length; payloadCrc = crc; cacheOffset = length;
    // Validate and retain the new font set once, with old assets already released.
    if (!loadFonts(err)) return false;
    if (!payload.begin(0, length, fs.totalBytes())) { err = "compression"; clearFonts(); return false; }
    File out = fs.open(CONTENT, "a");
    if (!out) { err = "open"; clearFonts(); return false; }
    // One scratch record is shared by the serialized installer, never on task stack.
    std::unique_ptr<CtTemplate> compiledStorage(new (std::nothrow) CtTemplate);
    if (!compiledStorage) { err = "oom"; clearFonts(); return false; }
    CtTemplate &compiled = *compiledStorage;
    for (size_t i = 0; i < entries.size(); ++i) {
        JsonBudget templateBudget(128 * 1024);
        JsonBudget filterBudget(8192); JsonDocument tf(&filterBudget), td(&templateBudget);
        tf["key"] = true; tf["source"] = true;
        Reader tr(payload, entries[i].at, entries[i].length);
        if (deserializeJson(td, tr, DeserializationOption::Filter(tf)) ||
            strcmp(td["key"]["template_id"] | "", next.ids[i]) ||
            strcmp(td["key"]["render_target"] | "", next.renderTarget) || td["source"].isNull()) {
            err = "template"; clearFonts(); return false;
        }
        String source;
        if (measureJson(td["source"]) > TEMPLATE_LIMIT) { err = "source_size"; clearFonts(); return false; }
        serializeJson(td["source"], source); td.clear();
        if (!renderer.tplCompile(source, compiled, err) || strcmp(compiled.id, next.ids[i])) {
            if (!err.length()) err = "compile_id"; clearFonts(); return false;
        }
        source = "";
        size_t written = 0, cap = renderer.tplCtSize() + 12;
        uint8_t *blob = (uint8_t *)malloc(cap);
        if (!blob) { err = "oom"; clearFonts(); return false; }
        uint8_t n[4]; put32(n, cap);
        bool ok = renderer.tplCtSerialize(compiled, blob, cap, written) && written == cap &&
            write(out, next.ids[i], BS_ID_LEN) && write(out, n, 4) && write(out, blob, written);
        free(blob);
        if (!ok) { err = "write"; clearFonts(); return false; }
    }
    out.close();
    contentLength = length + cacheSize;
    File check = fs.open(CONTENT, "r");
    uint32_t readPayload;
    if (!check || check.size() != contentLength || !crcRange(check, 0, length, readPayload) ||
        readPayload != crc || !crcRange(check, 0, contentLength, contentCrc)) { err = "readback"; clearFonts(); return false; }
    // Validate every compiled record from flash before publishing the commit marker.
    for (size_t i = 0; i < entries.size(); ++i) {
        size_t cap = renderer.tplCtSize() + 12; auto *blob = (uint8_t *)malloc(cap);
        if (!blob) { err = "oom"; clearFonts(); return false; }
        uint32_t at = cacheOffset + i * (BS_ID_LEN + 4 + cap) + BS_ID_LEN + 4;
        bool ok = check.seek(at) && check.read(blob, cap) == cap && renderer.tplCtDeserialize(blob, cap, compiled, err) &&
            !strcmp(compiled.id, next.ids[i]);
        free(blob);
        if (!ok) { err = "readback"; clearFonts(); return false; }
    }
    profile = next; profile.bundleCrc = contentCrc; ++sequence;
    receipt.length = length; receipt.crc = crc; strcpy(receipt.context, context);
    if (!writeMeta()) { profile = BsProfile{}; err = "commit"; clearFonts(); return false; }
    configured = true; return true;
}

bool BundleStore::bsInstall(const String &body, const char *fw, const char *rt, const char *ctx, String &err) {
    if (!bsStartReceive(body.length(), err)) return false;
    File f = fs.open(INCOMING, "w");
    bool ok = f && write(f, body.c_str(), body.length()); f.close();
    if (!ok) { err = "write"; return false; }
    f = fs.open(INCOMING, "r");
    return bsInstall(f, body.length(), v2Crc32((const uint8_t *)body.c_str(), body.length()), fw, rt, ctx, err);
}

bool BundleStore::bsLoadCompiled(uint8_t index, CtTemplate &out, String &err) {
    if (!configured || index >= profile.count) { err = "unconfigured"; return false; }
    if (!loadFonts(err)) return false;
    File f = fs.open(CONTENT, "r");
    size_t cap = 12 + renderer.tplCtSize();
    uint32_t at = cacheOffset + index * (BS_ID_LEN + 4 + cap);
    char id[BS_ID_LEN]; uint8_t len[4];
    if (!f || !f.seek(at) || f.read((uint8_t *)id, sizeof(id)) != sizeof(id) ||
        !memchr(id, 0, sizeof(id)) || strcmp(id, profile.ids[index]) ||
        f.read(len, 4) != 4 || le32(len) != cap) { err = "cache"; return false; }
    uint8_t *blob = (uint8_t *)malloc(cap);
    if (!blob) { err = "oom"; return false; }
    bool ok = f.read(blob, cap) == cap && renderer.tplCtDeserialize(blob, cap, out, err);
    free(blob); return ok;
}

bool BundleStore::bsSetActive(uint8_t index, const char *context, String &err, const BsActivationReceipt *transaction) {
    if (!configured || index >= profile.count || !context || !*context || strlen(context) >= BS_CTX_LEN) { err = "index"; return false; }
    std::unique_ptr<BsActivationReceipt> replacement;
    if (transaction) {
        replacement.reset(new (std::nothrow) BsActivationReceipt(*transaction));
        if (!replacement) { err = "oom"; return false; }
        activation.swap(replacement);
    }
    auto previous = profile; uint32_t seq = sequence;
    profile.initial = index; strcpy(profile.contextId, context); ++sequence;
    if (!writeMeta()) {
        profile = previous; sequence = seq;
        if (transaction) activation.swap(replacement);
        err = "commit"; return false;
    }
    return true;
}

bool BundleStore::bsConfigured() { return configured; }

bool BundleStore::bsReceipt(BsReceipt &out) {
    if (!configured || !*receipt.owner || !*receipt.request) return false;
    out = receipt; return true;
}

bool BundleStore::bsProfile(BsProfile &out) { if (!configured) return false; out = profile; return true; }

bool BundleStore::bsActiveJobPayload(const char *job, uint32_t &crc) {
    if (!configured || !job || strcmp(job, profile.jobId)) return false;
    crc = payloadCrc; return true;
}

uint32_t BundleStore::bsCommitSeq() { return sequence; }

const char *BundleStore::bsLastError() { return lastError.c_str(); }

bool BundleStore::bsHasSource(uint8_t index) { return configured && index < profile.count && payloadLength; }

bool BundleStore::bsRecoveryDigest(String &out) {
    JsonBudget diagnosticBudget(8192); JsonDocument d(&diagnosticBudget); d["configured"] = configured;
    if (configured) {
        d["active_template_id"] = profile.ids[profile.initial]; d["context_id"] = profile.contextId;
        d["job_id"] = profile.jobId; d["commit_seq"] = sequence;
        for (uint8_t i = 0; i < profile.count; ++i) { d["template_ids"].add(profile.ids[i]); d["sources"].add(true); }
    }
    out = ""; serializeJson(d, out); return configured;
}

#if !defined(CODEX_AP01_NATIVE)
BundleStore &bsDefaultStore() {
    static BundleStore store(LittleFS, tplDefaultRenderer());
    return store;
}
const char * bsReceivePath() { return bsDefaultStore().bsReceivePath(); }
uint32_t bsDecodedLimit() { return bsDefaultStore().bsDecodedLimit(); }
uint32_t bsCompressionProtocol() { return bsDefaultStore().bsCompressionProtocol(); }
bool bsBegin() { return bsDefaultStore().bsBegin(); }
size_t bsFreeBytes() { return bsDefaultStore().bsFreeBytes(); }
size_t bsMaxBundleBytes() { return bsDefaultStore().bsMaxBundleBytes(); }
bool bsStartReceive(uint32_t length, String &err, const char *owner, const char *request) { return bsDefaultStore().bsStartReceive(length, err, owner, request); }
bool bsInstall(File &f, uint32_t length, uint32_t crc, const char *fw,
               const char *rt, const char *context, String &err, void (*beforeReplace)()) { return bsDefaultStore().bsInstall(f, length, crc, fw, rt, context, err, beforeReplace); }
bool bsInstall(const String &body, const char *fw, const char *rt, const char *ctx, String &err) { return bsDefaultStore().bsInstall(body, fw, rt, ctx, err); }
bool bsLoadCompiled(uint8_t index, CtTemplate &out, String &err) { return bsDefaultStore().bsLoadCompiled(index, out, err); }
bool bsSetActive(uint8_t index, const char *context, String &err) { return bsDefaultStore().bsSetActive(index, context, err); }
bool bsConfigured() { return bsDefaultStore().bsConfigured(); }
bool bsReceipt(BsReceipt &out) { return bsDefaultStore().bsReceipt(out); }
bool bsProfile(BsProfile &out) { return bsDefaultStore().bsProfile(out); }
bool bsActiveJobPayload(const char *job, uint32_t &crc) { return bsDefaultStore().bsActiveJobPayload(job, crc); }
uint32_t bsCommitSeq() { return bsDefaultStore().bsCommitSeq(); }
const char * bsLastError() { return bsDefaultStore().bsLastError(); }
bool bsHasSource(uint8_t index) { return bsDefaultStore().bsHasSource(index); }
bool bsRecoveryDigest(String &out) { return bsDefaultStore().bsRecoveryDigest(out); }
#endif
