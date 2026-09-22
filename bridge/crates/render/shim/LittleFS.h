// In-memory LittleFS surface for executing the real bundle store in host tests.
#pragma once
#include <algorithm>
#include <map>
#include <string>
#include <vector>
#include "Arduino.h"

class File;
class LittleFSClass {
public:
    std::map<std::string, std::vector<uint8_t>> files;
    size_t capacity = 1024 * 1024;
    long long writeBudget = -1; // inject a torn write after exactly N bytes
    bool begin(bool, const char *, int, const char *) { return true; }
    bool exists(const char *p) { return !strcmp(p, "/") || !strcmp(p, "/bundle") || files.count(p); }
    bool mkdir(const char *) { return true; }
    bool remove(const char *p) { return files.erase(p) != 0; }
    size_t totalBytes() const { return capacity; }
    size_t usedBytes() const { size_t n = 0; for (const auto &f : files) n += f.second.size(); return n; }
    File open(const char *path, const char *mode);
};
inline LittleFSClass LittleFS;

class File {
    std::string path;
    size_t pos = 0;
    bool opened = false;
public:
    File() = default;
    File(const char *p, const char *mode) : path(p) {
        if (mode[0] == 'w') LittleFS.files[path].clear();
        if (mode[0] == 'a') pos = LittleFS.files[path].size();
        opened = LittleFS.files.count(path) != 0;
    }
    explicit operator bool() const { return opened; }
    size_t size() const { return opened ? LittleFS.files.at(path).size() : 0; }
    size_t position() const { return pos; }
    bool seek(size_t at) { if (!opened || at > size()) return false; pos = at; return true; }
    int available() const { return opened && pos < size(); }
    size_t read(uint8_t *out, size_t len) {
        if (!opened) return 0;
        len = std::min(len, size() - pos);
        memcpy(out, LittleFS.files[path].data() + pos, len); pos += len; return len;
    }
    int read() { uint8_t b; return read(&b, 1) == 1 ? b : -1; }
    size_t write(const uint8_t *data, size_t len) {
        if (!opened) return 0;
        if (LittleFS.writeBudget >= 0) len = std::min(len, (size_t)LittleFS.writeBudget);
        size_t room = LittleFS.capacity > LittleFS.usedBytes() ? LittleFS.capacity - LittleFS.usedBytes() : 0;
        len = std::min(len, room + size() - pos);
        auto &bytes = LittleFS.files[path];
        if (pos + len > bytes.size()) bytes.resize(pos + len);
        memcpy(bytes.data() + pos, data, len); pos += len;
        if (LittleFS.writeBudget >= 0) LittleFS.writeBudget -= len;
        return len;
    }
    void close() { opened = false; }
};
inline File LittleFSClass::open(const char *path, const char *mode) { return File(path, mode); }
