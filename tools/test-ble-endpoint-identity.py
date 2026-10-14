"""Run the production endpoint handler/codec with Wi-Fi initially off.

Radio, peer and storage are HAL stubs. Requires an existing --out directory.
"""
import argparse
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--out', type=Path, required=True)
args = parser.parse_args()
assert args.out.is_dir(), 'pre-create output directory according to AGENTS.md'
source = (ROOT / 'src/firmware/transport.cpp').read_text()
start = source.index('void handleBleEndpoint(')
handler = source[start:source.index('static void randomHex(', start)]
radio = (ROOT / 'src/firmware/radio.cpp').read_text()
mac = radio[radio.index('String macText() {'):radio.index('WifiPhase wifiPhase() {')]
prefix = r'''
#include "ble_wire.h"
#include <cassert>
#include <cstdio>
static bool wifiStarted = false, bonded = true;
static int storageWrites = 0;
static String lastReply;
struct WifiHal {
    // Installed SDK returns zeros when the STA netif does not exist.
    String macAddress() { return wifiStarted ? "02:00:00:00:00:01" : "00:00:00:00:00:00"; }
} WiFi;
namespace radio {
static uint8_t hostMac[] = {2, 0, 0, 0, 0, 1};
'''
stubs = r'''
}
static void bleNotifyStatus(const String &reply) { lastReply = reply; }
static String blePeerAddress() { return "02:00:00:00:00:02"; }
static String blePeerIdentity() { return "02:00:00:00:00:02/1"; }
static bool blePeerIsBonded() { return bonded; }
static bool blePeerIsEncrypted() { return bonded; }
static const char *storeCheckEndpoint(const String&, uint16_t, const String&, const String&, const String&) { return nullptr; }
static const char *storeUpsert(const String&, const String&, uint16_t, const String&, const String&, const String&, const String&, bool) { ++storageWrites; return nullptr; }
static uint32_t esp_random() { return 42; }
namespace power { struct Recovery { uint64_t physicalUntil() { return 300000; } };
struct State { Recovery bleRecovery; } state; }
namespace firmware { namespace clock { static uint64_t v2NowMs() { return 1000; } }
namespace diagnostics { static void traceSetTransport(int) {} static void traceSetStage(int) {} }
namespace transport {
constexpr int TRACE_STAGE_COMMAND = 4, TRACE_STAGE_REPLY = 5;
'''
checks = r'''
}}
int main() {
    for (bool started : {true, false}) {
        wifiStarted = started;
        for (bool secure : {true, false}) for (bool checkOnly : {true, false}) {
            bonded = secure;
            JsonDocument request;
            request["schema"] = 2; request["port"] = 8765;
            request["host"] = "192.0.2.1"; request["token"] = "test-only";
            request["bridge_id"] = "test-bridge"; request["request_id"] = "ep-test";
            request["check_only"] = checkOnly;
            String text; serializeJson(request, text);
            int before = storageWrites;
            firmware::transport::handleBleEndpoint(text);
            JsonDocument reply; assert(!deserializeJson(reply, lastReply));
            if (reply["device_mac"] != radio::macText()) {
                std::printf("FAIL: Wi-Fi started=%d endpoint ACK MAC=%s expected=%s\n",
                    started, reply["device_mac"].as<const char*>(), radio::macText().c_str());
                return 1;
            }
            assert(reply["ack"] == "endpoint" && reply["ok"] == secure);
            assert(reply["stage"] == (!secure ? "binding" : checkOnly ? "verified" : "persisted"));
            assert(storageWrites == before + (secure && !checkOnly));
        }
    }
    std::puts("PASS: 8 endpoint ACK cases preserve device identity with Wi-Fi off/on; verification remains read-only");
}
'''
cpp = args.out / 'ble-endpoint-identity.cpp'
binary = args.out / 'ble-endpoint-identity'
cpp.write_text(prefix + mac + stubs + handler + checks)
subprocess.run(['c++', '-std=c++17', '-DARDUINOJSON_ENABLE_ARDUINO_STRING=1',
    '-include', str(ROOT / 'bridge/crates/render/shim/Arduino.h'),
    '-I' + str(ROOT / 'bridge/crates/render/shim'),
    '-I' + str(ROOT / '.pio/libdeps/zectrix-note4-b/ArduinoJson/src'),
    '-I' + str(ROOT / 'src'), str(cpp), str(ROOT / 'src/ble_wire.cpp'),
    str(ROOT / 'src/v2_sync.cpp'), '-o', str(binary)], check=True)
subprocess.run([str(binary.resolve())], check=True)
