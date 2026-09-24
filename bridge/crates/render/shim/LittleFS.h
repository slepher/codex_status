// In-memory LittleFS surface for executing the real bundle store and font store
// in host tests.
#pragma once
#include <algorithm>
#include <map>
#include <set>
#include <string>
#include <vector>
#include "Arduino.h"

// The real FS.h constants; the firmware sources use them by name.
#ifndef FILE_READ
#define FILE_READ   "r"
#define FILE_WRITE  "w"
#define FILE_APPEND "a"
#endif

class File;
class LittleFSClass {
public:
    std::map<std::string, std::vector<uint8_t>> files;
    // Directories the firmware created; `mkdir` is otherwise a no-op, which is
    // not enough for the font store (it enumerates and removes directories).
    std::set<std::string> dirs;
    size_t capacity = 1024 * 1024;
    long long writeBudget = -1; // inject a torn write after exactly N bytes
    bool begin(bool, const char *, int, const char *) { return true; }
    bool exists(const char *p) {
        if (!strcmp(p, "/")) return true;
        // The bundle store keeps its own reserved path.
        if (!strcmp(p, "/bundle")) return true;
        if (dirs.count(p) || files.count(p)) return true;
        // A prefix of a known path is a directory even if mkdir was never called.
        std::string prefix = std::string(p) + "/";
        for (const auto &f : files) {
            if (f.first.compare(0, prefix.size(), prefix) == 0) return true;
        }
        return false;
    }
    bool mkdir(const char *p) { dirs.insert(p); return true; }
    bool rmdir(const char *p) {
        std::string prefix = std::string(p) + "/";
        for (auto it = files.begin(); it != files.end();) {
            it = it->first.compare(0, prefix.size(), prefix) == 0 ? files.erase(it) : std::next(it);
        }
        dirs.erase(p);
        return true;
    }
    bool remove(const char *p) { return files.erase(p) != 0; }
    bool rename(const char *from, const char *to);
    size_t totalBytes() const { return capacity; }
    size_t usedBytes() const { size_t n = 0; for (const auto &f : files) n += f.second.size(); return n; }
    File open(const char *path, const char *mode);
};
inline LittleFSClass LittleFS;

class File {
    std::string path;
    size_t pos = 0;
    bool opened = false;
    // Directory iteration state: the children captured when the directory was
    // opened, and how far we have walked them.
    bool isDir = false;
    std::vector<std::pair<std::string, size_t>> children;
    size_t childIndex = 0;
    /// Backing storage for `name()`, mirroring the device's in-object name buffer.
    std::string nameBuf;
public:
    File() = default;
    File(const char *p, const char *mode) : path(p) {
        if (mode[0] == 'w') LittleFS.files[path].clear();
        if (mode[0] == 'a') pos = LittleFS.files[path].size();
        opened = LittleFS.files.count(path) != 0;
        size_t slash = path.find_last_of('/');
        nameBuf = slash == std::string::npos ? path : path.substr(slash + 1);
        if (!opened) {
            std::string prefix = path + "/";
            for (const auto &f : LittleFS.files) {
                if (f.first.compare(0, prefix.size(), prefix) != 0) continue;
                std::string rest = f.first.substr(prefix.size());
                if (rest.find('/') != std::string::npos) continue;   // not a direct child
                children.emplace_back(rest, f.second.size());
            }
            if (!children.empty() || LittleFS.dirs.count(path)) {
                opened = true;
                isDir = true;
            }
        }
    }
    explicit operator bool() const { return opened; }
    bool isDirectory() const { return isDir; }
    size_t size() const { return opened && !isDir ? LittleFS.files.at(path).size() : 0; }
    size_t position() const { return pos; }
    bool seek(size_t at) { if (!opened || at > size()) return false; pos = at; return true; }
    int available() const { return opened && !isDir && pos < size(); }

    /// Name of the current directory entry, or the file's own base name.
    /// Returns `const char *` exactly like the device's `fs::File::name()`, so
    /// firmware code that wraps it in a String compiles in both builds.
    const char *name() const { return nameBuf.c_str(); }

    File openNextFile() {
        if (!isDir || childIndex >= children.size()) return File();
        const std::string &child = children[childIndex].first;
        childIndex++;
        File f((path + "/" + child).c_str(), "r");
        f.nameBuf = child;
        return f;
    }

    size_t read(uint8_t *out, size_t len) {
        if (!opened || isDir) return 0;
        len = std::min(len, size() - pos);
        memcpy(out, LittleFS.files[path].data() + pos, len); pos += len; return len;
    }
    int read() { uint8_t b; return read(&b, 1) == 1 ? b : -1; }
    /// The device's `fs::File` inherits `Stream::readBytes`; ArduinoJson's
    /// generic (non-`Stream`) reader requires the same name here, which is what
    /// the streamed Bundle validation uses on both sides.
    size_t readBytes(char *out, size_t len) { return read((uint8_t *)out, len); }
    size_t write(const uint8_t *data, size_t len) {
        if (!opened || isDir) return 0;
        if (LittleFS.writeBudget >= 0) len = std::min(len, (size_t)LittleFS.writeBudget);
        size_t room = LittleFS.capacity > LittleFS.usedBytes() ? LittleFS.capacity - LittleFS.usedBytes() : 0;
        len = std::min(len, room + size() - pos);
        auto &bytes = LittleFS.files[path];
        if (pos + len > bytes.size()) bytes.resize(pos + len);
        memcpy(bytes.data() + pos, data, len); pos += len;
        if (LittleFS.writeBudget >= 0) LittleFS.writeBudget -= len;
        return len;
    }
    void close() { opened = false; isDir = false; }
};
inline bool LittleFSClass::rename(const char *from, const char *to) {
    auto it = files.find(from);
    if (it == files.end()) return false;
    files[to] = it->second;
    files.erase(it);
    return true;
}
inline File LittleFSClass::open(const char *path, const char *mode) { return File(path, mode); }
