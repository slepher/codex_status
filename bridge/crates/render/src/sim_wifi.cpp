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
    if (testNvsBlob.size() && (memcmp(testNvsBlob.data(),"WFS3",std::min(size_t(4),testNvsBlob.size())) && memcmp(testNvsBlob.data(),"WFS4",std::min(size_t(4),testNvsBlob.size())))) testNvsLegacyBlob.swap(testNvsBlob);
    wifiManagementResetSession(); running = false; storeReady = false; requestId="";transactionPeer="";transactionUnified=false;
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
    int encrypted, int outcome, int startAllowed) {
    if (running && outcome == 1) { WiFi.connected = WL_CONNECTED; WiFi.ipReady = true; }
    else if (running && outcome == 2) { WiFi.connected = WL_CONNECTED; WiFi.ipReady = false; }
    wifiSetupPoll(now, peer, generation, encrypted != 0, startAllowed != 0);
    return running ? 1 : 0;
}
extern "C" void codex_sim_wifi_storage_failure(int fail) { testNvsFail = fail != 0; }
extern "C" void codex_sim_wifi_constrain(uint64_t now,uint64_t until,int ownerAllowed) {
    wifiSetupConstrain(now,until,ownerAllowed!=0);
}

extern "C" void codex_sim_wifi_legacy(const char *json) { simWifiLegacy = json; }

extern "C" int codex_sim_wifi_control(const char *input,const char *principal,const char *nameSpace,uint32_t generation,
    uint64_t now,uint64_t until,uint64_t physical,int claimed,int sessionValid,char *out,int cap) {
    JsonDocument doc;if(deserializeJson(doc,input))return -1;
    WifiControlContext ctx;ctx.principal=principal;ctx.nameSpace=nameSpace;ctx.generation=generation;ctx.until=until;ctx.physicalUntil=physical;
    ctx.claimed=claimed!=0;ctx.sessionValid=sessionValid!=0;
    String response=wifiControlCommand(doc,ctx,now,simWifiLegacy);if(response.length()>=(size_t)cap)return -2;
    memcpy(out,response.c_str(),response.length()+1);return response.length();
}

extern "C" int codex_sim_wifi_read_page(const char *input,const char *peer,uint32_t generation,const char *text,char *out,int cap) {
    JsonDocument doc;if(deserializeJson(doc,input))return -1;String response=wifiSetupReadPage(doc,peer,generation,text);
    if(response.length()>=(size_t)cap)return -2;memcpy(out,response.c_str(),response.length()+1);return response.length();
}

extern "C" int codex_sim_wifi_storage_status(char *out,int cap) {
    JsonDocument doc;wifiSchemesStorageStatus(doc.to<JsonObject>());
    String response;serializeJson(doc,response);if(response.length()>=(size_t)cap)return -2;
    memcpy(out,response.c_str(),response.length()+1);return response.length();
}

// Test-only RF observation: never export credential bytes.
extern "C" int codex_sim_wifi_observe(char *out,int cap) {
    JsonDocument doc;doc["begins"]=WiFi.begins;doc["ssid"]=WiFi.SSID();
    doc["connected"]=WiFi.status()==WL_CONNECTED;doc["has_ip"]=!(WiFi.localIP()==IPAddress(0,0,0,0));
    doc["busy"]=running;doc["stage"]=stage;doc["error"]=error;
    doc["candidate_credentials_used"]=running && WiFi.SSID()==String(candidate.ssid) && WiFi.psk()==String(candidate.password);
    String response;serializeJson(doc,response);if(response.length()>=(size_t)cap)return -2;
    memcpy(out,response.c_str(),response.length()+1);return response.length();
}
