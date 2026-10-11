#include "bundle_payload.h"
#include "v2_state.h"
#include <algorithm>
#include <new>
#include <string.h>
#ifndef CODEX_BUNDLE_HOST
#include <miniz.h>
#else
extern "C" bool codex_bundle_inflate(uint8_t *, size_t, const uint8_t *, size_t);
#endif
namespace {
uint32_t u32(const uint8_t *b) { return uint32_t(b[0]) | uint32_t(b[1])<<8 | uint32_t(b[2])<<16 | uint32_t(b[3])<<24; }
}
BundlePayload::~BundlePayload() { release(); }
void BundlePayload::release() {
    offsets.reset(); input.reset(); output.reset(); free(decoder); decoder = nullptr;
    valid = packed = false; cached = UINT32_MAX; pos = 0;
}
bool BundlePayload::begin(uint32_t at, uint32_t bytes, uint32_t decodedLimit) {
    release(); base = at; wire = bytes; length = bytes;
    if (!bytes || uint64_t(at) + bytes > file.size() || !file.seek(at)) return false;
    uint8_t h[24] = {};
    size_t got = file.read(h, std::min<uint32_t>(bytes, sizeof(h)));
    if (got < 4 || memcmp(h, "CSZ1", 4)) { valid = bytes <= decodedLimit; return valid; }
    packed = true;
    length = u32(h + 4); crc = u32(h + 8); blocks = u32(h + 16);
    if (got != sizeof(h) || !length || length > decodedLimit || u32(h + 12) != BLOCK ||
        u32(h + 20) || blocks != (uint64_t(length) + BLOCK - 1) / BLOCK ||
        uint64_t(24) + uint64_t(blocks) * 4 >= bytes) return false;
    offsets.reset(new (std::nothrow) uint32_t[blocks + 1]);
    if (!offsets) return false;
    uint64_t end = 24 + uint64_t(blocks) * 4;
    for (uint32_t i = 0; i < blocks; ++i) {
        uint8_t n[4];
        if (file.read(n, 4) != 4 || !u32(n) || u32(n) > MAX_PACKED) return false;
        offsets[i] = uint32_t(end); end += u32(n);
        if (end > bytes) return false;
    }
    if (end != bytes) return false;
    offsets[blocks] = bytes;
    input.reset(new (std::nothrow) uint8_t[MAX_PACKED]);
    output.reset(new (std::nothrow) uint8_t[BLOCK]);
#ifndef CODEX_BUNDLE_HOST
    decoder = malloc(sizeof(tinfl_decompressor));
    if (!decoder) return false;
#endif
    valid = bool(input) && bool(output); return valid;
}
bool BundlePayload::load(uint32_t index) {
    if (cached == index) return true;
    size_t in = offsets[index + 1] - offsets[index];
    size_t out = std::min<uint32_t>(BLOCK, length - index * BLOCK);
    if (!file.seek(base + offsets[index]) || file.read(input.get(), in) != in) return false;
#ifdef CODEX_BUNDLE_HOST
    bool ok = codex_bundle_inflate(output.get(), out, input.get(), in);
#else
    auto *work = static_cast<tinfl_decompressor *>(decoder); tinfl_init(work);
    size_t consumed = in, written = out;
    bool ok = tinfl_decompress(work, input.get(), &consumed, output.get(), output.get(), &written,
        TINFL_FLAG_PARSE_ZLIB_HEADER | TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF) == TINFL_STATUS_DONE &&
        consumed == in && written == out;
#endif
    if (!ok) { valid = false; return false; }
    cached = index; return true;
}
size_t BundlePayload::read(uint8_t *dst, size_t n) {
    if (!valid) return 0;
    n = std::min<size_t>(n, length - pos);
    if (!packed) {
        if (file.position() != base + pos && !file.seek(base + pos)) return 0;
        size_t got = file.read(dst, n); pos += got; return got;
    }
    size_t done = 0;
    while (done < n) {
        if (!load(pos / BLOCK)) break;
        size_t take = std::min<size_t>(n - done, BLOCK - pos % BLOCK);
        memcpy(dst + done, output.get() + pos % BLOCK, take); pos += take; done += take;
    }
    return done;
}
int BundlePayload::read() { uint8_t c; return read(&c, 1) == 1 ? c : -1; }
bool BundlePayload::verify() {
    if (!valid) return false;
    if (!packed) return seek(0);
    uint32_t state = v2Crc32Start();
    for (uint32_t i = 0; i < blocks; ++i) {
        if (!load(i)) return false;
        state = v2Crc32Update(state, output.get(), std::min<uint32_t>(BLOCK, length - i * BLOCK));
    }
    return v2Crc32Finish(state) == crc && seek(0);
}
