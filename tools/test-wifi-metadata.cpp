#include "network_display_state.h"
#include <cassert>
int main() {
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
}
