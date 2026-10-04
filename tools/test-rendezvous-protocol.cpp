#include "rendezvous_protocol.h"
#include <cassert>
int main() {
    RendezvousHandoffProof proof;
    proof.sessionAccepted = proof.ownerAccepted = true;
    proof.bridge = proof.expectedBridge = "bridge";
    proof.id = proof.expectedId = "wake"; proof.challenge = proof.expectedChallenge = "http-only";
    proof.planId = proof.acceptedPlanId = 101;
    proof.generation = proof.expectedGeneration = 2; proof.wakeSeq = proof.expectedWakeSeq = 9;
    assert(rendezvousProofMatches(proof));
    proof.ownerAccepted=false;assert(!rendezvousProofMatches(proof));proof.ownerAccepted=true;
    proof.challenge="old";assert(!rendezvousProofMatches(proof));proof.challenge=proof.expectedChallenge;
    proof.planId=100;assert(!rendezvousProofMatches(proof));proof.planId=101;
    proof.wakeSeq=8;assert(!rendezvousProofMatches(proof));proof.wakeSeq=9;
    proof.expectedBridge="";assert(!rendezvousProofMatches(proof));
    RendezvousProtocol rv;
    rv.begin(0,4000); rv.connected(100); rv.admitHttp(500,120000);
    assert(rv.needsHttp() && rv.deadline()==500+RV_HANDOFF_MS);
    assert(!rv.handoff(700,true)); // listening is not ready yet
    rv.httpReady(); assert(!rv.handoff(800,false)); // stale identity/challenge
    assert(!rv.tick(10000)); // slow DHCP does not close BLE at 200ms
    assert(rv.handoff(15000,true)); assert(rv.deadline()==15200);
    assert(rv.handoff(15100,true) && rv.deadline()==15200);
    assert(rv.tick(15200) && rv.handedOff());
    assert(rv.handoff(16000,true) && !rv.active()); // lost ACK/readback
    rv.begin(0,4000); rv.connected(100); rv.admitHttp(500,2000);
    assert(rv.deadline()==2000 && rv.tick(2000)); // shorter legal plan wins
    assert(!rv.handoff(2001,true));
    rv.begin(0,4000); rv.connected(100); rv.finishExchange(500,0);
    assert(rv.deadline()==700 && rv.tick(700)); // sleep ACK grace
    rv.begin(0,4000); rv.connected(100); rv.admitHttp(500,90000);
    assert(rv.tick(500+RV_HANDOFF_MS) && !rv.handedOff()); // unreachable HTTP bounded
    rv.begin(0,4000); rv.planReceived(); rv.finishExchange(100,0); rv.tick(300);
    rv.begin(1000,4000); rv.connected(1010);
    assert(!rv.hasPlan() && !rv.tick(1210)); // old sleep cannot finish new exchange
    rv.planReceived(); rv.admitHttp(1300,120000); rv.httpReady();
    assert(rv.handoff(1500,true) && rv.tick(1700));
    rv.begin(0,4000); rv.connected(100); rv.admitHttp(500,120000); rv.httpReady();
    rv.admitHttp(1000,1500); assert(rv.deadline()==1500 && rv.tick(1500));
    rv.begin(0,4000); rv.connected(100); rv.admitHttp(500,120000); rv.httpReady();
    rv.finishExchange(1000,0); assert(rv.deadline()==1200 && rv.tick(1200));
    rv.begin(0,4000); rv.cancel(); assert(!rv.active());
}
