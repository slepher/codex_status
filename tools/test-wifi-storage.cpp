#include <cstdlib>
#include <new>
#include <cstddef>
#include <cassert>
#include <cstring>
#include <iostream>
#include <vector>
// Count requested C++ allocation bytes (allocator metadata is separate). Fail
// only nothrow allocations, which the embedded repository uses throughout.
static size_t liveBytes=0, peakBytes=0;
static long failAllocation=-1;
static bool allocationFaultInjected=false;
struct alignas(std::max_align_t) Allocation { size_t size; };
static void *allocate(size_t size) {
    auto *header=(Allocation *)std::malloc(sizeof(Allocation)+size);
    if(!header)throw std::bad_alloc(); header->size=size;
    liveBytes+=size; peakBytes=std::max(peakBytes,liveBytes); return header+1;
}
void *operator new(size_t size) { return allocate(size); }
void *operator new[](size_t size) { return allocate(size); }
void *operator new(size_t size,const std::nothrow_t &) noexcept { if(failAllocation==0){allocationFaultInjected=true;return nullptr;} if(failAllocation>0)--failAllocation; try{return allocate(size);}catch(...){return nullptr;} }
void *operator new[](size_t size,const std::nothrow_t &tag) noexcept { return ::operator new(size,tag); }
void operator delete(void *p) noexcept { if(p){auto *header=(Allocation *)p-1;liveBytes-=header->size;std::free(header);} }
void operator delete[](void *p) noexcept { ::operator delete(p); }
void operator delete(void *p,const std::nothrow_t &) noexcept { ::operator delete(p); }
void operator delete[](void *p,const std::nothrow_t &) noexcept { ::operator delete(p); }
#define CODEX_WIFI_SETUP_HOST_TEST
#include "../src/wifi_setup.cpp"
#include "../src/wifi_legacy.h"

static const char *bond="02:00:00:00:00:02/1";
static void reset() { testNvsBlob.clear();testNvsLegacyBlob.clear();testNvsCommitThenFail=testNvsFail=testNvsReadbackFail=testNvsNewReadbackFail=testNvsRemoveFail=false;testNvsByteLimit=SIZE_MAX;testNvsFailAfter=-1;testNvsCommitThenFailKey.clear();testNvsReadbackFailKey.clear();testNvsFailReadAfterWriteKey.clear();testNvsWrites.clear();wifiManagementResetSession();running=false;assert(wifiSchemesBegin()); }
static WifiStore example(size_t networks,size_t receipts,bool maximum=false) {
    WifiStore s; s.revision=10;
    for(size_t i=0;i<networks;++i) {
        WifiNetwork n;std::string id=maximum?std::string(31,'n')+char('a'+i):"network-"+std::to_string(i);
        std::string host=maximum?std::string(31,'h')+char('a'+i):"Host-"+std::to_string(i);
        n.bond.set((maximum ? std::string(20,'b')+char('a'+i/4) : "02:00:00:00:00:0"+std::to_string(2+i/4)+"/1").c_str());n.id.set(id.c_str());n.host.set(host.c_str());
        n.ssid.set(maximum?std::string(32,'s').c_str():"Device-2g");n.password.set(maximum?std::string(64,'a').c_str():"test-password");n.label.set(maximum?std::string(64,'l').c_str():"Desk");n.version=10;
        assert(!s.schemes.put(n,false));
    }
    if(networks) s.selectActive(0);
    for(size_t i=0;i<receipts;++i) {
        WifiReceipt r;r.bridge.set(maximum?std::string(64,'b').c_str():"installation");
        r.request.set((maximum?std::string(63,'r')+char('a'+i):"request-"+std::to_string(i)).c_str());r.bond.set(maximum ? std::string(21,'b').c_str() : bond);r.revision=10;
        if(maximum)r.error.set(std::string(32,'e').c_str());assert(s.receipts.append(r));
    }
    assert(wifiStoreShape(s));return s;
}
static std::vector<uint8_t> archive(const WifiStore &s) {
    WifiWriter sizing{nullptr};sizing.store(s);std::vector<uint8_t>b(sizing.pos);WifiWriter writer{b.data()};writer.store(s);
    wifiSetWord(b.data(),8,b.size());wifiSetWord(b.data(),12,v2Crc32(b.data(),b.size()));return b;
}
static void mutate(WifiStore &s) {
    if(s.schemes.networks.size()) {
        s.preserveActive();auto &n=s.schemes.networks[0];n.label.set(std::string(strlen(n.label),'x').c_str());n.version=++s.revision;
    }
    if(s.receipts.size())s.receipts[0].error.set(std::string(32,'m').c_str());
    else ++s.executionGeneration;
}
static void reseal(std::vector<uint8_t> &bytes) {memset(bytes.data()+12,0,4);uint32_t crc=v2Crc32(bytes.data(),bytes.size());for(size_t i=0;i<4;++i)bytes[12+i]=crc>>(8*i);}
static void refused(std::vector<uint8_t> bytes) {WifiStore decoded;assert(!wifiDecode5(bytes.data(),bytes.size(),decoded));}
static void legacyFixture(int version,bool compressed) {
    reset();std::unique_ptr<WifiLegacyStoreV2> old(new WifiLegacyStoreV2);
    auto &n=old->schemes.networks[0];strcpy(n.bond,bond);strcpy(n.id,"old");strcpy(n.host,"OldHost");strcpy(n.ssid,"OldDevice");strcpy(n.password,"test-password");
    strcpy(old->schemes.selectedBond,bond);strcpy(old->schemes.selectedId,"old");old->active=n;old->revision=version==1?0:7;old->activeVersion=old->versions[0]=old->revision;
    if(version==2){auto &r=old->receipts[0];strcpy(r.bridge,"installation");strcpy(r.request,"uncertain");strcpy(r.bond,bond);r.revision=7;memset(r.hash,0x33,32);strcpy(old->labels[0],"Old label");}old->seal();
    if(compressed){testNvsLegacyBlob.resize(sizeof(*old)+64);memcpy(testNvsLegacyBlob.data(),"WFZ2",4);size_t length=wifiDeflate(testNvsLegacyBlob.data()+4,testNvsLegacyBlob.size()-4,old.get(),sizeof(*old));assert(length);testNvsLegacyBlob.resize(length+4);}
    else {const auto *start=version==1?(const uint8_t *)&old->schemes:(const uint8_t *)old.get();size_t size=version==1?sizeof(old->schemes):sizeof(*old);testNvsLegacyBlob.assign(start,start+size);}
    auto previous=testNvsLegacyBlob;
    assert(!wifiSchemesBegin()&&testNvsLegacyBlob==previous&&testNvsBlob.empty());
    WifiControlContext context;context.principal="installation";context.nameSpace=bond;context.claimed=true;context.until=30000;
    JsonDocument request;request["op"]="apply";request["bridge_id"]="installation";request["request_id"]="must-not-overwrite";
    JsonDocument reply;assert(!deserializeJson(reply,wifiControlCommand(request,context,1000)));
    assert(reply["ok"]==false&&testNvsLegacyBlob==previous&&testNvsBlob.empty());

}
int main() {
    // Format bytes and corruption, including valid CRC on invalid length/count.
    reset();auto fixture=example(2,2);assert(saveV2(fixture));auto valid=archive(wifiStore);
    for(size_t size=0;size<valid.size();++size) {auto cut=valid;cut.resize(size);refused(std::move(cut));}
    for(size_t i=0;i<valid.size();++i){auto broken=valid;broken[i]^=0x80;refused(std::move(broken));}
    auto broken=valid;broken.push_back(0);refused(broken);
    for(auto [offset,value]:std::initializer_list<std::pair<size_t,uint8_t>>{{4,6},{5,9},{6,13},{7,9},{40,3},{41,22}}) {broken=valid;broken[offset]=value;reseal(broken);refused(broken);}
    // Every stored row has independent framing; valid CRC cannot hide an
    // embedded NUL, duplicate id/mapping, excess bonds or mismatched active.
    broken=valid;broken[42]=0;reseal(broken);refused(broken);
    broken=valid;broken.push_back(0);for(size_t i=0;i<4;++i)broken[8+i]=broken.size()>>(8*i);reseal(broken);refused(broken);
    size_t firstRecord=41+1+strlen(wifiStore.active.bond)+1+strlen(wifiStore.active.id);
    for(uint8_t length:{uint8_t(0),uint8_t(255)}) {broken=valid;broken[firstRecord]=length;reseal(broken);refused(broken);}
    broken=valid;broken[firstRecord+10]=22;reseal(broken);refused(broken);
    WifiStore duplicate=wifiStore;duplicate.schemes.networks[1].id=duplicate.schemes.networks[0].id;assert(!saveV2(duplicate));
    duplicate=wifiStore;duplicate.activeVersion=9;assert(!saveV2(duplicate));
    // Build the actual prior WFS4 layout, which had neither generation field.
    auto prior=example(2,2);prior.receipts[0].error.set("pending");prior.receipts[0].operation.set("trial");
    prior.receipts[0].request.set("w3_0000000000000001");WifiCursor legacyCursor;
    legacyCursor.bridge.set(prior.receipts[0].bridge);legacyCursor.bond.set(prior.receipts[0].bond);legacyCursor.high=1;assert(prior.cursors.append(legacyCursor));
    assert(saveV2(prior));auto wfs4=archive(wifiStore);
    wfs4.erase(wfs4.begin()+32,wfs4.begin()+40);wfs4[3]='4';wfs4[4]=4;
    size_t pos=33;
    if(wfs4[32]) {for(int i=0;i<(wfs4[32]==2?4:2);++i)pos+=1+wfs4[pos];}
    for(size_t i=0;i<wfs4[6];++i)pos+=2+wfs4[pos]+256*wfs4[pos+1];
    const size_t configurationEnd=pos;
    for(size_t i=0;i<wfs4[7];++i) {
        size_t length=wfs4[pos]+256*wfs4[pos+1];
        wfs4.erase(wfs4.begin()+pos+2+length-8,wfs4.begin()+pos+2+length);
        length-=8;wfs4[pos]=length;wfs4[pos+1]=length>>8;pos+=2+length;
    }
    for(size_t i=0;i<4;++i)wfs4[8+i]=wfs4.size()>>(8*i);reseal(wfs4);
    testNvsBlob=wfs4;wifiExecutionGeneration=0;
    assert(wifiSchemesBegin()&&testNvsBlob==wfs4 && !wifiStorageSplit);refused(wfs4);
    assert(wifiStore.receipts.size()==2 && !strcmp(wifiStore.receipts[0].error,"pending") && !wifiStore.receipts[0].executionGeneration);
    auto imported=wifiStore;assert(saveV2(imported)&&wifiStorageSplit&&wifiSchemesBegin());
    assert(!strcmp(wifiStore.receipts[0].error,"pending") && !wifiStore.receipts[0].executionGeneration);
    TestNvsKeys importedKeys;assert(testNvsKeys(importedKeys)&&!importedKeys.count("store3"));
    auto expectedConfig=std::vector<uint8_t>(wfs4.begin(),wfs4.begin()+configurationEnd);
    expectedConfig[5]=expectedConfig[7]=0;wifiSetWord(expectedConfig.data(),8,expectedConfig.size());reseal(expectedConfig);
    assert(importedKeys.at("config0")==expectedConfig); // exact original configuration encoding
    assert(wifiStore.cursors.size()==1&&wifiStore.cursors[0].high==1&&!strcmp(wifiStore.receipts[0].request,"w3_0000000000000001"));
    // Import is also a transaction: prepared separate blocks cannot hide or
    // retire an existing WFS4 until the selector itself commits.
    for(int fault=0;fault<4;++fault){
        testNvsBlob=wfs4;assert(wifiSchemesBegin());auto split=wifiStore;
        testNvsFailAfter=fault;assert(!saveV2(split));testNvsFailAfter=-1;
        assert(wifiSchemesBegin()&&!wifiStorageSplit&&wifiStore.receipts.size()==2);
        TestNvsKeys preserved;assert(testNvsKeys(preserved)&&preserved.at("store3")==wfs4);
    }
    for(auto version:{1,2})legacyFixture(version,false);legacyFixture(2,true);
    // Existing empty/wrong-type keys cannot be mistaken for a pristine store.
    for(bool current:{false,true}) {
        reset();testNvsEmptyCurrentKey=current;testNvsEmptyLegacyKey=!current;
        assert(!wifiSchemesBegin()&&testNvsBlob.empty()&&testNvsLegacyBlob.empty());
        testNvsEmptyCurrentKey=testNvsEmptyLegacyKey=false;
    }
    // Runtime never retires a legacy backup; explicit offline migration owns it.
    reset();auto current=example(1,0);assert(saveV2(current));testNvsLegacyBlob={1,2,3};
    assert(wifiSchemesBegin()&&wifiOldKeyPresent&&testNvsLegacyBlob.size()==3);
    WifiStore retained=wifiStore;assert(saveV2(retained)&&testNvsLegacyBlob.size()==3);
    // Full capacity and inactive edit/delete retain old active credentials.
    reset();auto full=example(12,8,true);assert(saveV2(full)&&wifiSchemesBegin());assert(schemes.networks.size()==12&&wifiStore.receipts.size()==8);
    WifiStore oneBond=example(4,0);WifiNetwork extraOwn=oneBond.schemes.networks[0];extraOwn.id.set("extra");extraOwn.host.set("extra");assert(!oneBond.schemes.put(extraOwn,false)&&oneBond.schemes.networks.size()==5);
    WifiStore threeBonds=example(1,0);for(const char *b:{"other-1","other-2"}){WifiNetwork n=threeBonds.schemes.networks[0];n.bond.set(b);assert(!threeBonds.schemes.put(n,false));}extraOwn.bond.set("fourth");assert(!threeBonds.schemes.put(extraOwn,false)&&threeBonds.schemes.networks.size()==4);
    WifiNetwork excess=schemes.networks[0];excess.id.set("extra");excess.host.set("extra");assert(!strcmp(schemes.put(excess,false),"capacity"));
    WifiStore edited=wifiStore;edited.preserveActive();edited.schemes.networks[0].ssid.set(std::string(32,'x').c_str());edited.schemes.networks[0].password.set(std::string(64,'b').c_str());edited.schemes.networks[0].version=++edited.revision;
    assert(saveV2(edited)&&testNvsBlob.size()<=WIFI_STORAGE_MAX&&wifiSchemesBegin());String ssid,password;assert(wifiSchemesCredentials(ssid,password)&&ssid==std::string(32,'s').c_str()&&password==std::string(64,'a').c_str());
    WifiStore erased=wifiStore;assert(erased.schemes.erase(0));assert(saveV2(erased)&&wifiSchemesBegin());assert(wifiSchemesCredentials(ssid,password)&&ssid==std::string(32,'s').c_str());
    // Fault each nothrow allocation in clone/edit/receipt/encode/readback.
    size_t exercisedAllocationFailures=0;
    for(long fault=0;fault<=140;++fault) {
        reset();auto initial=example(1,0);assert(saveV2(initial));auto previous=testNvsBlob;
        WifiControlContext context;context.principal="installation";context.nameSpace=bond;context.claimed=true;context.until=30000;
        JsonDocument apply;apply["op"]="apply";apply["bridge_id"]="installation";apply["request_id"]="new";apply["expected_revision"]=wifiStore.revision;
        auto n=apply["changes"].to<JsonArray>().add<JsonObject>();n["network_id"]="network-0";n["host_ssid"]="Host-0";n["ssid"]="Edited";n["credential"]="replace";n["password"]="new-password";
        allocationFaultInjected=false;
        testNvsCommitThenFailKey=fault==140?"commit":"";
        failAllocation=fault;String result=wifiControlCommand(apply,context,1000);failAllocation=-1;testNvsCommitThenFailKey.clear();
        if(allocationFaultInjected)++exercisedAllocationFailures;
        if(fault==140) {
            JsonDocument response;assert(!deserializeJson(response,result)&&response["error"]=="storage_unknown");
            JsonDocument query;query["op"]="result";query["bridge_id"]="installation";query["query_id"]="new";
            assert(!deserializeJson(response,wifiControlCommand(query,context,1000))&&response["outcome"]=="committed"&&response["revision"]==11);
        }
        assert(wifiSchemesBegin());
        if(wifiStore.revision==11) {assert(wifiStore.receipts.size()==1&&!strcmp(wifiStore.receipts[0].request,"new"));}
        else assert(wifiStore.revision==10&&wifiStore.receipts.size()==0);
    }
    std::cout<<"Allocation fault sites reached="<<exercisedAllocationFailures<<" of 140 probes; committed-then-error original result confirmed\n";
    // Old or complete new bytes after a simulated NVS atomic commit boundary;
    // readback failure never allows a stale-revision follow-up overwrite.
    reset();auto initial=example(1,0);assert(saveV2(initial));auto oldBytes=testNvsBlob;
    auto changed=example(2,1);changed.revision=11;testNvsWrites.clear();testNvsFailReadAfterWriteKey="commit";
    assert(!saveV2(changed));auto newBytes=testNvsBlob;testNvsFailReadAfterWriteKey.clear();
    assert(wifiSchemesBegin()&&schemes.networks.size()==2);
    testNvsCommitThenFailKey="commit";WifiStore committedDespiteError=example(3,1);
    assert(!saveV2(committedDespiteError)&&wifiStorageUncertain);testNvsCommitThenFailKey.clear();
    assert(wifiSchemesBegin()&&schemes.networks.size()==3&&!wifiStorageUncertain);
    testNvsBlob=oldBytes;assert(wifiSchemesBegin()&&schemes.networks.size()==1);testNvsBlob=newBytes;assert(wifiSchemesBegin()&&schemes.networks.size()==2);
    testNvsByteLimit=32;auto tooLarge=example(3,1);assert(!saveV2(tooLarge));testNvsByteLimit=SIZE_MAX;
    assert(wifiSchemesBegin()&&schemes.networks.size()==2);
    // Every preparation/selector write boundary keeps either the complete old
    // state or the complete new state. No split config/receipt visibility.
    for(int fault=0;fault<3;++fault){
        reset();auto base=example(1,1);assert(saveV2(base));auto next=example(2,2);next.revision=11;
        testNvsFailAfter=fault;assert(!saveV2(next));testNvsFailAfter=-1;assert(wifiSchemesBegin());
        assert(wifiStore.revision==10&&schemes.networks.size()==1&&wifiStore.receipts.size()==1);
    }
    for(const char *key:{"config1","txn1","commit"}){
        for(bool afterCommit:{false,true}){
            reset();auto base=example(1,1);assert(saveV2(base));auto next=example(2,2);next.revision=11;testNvsWrites.clear();
            if(afterCommit)testNvsCommitThenFailKey=key;else testNvsFailReadAfterWriteKey=key;
            assert(!saveV2(next));testNvsCommitThenFailKey.clear();testNvsFailReadAfterWriteKey.clear();assert(wifiSchemesBegin());
            bool committed=!strcmp(key,"commit");assert(wifiStore.revision==(committed?11:10));
            assert(schemes.networks.size()==(committed?2:1)&&wifiStore.receipts.size()==(committed?2:1));
        }
    }
    reset();auto separated=example(2,2);assert(saveV2(separated));TestNvsKeys before;assert(testNvsKeys(before));
    testNvsWrites.clear();auto txnOnly=wifiStore;txnOnly.receipts[0].error.set("timeout");assert(saveV2(txnOnly));
    TestNvsKeys after;assert(testNvsKeys(after));assert(after.at("config0")==before.at("config0")&&!after.count("config1"));
    assert(!testNvsWrites["config0"]&&!testNvsWrites["config1"]);
    const auto &cfg=after.at("config0");assert(!memcmp(cfg.data(),"WFS4",4)&&!cfg[5]&&!cfg[7]);
    const auto &tx=after.at("txn1");assert(!memcmp(tx.data(),"WTX1",4));
    assert(std::search(tx.begin(),tx.end(),std::begin("test-password"),std::end("test-password")-1)==tx.end());
    testNvsWrites.clear();auto unchanged=wifiStore;assert(saveV2(unchanged)&&testNvsWrites.empty());
    // A failed retirement leaves a verified new commit and bounded old slots;
    // retry removes them without writing credentials or replaying a command.
    auto update=example(3,2);testNvsRemoveFail=true;assert(saveV2(update)&&wifiStorageCleanupPending);testNvsRemoveFail=false;
    assert(wifiSchemesBegin());testNvsWrites.clear();auto cleanup=wifiStore;assert(saveV2(cleanup)&&!wifiStorageCleanupPending&&testNvsWrites.empty());
    TestNvsKeys keys;assert(testNvsKeys(keys)&&keys.size()==3);
    // Corrupt/truncated selector must never fall back to the legacy record.
    auto complete=testNvsBlob;auto marker=keys.at("commit");
    for(size_t n=0;n<marker.size();++n){keys["commit"]={marker.begin(),marker.begin()+n};assert(testNvsPublish(keys));assert(!wifiSchemesBegin());}
    for(size_t n=0;n<marker.size();++n){keys["commit"]=marker;keys["commit"][n]^=0x80;assert(testNvsPublish(keys));assert(!wifiSchemesBegin());}
    testNvsBlob=complete;assert(wifiSchemesBegin());assert(testNvsKeys(keys));
    const auto validKeys=keys;
    for(const char *key:{wifiConfigKey(marker[4]),wifiTransactionKey(marker[5])}){
        const auto original=validKeys.at(key);
        for(size_t n=0;n<original.size();++n){keys=validKeys;keys[key].resize(n);assert(testNvsPublish(keys));assert(!wifiSchemesBegin());}
        for(size_t n=0;n<original.size();++n){keys=validKeys;keys[key][n]^=0x80;assert(testNvsPublish(keys));assert(!wifiSchemesBegin());}
        // Valid CRCs do not authorize an unknown schema or unbounded count.
        for(auto [offset,value]:std::initializer_list<std::pair<size_t,uint8_t>>{{4,99},{5,9},{6,13},{7,9}}){
            keys=validKeys;auto &part=keys[key];part[offset]=value;reseal(part);
            auto &selector=keys["commit"];wifiSetWord(selector.data(),!strncmp(key,"config",6)?12:20,v2Crc32(part.data(),part.size()));
            wifiSetWord(selector.data(),24,0);wifiSetWord(selector.data(),24,v2Crc32(selector.data(),selector.size()));
            assert(testNvsPublish(keys));assert(!wifiSchemesBegin());
        }
    }
    // A missing selector cannot be guessed to mean fresh or legacy state.
    keys=validKeys;keys.erase("commit");assert(testNvsPublish(keys));assert(!wifiSchemesBegin());
    keys["store3"]=wfs4;assert(testNvsPublish(keys));assert(!wifiSchemesBegin());
    // First ever save has an explicit empty root before any prepared blobs.
    for(int fault=0;fault<4;++fault){reset();auto first=example(1,1);testNvsFailAfter=fault;
        assert(!saveV2(first));testNvsFailAfter=-1;assert(wifiSchemesBegin()&&schemes.networks.size()==0&&wifiStore.receipts.size()==0);}
    testNvsBlob=complete;assert(wifiSchemesBegin());
    std::cout<<"WFS4 + WTX1: compatible import, zero config writes for receipts, atomic split commit faults, unknown original result, cleanup and corruption passed\n";
    // Isolate production clone+encode+readback; exclude host NVS vectors from
    // logical target budgets. Measured host delta includes its vector resize.
    for(auto [networks,receipts]:std::initializer_list<std::pair<size_t,size_t>>{{0,0},{1,1},{2,2},{12,8}}) {
        reset();auto source=example(networks,receipts,networks==12);assert(saveV2(source));
        size_t storeHeap=wifiStore.heapBytes(), base=liveBytes;peakBytes=base;testNvsPeakStoredBytes=0;
        {std::unique_ptr<WifiStore> next(new(std::nothrow) WifiStore(wifiStore));assert(next);mutate(*next);assert(saveV2(*next));}
        size_t peak=peakBytes-base;
        size_t strings=storeHeap-networks*sizeof(WifiNetwork)-receipts*sizeof(WifiReceipt);
        std::cout<<"budget networks="<<networks<<" receipts="<<receipts<<" fixture_bytes="<<testNvsBlob.size()<<" config="<<wifiConfigurationBytes<<" txn="<<wifiTransactionBytes<<" selector=32 logical_total="<<wifiVerifiedStorageBytes<<" string_allocations_bytes="<<strings<<" host_store_heap="<<storeHeap<<" nvs_preparation_peak_bytes="<<testNvsPeakStoredBytes<<" host_clone_save_peak_delta="<<peak<<"\n";
    }
    reset();auto combined=example(12,8,true);
    for(size_t i=0;i<8;++i){WifiCursor c;c.bridge.set((std::string(63,'p')+char('a'+i)).c_str());c.bond.set(std::string(21,'b').c_str());c.high=8;assert(combined.cursors.append(c));combined.receipts[i].operation.set("activate");combined.receipts[i].target.set(std::string(32,'n').c_str());}
    combined.preserveActive();combined.schemes.networks[0].version=++combined.revision;
    assert(saveV2(combined));size_t base=liveBytes;peakBytes=base;testNvsPeakStoredBytes=0;
    {std::unique_ptr<WifiStore> next(new(std::nothrow) WifiStore(wifiStore));assert(next);mutate(*next);assert(saveV2(*next));}
    std::cout<<"combined upper bound fixture_bytes="<<testNvsBlob.size()<<" config="<<wifiConfigurationBytes<<" txn="<<wifiTransactionBytes<<" selector=32 logical_total="<<wifiVerifiedStorageBytes<<" heap="<<wifiStore.heapBytes()<<" nvs_preparation_peak_bytes="<<testNvsPeakStoredBytes<<" clone-save peak delta="<<peakBytes-base<<" cursor="<<sizeof(WifiCursor)<<" bytes\n";
    std::cout<<"host layout network="<<sizeof(WifiNetwork)<<" receipt="<<sizeof(WifiReceipt)<<" store="<<sizeof(WifiStore)<<" legacy="<<sizeof(WifiLegacyStoreV2)<<"\n";
    return 0;
}
