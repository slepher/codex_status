// Host shim for the single mbedtls call used by the template engine.
#pragma once

#include <cstddef>
#include <cstring>

inline int mbedtls_base64_decode(unsigned char *dst, size_t dlen, size_t *olen,
                                 const unsigned char *src, size_t slen) {
    static int table[256];
    static bool ready = false;
    if (!ready) {
        for (int i = 0; i < 256; i++) table[i] = -1;
        const char *alphabet =
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        for (int i = 0; i < 64; i++) table[(unsigned char)alphabet[i]] = i;
        ready = true;
    }
    size_t out = 0;
    unsigned buf = 0;
    int bits = 0;
    bool pad = false;
    for (size_t i = 0; i < slen; i++) {
        unsigned char c = src[i];
        if (c == '=') {
            pad = true;
            continue;
        }
        if (c == '\r' || c == '\n' || c == ' ' || c == '\t') continue;
        int v = table[c];
        if (v < 0) return -1;
        if (pad) return -1;
        buf = (buf << 6) | (unsigned)v;
        bits += 6;
        if (bits >= 8) {
            bits -= 8;
            if (out >= dlen) return -1;
            dst[out++] = (unsigned char)((buf >> bits) & 0xFF);
        }
    }
    if (olen) *olen = out;
    return 0;
}
