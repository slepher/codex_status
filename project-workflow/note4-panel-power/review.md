# Review

- The switch is Note4-only and stored before deep-sleep GPIO holds are released. The HTTP mutation reuses `requestAuthorized()`, preserving the existing token gate.
- The panel logic rail is selected independently of SSD2683 internal high-voltage power, which remains off after each waveform.
- Cache restore requires an RTC marker and a matching hash of the full 15 KB file. The RTC clock window is checked for bounds before overlaying it on the cached frame. A missing or bad cache cannot enable the partial path.
- The normal-wake refresh policy and template renderer are unchanged. The 1.54 build remains a required regression check.
- Hardware evidence is still needed for current draw, repeated minute partials, ghosting, and button wake behavior; a compiler result cannot establish those properties.
