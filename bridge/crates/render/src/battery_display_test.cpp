#include "battery_display.h"
#include "v2_state.h"
#define CHECK(x) do { if (!(x)) return __LINE__; } while (0)

extern "C" int codex_battery_display_check() {
    static_assert(sizeof(BatteryDisplay) == 16, "display-only RAM budget");
    BatteryDisplay battery;
    unsigned reads = 0;
    uint32_t mv = 4164; // 96%
    auto read = [&]() { ++reads; return mv; };
    CHECK(battery.percent() == -1);
    CHECK(battery.poll(0, read) && battery.percent() == 96 && reads == 1);
    // Rendering/diagnostics and early polls cannot consume new samples.
    mv = 3300;
    for (unsigned now = 0; now < 5000; ++now) {
        CHECK(!battery.poll(now, read) && battery.percent() == 96);
    }
    CHECK(reads == 1 && battery.rawMilliVolts() == 4164);
    CHECK(battery.poll(5000, read) && battery.percent() == 86 && reads == 2);
    CHECK(battery.rawMilliVolts() == 3300 && battery.sampledAt() == 5000);
    // Protection acts on the fresh raw reading, even while display remains high.
    CHECK(v2BatteryPowerOff(false, batteryPercentFromMilliVolts(mv)));
    CHECK(!v2BatteryPowerOff(false, battery.percent()));
    CHECK(!v2BatteryPowerOff(true, batteryPercentFromMilliVolts(mv)));

    BatteryDisplay jitter;
    CHECK(jitter.poll(0, [] { return 4164u; }));
    for (unsigned n = 1; n <= 90; ++n) {
        const uint32_t raw = 4155 + (n % 3) * 9; // 95/96/97
        CHECK(jitter.poll(n * 5000, [=] { return raw; }));
        CHECK(jitter.percent() == 96);
    }

    BatteryDisplay delayed;
    reads = 0; mv = 4200;
    CHECK(delayed.poll(0, read));
    mv = 3300;
    CHECK(delayed.poll(17000, read) && reads == 2 && delayed.percent() == 90);
    CHECK(delayed.sampledAt() == 17000);
    CHECK(!delayed.poll(17000, read) && !delayed.poll(19999, read));
    CHECK(delayed.poll(20000, read) && reads == 3 && delayed.percent() == 81);

    BatteryDisplay wrap;
    const uint32_t start = UINT32_MAX - 1000;
    CHECK(wrap.poll(start, read));
    CHECK(!wrap.poll(uint32_t(start + 4999), read));
    CHECK(wrap.poll(uint32_t(start + 5000), read));

    BatteryDisplay empty;
    CHECK(empty.poll(0, [] { return 0u; }) && empty.percent() == 0);
    CHECK(empty.poll(5000, [] { return 5000u; }) && empty.percent() == 10);
    for (unsigned n = 2; n < 100; ++n) CHECK(empty.poll(n * 5000, [] { return 4200u; }));
    CHECK(empty.percent() == 100);
    for (unsigned n = 100; n < 200; ++n) CHECK(empty.poll(n * 5000, [] { return 3300u; }));
    CHECK(empty.percent() == 0);
    BatteryDisplay reboot;
    CHECK(reboot.poll(100, [] { return 3750u; }) && reboot.percent() == 50);
    CHECK(batteryPercentFromMilliVolts(3300) == 0 && batteryPercentFromMilliVolts(4200) == 100);
    return 0;
}
