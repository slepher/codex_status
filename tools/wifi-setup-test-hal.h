#pragma once
#include <vector>
#include <map>
#include <string>
#ifdef _WIN32
extern "C" size_t codex_wifi_deflate(void *, size_t, const void *, size_t);
extern "C" size_t codex_wifi_inflate(void *, size_t, const void *, size_t);
extern "C" int codex_wifi_replace(const char *, const char *);
extern "C" void codex_wifi_sha256(const unsigned char *, size_t, unsigned char *);
#else
#include <zlib.h>
#endif
inline size_t wifiDeflate(void *out, size_t cap, const void *in, size_t size) {
#ifdef _WIN32
    return codex_wifi_deflate(out, cap, in, size);
#else
    uLongf n=cap; return compress2((Bytef *)out,&n,(const Bytef *)in,size,Z_DEFAULT_COMPRESSION)==Z_OK?n:0;
#endif
}
inline size_t wifiInflate(void *out, size_t cap, const void *in, size_t size) {
#ifdef _WIN32
    return codex_wifi_inflate(out, cap, in, size);
#else
    uLongf n=cap; return uncompress((Bytef *)out,&n,(const Bytef *)in,size)==Z_OK?n:0;
#endif
}
#ifdef CODEX_WIFI_SIM
#include <fstream>
#include <cstdio>
extern std::string wifiBlobPath;
#endif
#ifdef __APPLE__
#include <CommonCrypto/CommonDigest.h>
#elif !defined(_WIN32)
#include <openssl/sha.h>
#endif
inline int mbedtls_sha256(const unsigned char *data, size_t size, unsigned char *hash, int) {
#ifdef __APPLE__
    CC_SHA256(data, (CC_LONG)size, hash);
#else
#ifdef _WIN32
    codex_wifi_sha256(data, size, hash);
#else
    SHA256(data, size, hash);
#endif
#endif
    return 0;
}
static std::vector<uint8_t> testNvsBlob;
static std::vector<uint8_t> testNvsLegacyBlob;
static bool testNvsEmptyCurrentKey=false, testNvsEmptyLegacyKey=false;
static bool testNvsFail = false;
static bool testNvsCommitThenFail = false;
static bool testNvsReadbackFail = false;
static bool testNvsNewReadbackFail = false;
static bool testNvsRemoveFail = false;
static size_t testNvsByteLimit = SIZE_MAX;
// Synthetic NVS key map. The file container is only the Fake HAL, not a
// firmware format; each put/remove atomically replaces this synthetic image.
static std::string testNvsCommitThenFailKey, testNvsReadbackFailKey, testNvsFailReadAfterWriteKey;
static long testNvsFailAfter=-1;
static std::map<std::string,size_t> testNvsWrites;
static size_t testNvsPeakStoredBytes=0;
using TestNvsKeys=std::map<std::string,std::vector<uint8_t>>;
static bool testNvsKeys(TestNvsKeys &keys){
    keys.clear();
    if(testNvsBlob.empty())return true;
    if(testNvsBlob.size()<4 || memcmp(testNvsBlob.data(),"NVT1",4)){keys["store3"]=testNvsBlob;return true;}
    size_t at=4;
    while(at<testNvsBlob.size()){
        size_t n=testNvsBlob[at++];if(!n || n>15 || n+4>testNvsBlob.size()-at)return false;
        std::string key((const char *)testNvsBlob.data()+at,n);at+=n;size_t length=0;
        for(size_t i=0;i<4;++i)length|=size_t(testNvsBlob[at++])<<(8*i);
        if(length>16384 || length>testNvsBlob.size()-at || keys.count(key))return false;
        keys[key]={testNvsBlob.begin()+at,testNvsBlob.begin()+at+length};at+=length;
    }
    return keys.size()<=6;
}
static bool testNvsPublish(const TestNvsKeys &keys){
    std::vector<uint8_t> bytes={'N','V','T','1'};
    for(const auto &entry:keys){bytes.push_back(entry.first.size());bytes.insert(bytes.end(),entry.first.begin(),entry.first.end());
        for(size_t i=0;i<4;++i)bytes.push_back(entry.second.size()>>(8*i));bytes.insert(bytes.end(),entry.second.begin(),entry.second.end());}
#ifdef CODEX_WIFI_SIM
    auto temporary=wifiBlobPath+".tmp";
    std::ofstream file(temporary,std::ios::binary|std::ios::trunc);file.write((const char *)bytes.data(),bytes.size());file.close();if(!file)return false;
#ifdef _WIN32
    if(codex_wifi_replace(temporary.c_str(),wifiBlobPath.c_str()))return false;
#else
    if(std::rename(temporary.c_str(),wifiBlobPath.c_str()))return false;
#endif
#endif
    size_t raw=0;for(const auto &entry:keys)raw+=entry.second.size();
    testNvsPeakStoredBytes=std::max(testNvsPeakStoredBytes,raw);
    testNvsBlob=std::move(bytes);return true;
}
class Preferences {
public:
    bool begin(const char *,bool){return true;}
    void end(){}
    size_t freeEntries(){return 300;}
    bool isKey(const char *key){
        if(!strcmp(key,"scheme"))return testNvsEmptyLegacyKey || !testNvsLegacyBlob.empty();
        TestNvsKeys keys;if(!testNvsKeys(keys))return true;
        return keys.count(key) || (!strcmp(key,"store3") && testNvsEmptyCurrentKey);
    }
    size_t getBytesLength(const char *key){
        if(!strcmp(key,"scheme"))return testNvsLegacyBlob.size();
        TestNvsKeys keys;if(!testNvsKeys(keys))return 0;auto it=keys.find(key);return it==keys.end()?0:it->second.size();
    }
    bool remove(const char *key){
        if(testNvsRemoveFail)return false;
        if(!strcmp(key,"scheme")){testNvsLegacyBlob.clear();return true;}
        TestNvsKeys keys;if(!testNvsKeys(keys))return false;keys.erase(key);return testNvsPublish(keys);
    }
    size_t putBytes(const char *key,const void *data,size_t size){
        if(testNvsFail || size>testNvsByteLimit || testNvsFailAfter==0)return 0;
        if(testNvsFailAfter>0)--testNvsFailAfter;
        TestNvsKeys keys;if(!testNvsKeys(keys))return 0;auto bytes=(const uint8_t *)data;keys[key]={bytes,bytes+size};
        if(!testNvsPublish(keys))return 0;++testNvsWrites[key];
        return testNvsCommitThenFail || testNvsCommitThenFailKey==key?0:size;
    }
    size_t getBytes(const char *key,void *data,size_t size){
        TestNvsKeys keys;if(!testNvsKeys(keys))return 0;
        auto it=keys.find(key);const auto *blob=!strcmp(key,"scheme")?&testNvsLegacyBlob:it==keys.end()?nullptr:&it->second;
        if(!blob || testNvsReadbackFail || testNvsReadbackFailKey==key ||
            (testNvsFailReadAfterWriteKey==key && testNvsWrites[key]) ||
            (testNvsNewReadbackFail && !strcmp(key,"store3")) || size<blob->size())return 0;
        memcpy(data,blob->data(),blob->size());return blob->size();
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
