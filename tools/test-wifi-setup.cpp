#define CODEX_WIFI_SETUP_HOST_TEST
#include "../src/wifi_setup.cpp"
#include "../src/ble_wire.h"
#include <cassert>
#include <iostream>

static size_t maximumFrame = 0;
static JsonDocument command(const char *op, const char *id, const char *network = "home") {
    JsonDocument doc;
    doc["op"] = String(op); doc["request_id"] = String(id); doc["network_id"] = String(network);
    doc["device_mac"] = "02:00:00:00:00:01";
    return doc;
}
static JsonDocument execute(JsonDocument &doc, const char *bond = "02:00:00:00:00:02/1",
                            uint64_t physical = 120000, uint64_t light = 0, bool allowed = true) {
    JsonDocument reply;
    assert(!deserializeJson(reply, wifiSetupCommand(doc, bond, 1, 1000, physical, light, allowed)));
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
            assert(!deserializeJson(reply, wifiSetupCommand(doc, bond, 1, 1000, physical, light, allowed)));
        }
        assert(!deserializeJson(reply, bytes)); doc.remove("offset"); doc.remove("snapshot");
    }
    assert(reply["password"].isNull());
    return reply;
}
static void connected() { WiFi.connected = WL_CONNECTED; WiFi.ipReady = true; }
static JsonDocument setup(const char *id, bool replace = false) {
    auto doc = command("wifi_setup", id);
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
    auto first = setup("trial1");
    assert(execute(first, "02:00:00:00:00:02/1", 0)["error"] == "physical_required");
    assert(execute(first)["stage"] == "connecting");
    assert(testNvsBlob.empty()); // credentials must not be durable before IP
    int begins = WiFi.begins;
    assert(execute(first)["stage"] == "connecting" && WiFi.begins == begins);
    auto conflicting = setup("trial1"); conflicting["ssid"] = "different";
    assert(execute(conflicting)["error"] == "request_conflict");
    auto get = command("wifi_scheme_get", "get1");
    assert(execute(get, "02:00:00:00:00:03/1")["networks"].size() == 0);
    auto cancel = command("wifi_setup_cancel", "cancel1"); cancel["query_id"] = "trial1";
    assert(execute(cancel, "02:00:00:00:00:03/1")["error"] == "not_found");
    assert(execute(cancel)["error"] == "cancelled" && testNvsBlob.empty());
    auto second = setup("trial2"); assert(execute(second)["stage"] == "connecting");
    connected(); WiFi.ipReady = false;
    wifiSetupPoll(2000, "02:00:00:00:00:02/1", 1, true);
    assert(wifiSetupBusy() && testNvsBlob.empty()); // association without DHCP is not success
    WiFi.ipReady = true;
    wifiSetupPoll(2500, "02:00:00:00:00:02/1", 1, true);
    assert(!wifiSetupBusy() && !testNvsBlob.empty());
    auto before = testNvsBlob;
    assert(execute(get)["networks"].size() == 1);
    String summary; serializeJson(execute(get), summary);
    assert(execute(get)["networks"][0]["password"] == second["password"]);
    assert(execute(get, "02:00:00:00:00:03/1")["networks"].size() == 0);
    auto replace = setup("trial3", true); replace["ssid"] = "replacement";
    assert(execute(replace)["stage"] == "connecting");
    connected(); testNvsFail = true;
    wifiSetupPoll(3000, "02:00:00:00:00:02/1", 1, true);
    assert(!wifiSetupBusy() && before == testNvsBlob && WiFi.SSID() == "device-2g");
    testNvsFail = false;
    auto disconnected = setup("trial4", true); assert(execute(disconnected)["stage"] == "connecting");
    wifiSetupPoll(3000, "02:00:00:00:00:02/1", 2, true);
    assert(!wifiSetupBusy() && before == testNvsBlob); // a new connection cannot finish old work
    auto timed = setup("trial5", true); assert(execute(timed)["stage"] == "connecting");
    wifiSetupPoll(26000, "02:00:00:00:00:02/1", 1, true);
    assert(!wifiSetupBusy() && before == testNvsBlob);
    auto select = command("wifi_select", "select1");
    assert(execute(select, "02:00:00:00:00:03/1")["error"] == "not_found");
    assert(execute(select, "02:00:00:00:00:02/1", 120000, 0, false)["error"] == "occupied");
    assert(execute(select, "02:00:00:00:00:02/1", 0)["error"] == "window_required");
    select["bridge_id"] = "test-bridge";
    assert(execute(select, "02:00:00:00:00:02/1", 0, 60000)["stage"] == "accepted");
    wifiSetupRadioClosed("normal_window_closed");
    assert(wifiSetupBusy());
    String chosenSsid, chosenPassword;
    assert(wifiSchemesCredentials(chosenSsid, chosenPassword));
    WiFi.begin(chosenSsid.c_str(), chosenPassword.c_str()); connected();
    wifiSetupPoll(3000, "", 3, false);
    assert(!wifiSetupBusy()); // selection can commit after normal BLE closes
    before = testNvsBlob;
    assert(wifiSchemesBegin()); // reboot reloads the last successful whole record
    String ssid, pass; assert(wifiSchemesCredentials(ssid, pass) && ssid == "device-2g");
    auto invalid = setup("nul1", true); invalid["ssid"] = std::string("a\0b", 3);
    assert(execute(invalid)["error"] == "shape");
    auto remove = command("wifi_scheme_delete", "delete1");
    assert(execute(remove)["persisted"] == true);
    assert(execute(get)["networks"].size() == 0);
    assert(!wifiSchemesCredentials(ssid, pass));
    for (int i = 0; i < 4; ++i) {
        auto bounded = setup(("bounded" + std::to_string(i)).c_str());
        bounded["network_id"] = "n" + std::to_string(i);
        bounded["host_ssid"] = std::string(31, '\x01') + (char)('a' + i);
        bounded["ssid"] = std::string(32, '\x02');
        assert(execute(bounded)["stage"] == "connecting"); connected();
        wifiSetupPoll(3000, "02:00:00:00:00:02/1", 1, true);
    }
    get["request_id"] = std::string(64, 'r');
    assert(execute(get)["networks"].size() == 4); // maximal escaping is paged below ATT 512
    get["offset"] = 120; get["snapshot"] = 0;
    JsonDocument stale;
    deserializeJson(stale, wifiSetupCommand(get, "02:00:00:00:00:02/1", 2, 1000, 120000, 0, true));
    assert(stale["error"] == "read_snapshot");
    for (const char *op : {"wifi_setup", "wifi_select", "wifi_scheme_delete", "wifi_setup_status", "wifi_setup_cancel"}) {
        auto invalid = command(op, std::string(64, 'r').c_str());
        auto response = execute(invalid, "02:00:00:00:00:02/1", 0);
        assert(response["ok"] == false);
    }
    auto full = setup(std::string(64, 'r').c_str());
    full["network_id"] = std::string(32,'n'); full["ssid"] = "full"; full["host_ssid"] = "different";
    // Free a slot, then measure all progress/result fields with both max IDs.
    auto freeSlot = command("wifi_scheme_delete", "free", "n0"); assert(execute(freeSlot)["persisted"] == true);
    assert(execute(full)["stage"] == "connecting"); connected();
    wifiSetupPoll(3000, "02:00:00:00:00:02/1", 1, true);
    auto query = command("wifi_setup_status", std::string(64,'q').c_str()); query["query_id"] = std::string(64,'r');
    assert(execute(query)["persisted"] == true);
    std::cout << "Max final Wi-Fi frame=" << maximumFrame << " bytes\n";
    std::cout << "Wi-Fi transaction: DHCP, replay, conflict, bond, cancel, timeout, reconnect, storage, reboot, owner and redaction checks passed\n";
}
