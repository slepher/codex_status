#pragma once
#include "application.h"

struct Ap01Stream {
    void *context;
    // Both calls honor one absolute request deadline. EOF/error/timeout <= 0.
    int (*read)(void *, uint8_t *, size_t, uint64_t deadline);
    int (*write)(void *, const uint8_t *, size_t, uint64_t deadline);
};
// Serial, one request per connection, no chunked framing or keepalive ambiguity.
// Caller closes the connection after this returns. Parsing has no side effects.
bool ap01ServeHttp(Ap01Application &, bool maintenance, const Ap01Stream &,
                   uint64_t now, uint64_t deadline);
