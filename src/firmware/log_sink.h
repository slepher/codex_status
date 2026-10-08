#pragma once
#include <stdint.h>
#include <stddef.h>

class String;

namespace firmware::sync {
// Device log storage is disabled until the retained diagnostic ring is ready.
void appendLog(const char *text, size_t count, uint8_t flags);
String dumpLog();
}
