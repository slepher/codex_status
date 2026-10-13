#include "ble_wire.h"
#include "v2_sync.h"
#include <ctype.h>

bool bleWireText(const String &value, size_t limit, bool allowEmpty) {
    if (value.length() > limit || (!allowEmpty && !value.length())) return false;
    // Reject embedded NUL, invalid/overlong UTF-8, surrogates and > U+10FFFF.
    for (size_t i = 0; i < value.length();) {
        uint8_t c = value[i++];
        if (!c) return false;
        if (c < 0x80) continue;
        size_t n = c >= 0xc2 && c <= 0xdf ? 1 : c >= 0xe0 && c <= 0xef ? 2 :
                   c >= 0xf0 && c <= 0xf4 ? 3 : 0;
        if (!n || i + n > value.length()) return false;
        uint8_t first = value[i];
        if ((c == 0xe0 && first < 0xa0) || (c == 0xed && first >= 0xa0) ||
            (c == 0xf0 && first < 0x90) || (c == 0xf4 && first >= 0x90)) return false;
        while (n--) if (((uint8_t)value[i++] & 0xc0) != 0x80) return false;
    }
    return true;
}
bool bleWireFits(const String &bytes) { return bytes.length() && bytes.length() <= BLE_WIRE_MAX; }
static String encoded(const String &text) {
    char buffer[89]; // max input = 64 bytes, STANDARD base64 incl padding
    syncBase64((const uint8_t *)text.c_str(), text.length(), buffer, sizeof(buffer));
    return String(buffer);
}
static String finish(JsonDocument &doc) {
    if (doc.overflowed() || measureJson(doc) > BLE_WIRE_MAX) return String();
    String bytes; const size_t expected = measureJson(doc);
    const size_t written = serializeJson(doc, bytes);
    return written == expected && bytes.length() == expected && bleWireFits(bytes) ? bytes : String();
}
String bleBuildAuth(bool okay, const String &token) {
    JsonDocument doc; doc["ack"] = "auth";
    doc["ok"] = okay && bleWireText(token, 64, false);
    if (doc["ok"] == true) doc["token"] = token;
    return finish(doc);
}
String bleBuildEndpoint(const String &request, const String &mac, const String &principal,
                        const char *error, const char *stage) {
    if (!bleWireText(request,64,false) || !bleWireText(mac,17,false) ||
        !bleWireText(principal,64,false)) return String();
    JsonDocument doc; doc["ack"] = "endpoint"; doc["ble_wire"] = 1; doc["rid"] = encoded(request);
    doc["device_mac"] = mac; doc["pid"] = encoded(principal);
    doc["ok"] = !*error; doc["stage"] = stage;
    doc["result"] = !*error ? "applied" : !strcmp(error,"storage_unknown") ? "unknown" : "rejected";
    if (*error) doc["error"] = error;
    return finish(doc);
}
static bool macValid(const String &mac) {
    if (mac.length() != 17) return false;
    for (size_t i = 0; i < 17; ++i)
        if (i % 3 == 2 ? mac[i] != ':' : !isxdigit((unsigned char)mac[i])) return false;
    return true;
}
String bleBuildInfo(const BleInfoSnapshot &s) {
    if (!macValid(s.mac) || !bleWireText(s.ip, 15) ||
        !bleWireText(s.firmware, 64, false)) return String();
    JsonDocument doc;
    doc["schema"] = 1; doc["model"] = "CodexStatus"; doc["ble_wire"] = 1;
    doc["er"] = 1; // correlated registration with read-only verification
    doc["f"] = encoded(s.firmware); doc["mac"] = s.mac; doc["ip"] = s.ip;
    doc["http_port"] = 80; doc["rendezvous"] = true;
    doc["wifi_setup_protocol"] = s.wifiProtocol;
    doc["wifi_management"] = s.wifiManagement;
    if(s.wifiConnection) doc["wc"] = s.wifiConnection;
    if(s.bleClaim) doc["ble_claim"] = s.bleClaim;
    doc["wake_generation"] = s.generation; doc["wake_seq"] = s.wakeSeq;
    if (s.rfRemainingMs != UINT32_MAX) doc["rf_remaining_ms"] = s.rfRemainingMs;
    doc["pairingWindow"] = s.pairing; doc["peerBonded"] = s.bonded;
    doc["peerEncrypted"] = s.encrypted; doc["bundle_configured"] = s.configured;
    return finish(doc);
}
String bleBuildStatus(const BleStatusSnapshot &s) {
    if (!macValid(s.mac) || !bleWireText(s.nonce, 32, false) ||
        !bleWireText(s.context, 32) || !bleWireText(s.activeTemplate, 16) ||
        !bleWireText(s.job, 16)) return String();
    // Nonce is generated as 32 hex chars. Never accept a truncatable nonce.
    if (s.nonce.length() != 32) return String();
    for (size_t i = 0; i < 32; ++i) if (!isxdigit((unsigned char)s.nonce[i])) return String();
    JsonDocument doc;
    doc["op"] = "status"; doc["device_mac"] = s.mac;
    if (s.rendezvousVersion) doc["rendezvous_v"] = s.rendezvousVersion;
    doc["ac"] = s.automaticContact;
    if (s.networkError) doc["ne"] = !strcmp(s.networkError,"bridge_http_unreachable") ? 2 :
        !strcmp(s.networkError,"wifi_link_lost") ? 3 : 1;
    JsonArray a = doc["s"].to<JsonArray>();
    a.add(s.nonce); a.add(encoded(s.context)); a.add(encoded(s.activeTemplate)); a.add(encoded(s.job));
    a.add(s.appliedSeq); a.add(s.light); a.add(s.planId); a.add(s.remainingS); a.add(s.provisionalS);
    a.add(s.syncAvailable); a.add(s.syncEnabled); a.add(s.rounds); a.add(s.due);
    a.add(s.retrySkip); a.add(s.pending); a.add(s.commitSeq);
    return finish(doc);
}
String bleBuildReply(JsonDocument &doc, const String &request, uint32_t generation, uint32_t wakeSeq) {
    if (!bleWireText(request, 64, false)) return String();
    const bool wifi = doc["ack"] == "wifi";
    doc["ack"] = wifi ? "wifi" : "command";
    doc["ble_wire"] = 1; doc["rid"] = encoded(request);
    doc.remove("request_id"); doc.remove("wake_generation"); doc.remove("wake_seq");
    doc.remove("wake_stage"); doc.remove("wake_cause"); doc.remove("fw_target");
    JsonArray w = doc["w"].to<JsonArray>(); w.add(generation); w.add(wakeSeq);
    if (!doc["active_context_id"].isNull()) {
        JsonString json = doc["active_context_id"].as<JsonString>();
        if (!json) return String();
        String context; context.concat(json.c_str(), json.size());
        if (!bleWireText(context, 32)) return String();
        doc["ctx"] = encoded(context); doc.remove("active_context_id");
    }
    // Occupancy is queried over Wi-Fi (or wifi_owner), never echo its display
    // name, host or arbitrary stored strings in a rejected command.
    doc.remove("owner");
    JsonArray records = doc["records"].as<JsonArray>();
    while (measureJson(doc) > BLE_WIRE_MAX && !records.isNull() && records.size()) {
        records.remove(records.size() - 1); doc["more"] = true;
    }
    String bytes = finish(doc);
    if (bytes.length()) return bytes;
    JsonDocument fallback;
    fallback["ack"] = wifi ? "wifi" : "command";
    fallback["ble_wire"] = 1; fallback["rid"] = encoded(request);
    JsonArray wake = fallback["w"].to<JsonArray>(); wake.add(generation); wake.add(wakeSeq);
    fallback["result"] = "unknown"; fallback["error"] = "reply_too_large";
    if (wifi) fallback["ok"] = false;
    return finish(fallback);
}
