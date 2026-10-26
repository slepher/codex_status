#include "network_display_state.h"
#include <cassert>
int main() {
    assert(bridgeOfflineMinutes(1700000010,1700000000,true,6,-1,true)==0);
    assert(bridgeOfflineMinutes(1700000010,1700000000,true,6,-1,false)==-1);
    assert(bridgeOfflineMinutes(0,0,false,6,-1,true)==0);
    assert(wifiPhaseFor(false,false,false,false)==WifiPhase::Idle);
    assert(wifiPhaseFor(false,false,true,true)==WifiPhase::Idle);
    assert(wifiPhaseFor(true,false,true,false)==WifiPhase::Connecting);
    assert(wifiPhaseFor(false,true,false,true)==WifiPhase::Connected);
    assert(wifiPhaseFor(false,false,false,true)==WifiPhase::Off);
    NetworkDisplayState display;
    display.observe(false,-1,WifiPhase::Idle,-1);
    display.rendered(true,0);
    display.observe(false,-1,WifiPhase::Connecting,1);
    assert(display.due(1));
    display.rendered(true,1);
    display.observe(false,-1,WifiPhase::Connecting,0);
    assert(display.due(2));
    display.rendered(true,2);
    display.observe(false,-1,WifiPhase::Off,-1);
    assert(display.due(3));
    display.rendered(false,3);
    assert(!display.due(5002) && display.due(5003));
    display.rendered(true,5003);
    display.observe(false,-1,WifiPhase::Off,-1);
    assert(!display.pending);
    display.observe(true,-1,WifiPhase::Connected,-1,false);
    display.rendered(true,6000);
    display.observe(true,0,WifiPhase::Connected,-1,false);
    assert(display.pending && display.wifiPhase==WifiPhase::Connected);
    display.rendered(true,6001);
    display.observe(true,-1,WifiPhase::Connected,-1,false);
    assert(display.pending); // recovery redraws Bridge without Wi-Fi transition
    display.rendered(true,6002);
    display.observe(true,-1,WifiPhase::Connected,-1,false,true);
    assert(display.due(6003) && display.bleRadio); // automatic BLE also redraws builtin pages
    display.rendered(true,6003);
    display.observe(true,-1,WifiPhase::Connected,-1,false,false);
    assert(display.due(6004) && !display.bleRadio);
    display.rendered(false,6004);
    display.rendered(false,11004);
    display.rendered(false,16004);
    assert(!display.due(21004)); // preserve bounded retries
}
