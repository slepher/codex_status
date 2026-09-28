"""Deterministic overlap bounds for the proposed bridge_first 60 s windows.

This checks clock geometry only. It does not model BLE packet delivery,
Windows publisher scheduling, radio coexistence, or real deep sleep.
"""

PERIOD_MS = 60_000
SCAN_MS = 1_500
PC_BEFORE_MS = 1_000
PC_AFTER_MS = 1_500
MIN_OVERLAP_MS = 500  # sensitivity threshold, not a measured BLE guarantee


def overlap_ms(error_ms):
    return max(0, min(PC_AFTER_MS, error_ms + SCAN_MS)
               - max(-PC_BEFORE_MS, error_ms))


def first_failure(rate_ppm, hours=24, threshold_ms=MIN_OVERLAP_MS):
    for window in range(int(hours * 3_600_000 / PERIOD_MS) + 1):
        error = rate_ppm * window * PERIOD_MS / 1_000_000
        if overlap_ms(error) < threshold_ms:
            return window * PERIOD_MS / 3_600_000
    return None


def main():
    print(f"period={PERIOD_MS}ms scan={SCAN_MS}ms pc=[-{PC_BEFORE_MS},+{PC_AFTER_MS}]ms")
    print(f"zero-phase overlap={overlap_ms(0)}ms")
    print("error_ms: overlap_ms")
    for error in (-3000, -2500, -2000, -1000, 0, 500, 1000, 1500, 2000):
        print(f"{error:>5}: {overlap_ms(error):>4}")
    print(f"first window with <{MIN_OVERLAP_MS}ms overlap, no recalibration:")
    for rate in (20, 100, 1000, -20, -100, -1000):
        print(f"{rate:+5} ppm: {first_failure(rate)} hours")
    print("10-window calibration drift:")
    for rate in (20, 100, 1000, 10000):
        error = rate * 10 * PERIOD_MS / 1_000_000
        print(f"{rate:>5} ppm: {error:.0f}ms error, {overlap_ms(error):.0f}ms overlap")
    assert overlap_ms(0) == SCAN_MS
    assert overlap_ms(1500) == 0
    assert overlap_ms(-2500) == 0


if __name__ == "__main__":
    main()
