// Fake Wi-Fi is an adapter around the same production state machine/storage.
#define CODEX_WIFI_SETUP_HOST_TEST
#define CODEX_WIFI_SIM
#include "../../../..//src/wifi_setup.cpp"
#include <fstream>
#include <cstdio>
std::string wifiBlobPath;
static String simWifiLegacy="[]";
#include "../../../../tools/wifi-legacy-convert.inc"
// Offline only: decode and encode without opening NVS, advancing a boot
// generation, completing pending work, or touching simulator global state.
extern "C" int codex_wifi_convert_storage(uint8_t *input,int size,uint8_t *output,int capacity) {
    if(size<33 || size>WIFI_STORAGE_MAX || capacity<0)return -1;
    WifiStore decoded;
    bool explicitFormat=!memcmp(input,"WFS",3);
    if(!(explicitFormat?wifiDecodeExplicitOffline(input,size,decoded):wifiDecodeLegacyOffline(input,size,decoded)))return -1;
    WifiWriter sizing{nullptr};sizing.store(decoded);
    if(sizing.pos>WIFI_STORAGE_MAX || sizing.pos>size_t(capacity))return -2;
    WifiWriter writer{output};writer.store(decoded);
    for(size_t i=0;i<4;++i)output[8+i]=writer.pos>>(8*i);
    uint32_t crc=v2Crc32(output,writer.pos);for(size_t i=0;i<4;++i)output[12+i]=crc>>(8*i);
    return int(writer.pos);
}
extern "C" int codex_sim_wifi_open(const char *dir) {
    wifiBlobPath = std::string(dir) + "/wifi-schemes.bin";
    testNvsBlob.clear();
    std::ifstream file(wifiBlobPath, std::ios::binary);
    bool existed=bool(file);
    if (file) testNvsBlob.assign(std::istreambuf_iterator<char>(file), {});
    testNvsEmptyCurrentKey=existed && testNvsBlob.empty();
    testNvsLegacyBlob.clear();

    wifiManagementResetSession(); running = false; storeReady = false; requestId="";transactionPeer="";wifiExecutionGeneration=0;
    return wifiSchemesBegin() ? 1 : 0;
}
extern "C" int codex_sim_wifi_poll(uint64_t now, const char *peer, uint32_t generation,
    int encrypted, int outcome, int startAllowed) {
    if (running && outcome == 1) { WiFi.connected = WL_CONNECTED; WiFi.ipReady = true; }
    else if (running && outcome == 2) { WiFi.connected = WL_CONNECTED; WiFi.ipReady = false; }
    wifiSetupPoll(now, startAllowed != 0);
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
