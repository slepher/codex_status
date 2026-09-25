# Note4 400×300 template correction — 2026-09-24

- Restored the original `sleep-20` pixels as the `zzz` asset and placed it in the BLE cell for deep mode. BLE OFF leaves that cell blank.
- Wi-Fi On/Off and Bridge On/Off now select complete bitmaps rather than drawing an off bitmap over the on bitmap. Bridge selection uses `device.offline_mins` presence; Wi-Fi selection uses `device.state`.
- Date/time visible gap reduced from about 12 to 6 px (`device.now` x=75 → 69). Battery percentage is right aligned in its 44 px region.
- Root cause of the observed battery text sitting high: `ntthin18` has a 26 px line height, while its text region was only 20 px tall. The renderer centered the full line in that short region, shifting glyph ink up 3 px. The region is now `[344,5,44,26]`; the text element's y remains 5.
- Five host state renders (`BLE ON`, `BLE OFF`, `WIFI OFF`, Bridge offline, deep) compiled and compared with JSON/compiled/serialized rendering at zero pixel difference. A cell-by-cell pixel check matched every selected source bitmap, including the empty BLE OFF cell. The original and restored sleep bitmap bytes match.
- Controlled battery experiment: the old region produced `--%` ink at y=9..21, center 15; the 26 px region produced y=12..24, center 18, against battery outline y=12..23, center 17.5.
- No ROM or Bridge source edits, no Bridge restart, no publication, no device operation, no commit. Template corrections remain local and have not appeared on the device.
