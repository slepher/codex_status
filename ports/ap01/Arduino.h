#pragma once
// Native shared-engine surface: owned strings, no iostream/locale/global state.
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <utility>

#define F(x) x
#define LOW 0
#define HIGH 1
class String {
    char *bytes = nullptr;
    size_t count = 0, capacity = 0;
public:
    String() = default;
    String(const char *s) { if (s) concat(s, strlen(s)); }
    String(const char *s, size_t n) { concat(s, n); }
    String(const String &s) { concat(s.c_str(), s.length()); }
    String(String &&s) noexcept : bytes(s.bytes), count(s.count), capacity(s.capacity) {
        s.bytes = nullptr; s.count = s.capacity = 0;
    }
    explicit String(char c) { concat(c); }
    String(int v) : String((long long)v) {}
    String(unsigned v) : String((unsigned long long)v) {}
    String(long v) : String((long long)v) {}
    String(unsigned long v) : String((unsigned long long)v) {}
    String(long long v) { char b[32]; snprintf(b, sizeof(b), "%lld", v); concat(b); }
    String(unsigned long long v) { char b[32]; snprintf(b, sizeof(b), "%llu", v); concat(b); }
    ~String() { free(bytes); }
    String &operator=(String s) { swap(s); return *this; }
    void swap(String &s) noexcept {
        std::swap(bytes, s.bytes); std::swap(count, s.count); std::swap(capacity, s.capacity);
    }
    const char *c_str() const { return bytes ? bytes : ""; }
    size_t length() const { return count; }
    bool reserve(size_t n) {
        if (n <= capacity) return true;
        if (n == SIZE_MAX) return false;
        char *p = (char *)realloc(bytes, n + 1);
        if (!p) return false;
        bytes = p; capacity = n; bytes[count] = 0; return true;
    }
    bool concat(const char *s, size_t n) {
        if (!n) return true;
        if (!s || n > SIZE_MAX - count) return false;
        // A source inside this string must survive a realloc (e.g. s += s).
        uintptr_t source = (uintptr_t)s, start = (uintptr_t)bytes;
        bool internal = bytes && source >= start && source <= start + count;
        size_t offset = internal ? source - start : 0;
        if (internal && n > count - offset) return false;
        if (!reserve(count + n)) return false;
        if (internal) s = bytes + offset;
        memmove(bytes + count, s, n); count += n; bytes[count] = 0; return true;
    }
    bool concat(const char *s) { return !s || concat(s, strlen(s)); }
    bool concat(const String &s) { return concat(s.c_str(), s.length()); }
    bool concat(char c) { return concat(&c, 1); }
    String &operator+=(const String &s) { concat(s); return *this; }
    String &operator+=(const char *s) { concat(s); return *this; }
    String &operator+=(char c) { concat(c); return *this; }
    bool operator==(const String &s) const { return count == s.count && !memcmp(c_str(), s.c_str(), count); }
    bool operator==(const char *s) const { return !strcmp(c_str(), s ? s : ""); }
    bool operator!=(const String &s) const { return !(*this == s); }
    bool operator!=(const char *s) const { return !(*this == s); }
    char operator[](size_t i) const { return i < count ? bytes[i] : 0; }
    int indexOf(char c, size_t from = 0) const {
        for (size_t i = from; i < count; ++i) if (bytes[i] == c) return (int)i;
        return -1;
    }
    int indexOf(const char *s, size_t from = 0) const {
        if (!s || from > count) return -1;
        const char *p = strstr(c_str() + from, s); return p ? int(p - c_str()) : -1;
    }
    bool startsWith(const char *s) const { return s && strlen(s) <= count && !memcmp(c_str(), s, strlen(s)); }
    bool startsWith(const String &s) const { return startsWith(s.c_str()); }
    String substring(size_t from, size_t to) const {
        if (from > count || to <= from) return {};
        if (to > count) to = count;
        return String(c_str() + from, to - from);
    }
    String substring(size_t from) const { return substring(from, count); }
    long toInt() const { return strtol(c_str(), nullptr, 10); }
};
inline String operator+(String a, const String &b) { a += b; return a; }
inline String operator+(String a, const char *b) { a += b; return a; }
inline String operator+(const char *a, const String &b) { String s(a); s += b; return s; }
inline String operator+(String a, char b) { a += b; return a; }

// Paint debug diagnostics have no serial peripheral dependency on AP01.
struct Ap01PaintDiagnostics {
    void print(const char *) const {}
    void printf(const char *, ...) const {}
};
#define Serial Ap01PaintDiagnostics()
