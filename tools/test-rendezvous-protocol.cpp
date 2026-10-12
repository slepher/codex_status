#include "rendezvous_protocol.h"
#include <cassert>
#include <cstring>
int main() {
    assert(syncBudgetFailure(90999,1000,90000,0)==nullptr);
    assert(!strcmp(syncBudgetFailure(91000,1000,90000,0),"sync_total_budget_exhausted"));
    assert(!strcmp(syncBudgetFailure(1001,1000,91000,1000),"sync_no_progress"));
    assert(syncBudgetFailure(1001,1000,1000,0)==nullptr);
    assert(!strcmp(syncBudgetFailure(1001,1000,89999,UINT32_MAX),"sync_no_progress"));
    // Temporary transfer windows are independent of formal Light expiry.
    assert(rendezvousWindowUntil(1000, 3000, 81000) == 81000);
    assert(rendezvousWindowUntil(3000, 3000, 81000) == 81000);
    assert(rendezvousWindowUntil(81000, 3000, 81000) == 0);
    assert(rendezvousWindowUntil(1000, 101000, 81000) == 101000);
    RendezvousProtocol rv;
    // No Bridge/Plan: the production protocol never enters HTTP and expires.
    rv.begin(0,4000);
    assert(!rv.hasPlan() && !rv.needsHttp() && !rv.tick(3999));
    assert(rv.tick(4000) && !rv.needsHttp());

    rv.begin(0,4000); rv.connected(100); rv.planReceived(); rv.awaitNetworkStart(500,120000);
    assert(rv.hasPlan() && !rv.needsHttp() && rv.deadline()==500+RV_NETWORK_MS);
    rv.admitHttp(700,120000);
    assert(rv.needsHttp() && !rv.ready() && rv.deadline()==500+RV_NETWORK_MS);
    rv.httpReady(); assert(rv.ready() && !rv.tick(10000));
    rv.finishExchange(15000,0); assert(rv.deadline()==15200 && rv.tick(15200));
    rv.begin(0,4000); rv.connected(100); rv.planReceived(); rv.awaitNetworkStart(500,2000); rv.admitHttp(700,2000);
    assert(rv.deadline()==2000 && rv.tick(2000)); // shorter legal plan wins
    rv.begin(0,4000); rv.connected(100); rv.finishExchange(500,0);
    assert(rv.deadline()==700 && rv.tick(700)); // sleep ACK grace
    rv.begin(0,4000); rv.connected(100); rv.planReceived(); rv.awaitNetworkStart(500,90000);
    assert(rv.tick(500+RV_NETWORK_MS) && !rv.ready());
    rv.begin(0,4000); rv.planReceived(); rv.finishExchange(100,0); rv.tick(300);
    rv.begin(1000,4000); rv.connected(1010);
    assert(!rv.hasPlan() && !rv.tick(1210)); // old sleep cannot finish new exchange
    rv.planReceived(); rv.awaitNetworkStart(1300,120000); rv.admitHttp(1500,120000); rv.httpReady();
    rv.finishExchange(1500,0); assert(rv.tick(1700));
    rv.begin(0,4000); rv.connected(100); rv.admitHttp(500,120000); rv.httpReady();
    rv.admitHttp(1000,1500); assert(rv.deadline()==1500 && rv.tick(1500));
    rv.begin(0,4000); rv.connected(100); rv.admitHttp(500,120000); rv.httpReady();
    rv.finishExchange(1000,0); assert(rv.deadline()==1200 && rv.tick(1200));
    rv.begin(0,4000); rv.cancel(); assert(!rv.active());
}
