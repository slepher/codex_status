# Task and acceptance

- Persist the Note4 rail choice before wake holds are released. Default to `keep`; allow `off_cache` through a token-gated HTTP write and USB serial command. Expose the value through device status.
- On every successful normal entry to deep sleep, persist the exact displayed frame with an integrity marker. On thin wake, merge the retained clock-window pixels into that frame and seed the SSD2683 transition shadow. If any premise fails, use a full refresh.
- Keep the 1.54 target's GPIO6 behavior and refresh path unchanged.
- Validate with Note4 B and 1.54 builds, `git diff --check`, then bench test both settings after an approved flash.
