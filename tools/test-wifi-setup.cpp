#define CODEX_WIFI_SETUP_HOST_TEST
#include "../src/wifi_setup.cpp"
#include "../src/ble_wire.h"
#include <cassert>
#include <iostream>

static size_t maximumFrame = 0;
static JsonDocument command(const char *op, const char *id, const char *network = "home") {
    JsonDocument doc;
    doc["op"] = String(op); doc["request_id"] = String(id); doc["network_id"] = String(network);
    doc["device_mac"] = "02:00:00:00:00:01";doc["bridge_id"]="test-bridge";
    return doc;
}
static JsonDocument execute(JsonDocument &doc, const char *bond = "02:00:00:00:00:02/1",
                            uint64_t physical = 120000, uint64_t light = 0, bool allowed = true) {
    WifiControlContext context;context.principal="test-bridge";context.nameSpace=bond;context.generation=1;
    context.physicalUntil=physical;context.until=std::max(physical,light);context.claimed=allowed;
    JsonDocument reply;
    assert(!deserializeJson(reply, wifiControlCommand(doc,context,1000)));
    JsonDocument finalBody;
    finalBody.set(reply);
    String final = bleBuildReply(finalBody, doc["request_id"] | "", UINT32_MAX, UINT32_MAX);
    if (!final.length() || final.length() > 512) std::cerr << "frame failed op=" << (doc["op"] | "") << " id_bytes=" << strlen(doc["request_id"] | "") << " body_bytes=" << measureJson(reply) << "\n";
    assert(final.length() && final.length() <= 512);
    maximumFrame = std::max(maximumFrame, final.length());
    if (reply["chunk"].is<const char *>()) {
        std::string bytes;
        while (true) {
            JsonDocument body; body.set(reply);
            String frame = bleBuildReply(body, doc["request_id"] | "", UINT32_MAX, UINT32_MAX);
            assert(frame.length() && frame.length() <= 512);
            maximumFrame = std::max(maximumFrame, frame.length());
            JsonDocument encoded; assert(!deserializeJson(encoded, frame));
            assert(encoded["result"] != "unknown");
            std::string chunk = reply["chunk"].as<const char *>();
            for (size_t i = 0; i < chunk.size(); i += 2) bytes += (char)std::stoul(chunk.substr(i, 2), nullptr, 16);
            if (reply["next_offset"] == reply["total"]) break;
            doc["offset"] = reply["next_offset"].as<uint32_t>();
            doc["snapshot"] = reply["snapshot"].as<uint32_t>();
            assert(!deserializeJson(reply, wifiControlCommand(doc,context,1000)));
        }
        assert(!deserializeJson(reply, bytes)); doc.remove("offset"); doc.remove("snapshot");
    }
    assert(reply["password"].isNull());
    return reply;
}
static void connected() { WiFi.connected = WL_CONNECTED; WiFi.ipReady = true; }
static JsonDocument trial(const char *id, bool replace = false) {
    auto doc = command("wifi_trial", id);
    doc["ssid"] = "device-2g"; doc["host_ssid"] = "host-5g"; doc["replace"] = replace;
    // Generate ephemeral test input, never store a real password in fixtures.
    std::string input(12, ' ');
    for (size_t i = 0; i < input.size(); ++i) input[i] = (char)('a' + (i * 7 % 26));
    doc["password"] = input;
    return doc;
}
int main() {
    for (const std::string invalid : {std::string("a\0b",3), std::string("\xc0\xaf",2),
        std::string("\xed\xa0\x80",3), std::string("\xf4\x90\x80\x80",4)})
        assert(!bleWireText(String(invalid), 64));
    assert(bleWireText(String("中文\\\"\x01"),64));
    assert(wifiSchemesBegin());
    auto first=trial("trial1");
    assert(execute(first,"02:00:00:00:00:02/1",0)["error"]=="window_required");
    assert(execute(first)["stage"]=="connecting");
    assert(!testNvsBlob.empty()&&schemes.networks.size()==0); // durable receipt, no saved credentials
    int begins=WiFi.begins;
    assert(execute(first)["outcome"]=="accepted"&&WiFi.begins==begins);
    auto conflicting=first;conflicting["ssid"]="different";assert(execute(conflicting)["error"]=="request_conflict");
    auto cancel=command("wifi_setup_cancel","cancel1");cancel["query_id"]="trial1";
    assert(execute(cancel,"02:00:00:00:00:03/1")["error"]=="not_found");
    assert(execute(cancel)["error"]=="cancelled");
    auto second=trial("trial2");assert(execute(second)["stage"]=="connecting");
    connected();WiFi.ipReady=false;wifiSetupPoll(2000);
    assert(wifiSetupBusy()&&schemes.networks.size()==0); // disconnect/reconnect does not cancel; DHCP required
    WiFi.ipReady=true;wifiSetupPoll(2500);
    assert(!wifiSetupBusy()&&schemes.networks.size()==0);
    auto query=command("wifi_result","query");query["query_id"]="trial2";
    assert(execute(query)["outcome"]=="completed");
    auto save=command("wifi_save","save");save["expected_revision"]=0;
    auto row=save["changes"].to<JsonArray>().add<JsonObject>();row["network_id"]="home";
    row["host_ssid"]="host-5g";row["ssid"]="device-2g";row["credential"]="replace";row["password"]=second["password"];
    assert(execute(save)["outcome"]=="committed");
    auto get=command("wifi_scheme_get","get");
    assert(execute(get)["networks"][0]["password"]==second["password"]);
    assert(execute(get,"02:00:00:00:00:03/1")["networks"].size()==1); // owner manages all origins
    assert(execute(get,"02:00:00:00:00:02/1",120000,0,false)["error"]=="claim_required");
    auto activate=command("wifi_activate","activate");activate["version"]=1;
    assert(execute(activate,"02:00:00:00:00:02/1",120000,0,false)["error"]=="claim_required");
    assert(execute(activate,"02:00:00:00:00:02/1",0)["error"]=="window_required");
    assert(execute(activate,"02:00:00:00:00:02/1",0,60000)["stage"]=="accepted");
    wifiSetupPoll(2000);connected();wifiSetupPoll(2500);
    assert(!wifiSetupBusy()&&wifiSetupActivated());
    assert(wifiSchemesBegin());String ssid,pass;
    assert(wifiSchemesCredentials(ssid,pass)&&ssid=="device-2g");
    auto failed=trial("failed");failed["ssid"]="replacement";assert(execute(failed)["stage"]=="connecting");
    auto before=testNvsBlob;connected();testNvsFail=true;wifiSetupPoll(3000);testNvsFail=false;
    assert(!wifiSetupBusy()&&testNvsBlob==before&&WiFi.SSID()=="device-2g");
    query["query_id"]="failed";assert(execute(query)["outcome"]=="unknown");
    auto timed=trial("timed");assert(execute(timed)["stage"]=="connecting");begins=WiFi.begins;
    wifiSetupPoll(26000);assert(!wifiSetupBusy()&&WiFi.begins==begins);
    auto invalid=trial("nul");invalid["ssid"]=std::string("a\0b",3);assert(execute(invalid)["error"]=="shape");
    for(const char *op:{"wifi_setup","wifi_select","wifi_scheme_delete"}) {
        auto retired=command(op,"retired");before=testNvsBlob;begins=WiFi.begins;
        assert(execute(retired)["error"]=="unsupported"&&testNvsBlob==before&&WiFi.begins==begins);
    }
    // Independent maximal-escaping fixture: full device capacity, not the old per-bond subset.
    testNvsBlob.clear();running=false;requestId="";assert(wifiSchemesBegin());
    for(int batch=0;batch<3;++batch) {
        auto bounded=command("wifi_save",("bounded"+std::to_string(batch)).c_str());bounded["expected_revision"]=batch;
        auto rows=bounded["changes"].to<JsonArray>();
        for(int i=0;i<4;++i) {
            auto n=rows.add<JsonObject>();n["network_id"]="n"+std::to_string(batch*4+i);
            n["host_ssid"]=std::string(31,'\x01')+char('a'+batch*4+i);n["ssid"]=std::string(32,'\x02');n["credential"]="open";
        }
        assert(execute(bounded)["outcome"]=="committed");
    }
    get["request_id"]=std::string(64,'r');assert(execute(get)["networks"].size()==12);
    get["offset"]=120;get["snapshot"]=0;
    assert(execute(get)["error"]=="read_snapshot");
    auto full=trial(std::string(64,'r').c_str());full["network_id"]=std::string(32,'n');
    assert(execute(full)["stage"]=="connecting");connected();wifiSetupPoll(3000);
    auto outcome=command("wifi_setup_status",std::string(64,'q').c_str());outcome["query_id"]=std::string(64,'r');
    assert(execute(outcome)["stage"]=="trial_complete"&&execute(outcome)["persisted"]==false);
    std::cout<<"Max final Wi-Fi frame="<<maximumFrame<<" bytes\n";
    std::cout<<"Current Wi-Fi: DHCP, durable replay, conflict, namespace, cancel, timeout, reconnect, storage, owner, 12-row paging and redaction passed\n";
    return 0;
}
