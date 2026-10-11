#include "../src/data_activity_led.h"
#include <cassert>

int main() {
    DataActivityLed led;
    assert(!led.on && !led.tick(1000));
    assert(led.activity(1000) && led.on);
    assert(led.tick(1080) && !led.on);
    assert(!led.tick(1160) && !led.on);
    assert(led.activity(2000));
    for (uint32_t now = 2080; now <= 2800; now += 80) {
        assert(!led.activity(now - 1));
        const bool before = led.on;
        assert(led.tick(now) && led.on != before);
    }
    assert(!led.tick(2960) && !led.on);
    assert(led.activity(UINT32_MAX - 39));
    assert(led.tick(40) && !led.on);
    assert(!led.tick(120) && !led.on);
    assert(led.activity(121) && led.on);
    assert(!led.tick(10000) && !led.on); // Missed callbacks do not leave it lit.
}
