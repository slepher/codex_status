#include "plan_store.h"
#include "PlanPreferences.h"
#include "boot_capture.h"
#include "ble_recovery.h"
#include "network_display_state.h"
#include "plan_activation.h"
#define CHECK(x) do { if (!(x)) return __LINE__; } while (0)

extern "C" int codex_plan_store_check() {
    {
        PlanActivation activation;
        JsonDocument network;
        network["v"]=1;network["bridge_addr"]="192.168.1.2";network["bridge_port"]=8765;
        network["target"]["network_id"]="home";network["target"]["open"]=true;
        network["target"]["metadata_key"]=String(std::string(64,'a'));
        String frozen;
        CHECK(!activation.prepare(12,network.as<JsonVariantConst>(),frozen));
        CHECK(frozen.length()<=PLAN_NETWORK_MAX);
        activation.commit(12,std::move(frozen));
        CHECK(activation.active() && activation.planId()==12);
        String replay;CHECK(!activation.prepare(12,network.as<JsonVariantConst>(),replay));
        CHECK(replay==activation.request());
        network["bridge_port"]=8766;
        String conflict;CHECK(!strcmp(activation.prepare(12,network.as<JsonVariantConst>(),conflict),"plan_network_conflict"));
        network["bridge_addr"]="127.0.0.1";
        String invalid;CHECK(!strcmp(activation.prepare(13,network.as<JsonVariantConst>(),invalid),"plan_network_shape"));
        network["bridge_addr"]="192.168.1.2";network["v"]=2;
        CHECK(activation.prepare(13,network.as<JsonVariantConst>(),invalid));
    }
    using namespace plan_test;
    reset();
    constexpr uint64_t utc=1800000000;
    V2PlanState state; PlanStore store;
    CHECK(store.begin(state,0,utc) && !state.accepted());
    JsonDocument doc;
    doc["bridge_id"]="owner"; doc["plan_id"]=12ULL;
    doc["mode"]="light"; doc["light_duration_s"]=300;
    CHECK(store.apply(doc,state,1000,utc,false).accepted);
    CHECK(record.size()==53 && writes==1 && state.remainingS(61000)==240);
    CHECK(store.apply(doc,state,61000,utc+60,false).accepted && writes==1);
    CHECK(state.remainingS(61000)==240);
    V2PlanState restored; PlanStore reboot;
    CHECK(reboot.begin(restored,0,utc+60) && restored.remainingS(0)==239);
    CHECK(reboot.apply(doc,restored,1000,utc+61,false).accepted && writes==1);
    CHECK(restored.remainingS(1000)==238);
    doc["light_duration_s"]=301;
    CHECK(!reboot.apply(doc,restored,1000,utc+61,false).accepted);
    doc["plan_id"]=11ULL;
    CHECK(!reboot.apply(doc,restored,1000,utc+61,false).accepted);
    CHECK(reboot.begin(restored,0,utc-1) && restored.acceptedId()==12 && !restored.lightActive(0));
    CHECK(reboot.begin(restored,0,0) && !restored.lightActive(0));
    CHECK(reboot.begin(restored,0,utc+300) && !restored.lightActive(0));
    CHECK(reboot.begin(restored,0,utc+60));
    CHECK(reboot.reanchor(restored,10000,utc+1000));
    CHECK(reboot.begin(restored,0,utc+1001) && restored.remainingS(0)==227);
    auto valid=record;
    record[6]^=1;
    CHECK(!reboot.begin(restored,0,utc));
    record=valid; record.resize(48);
    CHECK(!reboot.begin(restored,0,utc));
    record=valid; record[3]='2';
    CHECK(!reboot.begin(restored,0,utc));
    record=valid;
    doc["plan_id"]=13ULL; doc["light_duration_s"]=100;
    failWrite=true;
    CHECK(!reboot.apply(doc,restored,0,utc+1001,false).accepted && restored.acceptedId()==12);
    failWrite=false; failReadAfterWrite=true;
    auto unknown=reboot.apply(doc,restored,0,utc+1001,false);
    CHECK(!unknown.accepted && !strcmp(unknown.result,"unknown") && restored.acceptedId()==12);
    failRead=failReadAfterWrite=false;
    const auto attempts=writes;
    CHECK(reboot.apply(doc,restored,10000,utc+1011,false).accepted);
    CHECK(writes==attempts && restored.acceptedId()==13 && restored.remainingS(10000)==89);
    doc["bridge_id"]="new-owner"; doc["plan_id"]=1ULL; doc["mode"]="sleep";
    CHECK(reboot.apply(doc,restored,10000,utc+1011,false).accepted);
    CHECK(reboot.begin(restored,0,utc+1012) && restored.acceptedId()==1 && !restored.lightActive(0));
    CHECK(planBootHttp(true,false,restored,0) && !planBootHttp(true,true,restored,0));
    CHECK(!planBootHttp(false,false,restored,0));
    CHECK(reboot.clear() && record.empty());
    CHECK(reboot.begin(restored,0,utc) && !restored.accepted());
    doc["mode"]="light"; doc["light_duration_s"]=300;
    doc["bridge_id"]="owner";
    CHECK(reboot.apply(doc,restored,0,0,false).accepted);
    CHECK(reboot.begin(restored,0,utc) && !restored.lightActive(0));
    return 0;
}

extern "C" int codex_boot_capture_check() {
    BleRecoveryWindow window;
    window.startup(1000);
    CHECK(!window.active(1001) && window.physicalUntil()==121000 && window.lightRemaining(1001)==300);
    CHECK(window.toggle(10000) && window.active(10000) && window.startupUntil==0);
    CHECK(!window.toggle(20000) && !window.active(20000) && window.physicalUntil()==0);
    BootCapture b; uint32_t at=0;
    b.begin(false,false,0);
    b.edge(true,100); b.edge(false,180); b.edge(true,300); b.edge(false,400);
    CHECK(b.action(10000,at)==BootCapture::Click && at==100);
    CHECK(b.action(10000,at)==BootCapture::Click && at==300);
    CHECK(b.action(10000,at)==BootCapture::None);
    b.edge(true,11000); b.edge(false,11020);
    CHECK(b.action(12000,at)==BootCapture::None);
    b.edge(true,13000);
    CHECK(b.action(15000,at)==BootCapture::Template);
    b.edge(false,16000);
    CHECK(b.action(16000,at)==BootCapture::None);
    b.edge(true,20000); b.edge(false,51000);
    CHECK(b.action(52000,at)==BootCapture::Template);
    CHECK(b.action(52000,at)==BootCapture::Config);
    CHECK(b.action(52000,at)==BootCapture::Reset);
    CHECK(b.action(52000,at)==BootCapture::None);
    b.begin(true,false,100); CHECK(b.take(150));
    b.edge(false,200); CHECK(b.action(300,at)==BootCapture::None);
    b.begin(false,false,0);
    b.edge(true,0xfffffff0); b.edge(false,0x50);
    CHECK(b.action(0x60,at)==BootCapture::Click && at==0xfffffff0);
    for (unsigned i=0;i<10;++i) { b.edge(true,1000+i*100); b.edge(false,1060+i*100); }
    CHECK(b.count==8 && b.dropped==2);
    NetworkDisplayState glyph;
    glyph.observe(true,-1,WifiPhase::Connected,-1,false); glyph.rendered(true,0);
    glyph.observe(true,-1,WifiPhase::Connected,-1,true); CHECK(glyph.due(0));
    glyph.rendered(false,0); CHECK(!glyph.due(4999) && glyph.due(5000));
    glyph.rendered(true,5000);
    glyph.observe(true,-1,WifiPhase::Connected,-1,false); CHECK(glyph.due(5001));
    return 0;
}
