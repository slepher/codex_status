# Note4 A/B ROM OTA bring-up status

## 2026-09-23 USB keep-awake fix and second authenticated A/B verification

**Device result:** `0.18.19-note4-a` was USB-flashed at `0x20000` after exact
USB serial/MAC `7C:4F:AD:B9:34:08` and ESP32-S3 rev 0.2 identification.
esptool verified the written image hash. Only otadata at `0xD000` for
`0x2000` bytes was erased to select A; NVS, B and storage partitions were
preserved. Serial and HTTP confirmed A boot from `ota_0`, installed
`codex-status-a`, `plugged=true`, PSRAM 8,351,272 bytes free and zero EPD
BUSY failures. Bridge's sleep plan had zero remaining time while the USB unit
remained in light mode and served HTTP.

One authenticated multipart `/doUpdate` then installed
`artifacts/codex-status-0.18.19-note4-b-usbfix.bin`. The task-local client
verified the B image SHA256 and exact A MAC/firmware/slot/template, then got an
operation token via the bonded encrypted BLE link. The OTA POST returned HTTP
200 `UPDATE OK`; the evidence records `upload_count=1`. Subsequent HTTP and
independent USB serial confirm `0.18.19-note4-b` from `ota_1`, next `ota_0`,
same MAC, retained Bundle job `b462a509` and active `codex-status-a`, PSRAM
8,351,272 bytes free and zero EPD BUSY failures. No second OTA was sent.

The OTA client's original postcheck reported a false failure because it
required `commit_seq` to stay exactly 41. It became 42 on reboot: firmware
rotates the active context when the volatile applied-data baseline is unknown,
and `bsSetActive` increments the slot commit sequence. The Bundle job and
template persisted. The client postcheck now requires the same committed job
and a nondecreasing sequence; the original evidence and error are retained.

| Evidence | Path | Result |
|---|---|---|
| A ROM | `artifacts/codex-status-0.18.19-note4-a-usbfix.bin` | SHA256 `1EBEF24CA3AC722C2F43D17E06F6858CDDCCC706372BF6996F2C8CCE607AE35D` |
| B ROM | `artifacts/codex-status-0.18.19-note4-b-usbfix.bin` | SHA256 `E01BED2D64761910AFBF1624B0AE5DEF36CAECC90D36877D20B0D5557623936D` |
| USB A flash | `artifacts/note4-usb-flash-01819-a-usbfix.log`, `artifacts/note4-usb-erase-otadata-01819-a.log` | image hash verified; only otadata erased |
| A HTTP state | `artifacts/note4-01819-a-status.json` | A from `ota_0`, next `ota_1`, template installed |
| OTA transaction | `artifacts/note4-ota-01819-a-to-b.json`, `artifacts/note4-ota-01819-client-retry.log` | one authenticated POST, HTTP 200, B observed |
| B HTTP and serial | `artifacts/note4-01819-b-status.json`, `artifacts/note4-01819-b-status-serial.log` | B from `ota_1`, template installed, USB present |
| USB stay-awake | `artifacts/note4-01819-b-usb-600s.json` | B uptime 610 s, `plugged=true`, `mode=light`, formal plan remaining 0, post-OTA hold 0, same job/template, zero EPD BUSY failures |

Recovery: use the official Note4 ROM download sequence (hold the front round
BOOT button, tap the recessed side RESET, release BOOT). Verify USB VID/PID
`303A:1001` and serial/MAC `7C:4F:AD:B9:34:08` before writing. To boot the
known-good corrected A image, write it at `0x20000`, erase **only** otadata
`0xD000` length `0x2000`, then reset. This preserves Wi-Fi NVS credentials and
the LittleFS template Bundle. B remains at `0x610000`. Never erase the whole
flash for this recovery.

The corrected B ROM passed the 610-second USB stay-awake check after the
post-OTA 300-second hold and formal sleep-plan expiry. It remains reachable
over USB and HTTP. Both PlatformIO Note4 builds and Python syntax checks
passed; `git diff --check` passed with only line-ending warnings from
concurrent Bridge files. No Git commit has been made. Bridge MCP's explicit
light-plan behavior is owned by the concurrent Bridge task, not this firmware
task; the user explicitly asked that task to correct its deep BLE delivery.

## 2026-09-23 USB-connected drop and BLE rendezvous recovery

The installed `0.18.18-note4-b` displayed the first Bundle, then disappeared
from Wi-Fi and USB despite the user's PC USB cable remaining attached. A USB
reinsert briefly re-enumerated COM5, followed by another drop. A 72-second BLE
scan found the exact `CodexStatus-B93408` advertisement. Bridge's read-only
`power_view_v2` showed successful rendezvous at 20:00:28 and 20:01:37 local
time, with formal `sleep` plans 26 and 27. Thus the MCU is cycling through
60-second deep-sleep rendezvous rather than permanently dead; the old image
does not provide a stable HTTP OTA window.

Root cause in firmware: the v2 formal-plan expiry directly called `enterDeep`
without the PC USB keep-awake check already used by `idleDeepDue`. The shared
deep-sleep entry now rejects sleep while `plugged && !rtcDeepOnUsb`, and the
v2 expiry block skips its transition and repeated log in that condition.
`pollPlug` now requires 10 seconds of absent USB SOF before changing to
unplugged, covering short host SOF gaps. There is no VBUS/CHG sense GPIO on
this board, so the USB signal remains PC-host SOF as specified in
`docs/power-state.md`.

Both corrected `0.18.19-note4-a/b` environments built successfully. Prepared
ROMs (not yet flashed) are
`artifacts/codex-status-0.18.19-note4-a-usbfix.bin` SHA256
`1EBEF24CA3AC722C2F43D17E06F6858CDDCCC706372BF6996F2C8CCE607AE35D`
and `artifacts/codex-status-0.18.19-note4-b-usbfix.bin` SHA256
`E01BED2D64761910AFBF1624B0AE5DEF36CAECC90D36877D20B0D5557623936D`.
These recovery steps were subsequently completed as recorded above. No Bridge
source or workflow file was changed.

## 2026-09-23 Blank-device Bundle diagnosis and PSRAM correction

After the user physically woke Note4, the PSRAM ROM was USB-flashed into
`ota_1` with esptool readback hash verification. Serial boot and direct HTTP
confirmed `0.18.18-note4-b`, exact MAC/target, Wi-Fi connected, and
`psram_free=8,351,272` bytes (previously 0); internal heap was 59,232 bytes.
The matching Bridge client then sent all seven 4096-byte/last-2510-byte raw
chunks of the 27,086-byte frozen Bundle. Each raw chunk ACKed and device logs
showed `ok=1`; COMMIT read the full body into PSRAM and passed its content CRC.
The device did **not** install it: the frozen Bundle lacked top-level
`bridge_id`, while its authenticated COMMIT command carried owner `8c94`, so
firmware correctly returned `rejected/owner`; `commit_seq` remains 0. The
concurrent Bridge task owns the payload field and persistent re-seal fix.
After that correction, the same job will be re-tested; no device-side owner
bypass or Bridge source change was made by this task.

At this earlier point the first explicit `codex-status-a` publish was pending;
the later Bridge publish succeeded with job `b462a509` and a photographed
screen. This task owns only Note4 firmware.
Original 4096-byte CHUNK attempts reported Windows 10054 after offsets 8192 or
12288. A 1024-byte diagnostic attempt stalled after offset 14336. The device
could recover over HTTP after USB reset. A temporary Note4 CLI test wrote and
removed 32 KiB from LittleFS in 1 KiB blocks, including the 16 KiB boundary;
each write completed in 0–16 ms. Diagnostic firmware 0.18.18 logged successful
4096-byte writes through the final 2510-byte chunk; no commit ACK was observed.
These results rule out a fixed HTTP 1024-byte limit or a 16 KiB LittleFS
boundary failure. The ordinary Arduino WebServer parser copies whole POST
bodies before the handler. A Note4-only raw callback now streams the authenticated
Bundle CHUNK request into LittleFS in the parser's 1436-byte buffer; the Bridge
task added matching `X-Request-Id`, `X-Session-Nonce`, `X-Offset` headers while
retaining query arguments and 4096-byte chunks. The matched pair passed the
full device chunk and CRC path; installation is pending Bridge payload repair.

The diagnostic `/status.json` exposed `heap_max_alloc=15348` and
`psram_free=0` even though esptool identifies 8 MiB embedded PSRAM. The
Note4 PlatformIO environment inherited an S3 board definition without PSRAM.
Added `board_build.psram_type=opi` and `BOARD_HAS_PSRAM=1` while retaining
16 MiB DIO/40 MHz flash; the first full ESP-IDF rebuild passed and generated
`artifacts/codex-status-0.18.18-note4-b-psram.bin` SHA256
`7C6D4147FE4F01A5FB4782E3589C730C7B1DE2FF25ECDABEBD751513018CD40A`.
The device entered a sleep state where COM5 still enumerated but USB ROM
handshake and HTTP both timed out. The user pressed the physical wake button;
the new ROM was then flashed and PSRAM verified as recorded above. Earlier diagnostic
ROM `artifacts/codex-status-0.18.18-note4-b-diag.bin` SHA256
`B5047A615F8C0F5BC4B3185C73BBC74B0A4902C1498F5A6A0A812DFD5D398593`
and raw-callback/no-PSRAM ROM `artifacts/codex-status-0.18.18-note4-b-raw.bin`
SHA256 `C4A8A16517DFF069B1EDD0BC080963744A302733C126FE03A2899D9EE8AB6A70`
are retained as local evidence. The physically running B slot now uses the
PSRAM ROM.

## 2026-09-23 Note4 publish failure diagnosis (user screenshot)

The screenshot's `a publish is already in progress for this device` was a
second-click conflict with the first in-memory Note4 publish job `6640f3be`.
MCP `platform_overview` shows that job in `waiting` state and pending state
`publishing`; it does not show an installed device bundle. **Correction:** the
selected `codex-status-a` template uses built-in fonts and can use the
existing ABI-1 complete Bundle path. The unfinished incremental asset protocol
is not a blocker for this first-template flow. The isolated old-runtime job was
left intact; no second publish was attempted in that runtime.

At the initial inspection the device reported `BLE ON`, `ble_on=true`,
`v2_bundle=false`, `commit_seq=0`, and zero installed v2 templates. Later,
the default sandboxed command environment reported `Bad access` for outbound
TCP. Two pings succeeded (92/98 ms), and ARP mapped the IP to Note4 MAC.
Outside that sandbox, direct HTTP `/status.json` returned 200; the earlier
claims that Wi-Fi or the device HTTP listener were unavailable were false.
The original Bridge log showed BLE rendezvous attempts ending in
`BLE status rejected`, and `/v2/status` returned 401 because Note4 had zero
stored Bridge endpoints. The `token_cached` flag in the Bridge overview only
meant a token was configured locally; it did not attest that Note4 accepted
it. The old running Bridge binary also registered this 400×300 device with
erroneous 200×200 dimensions. Those were genuine publish blockers.

Follow-up: Note4 firmware 0.18.16-note4-b adds explicit 400×300 capabilities;
its experimental HTTP/power-save diagnostics showed the listener was healthy
and were removed from the cleaned 0.18.17 build. An exact bonded/encrypted BLE
session provisioned a random Bridge endpoint token to Note4 NVS. Direct
authenticated `/v2/status` then returned 200 with `configured=false` and
`commit_seq=0`. The concurrent Bridge task owns the proxy fix and first
complete-Bundle publish. This Note4 task did not edit Bridge source or its
workflow documents.

## 2026-09-23 Bridge runtime binding and local Profile (later user request)

After a user screenshot showed the legacy `配置 ▾` menu rather than the
Note4 Profile, the Bridge was moved to isolated ignored runtime data at
`artifacts/note4-bridge-data/` (same executable, one active process pair).
The original `bridge/target/debug/data/` was left intact after isolation. The isolated
instance contains only Note4 in platform devices, and its saved
`codex-status-a` source/compiled CRCs match the original. Its v2 Profile
contains that one template with sync off. Windows UI inspection of the live
`设备` tab confirmed `Note4`, `0.18.13-note4-b`, `ota_1`, and
`Profile（1–8 项）: 1. codex-status-a / 初始 active`. The device itself
still reports zero installed templates; the Profile is local and unpublished.
The user-created `note4` entry in the top `模板` tab is an **empty legacy
profile** from a separate 200×200 template library; it is not the v2 device
Profile and cannot contain the 400×300 template. The legacy dropdown is
visually clipped at the window edge due to Bridge UI layout. Fixing that
would touch concurrent Bridge source, so this task leaves it unchanged.

The user requested backing up the currently bound 1.54-inch device, binding
Note4 so its live state is visible, and creating a Profile with one 400×300
template. This is a runtime operation; no Bridge source or Bridge workflow
document was changed. The 1.54-inch device's exact `bridge-app.json` and its
platform device record were saved under
`artifacts/bridge-before-note4-bind-20260923/`. SHA256:
`bridge-app.json` `57AE1435C12ACCD45C8BC697093EAE5822C2D11E0B1A4A25E4753B00643206AC`;
`device-record.json` `E4DAA83072F0DE64450554C3365074318941BB4E9BFF926D0DA329267C15AFBF`.
No device token or Wi-Fi password was copied into this backup.

After stopping the old current-build watchdog then main process, the Bridge
runtime config was set to Note4 MAC `7C4FADB93408`, IP `192.168.3.177`,
name `Note4`, and restarted with its panel visible. MCP `bridge_status`
reports Note4 online, `0.18.13-note4-b`, `ota_1`, battery 95% at inspection,
Wi-Fi `wd21-la`. Platform state retains the old 1.54-inch device and its
`quad/full/mini` Profile. A local Note4 Profile was saved via
`profile_save_v2`: `template_ids=[codex-status-a]`,
`initial_active_id=codex-status-a`, `sync_enabled=false`. The saved
`codex-status-a` template has render target
`epd-ssd2683-400x300-1bpp`, source CRC `2b523381` and compiled CRC
`41c31abd`. **Nothing was published to Note4.**

Current running Bridge binary registered Note4 with target
`epd-ssd2683-400x300-1bpp` but erroneous `width=200,height=200` because
Note4 firmware `/status.json` lacks the explicit capability fields required
by the concurrent newer Bridge source. The binary also ignored the Profile's
`render_target` field when saving. Publishing is blocked until the actual
firmware/Bridge capability contract is aligned; do not bypass it by editing
persisted capabilities. The current app instance maintains one active MAC/IP
link even though platform state stores multiple device records. To restore the
old active binding, stop the current-build watchdog then main process, copy
the backed-up `bridge-app.json` to
`bridge/target/debug/data/bridge-app.json`, and restart the Bridge without
the isolated `CODEX_STATUS_DATA` override. Its
platform record/Profile was preserved and needs no restore.

Date: 2026-09-23. **Completed:** USB-flashed A booted from `ota_0`; one authenticated Wi-Fi OTA installed B, which booted from `ota_1`. Workspace confirmed by user: `D:\Documents\PlatformIO\Projects\codex_status`. No Git commit.

## Final state and ROMs

At the original OTA verification, Note4 ESP32-S3 rev 0.2, Wi-Fi MAC `7C:4F:AD:B9:34:08`, 16 MiB flash, 8 MiB physical PSRAM. The then-current firmware was `0.18.13-note4-b`, running `ota_1`, next slot `ota_0`. It rejoined SSID `wd21-la` at `192.168.3.177` during verification. Password and device token are absent from this record and the evidence logs. HTTP and USB serial agree on the Note4 target, slot and firmware; B serial log reports LittleFS ready, Wi-Fi connected and auth token loaded from NVS. Display reports four EPD writes and zero BUSY failures. Full-black and half-black diagnostic frames visibly refreshed. The adjusted HD camera photo shows the normal status page with header and text; the unit was upside-down relative to the camera, as the user clarified (button belongs below the screen), so the photographed text direction does not indicate a driver rotation bug. The photo is still too soft for pixel-level text acceptance. Button/wake and wider temperature behavior remain unmeasured; the adapter uses full refresh and room-temperature fallback. No compiled bundle was installed.

| ROM | Size | SHA256 | Evidence |
|---|---:|---|---|
| `artifacts/codex-status-0.18.13-note4-a.bin` | 1,723,696 B | `57CDB610334908606EE07D6315CEBC45C388771A7D08BF839C4D3C4350737473` | `artifacts/note4-build-01813-a.log`, `artifacts/note4-final-a-status-serial.log` |
| `artifacts/codex-status-0.18.13-note4-b.bin` | 1,723,696 B | `0D5ADA4C3D559B98B6B2139A2EA5346B6EA0678D6976C8B73B0CF315D64510E7` | `artifacts/note4-build-01813-b.log`, `artifacts/note4-final-b-status-serial.log` |

Both headers: ESP32-S3, 16 MiB, DIO, **40 MHz**. Independent PlatformIO environments: `zectrix-note4-a`, `zectrix-note4-b`. `partitions_note4.csv`: NVS `0x9000/0x4000`, otadata `0xD000/0x2000`, ota_0 `0x20000/0x5F0000`, ota_1 `0x610000/0x5F0000`, LittleFS storage `0xC00000/0x400000`. Both partition binaries SHA256 `3309265E5627F2B83D2152A1DB8BC851972B0F54983FE3E60A12C090D8B86F28`. Flashed 40 MHz bootloader `artifacts/note4-bootloader-40m.bin` SHA256 `80F92A58A2C05EC25DF91BD838D977081FAA4438FFB27384BA6DF91CB937F0FB`.

## Step log

1. Read latest `PROGRESS.md` and historical Note4 material. Preserved concurrent Bridge and generic-platform edits. Verified COM5 identity and preflash backups before writing. [Official Note4 DevKit V1.0 guide](https://wiki.zectrix.com/en/software/note4-development-guide) documents EPD GPIO 6/8/9/10/11/12/13, buttons 39/18/0 and power hold GPIO17. Followed its [reference EPD driver](https://github.com/itopinion/zectrix-note4-epd-demo/blob/main/components/zectrix_epd/zectrix_epd.cc) for full refresh. Camera showed enclosure/USB, though PCB silkscreen was not visible.
2. Added Note4 target, SSD2683 full-refresh adapter, independent A/B builds and 16 MiB partition file. The first USB A (`0.18.6`) booted from `ota_0` with zero BUSY failures; full-black/half-black panel tests succeeded.
3. Initial 80 MHz bootloader/app showed NVS runtime error `0x110B` (`ESP_ERR_NVS_INVALID_STATE`, following an earlier write failure) and LittleFS mount failure. NVS is a reserved flash partition for persistent settings such as Wi-Fi credentials and the device auth token, not a separate chip. A read-only storage sector matched the factory backup. An app-only 40 MHz build still failed to write. Backed up the live original 16 KiB NVS, then erased that region for diagnosis; erasing alone did not fix the error. With the **40 MHz bootloader and app**, logs changed to `[tpl] store: 0 template(s)` and `[bundle] fs ready`; the device later saved/reloaded Wi-Fi and its auth token. The observed cause was this Note4's 80 MHz bootloader/flash configuration, not proven NVS data corruption. The other 1.54-inch board already requires 40 MHz. Factory NVS metadata is no longer active after the diagnostic erase, but is preserved in NVS and full-flash backups.
4. User supplied Wi-Fi credentials. Provisioned via USB CLI without storing/logging the password. BLE connected to exact name `CodexStatus-B93408`; GATT identity matched the MAC and reported bonded/encrypted peer. Token was obtained only in process memory.
5. Built final `0.18.13` A/B images, checked headers, partition fit/hash and SHA256. USB flashed 40 MHz bootloader and A with esptool hash verification. Serial and HTTP confirmed A, `ota_0`, next `ota_1`, Note4 target/MAC, four EPD writes and zero BUSY failures.
6. Existing Bridge `firmware_ota` was inspected, but its generic device/token cache was ambiguous while another device was present. Used task-local `ota_verify.py` with the project's bonded-BLE GATT token mechanism and exact Note4 identity. It checked B hash/marker and A preflight MAC/target/slot, then sent **one** authenticated multipart `/doUpdate` POST. HTTP transport closed during reboot before an ACK could be captured; no retry or second OTA was sent. Subsequent HTTP `/status.json` confirmed B on `ota_1`, same MAC/target, next `ota_0`; USB serial independently confirmed. B boot log confirmed LittleFS and NVS. This task did not edit Bridge source or its workflow.
7. After the user adjusted lighting/angle, captured a 1280×720 photo. Header and multiple status lines are visible. The user clarified that the button belongs below the screen, so the camera view held the device upside-down; no firmware rotation change was needed. A tentative 180-degree driver correction was immediately reverted **before build or flash**. Ran final whitespace check after documentation edits.

## Evidence index

| Evidence | Path | Result |
|---|---|---|
| Factory 16 MiB backup | `artifacts/note4-preflash-full-flash-16m.bin` | SHA256 `366DEA39643855FD5250D15BB8F23DA3B363ECA1705A0068B9AB5A598E01D110`, size 16,777,216 B; verified before flash |
| Factory partition table | `artifacts/note4-partition-table-0x8000.bin` | SHA256 `A82133FA4CD77C180D65FA75CA3B5C27BCEBFB8CC4D419362838852E995BA9E5` |
| Original live NVS before erase | `artifacts/note4-nvs-live-16k.bin` | SHA256 `17837955ADB5B06FB8F6A4EC59BB66A8C76C7A185951F4B5B6EDC8D3B412C944` |
| Early A/panel | `artifacts/note4-status-0186-serial.log`, `artifacts/note4-panelblack-camera.jpg`, `artifacts/note4-panelstripe-0186-camera.jpg` | `ota_0`, no BUSY failure, full/half-black visible |
| NVS/flash diagnosis | `artifacts/note4-nvs-erase.log`, `artifacts/note4-log-after-40m-bootloader.log` | Filesystems initialized after bootloader correction |
| USB flash | `artifacts/note4-usb-flash-bootloader-40m.log`, `artifacts/note4-usb-flash-01813-a.log` | esptool hash verified |
| Slot A boot | `artifacts/note4-final-a-status-serial.log`, `artifacts/note4-ota-evidence.json` before | A on `ota_0` |
| Authenticated OTA | `artifacts/note4-ota-client.log`, `artifacts/note4-ota-evidence.json`, `project-workflow/note4-ota-bringup/ota_verify.py` | Bonded/encrypted auth, one POST, B confirmed after reboot |
| Slot B and persistent storage | `artifacts/note4-final-b-status-serial.log`, `artifacts/note4-final-b-log-serial.log`, `artifacts/note4-ota-evidence.json` after | B on `ota_1`; LittleFS ready, Wi-Fi and NVS token loaded |
| Final camera | `artifacts/note4-final-b-ota-camera.jpg`, `artifacts/note4-final-b-ota-camera-adjusted-hd.jpg` | Adjusted view shows status page; unit inverted relative to camera, small text soft |

## Recovery

Verify the USB MAC before any write. To return from B to known-good A while keeping the newly provisioned NVS, enter ESP32-S3 ROM download mode, write `artifacts/codex-status-0.18.13-note4-a.bin` at `0x20000`, then erase **only** otadata at `0xD000` length `0x2000`; leave the 40 MHz bootloader and partition table installed. The repo's USB-upload instruction also requires clearing otadata after OTA. This path is documented, not exercised.

For complete factory restore, verify the backup size/SHA above, enter ROM download mode and write the full 16 MiB backup at `0x0`. It restores factory NVS, bootloader, partitions and slots, replacing the new Wi-Fi/BLE state. Example (verify current COM port first):

```powershell
$env:PYTHONIOENCODING = 'utf-8'
C:\Users\cogic\.platformio\penv\Scripts\python.exe -m esptool --chip esp32s3 --port COM5 --baud 460800 write-flash --no-progress --flash-mode dio --flash-freq 80m --flash-size 16MB 0x0 artifacts\note4-preflash-full-flash-16m.bin
```

Neither recovery path was executed. Earlier auto-review denied reading the PC's saved Wi-Fi credential; that command did not execute. The user subsequently supplied the credential directly.

## Completion check

- [x] Two distinct ROMs built, hashes recorded, 40 MHz image/bootloader and dual-slot layout checked.
- [x] USB A from `ota_0`; exactly one authenticated OTA to B, B from `ota_1`, independently verified by HTTP and serial.
- [x] NVS/LittleFS after bootloader fix and after OTA; recovery documented.
- [x] No Bridge source/workflow edit by this task; no Git commit.
- [ ] Pixel-level normal UI text, buttons/wake and wider temperature range still need physical acceptance; status page is visibly rendered.
- [x] `git diff --check`: exit code 0 after final source/documentation edits; Git emitted only CRLF normalization warnings on existing shared files.
