#pragma once
#include <LittleFS.h>
#include <memory>
#include <stdint.h>

// Bounded logical JSON view over raw JSON or CSZ1 independent zlib blocks.
// Owns one block's buffers, never a whole decoded Bundle. File remains caller-owned.
class BundlePayload {
    File &file;
    uint32_t base = 0, wire = 0, length = 0, pos = 0, crc = 0, blocks = 0;
    uint32_t cached = UINT32_MAX;
    bool packed = false, valid = false;
    std::unique_ptr<uint32_t[]> offsets;
    std::unique_ptr<uint8_t[]> input, output;
    void *decoder = nullptr;
    bool load(uint32_t index);
public:
    static constexpr uint32_t BLOCK = 16384, MAX_PACKED = BLOCK + 128;
    explicit BundlePayload(File &f) : file(f) {}
    ~BundlePayload();
    bool begin(uint32_t at, uint32_t bytes, uint32_t decodedLimit);
    void release();
    bool verify();
    uint32_t size() const { return length; }
    uint32_t position() const { return pos; }
    bool seek(uint32_t at) { if (!valid || at > length) return false; pos = at; return true; }
    int read();
    size_t read(uint8_t *dst, size_t n);
    size_t readBytes(char *dst, size_t n) { return read((uint8_t *)dst, n); }
};
