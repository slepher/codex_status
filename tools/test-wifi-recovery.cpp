#include "../src/ble_recovery.h"
#include "../src/boot_capture.h"
#include "../src/wifi_legacy.h"
#include <cassert>
#include <cstdio>

int main() {
    BootCapture capture;
    capture.begin(false, false, 0); // PWR/timer then BOOT during setup
    capture.edge(true, 100); capture.edge(false, 200);
    assert(capture.take(1000)); assert(!capture.take(1001));
    capture.begin(true, false, 0); // held across the first loop
    assert(!capture.take(49)); assert(capture.take(50));
    assert(!capture.take(100)); capture.edge(false, 200);
    assert(!capture.take(201));
    capture.begin(true, true, 0); // GPIO0 already consumed as wake source
    assert(!capture.take(100)); capture.edge(false, 200);
    assert(!capture.take(201));
    capture.edge(true, 300); capture.edge(false, 400);
    assert(capture.take(401));
    capture.begin(false, false, 0); capture.edge(true, 10); capture.edge(false, 30);
    assert(!capture.take(100)); // bounce is not a physical request
    BleRecoveryWindow pwr;
    pwr.lightUntil = 300000; // PWR receives Light without BLE-open preference
    assert(!pwr.active(6977) && pwr.lightRemaining(6977) == 294);
    V2PlanState noPlan;
    assert(v2PowerSleepDecision(true, true, false, false, pwr.lightRemaining(6977)>0,
                               noPlan, true, 0, 0, 6977) == V2_POWER_STAY_LIGHT);
    BleRecoveryWindow recovery;
    recovery.open(1000);
    recovery.open(60000); // traffic/repeated CLI must not renew authorization
    assert(recovery.until == 121000 && recovery.active(120999));
    assert(!recovery.active(121000) && recovery.remaining(121000) == 0);
    V2PlanState plan;
    assert(v2PowerSleepDecision(true, true, false, false, recovery.active(120000),
                               plan, true, 0, 0, 400000) == V2_POWER_STAY_LIGHT);
    assert(v2PowerSleepDecision(true, true, false, false, false,
                               plan, true, 0, 0, 400000) == V2_POWER_BOOT_ENDED);
    recovery.close(); assert(!recovery.active(1));
    recovery = {};
    assert(recovery.toggle(1000));
    assert(recovery.until == 121000 && recovery.lightUntil == 301000);
    assert(!recovery.toggle(60000)); // OFF does not cancel Light or renew it
    assert(recovery.until == 0 && recovery.lightUntil == 301000);
    assert(recovery.toggle(90000)); // a new physical ON renews both
    assert(recovery.until == 210000 && recovery.lightUntil == 390000);
    recovery.open(100000); // protocol/CLI repetition is not a physical toggle
    assert(recovery.lightUntil == 390000 && recovery.until == 210000);
    assert(!recovery.active(210000) && recovery.lightRemaining(210000) == 180);
    assert(recovery.lightRemaining(390000) == 0);
    WifiLegacySchemes schemes;
    WifiLegacyNetwork network;
    strcpy(network.bond, "aa:bb:cc:dd:ee:ff/1");
    strcpy(network.id, "home"); strcpy(network.host, "host-5g"); strcpy(network.ssid, "device-2g");
    assert(!schemes.put(network, false));
    assert(!strcmp(schemes.put(network, false), "exists"));
    assert(schemes.find("other", "home") == -1);
    strcpy(network.id, "other");
    assert(!strcmp(schemes.put(network, false), "mapping_conflict"));
    for (int i = 1; i < 4; ++i) {
        snprintf(network.id, sizeof(network.id), "id%d", i);
        snprintf(network.host, sizeof(network.host), "host%d", i);
        assert(!schemes.put(network, false));
    }
    strcpy(network.id, "full"); strcpy(network.host, "full");
    assert(!strcmp(schemes.put(network, false), "capacity"));
    schemes.seal(); assert(schemes.valid());
    WifiLegacySchemes reboot = schemes; assert(reboot.valid());
    reboot.networks[0].password[0] = 'x'; assert(!reboot.valid());
    reboot = schemes; reboot.erase(reboot.find(reboot.selectedBond, reboot.selectedId));
    reboot.seal(); assert(reboot.valid() && !*reboot.selectedId);
    assert(schemes.valid() && *schemes.selectedId); // failed candidate cannot alter old blob
    puts("BLE recovery / bond isolation / capacity / mapping / atomic candidate checks passed");
}
