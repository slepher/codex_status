#define CODEX_WIFI_SETUP_HOST_TEST
#include "../src/wifi_setup.cpp"
#include "../src/ble_wire.h"
#include <cassert>
#include <iostream>

static const char *bond = "02:00:00:00:00:02/1";
static const char *other = "02:00:00:00:00:03/1";
static JsonDocument request(const char *op, const char *id = "read") {
    JsonDocument d; d["op"] = String(op); d["request_id"] = String(id);
    d["bridge_id"] = "installation"; d["device_mac"] = "020000000001"; return d;
}
static WifiControlContext management;
static JsonDocument invoke(JsonDocument &d, uint64_t now = 1000) {
    JsonDocument result; assert(!deserializeJson(result, wifiControlCommand(d, management, now,
        "[{\"network_id\":\"legacy-0\",\"ssid\":\"USB-network\"}]"))); return result;
}
static JsonDocument snapshot() {
    auto d = request("snapshot");
    auto page = invoke(d); std::string text;
    while (true) {
        assert(page["ok"] == true); text += page["chunk"].as<const char *>();
        if (page["complete"] == true) break;
        d["snapshot_id"] = page["snapshot_id"]; d["offset"] = page["next_offset"];
        page = invoke(d);
    }
    JsonDocument result; assert(!deserializeJson(result, text)); return result;
}
static JsonDocument change(const char *id, uint64_t revision, const char *ssid = "Device") {
    auto d = request("apply", id); d["expected_revision"] = revision;
    auto n = d["changes"].to<JsonArray>().add<JsonObject>(); n["network_id"] = "home";
    n["host_ssid"] = "Host"; n["ssid"] = ssid; n["label"] = "Desk"; n["credential"] = "open";
    return d;
}
static String control(JsonDocument &doc,const String &peer,uint32_t generation,uint64_t now,uint64_t physical,uint64_t until,bool claimed) {
    WifiControlContext context;context.principal="installation";context.nameSpace=peer;context.generation=generation;
    context.physicalUntil=physical;context.until=std::max(physical,until);context.claimed=claimed;
    return wifiControlCommand(doc,context,now);
}
int main() {
    management.principal="installation";management.nameSpace=bond;management.claimed=true;management.until=121000;

    // The current error envelope must fit the bounded BLE attribute.
    auto largestError = request("wifi_save");
    largestError["ack"]="wifi";largestError["ok"]=false;largestError["stage"]="rejected";largestError["error"]="storage_unknown";
    largestError["storage_stage"]="storage_encode";largestError["encode_stage"]="initialize";largestError["encode_status"]=-2;
    largestError["storage_bytes"]=0;largestError["unencoded_bytes"]=5248;largestError["nvs_free_entries"]=0;
    largestError["device_mac"]="AA:BB:CC:DD:EE:FF";
    String largestFrame=bleBuildReply(largestError,String(std::string(64,'r').c_str()),UINT32_MAX,UINT32_MAX);
    JsonDocument decodedError;assert(!deserializeJson(decodedError,largestFrame));
    assert(largestFrame.length()<=512&&decodedError["error"]=="storage_unknown");
    testNvsBlob.clear();assert(wifiSchemesBegin());
    // Unsupported persisted bytes remain intact and block writes.
    testNvsLegacyBlob={1,2,3};const auto old=testNvsLegacyBlob;
    assert(!wifiSchemesBegin());auto blocked=change("blocked",0);
    assert(invoke(blocked)["ok"]==false&&testNvsLegacyBlob==old&&testNvsBlob.empty());
    testNvsLegacyBlob.clear();assert(wifiSchemesBegin());
    auto initial=change("initial",0);assert(invoke(initial)["outcome"]=="committed");
    testNvsBlob.back()^=1;const auto corrupt=testNvsBlob;
    assert(!wifiSchemesBegin()&&testNvsBlob==corrupt); // reject, never erase corruption
    testNvsBlob.clear();wifiManagementResetSession();assert(wifiSchemesBegin());
    auto first = change("txn1",0); int begins = WiFi.begins;
    assert(invoke(first)["outcome"] == "committed" && WiFi.begins == begins);
    assert(!*wifiStore.active.id); // save never activates
    auto blob = testNvsBlob;
    assert(invoke(first)["revision"] == 1 && testNvsBlob == blob); // identical replay
    first["expected_revision"] = 100;
    assert(invoke(first)["outcome"] == "committed"); // session/revision envelope not business ID
    first["changes"][0]["ssid"] = "Other";
    assert(invoke(first)["error"] == "request_conflict" && testNvsBlob == blob);
    auto full = snapshot(); assert(full["networks"].size() == 2);
    assert(full["networks"][0]["writable"] == true && full["networks"][1]["bond"] == "legacy");
    String serialized; serializeJson(full, serialized);
    assert(full["networks"][0]["network"]["password"]=="");
    assert(full["networks"][1]["network"]["password"].isNull());
    auto pageRequest = request("snapshot"); auto page = invoke(pageRequest);
    pageRequest["snapshot_id"] = page["snapshot_id"]; pageRequest["offset"] = page["next_offset"];
    if (page["complete"] != true) assert(invoke(pageRequest, 31001)["error"] == "snapshot_expired");
    management.nameSpace=other; // provenance does not restrict current owner reads
    full=snapshot();assert(full["networks"][0]["writable"]==true);
    management.claimed=false;full=snapshot();assert(full["networks"][0]["writable"]==false);
    assert(full["networks"][0]["network"]["password"].isNull());management.claimed=true;
    auto second=change("txn2",1,"OtherDevice");second["changes"][0]["network_id"]="travel";
    assert(invoke(second)["outcome"]=="committed");management.nameSpace=bond;
    auto expiryPageRequest=request("snapshot");
    auto expiryPage=invoke(expiryPageRequest);assert(expiryPage["complete"]==false);
    expiryPageRequest["snapshot_id"]=expiryPage["snapshot_id"];expiryPageRequest["offset"]=expiryPage["next_offset"];
    assert(invoke(expiryPageRequest,31001)["error"]=="snapshot_expired");
    auto invalid = change("txn3",2);
    auto n = invalid["changes"].as<JsonArray>().add<JsonObject>(); n["network_id"] = "invalid";
    blob = testNvsBlob;
    assert(invoke(invalid)["error"] == "shape" && wifiStore.revision == 2);
    auto rejected = request("result"); rejected["query_id"]="txn3"; assert(invoke(rejected)["outcome"]=="rejected");
    blob=testNvsBlob;
    auto storage = change("txn4",2,"Edited"); testNvsFail = true;
    assert(invoke(storage)["error"] == "storage_unknown" && blob == testNvsBlob); testNvsFail = false;
    assert(wifiSchemesBegin()); // revision and receipt survive reboot
    auto query = request("result"); query["query_id"] = "txn1";
    assert(invoke(query)["outcome"] == "committed");
    query["bridge_id"]="different";assert(invoke(query)["error"]=="principal_mismatch");
    management.principal="different";assert(invoke(query)["outcome"]=="unknown");management.principal="installation";
    // Isolate the RF lifecycle from the receipt-capacity fixture below.
    testNvsBlob.clear();running=false;assert(wifiSchemesBegin());
    auto seed=change("rf-seed",0);auto travel=seed["changes"].as<JsonArray>().add<JsonObject>();
    travel["network_id"]="travel";travel["host_ssid"]="Travel";travel["ssid"]="Travel";travel["credential"]="open";
    assert(invoke(seed)["outcome"]=="committed");
    auto activate = request("wifi_activate", "activate1"); activate["network_id"] = "home"; activate["version"] = 1;
    JsonDocument result; assert(!deserializeJson(result, control(activate,bond,1,1000,121000,0,true)));
    assert(result["stage"] == "connecting"); WiFi.connected = WL_CONNECTED; WiFi.ipReady = true;
    wifiSetupPoll(2000); assert(wifiSetupActivated());
    auto postCommitCancel=request("wifi_setup_cancel","cancel-activation");postCommitCancel["query_id"]="activate1";blob=testNvsBlob;
    deserializeJson(result,control(postCommitCancel,bond,1,2000,121000,0,true));
    assert(result["persisted"]==true&&blob==testNvsBlob);
    String activeSsid, activePass; assert(wifiSchemesCredentials(activeSsid, activePass) && activeSsid == "Device");
    auto edit = change("txn5",wifiStore.revision,"PendingVersion"); assert(invoke(edit)["outcome"] == "committed");
    assert(wifiSchemesCredentials(activeSsid, activePass) && activeSsid == "Device");
    assert(wifiStore.activeVersion == 1);
    auto trial = request("wifi_trial", "trial"); trial["network_id"] = "trial";
    trial["ssid"] = "Candidate"; trial["host_ssid"] = "Host"; trial["password"] = ""; trial["replace"] = false;
    blob = testNvsBlob;
    assert(!deserializeJson(result,control(trial,bond,1,1000,121000,0,true)));
    WiFi.connected = WL_CONNECTED; WiFi.ipReady = true;
    wifiSetupPoll(2000);
    assert(!wifiSetupBusy() && !wifiSetupActivated() && testNvsBlob != blob && schemes.findShared("trial")==-1 && WiFi.SSID() == "Device");
    // Production protocol: no HTTP snapshot is needed to match one Bridge row.
    auto match=request("wifi_match",std::string(64,'m').c_str());match["network_id"]="home";
    const int home=schemes.find(bond,"home");match["metadata_key"]=wifiMetadataKey(schemes.networks[home],schemes.networks[home].label.c_str(),false);match["open"]=false;
    auto matching=[&](){JsonDocument r;assert(!deserializeJson(r,control(match,bond,1,2000,0,60000,true)));return r;};
    assert(matching()["matches"]==true&&matching()["active"]==false); // edited, not silently active
    match["metadata_key"]=std::string(64,'0');assert(matching()["matches"]==false);
    match["metadata_key"]=wifiMetadataKey(schemes.networks[home],schemes.networks[home].label.c_str(),false);
    WiFi.connected=WL_CONNECTED;WiFi.ipReady=true; // old network has recovered after trial
    auto ordinary=request("wifi_activate","ordinary-connect");ordinary["network_id"]="home";ordinary["version"]=schemes.networks[home].version;
    assert(!deserializeJson(result,control(ordinary,bond,1,2000,0,60000,true)));
    assert(result["stage"]=="accepted");const int ordinaryBegins=WiFi.begins;
    wifiSetupPoll(2001);assert(WiFi.begins>ordinaryBegins&&wifiSetupBusy()); // BLE is still encrypted/live
    wifiDisconnectReason=202;wifiSetupPoll(27001);
    assert(!wifiSetupBusy()&&matching()["error"]=="authentication_failed"&&matching()["reason"]==202);
    assert(WiFi.begins==ordinaryBegins+1); // expired attempt cannot start fallback RF
    assert(wifiSchemesCredentials(activeSsid,activePass)&&activeSsid=="Device");
    JsonDocument failedFrame;failedFrame.set(matching());
    String failedWire=bleBuildReply(failedFrame,match["request_id"].as<String>(),UINT32_MAX,UINT32_MAX);
    assert(failedWire.length()&&failedWire.length()<=512&&failedWire.indexOf("password")==-1);
    ordinary["request_id"]="ordinary-success";
    deserializeJson(result,control(ordinary,bond,1,30000,0,60000,true));
    wifiSetupPoll(30001);WiFi.connected=WL_CONNECTED;WiFi.ipReady=false;
    wifiSetupPoll(31000);assert(wifiSetupBusy()); // DHCP is part of proof
    WiFi.ipReady=true;wifiSetupPoll(32000);
    assert(!wifiSetupBusy()&&matching()["connected"]==true&&matching()["error"].isNull());
    wifiSetupConnectionStarted(String(schemes.networks[home].ssid));wifiDisconnectReason=202;WiFi.connected=0;WiFi.ssid="";
    assert(matching()["error"]=="authentication_failed"&&matching()["stage"]=="failed");
    WiFi.connected=WL_CONNECTED;WiFi.ssid=String(schemes.networks[home].ssid);wifiDisconnectReason=0;
    auto wrongVersion=ordinary;wrongVersion["request_id"]="old-version";wrongVersion["version"]=0;
    deserializeJson(result,control(wrongVersion,bond,1,33000,0,60000,true));assert(result["error"]=="revision_conflict");
    auto otherMatch=match;deserializeJson(result,control(otherMatch,other,1,33000,0,60000,true));assert(result["matches"]==true);
    std::cout<<"BLE network feedback: matching, old-version rejection/shared owner access, BLE-alive association, auth failure, DHCP and wire budget passed\n";
    wifiManagementResetSession();assert(invoke(edit)["outcome"]=="committed");
    auto unauthorized=change("txn6",wifiStore.revision);management.sessionValid=false;
    assert(invoke(unauthorized)["error"]=="session_expired");management.sessionValid=true;
    auto protectedDelete=change("delete-active",wifiStore.revision);
    protectedDelete["changes"][0]["deleted"]=true;
    String beforeSsid,beforePassword;
    assert(wifiSchemesCredentials(beforeSsid,beforePassword));
    const auto beforeActiveVersion=wifiStore.activeVersion;
    const int beforeDeleteBegins=WiFi.begins;
    assert(invoke(protectedDelete,2000)["outcome"]=="committed");
    assert(schemes.find(bond,"home")<0 && schemes.find(bond,"travel")>=0);
    assert(wifiSchemesCredentials(activeSsid,activePass) && activeSsid==beforeSsid && activePass==beforePassword);
    assert(wifiStore.activeVersion==beforeActiveVersion && WiFi.begins==beforeDeleteBegins);
    assert(wifiSchemesBegin());
    assert(wifiSchemesCredentials(activeSsid,activePass) && activeSsid==beforeSsid && activePass==beforePassword);
    auto expired=change("expired",wifiStore.revision);
    assert(invoke(expired,122000)["error"]=="window_required");
    // Conservative durable result retention refuses new writes at capacity;
    // it never silently evicts a transaction whose ACK could have been lost.
    int free=8-wifiStore.receipts.size();
    for(int i=0;i<free;++i){auto fill=change(("fill"+std::to_string(i)).c_str(),wifiStore.revision);assert(invoke(fill,2000)["outcome"]=="committed");}
    auto fullLedger=change("ledger-full",wifiStore.revision);const auto finalBlob=testNvsBlob;
    assert(invoke(fullLedger,2000)["error"]=="result_capacity"&&finalBlob==testNvsBlob);
    std::cout << "Current management: migration/corruption/atomic/replay/owner/legacy/active/trial/reboot/expiry/ledger passed\n";
    // Routine, owner-approved paired BLE writes borrow the formal window,
    // atomically commit two rows, and recover by receipt without a full read.
    testNvsBlob.clear();assert(wifiSchemesBegin());wifiManagementResetSession();
    auto batch=request("wifi_save","formal-batch");batch["expected_revision"]=0;
    auto rows=batch["changes"].to<JsonArray>();
    for(const char *id : {"home","travel"}) {auto n=rows.add<JsonObject>();n["network_id"]=id;n["host_ssid"]=String(id);n["ssid"]="Device";n["label"]="Network";n["credential"]="open";}
    WifiControlContext formal;formal.principal="installation";formal.nameSpace=bond;formal.generation=9;
    auto bleInvoke=[&](JsonDocument &doc,bool allowed=true,uint64_t now=1000,uint64_t until=30000){
        formal.claimed=allowed;formal.occupied=!allowed;formal.until=until;
        JsonDocument r;assert(!deserializeJson(r,wifiControlCommand(doc,formal,now)));return r;
    };
    assert(bleInvoke(batch,false)["error"]=="occupied" && wifiStore.revision==0);
    assert(bleInvoke(batch,true,1000,0)["error"]=="window_required" && wifiStore.revision==0);
    auto committed=bleInvoke(batch);assert(committed["outcome"]=="committed"&&committed["revision"]==1&&committed["bond"]==bond);
    assert(schemes.find(bond,"home")>=0&&schemes.find(bond,"travel")>=0);
    auto formalQuery=request("wifi_result","query");formalQuery["query_id"]="formal-batch";
    assert(bleInvoke(formalQuery)["outcome"]=="committed");
    formal.nameSpace=other;assert(bleInvoke(formalQuery)["outcome"]=="unknown");formal.nameSpace=bond;
    auto changed=batch;changed["changes"][0]["ssid"]="Different";assert(bleInvoke(changed)["error"]=="request_conflict");
    auto expiredBatch=batch;expiredBatch["request_id"]="expired-formal";expiredBatch["expected_revision"]=1;
    assert(bleInvoke(expiredBatch,true,30000,30000)["error"]=="window_required");
    std::cout<<"Formal BLE window: atomic batch, owner/expiry/bond rejection, durable receipt and replay passed\n";
    // Permanent sequence fence survives ACK retirement, channel/session reset
    // and atomic NVS failures; legacy arbitrary IDs cannot be safely retired.
    auto seq=change("w3_0000000000000001",wifiStore.revision);
    assert(bleInvoke(seq)["outcome"]=="committed");
    auto confirm=request("result_ack");confirm["query_id"]=seq["request_id"];confirm["result_peer"]=bond;
    const auto beforeAck=testNvsBlob;
    testNvsFail=true;assert(bleInvoke(confirm)["error"]=="storage_unknown"&&testNvsBlob==beforeAck);testNvsFail=false;
    assert(wifiSchemesBegin()&&bleInvoke(seq)["outcome"]=="committed");
    testNvsCommitThenFail=true;assert(bleInvoke(confirm)["error"]=="storage_unknown");testNvsCommitThenFail=false;
    assert(wifiSchemesBegin()&&bleInvoke(confirm)["outcome"]=="retired");
    assert(bleInvoke(confirm)["outcome"]=="retired");
    assert(bleInvoke(seq)["error"]=="request_retired");
    seq["expected_revision"]=wifiStore.revision;seq["changes"][0]["ssid"]="Replay";
    assert(bleInvoke(seq)["error"]=="request_retired");
    confirm["query_id"]="formal-batch";assert(bleInvoke(confirm)["error"]=="legacy_receipt_retained");
    confirm["query_id"]="w3_0000000000000001";confirm["bridge_id"]="foreign";assert(bleInvoke(confirm)["error"]=="principal_mismatch");
    formal.principal="foreign";assert(bleInvoke(confirm)["outcome"]=="unknown");formal.principal="installation";
    seq["request_id"]="w3_0000000000000002";assert(bleInvoke(seq)["outcome"]=="committed");
    const auto committedSeq=testNvsBlob;seq["changes"][0]["ssid"]="Conflict";
    assert(bleInvoke(seq)["error"]=="request_conflict"&&testNvsBlob==committedSeq);
    wifiManagementResetSession();assert(wifiSchemesBegin());
    auto seqQuery=request("result");seqQuery["query_id"]="w3_0000000000000002";seqQuery["result_peer"]=bond;
    assert(bleInvoke(seqQuery)["outcome"]=="committed");
    std::cout<<"WFS4 permanent fence: confirmation, retained legacy, cross-principal, replay and commit-then-error restart passed\n";

    // Both adapters call this exact production handler. USB continuous has no
    // permit/BLE/PowerPlan prerequisite; ownership still gates every side effect.
    testNvsBlob.clear();running=false;wifiManagementResetSession();assert(wifiSchemesBegin());
    WifiControlContext ctx;ctx.principal="installation";ctx.nameSpace=bond;ctx.until=UINT64_MAX;ctx.claimed=true;
    auto unified=[&](JsonDocument &d){JsonDocument r;assert(!deserializeJson(r,wifiControlCommand(d,ctx,1000)));return r;};
    auto usb=change("w3_0000000000000010",0);ctx.claimed=false;
    assert(unified(usb)["error"]=="claim_required"&&wifiStore.revision==0);
    auto forged=usb;forged["permit"]="retired-permit";forged["namespace"]=other;forged["result_peer"]=other;
    assert(unified(forged)["error"]=="claim_required"&&wifiStore.revision==0);
    ctx.occupied=true;assert(unified(usb)["error"]=="occupied");ctx.occupied=false;ctx.claimed=true;
    ctx.sessionValid=false;assert(unified(usb)["error"]=="session_expired");ctx.sessionValid=true;
    assert(unified(usb)["outcome"]=="committed");
    auto queryUsb=request("result");queryUsb["query_id"]=usb["request_id"];
    ctx.until=0;ctx.claimed=false;ctx.sessionValid=false;
    assert(unified(queryUsb)["outcome"]=="committed"&&unified(usb)["outcome"]=="committed");
    auto snap=request("snapshot");assert(unified(snap)["error"]=="window_required");
    ctx.until=UINT64_MAX;ctx.claimed=true;ctx.sessionValid=true;
    auto activateUsb=request("activate","w3_0000000000000011");activateUsb["network_id"]="home";activateUsb["version"]=1;
    const int rfBeforePending=WiFi.begins;testNvsFail=true;
    assert(unified(activateUsb)["error"]=="storage_unknown"&&WiFi.begins==rfBeforePending);testNvsFail=false;assert(wifiSchemesBegin());
    auto started=unified(activateUsb);assert(started["ok"]==true&&wifiSetupBusy());
    auto pendingBytes=testNvsBlob;auto queryActivation=request("result");queryActivation["query_id"]=activateUsb["request_id"];
    assert(unified(queryActivation)["outcome"]=="accepted");
    wifiSetupPoll(1001);assert(wifiSetupBusy()); // transport disconnect is not cancellation
    WiFi.connected=WL_CONNECTED;WiFi.ipReady=true;wifiSetupPoll(1002);
    assert(wifiSetupActivated()&&unified(queryActivation)["outcome"]=="committed");
    auto completedBytes=testNvsBlob;requestId="";assert(wifiSchemesBegin());
    const int rfAfterCommit=WiFi.begins;ctx.claimed=false;ctx.sessionValid=false;
    assert(unified(activateUsb)["outcome"]=="committed"&&WiFi.begins==rfAfterCommit);
    testNvsBlob=pendingBytes;wifiExecutionGeneration=0;assert(wifiSchemesBegin());assert(unified(queryActivation)["error"]=="interrupted");
    assert(unified(activateUsb)["error"]=="interrupted"&&WiFi.begins==rfAfterCommit);
    testNvsBlob=completedBytes;assert(wifiSchemesBegin());ctx.claimed=true;ctx.sessionValid=true;
    auto usbTrial=request("trial","w3_0000000000000012");usbTrial["network_id"]="trial";usbTrial["host_ssid"]="Host";usbTrial["ssid"]="Candidate";usbTrial["password"]="";usbTrial["replace"]=false;
    assert(unified(usbTrial)["ok"]==true);WiFi.connected=WL_CONNECTED;WiFi.ipReady=true;
    wifiSetupPoll(1002);assert(!wifiSetupBusy());requestId="";assert(wifiSchemesBegin());
    auto queryTrial=request("result");queryTrial["query_id"]=usbTrial["request_id"];assert(unified(queryTrial)["outcome"]=="completed");
    usbTrial["request_id"]="w3_0000000000000013";ctx.until=1005;assert(unified(usbTrial)["ok"]==true);
    int beforeClose=WiFi.begins;wifiSetupConstrain(1005,0,true);assert(!wifiSetupBusy()&&WiFi.begins==beforeClose);
    std::cout<<"Unified HTTP/BLE: USB no-permit, strict claim, stale session replay, pending/active atomicity, durable trial and no RF after deadline passed\n";
    // A sole saved active configuration remains protected. Build this fixture
    // separately: the earlier deletion intentionally has another bond's row.
    testNvsBlob.clear();running=false;
    wifiManagementResetSession();assert(wifiSchemesBegin());
    ctx.until=UINT64_MAX;ctx.claimed=true;ctx.sessionValid=true;ctx.nameSpace=bond;
    auto only=change("only",0);
    assert(unified(only)["outcome"]=="committed");
    wifiStore.selectActive(schemes.find(bond,"home"));assert(saveV2(wifiStore));
    auto deleteOnly=change("delete-only",wifiStore.revision);
    deleteOnly["changes"][0]["deleted"]=true;
    const auto onlyRevision=wifiStore.revision;
    assert(unified(deleteOnly)["error"]=="active_protected");
    assert(wifiStore.revision==onlyRevision && schemes.networks.size()==1);
    assert(wifiSchemesBegin() && wifiSchemesCredentials(activeSsid,activePass) && activeSsid=="Device");
    std::cout<<"Active deletion: sole configuration refused, replacement allowed, live credentials retained across reload\n";
    testNvsBlob.clear();running=false;
    wifiManagementResetSession();assert(wifiSchemesBegin());
    ctx.until=UINT64_MAX;ctx.claimed=true;ctx.sessionValid=true;ctx.nameSpace=bond;
    auto namespaceA=change("w3_0000000000000001",0);
    assert(unified(namespaceA)["outcome"]=="committed");
    ctx.nameSpace=other;
    auto namespaceB=change("w3_0000000000000001",1,"SecondNamespace");
    namespaceB["changes"][0]["network_id"]="travel";
    assert(unified(namespaceB)["outcome"]=="committed");
    assert(wifiSchemesBegin() && wifiStore.receipts.size()==2);
    auto namespaceQuery=request("result");namespaceQuery["query_id"]=namespaceA["request_id"];
    assert(unified(namespaceQuery)["revision"]==2 && unified(namespaceQuery)["bond"]==other);
    auto conflicting=namespaceB;conflicting["changes"][0]["ssid"]="Conflict";
    assert(unified(conflicting)["error"]=="request_conflict");
    auto namespaceAck=request("result_ack");namespaceAck["query_id"]=namespaceA["request_id"];
    assert(unified(namespaceAck)["outcome"]=="retired");
    assert(unified(namespaceQuery)["outcome"]=="retired");
    ctx.nameSpace=bond;
    assert(unified(namespaceQuery)["outcome"]=="committed" && unified(namespaceQuery)["revision"]==1);
    assert(unified(namespaceA)["outcome"]=="committed");
    assert(unified(namespaceAck)["outcome"]=="retired");
    assert(wifiSchemesBegin() && wifiStore.receipts.size()==0 && wifiStore.cursors.size()==2);
    assert(unified(namespaceA)["error"]=="request_retired");
    ctx.nameSpace=other;assert(unified(namespaceB)["error"]=="request_retired");
    std::cout<<"TransactionKey: namespace-isolated same ID, conflict, reload, ACK and replay fences passed\n";
    for (bool trialOperation : {false,true}) {
        testNvsBlob.clear();running=false;wifiExecutionGeneration=0;
        wifiManagementResetSession();assert(wifiSchemesBegin());
        ctx.nameSpace=bond;ctx.until=UINT64_MAX;ctx.claimed=true;ctx.sessionValid=true;
        auto setup=change("w3_0000000000000001",0);assert(unified(setup)["outcome"]=="committed");
        auto command=request(trialOperation?"trial":"activate","w3_0000000000000002");
        command["network_id"]="home";command["version"]=1;
        if(trialOperation){command["host_ssid"]="TrialHost";command["ssid"]="TrialDevice";command["password"]="";command["replace"]=false;}
        assert(unified(command)["ok"]==true);
        auto pending=testNvsBlob;
        auto outcome=request("result");outcome["query_id"]=command["request_id"];
        // Simulated reboot: recovery must be committed and read back before
        // claiming an interrupted terminal state or allowing receipt retirement.
        running=false;requestId="";wifiExecutionGeneration=0;
        testNvsFail=true;assert(!wifiSchemesBegin()&&testNvsBlob==pending);
        assert(unified(outcome)["error"]=="storage");testNvsFail=false;
        testNvsReadbackFail=true;assert(!wifiSchemesBegin());testNvsReadbackFail=false;
        assert(wifiSchemesBegin()&&unified(outcome)["error"]=="interrupted");
        assert(!*wifiStore.active.id && wifiStore.revision==1);
        auto retire=request("result_ack");retire["query_id"]=command["request_id"];
        assert(unified(retire)["outcome"]=="retired"&&unified(retire)["outcome"]=="retired");
        const int noReplay=WiFi.begins;assert(unified(command)["error"]=="request_retired"&&WiFi.begins==noReplay);
        // Old pending has no generation proof and must stay unresolved.
        testNvsBlob=pending;assert(wifiSchemesBegin());
        auto legacyPending=wifiStore;legacyPending.executionGeneration=0;
        for(auto &r:legacyPending.receipts){r.executionGeneration=0;if(!strcmp(r.request,"w3_0000000000000002"))r.error.set("pending");}
        assert(saveV2(legacyPending));wifiExecutionGeneration=0;assert(wifiSchemesBegin());
        assert(unified(outcome)["outcome"]=="unknown"&&unified(retire)["outcome"]=="unknown");
    }
    std::cout<<"WTX1 interruption: atomic recovery/readback failure, old pending retained, no replay after ACK passed\n";
    // Identical namespace/request ID under a different principal is a new
    // transaction, never a hit in a transport-local replay shortcut.
    testNvsBlob.clear();running=false;requestId="";assert(wifiSchemesBegin());
    ctx.principal="installation";ctx.nameSpace=bond;ctx.claimed=true;ctx.sessionValid=true;ctx.until=UINT64_MAX;
    auto principalTrial=request("trial","w3_0000000000000001");principalTrial["network_id"]="principal-trial";
    principalTrial["host_ssid"]="Host";principalTrial["ssid"]="Candidate";principalTrial["password"]="";principalTrial["replace"]=false;
    assert(unified(principalTrial)["ok"]==true);WiFi.connected=WL_CONNECTED;WiFi.ipReady=true;wifiSetupPoll(1002);
    ctx.principal="other-installation";principalTrial["bridge_id"]=ctx.principal;
    assert(unified(principalTrial)["ok"]==true&&wifiSetupBusy()&&wifiStore.receipts.size()==2);
    auto foreignCancel=request("setup_cancel","cancel");foreignCancel["query_id"]=principalTrial["request_id"];
    ctx.principal="installation";assert(unified(foreignCancel)["error"]=="not_found"&&wifiSetupBusy());
    auto principalResult=request("result");principalResult["query_id"]=principalTrial["request_id"];
    assert(unified(principalResult)["outcome"]=="completed");
    ctx.principal="other-installation";principalResult["bridge_id"]=ctx.principal;
    assert(unified(principalResult)["outcome"]=="accepted");
    WiFi.connected=WL_CONNECTED;WiFi.ipReady=true;wifiSetupPoll(1003);
    assert(unified(principalResult)["outcome"]=="completed");ctx.principal="installation";
    std::cout<<"Control identity: same namespace/ID across principals stays isolated; foreign cancel cannot stop current work\n";
    const auto retiredBefore=testNvsBlob;
    const int retiredBegins=WiFi.begins;
    for(const char *op : {"wifi_authorize","wifi_migrate","wifi_setup","wifi_select","wifi_scheme_delete","wifi_future_unknown"}) {
        auto retired=request(op,"retired-probe");
        assert(unified(retired)["error"]=="unsupported");
        assert(testNvsBlob==retiredBefore && WiFi.begins==retiredBegins);
    }
    std::cout<<"Current context rejects retired/unknown Wi-Fi operations without storage/radio effects\n";
    return 0;
}
