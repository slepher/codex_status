"""Guard obsolete autonomous association in the actual firmware adapter."""
from pathlib import Path
src = (Path(__file__).resolve().parents[1] / "src/main.cpp").read_text(encoding="utf-8")
assert "connectBest(" not in src, "normal startup still blocks on legacy Wi-Fi"
assert "retryWifi(" not in src, "autonomous retry of cached network still present"
assert "WiFi.scanNetworks(" not in src, "legacy scan fallback still reachable"
assert "WiFi.reconnect(" not in src, "power-save path still reassociates independently"
setup = src[src.index("void setup() {"):src.index("static void handleSerialCli()", src.index("void setup() {"))]
assert "announceUdp.begin(" not in setup, "BLE-only boot touches uninitialized TCP/IP"
assert "startNormalMode();" not in setup, "startup bypasses BLE contact gate"
assert "v2RendezvousBegin(deepWakePath);" in setup
assert "WiFi.setAutoReconnect(false);" in setup, "USB maintenance inherits driver reconnect"
assert setup.index("loadOrIssueAuthToken();") < setup.index("v2RendezvousBegin(deepWakePath);"), "BLE bootstrap lacks operation token"
normal = src[src.index("static void startNormalMode() {"):src.index("static void persistMode()", src.index("static void startNormalMode() {"))]
assert "WiFi.begin(" not in normal and "WiFi.mode(WIFI_STA)" not in normal
loop = src[src.index("void loop() {"):]
assert loop.index("serviceV2Ble();") < loop.index("wifiSetupPoll(")
assert "nextBleContactMs" in loop, "USB offline has no bounded BLE retry"
print("PASS: production startup/loop adapter has BLE-first contact and no autonomous legacy association")

# BLE bootstrap claim retains both device-operation token and command session.
claim = src[src.index("handleBleClaim(JsonDocument &doc) {"):src.index("static void serviceV2Ble() {")]
assert "handleBleClaim(doc);" in src
assert "v2CheckCommandSession" in claim and 'doc["operation_token"]' in claim and "authValid()" in claim
assert "operationToken.size() == strlen(operationToken.c_str())" in claim
assert 'doc["force"].isNull()' in claim and 'doc["release"].isNull()' in claim
assert "ownerApplyClaim(args, allowed)" in claim
print("PASS: BLE claim adapter retains token, session and no-force authorization")
