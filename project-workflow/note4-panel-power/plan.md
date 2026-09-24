# Note4 panel power modes

1. Add a persisted Note4-only `pm/panel_pwr` setting. Default `keep`; expose readback and token-gated runtime switch, plus USB serial switch.
2. Apply the setting to the Note4 rail at panel sleep and deep-sleep hold/release. Keep the internal high-voltage supply off between refreshes.
3. Preserve a verified old-frame baseline across deep sleep for the `off_cache` path and use the same safe partial-refresh fallback rules in both modes.
4. Build both firmware targets and inspect the diff. Do not flash until hardware testing is approved.
