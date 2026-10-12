// Exercise the actual callback handlers; no radio, credentials or NVS writes.
#define CODEX_WIFI_SETUP_HOST_TEST
#include "../src/wifi_setup.cpp"
#include "../src/rendezvous_protocol.h"
#include "../src/v2_sync.h"
#include <cassert>
#include <iostream>
int main() {
    wifiSetupConnectionStarted("fixture");
    assert(wifiSetupConnectionFailure()==nullptr);
    wifiSetupDisconnected(201);
    wifiSetupDisconnected(36); // teardown cannot erase AP-not-found
    assert(!strcmp(wifiSetupConnectionFailure(),"network_not_found"));
    assert(wifiSetupConnectionReason()==201);
    assert((wifiSetupDisconnectEvent() & 0xffff)==36);
    wifiSetupGotIp();
    assert(wifiSetupConnectionFailure()==nullptr && wifiSetupIpEvent()==1);
    wifiSetupConnectionStarted("fixture");
    wifiSetupDisconnected(202);
    wifiSetupDisconnected(36);
    assert(!strcmp(wifiSetupConnectionFailure(),"authentication_failed"));
    wifiSetupConnectionStarted("next-fixture");
    assert(wifiSetupConnectionFailure()==nullptr);
    assert(syncDiagnosticText("[net] stage=network_start result=accepted"));
    assert(syncDiagnosticText("[net] stage=wifi_ip count=1"));
    assert(syncDiagnosticText("[sync] stage=end reason=sync_total_budget_exhausted"));
    assert(!syncDiagnosticText("unselected verbose text"));
    assert(syncBudgetFailure(90999,1000,90000,0)==nullptr);
    assert(!strcmp(syncBudgetFailure(91000,1000,90000,0),"sync_total_budget_exhausted"));
    assert(!strcmp(syncBudgetFailure(1001,1000,91000,1000),"sync_no_progress"));
    std::cout << "PASS: AP/auth reason survives teardown; new attempt/IP resets it; critical stages retained; budgets distinct\n";
}
