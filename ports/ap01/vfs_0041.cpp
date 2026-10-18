#include "LittleFS.h"

// 0041-only NuttX/LittleFS ABI. Evidence: 0041-littlefs-abi.json and the
// matching immutable ROM. No public Linux ioctl/flag/struct assumptions.
#define ROM_FN(address, type) ((type)(uintptr_t)(address))
namespace {
struct Lookup {
    const char *path;
    void *node, *peer, *parent;
    const char *relative;
    void *linkBuffer;
    uint8_t nofollow, reserved[3];
};
static_assert(sizeof(Lookup) == 28, "0041 inode_search_s ABI");
struct Stat { uint32_t words[15]; }; // Matching stat callback zeroes 0x3c bytes.
struct StatFs { uint32_t magic, nameMax, blockSize, blocks, free, available; };
static_assert(sizeof(Stat) == 60 && sizeof(StatFs) == 24, "0041 VFS output ABI");

class Mount {
    Lookup lookup{};
    bool held = false;
public:
    explicit Mount(const char *path) {
        lookup.path = path; lookup.nofollow = 1;
        held = ROM_FN(0xa01a3772, int (*)(Lookup *))(&lookup) >= 0;
    }
    ~Mount() {
        if (held && lookup.node) ROM_FN(0xa0027c44, void (*)(void *))(lookup.node);
        if (lookup.linkBuffer) ROM_FN(0xa002e14e, void (*)(void *))(lookup.linkBuffer);
    }
    bool valid() const {
        if (!held || !lookup.node || !lookup.relative) return false;
        const uint8_t *inode = (const uint8_t *)lookup.node;
        uint16_t flags; uintptr_t operations;
        memcpy(&flags, inode + 14, 2); memcpy(&operations, inode + 16, 4);
        return (flags & 15) == 3 && operations == 0xa01bf590u;
    }
    void *node() const { return lookup.node; }
    const char *relative() const { return lookup.relative; }
};
int statPath(const char *path, Stat &out) {
    Mount mount(path);
    return mount.valid() ? ROM_FN(0xa0044cd4, int (*)(void *, const char *, Stat *))
        (mount.node(), mount.relative(), &out) : -19;
}
int mkdirPath(void *, const char *path) {
    Mount mount(path);
    return mount.valid() ? ROM_FN(0xa00485e4, int (*)(void *, const char *, unsigned))
        (mount.node(), mount.relative(), 0700) : -19;
}
int syncFile(int fd) {
    void *file = nullptr;
    int result = ROM_FN(0xa0025904, int (*)(int, void **))(fd, &file);
    if (result < 0) return result;
    result = ROM_FN(0xa0025988, int (*)(void *))(file);
    ROM_FN(0xa0027e54, void (*)(void *))(file);
    return result;
}
int prepare(void *, const char *root) {
    Stat info{};
    int result = statPath(root, info);
    if (result == -2) result = mkdirPath(nullptr, root);
    if (result || statPath(root, info)) return -1;
    return (info.words[2] & 0170000) == 0040000 ? 0 : -1;
}
int openFile(void *, const char *path, unsigned mode) {
    // Verified NuttX flags: RDONLY=1, WRONLY=2, CREAT=4, APPEND=16, TRUNC=32.
    unsigned flags = mode == 0 ? 1 : mode == 1 ? 38 : mode == 2 ? 22 : 0;
    if (!flags) return -1;
    int fd = ROM_FN(0xa003f4f6, int (*)(const char *, int, ...))(path, flags, 0600);
    if (fd >= 0 && mode && syncFile(fd)) {
        ROM_FN(0xa0026888, int (*)(int))(fd); return -1;
    }
    return fd;
}
int closeFile(void *, int fd) { return ROM_FN(0xa0026888, int (*)(int))(fd); }
int readFile(void *, int fd, void *out, size_t n) {
    return n <= INT32_MAX ? ROM_FN(0xa003f6a2, int (*)(int, void *, size_t))(fd, out, n) : -1;
}
int writeFile(void *, int fd, const void *bytes, size_t n) {
    if (n > INT32_MAX) return -1;
    size_t done = 0;
    while (done < n) {
        int result = ROM_FN(0xa0027e94, int (*)(int, const void *, size_t))
            (fd, (const uint8_t *)bytes + done, n - done);
        if (result <= 0) return -1;
        done += result;
    }
    return syncFile(fd) ? -1 : (int)done;
}
int32_t seekFile(void *, int fd, int32_t offset, unsigned origin) {
    return ROM_FN(0xa00281d8, int32_t (*)(int, int32_t, int))(fd, offset, origin);
}
int32_t fileSize(void *context, int fd) {
    int32_t current = seekFile(context, fd, 0, 1);
    if (current < 0) return -1;
    int32_t length = seekFile(context, fd, 0, 2);
    if (seekFile(context, fd, current, 0) != current) return -1;
    return length;
}
int exists(void *, const char *path) {
    Stat info{}; int result = statPath(path, info);
    return result == 0 ? 1 : result == -2 ? 0 : -1;
}
int removeFile(void *, const char *path) { return ROM_FN(0xa003f6fa, int (*)(const char *))(path); }
int renameFile(void *, const char *from, const char *to) {
    Mount a(from), b(to);
    if (!a.valid() || !b.valid() || a.node() != b.node()) return -19;
    return ROM_FN(0xa004888c, int (*)(void *, const char *, const char *))
        (a.node(), a.relative(), b.relative());
}
int statFs(void *, const char *path, uint64_t *total, uint64_t *used) {
    Mount mount(path); StatFs stats{};
    if (!mount.valid() || ROM_FN(0xa00421bc, int (*)(void *, StatFs *))(mount.node(), &stats) ||
        stats.magic != 0x0a732923 || !stats.blockSize || stats.free > stats.blocks) return -1;
    *total = uint64_t(stats.blockSize) * stats.blocks;
    *used = uint64_t(stats.blockSize) * (stats.blocks - stats.free);
    return *total && *total <= 0x140000 ? 0 : -1;
}
}
Ap01VfsOps ap01RomVfs() {
    return {nullptr, prepare, openFile, closeFile, readFile, writeFile, seekFile,
            fileSize, exists, mkdirPath, removeFile, renameFile, statFs};
}
