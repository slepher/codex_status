// Fake Wi-Fi is an adapter around the same production state machine/storage.
#define CODEX_WIFI_SETUP_HOST_TEST
#define CODEX_WIFI_SIM
#include "../../../..//src/wifi_setup.cpp"
#include <fstream>
#include <cstdio>
std::string wifiBlobPath;
static String simWifiLegacy="[]";
extern "C" int codex_sim_wifi_open(const char *dir) {
    wifiBlobPath = std::string(dir) + "/wifi-schemes.bin";
    testNvsBlob.clear();
    std::ifstream file(wifiBlobPath, std::ios::binary);
    if (file) testNvsBlob.assign(std::istreambuf_iterator<char>(file), {});
    testNvsLegacyBlob.clear();
    if (testNvsBlob.size() && memcmp(testNvsBlob.data(),"WFS3",std::min(size_t(4),testNvsBlob.size()))) testNvsLegacyBlob.swap(testNvsBlob);
    wifiManagementResetSession(); running = false; storeReady = false;
    return wifiSchemesBegin() ? 1 : 0;
}
extern "C" int codex_sim_wifi_command(const char *input, const char *peer, uint32_t generation,
    uint64_t now, uint64_t physical, uint64_t light, int allowed, char *out, int cap) {
    JsonDocument doc;
    if (deserializeJson(doc, input)) return -1;
    String operation = doc["op"] | "";
    String reply;
    if (operation == "wifi_authorize") reply = wifiManagementAuthorize(doc, peer, now, physical, light);
    else if (operation == "snapshot" || operation == "apply" || operation == "result")
        reply = wifiManagementCommand(doc, now, simWifiLegacy);
    else reply = wifiSetupCommand(doc, peer, generation, now, physical, light, allowed != 0);
    if (reply.length() >= (size_t)cap) return -2;
    memcpy(out, reply.c_str(), reply.length() + 1); return reply.length();
}
extern "C" int codex_sim_wifi_poll(uint64_t now, const char *peer, uint32_t generation,
    int encrypted, int outcome) {
    if (running && outcome == 1) { WiFi.connected = WL_CONNECTED; WiFi.ipReady = true; }
    else if (running && outcome == 2) { WiFi.connected = WL_CONNECTED; WiFi.ipReady = false; }
    wifiSetupPoll(now, peer, generation, encrypted != 0);
    return running ? 1 : 0;
}
extern "C" void codex_sim_wifi_storage_failure(int fail) { testNvsFail = fail != 0; }

extern "C" void codex_sim_wifi_legacy(const char *json) { simWifiLegacy = json; }
