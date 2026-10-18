#pragma once
#include <ArduinoJson.h>
#include <stdlib.h>

// Request-local replacement for ArduinoJson's writable DefaultAllocator
// singleton. A document must be destroyed before this stack object.
class LocalJsonAllocator : public ArduinoJson::Allocator {
public:
    void *allocate(size_t n) override { return malloc(n); }
    void deallocate(void *p) override { free(p); }
    void *reallocate(void *p, size_t n) override { return realloc(p, n); }
};
