#include "plan_store.h"
#ifdef CODEX_BUNDLE_HOST
#include "PlanPreferences.h"
#else
#include <Preferences.h>
#endif

namespace {
constexpr size_t HEADER = 48, MAX_RECORD = HEADER + 128;
uint64_t get(const uint8_t *p, unsigned n) {
    uint64_t v = 0; for (unsigned i=0;i<n;++i) v |= (uint64_t)p[i] << (8*i); return v;
}
void put(uint8_t *p, uint64_t v, unsigned n) {
    for (unsigned i=0;i<n;++i) p[i] = (uint8_t)(v >> (8*i));
}
uint32_t crc(const uint8_t *p, size_t n) {
    uint32_t v=~0u;
    for (size_t i=0;i<n;++i) {
        if (i>=44 && i<48) continue;
        v ^= p[i];
        for (unsigned b=0;b<8;++b) v=(v>>1)^(0xedb88320u & (0u-(v&1u)));
    }
    return ~v;
}
}

bool PlanStore::begin(V2PlanState &state, uint64_t nowMs, uint64_t utc) {
    Preferences p;
    if (!p.begin("pm", true)) { status_="read_failed"; return false; }
    const size_t size=p.getBytesLength("plan");
    if (!size && !p.isKey("plan")) {
        p.end(); state.reset(); principal_=""; request_={};
        status_="missing"; uncertain_=false; return true;
    }
    uint8_t bytes[MAX_RECORD];
    if (size<=HEADER || size>sizeof(bytes) || p.getBytes("plan",bytes,size)!=size) {
        p.end(); status_="record_invalid"; return false;
    }
    p.end();
    const uint32_t grant=(uint32_t)get(bytes+24,4);
    const uint64_t start=get(bytes+28,8), expiry=get(bytes+36,8);
    if (memcmp(bytes,"PPL1",4) || get(bytes+4,2)!=size || bytes[6]>V2_PLAN_LIGHT ||
        bytes[7]!=size-HEADER || memchr(bytes+HEADER,0,bytes[7]) ||
        get(bytes+44,4)!=crc(bytes,size) || grant>V2_MAX_LIGHT_S ||
        (bytes[6]==V2_PLAN_SLEEP && grant) || expiry<start || expiry-start>grant) {
        status_="record_invalid"; return false;
    }
    const V2PowerPlan plan{get(bytes+8,8),bytes[6],(uint32_t)get(bytes+16,4),(uint32_t)get(bytes+20,4)};
    // RTC has whole seconds. Discard its subsecond uncertainty instead of
    // gaining up to a second each time the application reboots.
    const uint32_t remaining=start>=1600000000ULL && utc>=start && expiry>utc+1
        ? (uint32_t)(expiry-utc-1) : 0;
    String principal((const char *)bytes+HEADER,bytes[7]);
    if (principal.length()!=bytes[7]) { status_="allocation_failed"; return false; }
    principal_=std::move(principal); request_=plan;
    state.restore(plan,grant,remaining,nowMs);
    uncertain_=false;
    status_=plan.mode==V2_PLAN_SLEEP ? "restored_sleep" : remaining ? "restored_light" : "expired_or_clock_invalid";
    return true;
}

PlanStore::Commit PlanStore::save(const V2PowerPlan &plan, uint32_t grant,
                                 const String &principal, uint64_t start, uint64_t expiry) {
    if (!principal.length() || principal.length()>128) return Failed;
    uint8_t bytes[MAX_RECORD], verify[MAX_RECORD];
    const size_t size=HEADER+principal.length();
    memset(bytes,0,size); memcpy(bytes,"PPL1",4);
    put(bytes+4,size,2); bytes[6]=plan.mode; bytes[7]=(uint8_t)principal.length();
    put(bytes+8,plan.planId,8); put(bytes+16,plan.lightDurationS,4); put(bytes+20,plan.rendezvousPeriodS,4);
    put(bytes+24,grant,4); put(bytes+28,start,8); put(bytes+36,expiry,8);
    memcpy(bytes+HEADER,principal.c_str(),principal.length()); put(bytes+44,crc(bytes,size),4);
    Preferences p;
    if (!p.begin("pm",false)) return Failed;
    const bool wrote=p.putBytes("plan",bytes,size)==size;
    p.end();
    if (!p.begin("pm",true)) return Unknown;
    const size_t found=p.getBytesLength("plan");
    const bool absent=!found && !p.isKey("plan");
    const bool read=found>0 && found<=sizeof(verify) && p.getBytes("plan",verify,found)==found;
    p.end();
    if (!read && !absent) return Unknown;
    if (read && found==size && !memcmp(bytes,verify,size)) return Saved;
    return wrote ? Unknown : Failed;
}

V2PlanDecision PlanStore::apply(JsonDocument &doc, V2PlanState &state, uint64_t nowMs,
                               uint64_t utc, bool provisional) {
    V2PlanDecision failure; failure.planId=doc["plan_id"]|0ULL; failure.includeContext=true;
    if (uncertain_ && !begin(state,nowMs,utc)) {
        failure.result="unknown"; failure.error="plan_storage_unknown"; return failure;
    }
    String principal=doc["bridge_id"]|"";
    if (!principal.length() || principal.length()>128) { failure.error="plan_principal"; return failure; }
    V2PlanState next=state;
    if (principal!=principal_) next.reset();
    const bool same=next.accepted() && next.acceptedId()==failure.planId;
    V2PlanDecision decision=v2DecidePlan(doc,next,nowMs,provisional);
    if (!decision.accepted) return decision;
    if (!same) {
        const uint64_t start=utc>=1600000000ULL ? utc : 0;
        const uint64_t expiry=start ? start+next.grantedS() : 0;
        const Commit result=save(decision.plan,next.grantedS(),principal,start,expiry);
        if (result!=Saved) {
            uncertain_=result==Unknown; status_=uncertain_?"write_unknown":"write_failed";
            failure.result=uncertain_?"unknown":"rejected";
            failure.error=uncertain_?"plan_storage_unknown":"plan_storage_failed"; return failure;
        }
        principal_=std::move(principal); request_=decision.plan;
    }
    state=next; status_="persisted";
    return decision;
}

bool PlanStore::reanchor(const V2PlanState &state, uint64_t nowMs, uint64_t utc) {
    if (!state.accepted() || !principal_.length()) return !uncertain_;
    if (uncertain_) return false;
    // Preserve the actual monotonic deadline across a deliberate RTC correction.
    const uint64_t remaining=state.lightActive(nowMs) ? (state.deadlineMs()-nowMs)/1000ULL : 0;
    const uint64_t start=utc>=1600000000ULL ? utc : 0, expiry=start?start+remaining:0;
    const Commit result=save(request_,state.grantedS(),principal_,start,expiry);
    if (result!=Saved) { uncertain_=result==Unknown; return false; }
    return true;
}
bool PlanStore::clear() {
    Preferences p; if (!p.begin("pm",false)) return false;
    const bool ok=!p.isKey("plan") || p.remove("plan"); p.end();
    if (ok) { principal_=""; request_={}; uncertain_=false; status_="missing"; }
    return ok;
}
