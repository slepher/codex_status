#include "../src/ble_recovery.h"
#include "../src/wifi_legacy.h"
#include <cassert>
#include <cstdio>

int main() {
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
