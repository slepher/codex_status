#pragma once
#include <map>
#include <string>
#define RTC_DATA_ATTR
inline uint64_t ownerTestRtcUs = 10000000ULL;
inline uint32_t ownerTestMillis = 2000;
enum { ESP_RST_POWERON, ESP_RST_DEEPSLEEP, ESP_RST_SW };
inline int ownerTestReset = ESP_RST_POWERON;
inline uint64_t esp_rtc_get_time_us() { return ownerTestRtcUs; }
inline int esp_reset_reason() { return ownerTestReset; }
inline unsigned long millis() { return ownerTestMillis; }
inline bool ownerTestWriteFail = false;
inline bool ownerTestReadFail = false;
inline std::map<std::string, String> ownerTestStrings;
inline std::map<std::string, uint32_t> ownerTestNumbers;
struct Preferences {
 bool begin(const char *, bool readOnly) { return !readOnly || !ownerTestReadFail; }
 void end() {}
 void clear() { ownerTestStrings.clear(); ownerTestNumbers.clear(); }
 String getString(const char *k, const char *v) { auto i=ownerTestStrings.find(k); return i==ownerTestStrings.end()?String(v):i->second; }
 uint32_t getUInt(const char *k,uint32_t v) { auto i=ownerTestNumbers.find(k);return i==ownerTestNumbers.end()?v:i->second; }
 uint16_t getUShort(const char *k,uint16_t v) { return (uint16_t)getUInt(k,v); }
 size_t putString(const char *k,const String &v) { if(ownerTestWriteFail)return 0;ownerTestStrings[k]=v;return v.length(); }
 size_t putUInt(const char *k,uint32_t v) { if(ownerTestWriteFail)return 0;ownerTestNumbers[k]=v;return 4; }
 size_t putUShort(const char *k,uint16_t v) { if(ownerTestWriteFail)return 0;ownerTestNumbers[k]=v;return 2; }
};
struct OwnerTestLog { template<class... T> void printf(const char *,T...){} void println(const char *){} };
inline OwnerTestLog DevLog;
