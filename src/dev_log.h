// Recent device log buffer: tees everything to Serial and keeps a ring for
// `GET /log` so the bridge/agent can read device logs over Wi-Fi.
#pragma once
#include <Arduino.h>

class DevLogger {
public:
    void printf(const char *fmt, ...) __attribute__((format(printf, 2, 3)));
    void println(const char *s);
    void println();
    void print(const char *s);
    // Copy of the ring buffer (may start mid-line when the ring wrapped).
    String dump();

private:
    void append(const char *data, size_t len);
};

extern DevLogger DevLog;
