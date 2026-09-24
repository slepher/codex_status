# Task 1: Correct Note4 icon states

- Bridge On and Bridge Off: draw one complete icon for the applicable state.
- Wi-Fi On and Wi-Fi Off: draw one complete icon for the applicable state.
- BLE On: draw only for `device.state == "BLE ON"`; BLE Off: leave blank.
- Deep mode: draw the preexisting `sleep-20` artwork at the BLE cell coordinates.
- Keep date/time/battery vertically centered. Reduce the visible date/time gap from about 12 to 6 px. Right-align battery text in the 44 px region. Match its region height to the font's 26 px line height so the ink is not shifted upward by the old 20 px region.
- Verify JSON and compiled render parity and inspect the visible icon cells for representative states.
