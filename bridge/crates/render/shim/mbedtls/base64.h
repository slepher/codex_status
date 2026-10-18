// Host shim for the single mbedtls call used by the template engine.
#pragma once

#include <cstddef>
#include <cstring>

inline int mbedtls_base64_decode(unsigned char *dst, size_t dlen, size_t *olen,
                                 const unsigned char *src, size_t slen) {
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
        int v = c >= 'A' && c <= 'Z' ? c - 'A'
              : c >= 'a' && c <= 'z' ? c - 'a' + 26
              : c >= '0' && c <= '9' ? c - '0' + 52
              : c == '+' ? 62 : c == '/' ? 63 : -1;
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
