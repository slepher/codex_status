"""Guard obsolete autonomous association in the actual firmware adapter."""
from pathlib import Path
firmware = Path(__file__).resolve().parents[1] / "src/firmware"
src = "\n".join(path.read_text(encoding="utf-8") for path in firmware.glob("*.cpp"))
application = (firmware / "application.cpp").read_text(encoding="utf-8")
radio = (firmware / "radio.cpp").read_text(encoding="utf-8")
transport = (firmware / "transport.cpp").read_text(encoding="utf-8")
assert "connectBest(" not in src, "normal startup still blocks on legacy Wi-Fi"
assert "retryWifi(" not in src, "autonomous retry of cached network still present"
assert "WiFi.scanNetworks(" not in src, "legacy scan fallback still reachable"
assert "WiFi.reconnect(" not in src, "power-save path still reassociates independently"
setup = application[application.index("void initialize("):application.index("void tick() {")]
assert "announceUdp.begin(" not in setup, "BLE-only boot touches uninitialized TCP/IP"
assert "startNormalMode();" not in setup, "startup bypasses BLE contact gate"
assert "v2RendezvousBegin(power::state.deepWakePath);" in setup
assert "WiFi.setAutoReconnect(false);" in setup, "USB maintenance inherits driver reconnect"
assert setup.index("loadOrIssueAuthToken();") < setup.index("v2RendezvousBegin(power::state.deepWakePath);"), "BLE bootstrap lacks operation token"
normal = radio[radio.index("void startNormalMode() {"):]
assert "WiFi.begin(" not in normal and "WiFi.mode(WIFI_STA)" not in normal
loop = application[application.index("void tick() {"):]
assert loop.index("serviceV2Ble();") < loop.index("wifiSetupPoll(")
assert "nextBleContactMs" in loop, "USB offline has no bounded BLE retry"
print("PASS: production startup/loop adapter has BLE-first contact and no autonomous legacy association")

# BLE bootstrap claim retains both device-operation token and command session.
claim = transport[transport.index("handleBleClaim(JsonDocument &doc) {"):transport.index("static void __attribute__((noinline)) handleBleWifiCapabilities(", transport.index("handleBleClaim(JsonDocument &doc) {"))]
assert "handleBleClaim(doc);" in src
assert "v2CheckCommandSession" in claim and 'doc["operation_token"]' in claim and "authValid()" in claim
assert "operationToken.size() == strlen(operationToken.c_str())" in claim
assert 'doc["force"].isNull()' in claim and 'doc["release"].isNull()' in claim
assert "ownerApplyClaim(args, allowed)" in claim
print("PASS: BLE claim adapter retains token, session and no-force authorization")

# Initial INFO must have the real provider before bleBegin publishes it.
ble_start=radio[radio.index("void enterBleOn("):radio.index("void bleOff(")]
assert ble_start.index("bleSetInfoProvider(infoExtraJson);") < ble_start.index("bleBegin("), "initial INFO has no MAC provider"
print("PASS: initial INFO provider installed before GATT publication")
