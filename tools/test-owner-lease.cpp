// Compile the production store; only clocks/reset/NVS/logging are simulated.
#define CODEX_OWNER_TEST
#include "../src/owner_store.cpp"
#include "../src/v2_claim_command.cpp"
#include <cassert>
#include <iostream>
static OwnerRec claim(const char *id="A",uint32_t lease=300) {
 OwnerRec r;r.id=id;r.lease=lease;ownerClaim(r,false);return r;
}
static void deepWake(uint64_t elapsedUs) {
 ownerTestRtcUs+=elapsedUs;ownerTestMillis=2000;ownerTestReset=ESP_RST_DEEPSLEEP;ownerBegin();
}
int main() {
 OwnerRec out;
 claim();const auto first=ownerDeadline.untilUs;
 assert(ownerRemainingS()==300 && !ownerAllows("B"));
 assert(ownerDeadline.untilUs==first);
 // Repeated wake must never restart the lease or exclude asleep time.
 for(int i=0;i<4;++i) { deepWake(60000000);assert(ownerGet(out));assert(ownerDeadline.untilUs==first);assert(ownerRemainingS()==300-60*(i+1)); }
 deepWake(60000000);assert(!ownerGet(out));assert(ownerTestStrings.empty());
 claim();ownerTestRtcUs+=299999999;assert(ownerGet(out)&&ownerRemainingS()==1);
 ++ownerTestRtcUs;assert(!ownerGet(out));
 claim();ownerTestRtcUs+=100000000;assert(ownerAllows("A"));assert(ownerRemainingS()==300);
 deepWake(310000000);assert(!ownerGet(out));
 // Cold/soft reset cannot reuse a valid-looking RTC record as a new grant.
 claim();ownerTestReset=ESP_RST_SW;ownerBegin();assert(!ownerGet(out));
 claim();ownerTestReset=ESP_RST_POWERON;ownerBegin();assert(!ownerGet(out));
 claim();ownerDeadline.checksum^=1;deepWake(1);assert(!ownerGet(out));
 claim();ownerTestStrings["id"]="B";deepWake(1);assert(!ownerGet(out));
 claim();ownerTestRtcUs-=1;ownerTouch();assert(!ownerGet(out));
 // The old persisted uptime-only owner has no expiry proof after upgrade.
 claim();ownerDeadline={};deepWake(1);assert(!ownerGet(out));
 claim("A",3600);deepWake(3599000000ULL);assert(ownerRemainingS()==1);
 deepWake(1000000);assert(!ownerGet(out));
 auto args=v2PrepareClaim("B","host","192.0.2.1","8765","300",true,false,false);
 assert(ownerApplyClaim(args,false).status==401 && !ownerGet(out));
 assert(ownerApplyClaim(args,true).status==200 && ownerGet(out) && out.id=="B");
 const auto deadline=ownerDeadline.untilUs;
 args.request.id="A";
 assert(ownerApplyClaim(args,true).status==409 && ownerDeadline.untilUs==deadline);
 args.validId=false;assert(ownerApplyClaim(args,true).status==400);
 args.validId=true;ownerTestRtcUs+=300000000;
 ownerTestWriteFail=true;
 assert(ownerApplyClaim(args,true).status==500 && !ownerGet(out));
 ownerTestWriteFail=false;
 assert(ownerApplyClaim(args,true).status==200 && ownerGet(out) && out.id=="A");
 args.request.id="B";args.force=true;
 assert(ownerApplyClaim(args,false).status==401 && ownerGet(out) && out.id=="A");
 ownerTestReadFail=true;
 assert(ownerApplyClaim(args,true).status==500 && ownerGet(out) && out.id=="A");
 ownerTestReadFail=false;
 assert(ownerApplyClaim(args,true).status==200 && ownerGet(out) && out.id=="B");
 args.force=false;args.release=true;args.request.id="A";
 assert(ownerApplyClaim(args,true).status==409 && ownerGet(out) && out.id=="B");
 args.request.id="B";
 assert(ownerApplyClaim(args,true).status==200 && !ownerGet(out));
 assert(ownerApplyClaim(args,true).status==200 && !ownerGet(out));
 std::cout<<"PASS: actual owner store expiry, repeated deep wake, exact deadline, renewal, foreign refusal, cold reset, corruption, identity, RTC regression and legacy recovery\n";
}
