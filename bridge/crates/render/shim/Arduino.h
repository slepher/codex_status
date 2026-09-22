// Minimal host shim for the Arduino API surface used by the firmware engine.
#pragma once

#include <cstdarg>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>

#define LOW 0
#define HIGH 1
#define F(x) x
#ifdef _MSC_VER
#define __attribute__(x)
#endif

inline void delay(unsigned long) {}
inline void digitalWrite(int, int) {}
inline int digitalRead(int) { return 0; }

class String {
public:
    String() = default;
    String(const char *s) : s_(s ? s : "") {}
    String(const std::string &s) : s_(s) {}
    String(char c) : s_(1, c) {}
    String(int v) { char b[32]; std::snprintf(b, sizeof(b), "%d", v); s_ = b; }
    String(unsigned int v) { char b[32]; std::snprintf(b, sizeof(b), "%u", v); s_ = b; }
    String(long v) { char b[32]; std::snprintf(b, sizeof(b), "%ld", v); s_ = b; }
    String(unsigned long v) { char b[32]; std::snprintf(b, sizeof(b), "%lu", v); s_ = b; }
    String(long long v) { char b[32]; std::snprintf(b, sizeof(b), "%lld", v); s_ = b; }
    String(unsigned long long v) { char b[32]; std::snprintf(b, sizeof(b), "%llu", v); s_ = b; }

    const char *c_str() const { return s_.c_str(); }
    size_t length() const { return s_.size(); }
    void reserve(size_t n) { s_.reserve(n); }

    String &operator+=(const String &other) { s_ += other.s_; return *this; }
    String &operator+=(const char *other) { if (other) s_ += other; return *this; }
    String &operator+=(char c) { s_ += c; return *this; }

    // ArduinoJson writes through this API when ARDUINOJSON_ENABLE_ARDUINO_STRING
    // is enabled; the engine never serializes into a String, but the template
    // must still compile.
    bool concat(const String &other) { s_ += other.s_; return true; }
    bool concat(const char *other) { if (other) s_ += other; return true; }
    bool concat(const char *data, size_t len) {
        if (data && len) s_.append(data, len);
        return true;
    }
    bool concat(char c) { s_ += c; return true; }

    bool operator==(const String &other) const { return s_ == other.s_; }
    bool operator==(const char *other) const { return s_ == (other ? other : ""); }
    bool operator!=(const String &other) const { return !(*this == other); }
    bool operator!=(const char *other) const { return !(*this == other); }

    bool startsWith(const String &prefix) const { return s_.rfind(prefix.s_, 0) == 0; }
    bool startsWith(const char *prefix) const {
        const char *p = prefix ? prefix : "";
        return s_.rfind(p, 0) == 0;
    }
    int indexOf(char c, size_t from = 0) const {
        auto pos = s_.find(c, from);
        return pos == std::string::npos ? -1 : static_cast<int>(pos);
    }
    int indexOf(const char *needle, size_t from = 0) const {
        auto pos = s_.find(needle ? needle : "", from);
        return pos == std::string::npos ? -1 : static_cast<int>(pos);
    }
    String substring(size_t from) const { return String(s_.substr(from)); }
    String substring(size_t from, size_t to) const {
        if (to <= from) return String();
        return String(s_.substr(from, to - from));
    }
    int toInt() const { return std::atoi(s_.c_str()); }
    char operator[](size_t i) const { return i < s_.size() ? s_[i] : '\0'; }

private:
    std::string s_;
};

inline String operator+(const String &a, const String &b) { String r(a); r += b; return r; }
inline String operator+(const String &a, const char *b) { String r(a); r += b; return r; }
inline String operator+(const char *a, const String &b) { String r(a); r += b; return r; }
inline String operator+(const String &a, char b) { String r(a); r += b; return r; }

class SerialClass {
public:
    void begin(unsigned long) {}
    void print(const char *s) { if (s) std::fputs(s, stderr); }
    void print(const String &s) { std::fputs(s.c_str(), stderr); }
    void print(char c) { std::fputc(c, stderr); }
    void print(int v) { std::fprintf(stderr, "%d", v); }
    void println() { std::fputc('\n', stderr); }
    void println(const char *s) { if (s) std::fputs(s, stderr); std::fputc('\n', stderr); }
    void println(const String &s) { std::fputs(s.c_str(), stderr); std::fputc('\n', stderr); }
    void println(int v) { std::fprintf(stderr, "%d\n", v); }
    void printf(const char *fmt, ...) {
        va_list args;
        va_start(args, fmt);
        std::vfprintf(stderr, fmt, args);
        va_end(args);
    }
};

extern SerialClass Serial;
