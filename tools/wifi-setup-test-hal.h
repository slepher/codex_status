#pragma once
#include <vector>
#include <zlib.h>
inline size_t wifiDeflate(void *out, size_t cap, const void *in, size_t size) {
    uLongf n=cap; return compress2((Bytef *)out,&n,(const Bytef *)in,size,Z_DEFAULT_COMPRESSION)==Z_OK?n:0;
}
inline size_t wifiInflate(void *out, size_t cap, const void *in, size_t size) {
    uLongf n=cap; return uncompress((Bytef *)out,&n,(const Bytef *)in,size)==Z_OK?n:0;
}
#ifdef CODEX_WIFI_SIM
#include <fstream>
#include <cstdio>
extern std::string wifiBlobPath;
#endif
#ifdef __APPLE__
#include <CommonCrypto/CommonDigest.h>
#else
#include <openssl/sha.h>
#endif
inline int mbedtls_sha256(const unsigned char *data, size_t size, unsigned char *hash, int) {
#ifdef __APPLE__
    CC_SHA256(data, (CC_LONG)size, hash);
#else
    SHA256(data, size, hash);
#endif
    return 0;
}
static std::vector<uint8_t> testNvsBlob;
static std::vector<uint8_t> testNvsLegacyBlob;
static bool testNvsFail = false;
static bool testNvsCommitThenFail = false;
static bool testNvsReadbackFail = false;
static bool testNvsNewReadbackFail = false;
static bool testNvsRemoveFail = false;
static size_t testNvsByteLimit = SIZE_MAX;
class Preferences {
public:
    bool begin(const char *, bool) { return true; }
    void end() {}
    size_t freeEntries() { return 300; }
    size_t getBytesLength(const char *key) { return !strcmp(key,"scheme") ? testNvsLegacyBlob.size() : testNvsBlob.size(); }
    bool remove(const char *key) { if (testNvsRemoveFail) return false; if (!strcmp(key,"scheme")) testNvsLegacyBlob.clear(); return true; }
    size_t putBytes(const char *, const void *data, size_t size) {
        if (testNvsFail || size > testNvsByteLimit) return 0;
        auto bytes = (const uint8_t *)data;
#ifdef CODEX_WIFI_SIM
        auto temporary = wifiBlobPath + ".tmp";
        std::ofstream file(temporary, std::ios::binary | std::ios::trunc);
        file.write((const char *)bytes, size); file.close();
        if (!file || std::rename(temporary.c_str(), wifiBlobPath.c_str())) return 0;
#endif
        testNvsBlob.assign(bytes, bytes + size); return testNvsCommitThenFail ? 0 : size;
    }
    size_t getBytes(const char *key, void *data, size_t size) {
        const auto &blob = !strcmp(key,"scheme") ? testNvsLegacyBlob : testNvsBlob;
        if (testNvsReadbackFail || (testNvsNewReadbackFail && !strcmp(key,"store3")) || size < blob.size()) return 0;
        memcpy(data, blob.data(), blob.size()); return blob.size();
    }
};
class IPAddress {
    bool valid;
public:
    IPAddress(int a, int b, int c, int d) : valid(a || b || c || d) {}
    bool operator==(const IPAddress &other) const { return valid == other.valid; }
    String toString() const { return valid ? "192.0.2.10" : "0.0.0.0"; }
};
enum { WIFI_STA = 1, WIFI_AP = 2, WIFI_AP_STA = 3, WL_CONNECTED = 3 };
static struct TestWiFi {
    String ssid, password;
    int connected = 0, modeValue = WIFI_STA, begins = 0;
    bool ipReady = false;
    int status() const { return connected; }
    String SSID() const { return ssid; }
    String psk() const { return password; }
    IPAddress localIP() const { return ipReady ? IPAddress(192, 0, 2, 10) : IPAddress(0, 0, 0, 0); }
    void disconnect(bool) { connected = 0; ipReady = false; }
    void begin(const char *s, const char *p) { ssid = s; password = p; ++begins; }
    int getMode() const { return modeValue; }
    void mode(int value) { modeValue = value; }
} WiFi;
