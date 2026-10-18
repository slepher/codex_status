#include <time.h>
#include <stdint.h>
#include <string.h>

bool ap01Calendar(time_t epoch, struct tm &result) {
#if defined(__riscv)
    if (epoch < 0 || uint64_t(epoch) > UINT32_MAX) return false;
    // Original localtime uses a shared output. Its timezone engine already
    // accepts a caller-owned 44-byte tm (including gmtoff/zone extensions).
    ((void (*)())(uintptr_t)0xa0059d98)();
    uint32_t seconds = epoch, fields[11] = {};
    if (!((void *(*)(const uint32_t *, int, uint32_t *))(uintptr_t)0xa00581fa)
        (&seconds, 0, fields)) return false;
    result = tm{};
    result.tm_sec=fields[0]; result.tm_min=fields[1]; result.tm_hour=fields[2];
    result.tm_mday=fields[3]; result.tm_mon=fields[4]; result.tm_year=fields[5];
    result.tm_wday=fields[6]; result.tm_yday=fields[7]; result.tm_isdst=fields[8];
    return true;
#else
    return localtime_r(&epoch, &result) != nullptr;
#endif
}
