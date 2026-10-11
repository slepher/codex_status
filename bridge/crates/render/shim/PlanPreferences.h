#pragma once
#include <vector>
#include <cstring>

// Fault-injected NVS boundary; production PlanStore serialization is unchanged.
namespace plan_test {
inline std::vector<unsigned char> record;
inline unsigned writes = 0;
inline bool failWrite = false, failReadAfterWrite = false, failRead = false;
inline void reset() { record.clear(); writes=0; failWrite=failReadAfterWrite=failRead=false; }
}
class Preferences {
public:
    bool begin(const char *, bool readOnly) { return !(readOnly && plan_test::failRead); }
    void end() {}
    bool isKey(const char *) { return !plan_test::record.empty(); }
    size_t getBytesLength(const char *) { return plan_test::record.size(); }
    size_t getBytes(const char *, void *out, size_t n) {
        if (plan_test::failRead || n < plan_test::record.size()) return 0;
        std::memcpy(out,plan_test::record.data(),plan_test::record.size());
        return plan_test::record.size();
    }
    size_t putBytes(const char *, const void *data, size_t n) {
        ++plan_test::writes;
        if (plan_test::failWrite) return 0;
        const auto *p=static_cast<const unsigned char *>(data);
        plan_test::record.assign(p,p+n);
        plan_test::failRead=plan_test::failReadAfterWrite;
        return n;
    }
    bool remove(const char *) { plan_test::record.clear(); return true; }
};
