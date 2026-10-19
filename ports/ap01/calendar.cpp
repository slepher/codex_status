#include <time.h>
#include <stdint.h>
#include <string.h>

bool ap01Calendar(time_t epoch, struct tm &result) {
    if (epoch < 0 || uint64_t(epoch) > UINT32_MAX) return false;
    // Match the AP01's local civil clock (Hong Kong, UTC+8). A background
    // task has no TZ environment; tzset/localtime there silently uses UTC.
    constexpr int localOffset = 8 * 60 * 60;
#if defined(__riscv)
    // Original timesub accepts epoch, UTC offset, optional leap-second table
    // and a caller-owned 44-byte tm. No global timezone or shared tm mutation.
    uint32_t fields[11] = {};
    if (!((void *(*)(uint32_t, int, const void *, uint32_t *))(uintptr_t)0xa01b7f4a)
        (uint32_t(epoch), localOffset, nullptr, fields)) return false;
    result = tm{};
    result.tm_sec=fields[0]; result.tm_min=fields[1]; result.tm_hour=fields[2];
    result.tm_mday=fields[3]; result.tm_mon=fields[4]; result.tm_year=fields[5];
    result.tm_wday=fields[6]; result.tm_yday=fields[7]; result.tm_isdst=fields[8];
    return true;
#else
    time_t local = epoch + localOffset;
    return gmtime_r(&local, &result) != nullptr;
#endif
}
