#pragma once
#include "Arduino.h"

// Explicit kernel VFS boundary. All paths are below the caller-owned private
// root on the already-mounted /resource volume; this port never formats it.
// write returns bytes only after sync; rename commits the filesystem metadata.
struct Ap01VfsOps {
    void *context;
    int (*prepare)(void *, const char *);
    int (*open)(void *, const char *, unsigned); // 0 read, 1 truncate, 2 append
    int (*close)(void *, int);
    int (*read)(void *, int, void *, size_t);
    int (*write)(void *, int, const void *, size_t);
    int32_t (*seek)(void *, int, int32_t, unsigned);
    int32_t (*size)(void *, int);
    int (*exists)(void *, const char *);
    int (*mkdir)(void *, const char *);
    int (*remove)(void *, const char *);
    int (*rename)(void *, const char *, const char *);
    int (*statfs)(void *, const char *, uint64_t *, uint64_t *);
};
class File {
    const Ap01VfsOps *io = nullptr;
    int fd = -1;
public:
    File() = default;
    File(const Ap01VfsOps &ops, int handle) : io(&ops), fd(handle) {}
    File(const File &) = delete;
    File &operator=(const File &) = delete;
    File(File &&other) noexcept : io(other.io), fd(other.fd) { other.fd = -1; }
    File &operator=(File &&other) noexcept {
        if (this != &other) { close(); io = other.io; fd = other.fd; other.fd = -1; }
        return *this;
    }
    ~File() { close(); }
    explicit operator bool() const { return io && fd >= 0; }
    void close() { if (*this) { io->close(io->context, fd); fd = -1; } }
    size_t size() const { int32_t n = *this ? io->size(io->context, fd) : -1; return n < 0 ? 0 : (size_t)n; }
    size_t position() const { int32_t n = *this ? io->seek(io->context, fd, 0, 1) : -1; return n < 0 ? 0 : (size_t)n; }
    bool seek(uint32_t at) { return *this && at <= INT32_MAX && io->seek(io->context, fd, at, 0) == (int32_t)at; }
    size_t read(uint8_t *p, size_t n) {
        int got = *this ? io->read(io->context, fd, p, n) : -1;
        return got < 0 ? 0 : (size_t)got;
    }
    int read() { uint8_t c; return read(&c, 1) == 1 ? c : -1; }
    size_t readBytes(char *p, size_t n) { return read((uint8_t *)p, n); }
    size_t write(const uint8_t *p, size_t n) {
        int got = *this ? io->write(io->context, fd, p, n) : -1;
        return got < 0 ? 0 : (size_t)got;
    }
};
class LittleFSClass {
    Ap01VfsOps io;
    const char *root;
    bool path(const char *relative, char (&out)[128]) const {
        if (!relative || relative[0] != '/' || strstr(relative, "//")) return false;
        for (const char *p = relative; *p; ++p)
            if (*p == '/' && p[1] == '.' && (p[2] == '/' || !p[2] ||
                (p[2] == '.' && (p[3] == '/' || !p[3])))) return false;
        size_t a = strlen(root), b = strlen(relative);
        if (a + b >= sizeof(out)) return false;
        memcpy(out, root, a); memcpy(out + a, relative, b + 1); return true;
    }
    bool capacity(uint64_t &total, uint64_t &used) const {
        return !io.statfs(io.context, root, &total, &used) && used <= total && total <= SIZE_MAX;
    }
public:
    LittleFSClass(const Ap01VfsOps &ops, const char *privateRoot) : io(ops), root(privateRoot) {}
    LittleFSClass(const LittleFSClass &) = delete;
    LittleFSClass &operator=(const LittleFSClass &) = delete;
    bool begin(bool format, const char *, unsigned, const char *) {
        return !format && !io.prepare(io.context, root);
    }
    File open(const char *relative, const char *mode) {
        char absolute[128];
        if (!mode || mode[1] || !path(relative, absolute)) return {};
        unsigned flags = mode[0] == 'r' ? 0 : mode[0] == 'w' ? 1 : mode[0] == 'a' ? 2 : 3;
        if (flags == 3) return {};
        return File(io, io.open(io.context, absolute, flags));
    }
    int inspect(const char *relative) { char absolute[128]; return path(relative, absolute) ? io.exists(io.context, absolute) : -1; }
    bool exists(const char *relative) { return inspect(relative) == 1; }
    bool mkdir(const char *relative) { char absolute[128]; return path(relative, absolute) && !io.mkdir(io.context, absolute); }
    bool remove(const char *relative) { char absolute[128]; return path(relative, absolute) && !io.remove(io.context, absolute); }
    bool rename(const char *from, const char *to) {
        char a[128], b[128]; return path(from, a) && path(to, b) && !io.rename(io.context, a, b);
    }
    size_t totalBytes() const { uint64_t total, used; return capacity(total, used) ? (size_t)total : 0; }
    size_t usedBytes() const { uint64_t total, used; return capacity(total, used) ? (size_t)used : SIZE_MAX; }
};
