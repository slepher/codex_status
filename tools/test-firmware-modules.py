"""Guard the firmware's Arduino glue and integration-state boundaries."""
from pathlib import Path
import re

root = Path(__file__).resolve().parents[1]
entry = (root / "src/main.cpp").read_text(encoding="utf-8")
firmware = root / "src/firmware"
assert re.search(r"void setup\(\)\s*\{\s*firmware::application::initialize\(FW_VERSION, OTA_IMAGE_IDENTITY\);\s*\}", entry)
assert re.search(r"void loop\(\)\s*\{\s*firmware::application::tick\(\);\s*\}", entry)
assert "getArduinoLoopTaskStackSize() { return 12288; }" in entry
assert "codex-status-ota-v1|" in entry
assert not re.search(r"\b(?:WiFi|WebServer|JsonDocument|Preferences|RTC_DATA_ATTR)\b", entry)
assert not re.search(r"#include\s*[<\"].*\.(?:cpp|inc)[>\"]", entry)

modules = ("application", "clock", "diagnostics", "display", "platform", "power", "radio", "sync", "transport")
for module in modules:
    source = (firmware / f"{module}.cpp").read_text(encoding="utf-8")
    header = (firmware / f"{module}_internal.h").read_text(encoding="utf-8")
    assert f"namespace firmware::{module}" in source
    assert not re.search(r"using namespace firmware::", source), module
    # Only typed storage objects cross the internal header boundary; individual
    # legacy globals may not leak back through a shared extern declaration list.
    assert all(re.fullmatch(r"extern (?:State|Retained) (?:state|retained);", line)
               for line in header.splitlines() if line.startswith("extern ")), module
    if "extern Retained retained;" in header:
        assert "RTC_DATA_ATTR Retained retained;" in source, module

logger = (root / "src/dev_log.cpp").read_text(encoding="utf-8")
assert "firmware::sync::appendLog(output, count, flags);" in logger
assert "extern SyncRtc" not in logger
app = (firmware / "application.cpp").read_text(encoding="utf-8")
display = (firmware / "display.cpp").read_text(encoding="utf-8")
radio = (firmware / "radio.cpp").read_text(encoding="utf-8")
assert "radio::pollWifi();\n    display::serviceNetworkGlyph();" in app
assert "state.networkGlyph.observe(radio::state.wifiUp, currentBridgeOfflineMinutes(), phase, blink);" in display
assert "memcmp(state.lastDisplayedFrame, state.frame, EPD_FB_BYTES) == 0" in display
assert "if (state.networkGlyph.attempts) state.forceCleanRefresh = true;" in display
rv = radio.split("static void rendezvousWifiTick() {", 1)[1].split("void v2RendezvousTick()", 1)[0]
assert "WiFi.status() == WL_CONNECTED" in rv and "state.wifiConnActive = false;" in rv
print("PASS: Arduino glue, state/RTC ownership, log boundary and independent network redraw integration")
