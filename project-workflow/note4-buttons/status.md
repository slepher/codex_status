# Note4 ENTER wake investigation — 2026-09-24

- Installed device `7C4FADB93408` is still `0.18.20-note4-b` on `ota_0`; the `0.18.21` panel-power/cache candidate has not been flashed.
- Direct no-proxy HTTP GETs to `192.168.3.177` returned 200 for `/status.json`, `/history`, and `/log` at 14:33–14:34 +08. Current boot reports `wake=power-on`, `last_wake_code=0`, and history contains only that power-on boot after USB attachment. Opening the serial port can reset the board; the earlier ENTER press cannot be reconstructed from this boot.
- The live log contains repeated `[clk] window write failed; full refresh required`; status showed `epd_busy_fails=8`. This proves clock-window display failures in the current session, but does not prove whether the earlier ENTER press woke the MCU.
- The Note4 build maps ENTER to GPIO0, and `armWakeSources()` configures active-low EXT1 wake on GPIO0 and GPIO18. No source-level wake configuration defect has been established.
- User chose PGUP/GPIO39 for awake-only template cycling. ENTER is the required deep-sleep wake button. A future observed ENTER press while the device is actually deep should be followed immediately by no-proxy `/status.json` and `/history`, without opening COM5; `wake=ext1` would separate button detection from a screen refresh failure.
