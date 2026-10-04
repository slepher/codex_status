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
static JsonDocument invoke(JsonDocument &d, uint64_t now = 1000) {
    JsonDocument result; assert(!deserializeJson(result, wifiManagementCommand(d, now,
        "[{\"network_id\":\"legacy-0\",\"ssid\":\"USB-network\"}]"))); return result;
}
static JsonDocument snapshot(const String &permit = "") {
    auto d = request("snapshot"); d["permit"] = permit;
    auto page = invoke(d); std::string text;
    while (true) {
        assert(page["ok"] == true); text += page["chunk"].as<const char *>();
        if (page["complete"] == true) break;
        d["snapshot_id"] = page["snapshot_id"]; d["offset"] = page["next_offset"];
        page = invoke(d);
    }
    JsonDocument result; assert(!deserializeJson(result, text)); return result;
}
static JsonDocument change(const char *id, uint64_t revision, const String &permit, const char *ssid = "Device") {
    auto d = request("apply", id); d["expected_revision"] = revision; d["permit"] = permit;
    auto n = d["changes"].to<JsonArray>().add<JsonObject>(); n["network_id"] = "home";
    n["host_ssid"] = "Host"; n["ssid"] = ssid; n["label"] = "Desk"; n["credential"] = "open";
    return d;
}
int main() {
    // Explicit old-slot migration exercises production atomic storage, not a
    // compatibility projection; credentials never leave this test/device RAM.
    testNvsBlob.clear(); assert(wifiSchemesBegin());
    WifiNetwork slots[2] = {};
    slots[0].id.set("wifi-slot-0"); slots[0].ssid.set("First-AP"); slots[0].password.set("first-secret");
    slots[1].id.set("wifi-slot-1"); slots[1].ssid.set("Current-AP"); slots[1].password.set("current-secret");
    auto migration = request("wifi_migrate","slots");
    auto migrateSlots = [&](uint64_t until=121000,bool allowed=true) {
        JsonDocument r; deserializeJson(r,wifiSchemesMigrate(migration,bond,slots,2,"Current-AP",1000,until,allowed)); return r;
    };
    auto largestError = request("wifi_migrate");
    largestError["ack"]="wifi";largestError["ok"]=false;largestError["stage"]="rejected";largestError["error"]="storage_unknown";
    largestError["storage_stage"]="storage_encode";largestError["encode_stage"]="initialize";largestError["encode_status"]=-2;
    largestError["storage_bytes"]=0;largestError["unencoded_bytes"]=5248;largestError["nvs_free_entries"]=0;
    largestError["device_mac"]="AA:BB:CC:DD:EE:FF";
    String largestFrame=bleBuildReply(largestError,String(std::string(64,'r').c_str()),UINT32_MAX,UINT32_MAX);
    JsonDocument decodedError;assert(!deserializeJson(decodedError,largestFrame));
    assert(largestFrame.length()<=512&&decodedError["error"]=="storage_unknown");
    int initialBegins = WiFi.begins;
    assert(migrateSlots(1000)["error"]=="physical_required" && testNvsBlob.empty());
    assert(migrateSlots(121000,false)["error"]=="unauthorized_or_occupied" && testNvsBlob.empty());
    testNvsFail=true; assert(migrateSlots()["error"]=="storage_unknown" && testNvsBlob.empty()); testNvsFail=false;
    testNvsReadbackFail=true;
    assert(migrateSlots()["error"]=="storage_unknown" && !testNvsBlob.empty() && wifiStore.revision==0);
    const auto uncertainBlob=testNvsBlob; testNvsReadbackFail=false;
    assert(wifiSchemesBegin() && migrateSlots()["persisted"]==true && testNvsBlob==uncertainBlob);
    assert(wifiStore.revision==1);
    String migratedSsid,migratedPassword;
    assert(wifiSchemesCredentials(migratedSsid,migratedPassword) && migratedSsid=="Current-AP" && migratedPassword=="current-secret");
    auto migrationBlob=testNvsBlob;
    assert(wifiSchemesBegin() && migrateSlots()["persisted"]==true && migrationBlob==testNvsBlob);
    slots[0].password.set("conflicting-secret");
    assert(migrateSlots()["error"]=="migration_conflict" && migrationBlob==testNvsBlob);
    assert(WiFi.begins==initialBegins);
    testNvsBlob.clear(); assert(wifiSchemesBegin());
    slots[0].password.set("first-secret"); slots[0].ssid.set("Current-AP");
    assert(migrateSlots()["ok"]==false && testNvsBlob.empty()); // ambiguous mapping, no silent merge
    slots[0].ssid.set("First-AP");
    assert(migrateSlots()["persisted"]==true);
    WifiNetwork extra = {}; extra.id.set("wifi-slot-2"); extra.ssid.set("Third-AP");
    JsonDocument extraResult; deserializeJson(extraResult,wifiSchemesMigrate(migration,bond,&extra,1,"Third-AP",1000,121000,true));
    assert(extraResult["persisted"]==true && wifiSchemesCredentials(migratedSsid,migratedPassword) && migratedSsid=="Current-AP");
    testNvsBlob.clear(); assert(wifiSchemesBegin());
    testNvsByteLimit=1024;
    assert(migrateSlots()["persisted"]==true); // small real data must fit limited NVS
    assert(testNvsBlob.size()<=testNvsByteLimit);
    std::cout << "NVS two-slot encoding " << sizeof(WifiStore) << " -> " << testNvsBlob.size() << " B\n";
    testNvsByteLimit=SIZE_MAX;testNvsBlob.clear();assert(wifiSchemesBegin());
    // Read-only v1 migration preserves both the blob and active credentials.
    WifiLegacySchemes old; WifiLegacyNetwork previous;
    strcpy(previous.bond,other);strcpy(previous.id,"migrated");strcpy(previous.host,"OldHost");strcpy(previous.ssid,"OldDevice");
    assert(!old.put(previous,false));old.seal();
    testNvsLegacyBlob.assign((const uint8_t *)&old,(const uint8_t *)&old+sizeof(old));
    assert(wifiSchemesBegin()&&testNvsLegacyBlob.empty()&&!testNvsBlob.empty());
    String oldSsid,oldPass;assert(wifiSchemesCredentials(oldSsid,oldPass)&&oldSsid=="OldDevice");
    auto migrateGrant=request("wifi_authorize");JsonDocument migrated;
    deserializeJson(migrated,wifiManagementAuthorize(migrateGrant,bond,1000,121000,0));
    auto migrate=change("migrate",0,migrated["permit"].as<String>());
    assert(invoke(migrate)["outcome"]=="committed"&&testNvsLegacyBlob.empty());
    assert(wifiSchemesCredentials(oldSsid,oldPass)&&oldSsid=="OldDevice");
    testNvsBlob.back()^=1;const auto corrupt=testNvsBlob;
    assert(!wifiSchemesBegin()&&testNvsBlob==corrupt); // reject, never erase corruption
    testNvsBlob.clear();wifiManagementResetSession();assert(wifiSchemesBegin());
    auto authorize = request("wifi_authorize"); JsonDocument ack;
    assert(!deserializeJson(ack, wifiManagementAuthorize(authorize, bond, 1000, 121000, 0)));
    String permit = ack["permit"].as<String>();
    String frame = bleBuildReply(ack, "read", 1, 1); assert(frame.length() && frame.length() <= 512);
    auto first = change("txn1",0,permit); int begins = WiFi.begins;
    assert(invoke(first)["outcome"] == "committed" && WiFi.begins == begins);
    assert(!*wifiStore.active.id); // save never activates
    auto blob = testNvsBlob;
    assert(invoke(first)["revision"] == 1 && testNvsBlob == blob); // identical replay
    first["expected_revision"] = 100;
    assert(invoke(first)["outcome"] == "committed"); // session/revision envelope not business ID
    first["changes"][0]["ssid"] = "Other";
    assert(invoke(first)["error"] == "request_conflict" && testNvsBlob == blob);
    auto full = snapshot(permit); assert(full["networks"].size() == 2);
    assert(full["networks"][0]["writable"] == true && full["networks"][1]["bond"] == "legacy");
    String serialized; serializeJson(full, serialized);
    assert(full["networks"][0]["network"]["password"]=="");
    assert(full["networks"][1]["network"]["password"].isNull());
    auto pageRequest = request("snapshot"); auto page = invoke(pageRequest);
    pageRequest["snapshot_id"] = page["snapshot_id"]; pageRequest["offset"] = page["next_offset"];
    if (page["complete"] != true) assert(invoke(pageRequest, 31001)["error"] == "snapshot_expired");
    assert(!deserializeJson(ack, wifiManagementAuthorize(authorize, other, 1000, 121000, 0)));
    auto foreignPermit = ack["permit"].as<String>();
    full = snapshot(foreignPermit); assert(full["networks"][0]["writable"] == false);
    assert(full["networks"][0]["network"]["password"].isNull());
    // Add a second-bond network, no cross-bond replacement.
    auto second = change("txn2", 1, foreignPermit, "OtherDevice");
    assert(invoke(second)["outcome"] == "committed");
    auto expiryPageRequest=request("snapshot");expiryPageRequest["permit"]=permit;
    auto expiryPage=invoke(expiryPageRequest);assert(expiryPage["complete"]==false);
    expiryPageRequest["snapshot_id"]=expiryPage["snapshot_id"];expiryPageRequest["offset"]=expiryPage["next_offset"];
    assert(invoke(expiryPageRequest,31001)["error"]=="snapshot_expired");
    auto invalid = change("txn3", 2, permit);
    auto n = invalid["changes"].as<JsonArray>().add<JsonObject>(); n["network_id"] = "invalid";
    blob = testNvsBlob;
    assert(invoke(invalid)["error"] == "shape" && wifiStore.revision == 2);
    auto rejected = request("result"); rejected["query_id"]="txn3"; assert(invoke(rejected)["outcome"]=="rejected");
    blob=testNvsBlob;
    auto storage = change("txn4", 2, permit, "Edited"); testNvsFail = true;
    assert(invoke(storage)["error"] == "storage_unknown" && blob == testNvsBlob); testNvsFail = false;
    assert(wifiSchemesBegin()); // revision and receipt survive reboot
    auto query = request("result"); query["query_id"] = "txn1";
    assert(invoke(query)["outcome"] == "committed");
    query["bridge_id"] = "different"; assert(invoke(query)["outcome"] == "unknown");
    auto activate = request("wifi_activate", "activate1"); activate["network_id"] = "home"; activate["version"] = 1;
    JsonDocument result; assert(!deserializeJson(result, wifiSetupCommand(activate,bond,1,1000,121000,0,true)));
    assert(result["stage"] == "connecting"); WiFi.connected = WL_CONNECTED; WiFi.ipReady = true;
    wifiSetupPoll(2000,bond,1,true); assert(wifiSetupActivated());
    auto postCommitCancel=request("wifi_setup_cancel","cancel-activation");postCommitCancel["query_id"]="activate1";blob=testNvsBlob;
    deserializeJson(result,wifiSetupCommand(postCommitCancel,bond,1,2000,121000,0,true));
    assert(result["persisted"]==true&&blob==testNvsBlob);
    String activeSsid, activePass; assert(wifiSchemesCredentials(activeSsid, activePass) && activeSsid == "Device");
    auto edit = change("txn5",wifiStore.revision,permit,"PendingVersion"); assert(invoke(edit)["outcome"] == "committed");
    assert(wifiSchemesCredentials(activeSsid, activePass) && activeSsid == "Device");
    assert(wifiStore.activeVersion == 1);
    auto trial = request("wifi_trial", "trial"); trial["network_id"] = "trial";
    trial["ssid"] = "Candidate"; trial["host_ssid"] = "Host"; trial["password"] = ""; trial["replace"] = false;
    blob = testNvsBlob;
    assert(!deserializeJson(result,wifiSetupCommand(trial,bond,1,1000,121000,0,true)));
    WiFi.connected = WL_CONNECTED; WiFi.ipReady = true;
    wifiSetupPoll(2000,bond,1,true);
    assert(!wifiSetupBusy() && !wifiSetupActivated() && testNvsBlob == blob && WiFi.SSID() == "Device");
    // Production protocol: no HTTP snapshot is needed to match one Bridge row.
    auto match=request("wifi_match",std::string(64,'m').c_str());match["network_id"]="home";
    const int home=schemes.find(bond,"home");match["metadata_key"]=wifiMetadataKey(schemes.networks[home],schemes.networks[home].label.c_str(),false);match["open"]=false;
    auto matching=[&](){JsonDocument r;assert(!deserializeJson(r,wifiSetupCommand(match,bond,1,2000,0,60000,true)));return r;};
    assert(matching()["matches"]==true&&matching()["active"]==false); // edited, not silently active
    match["metadata_key"]=std::string(64,'0');assert(matching()["matches"]==false);
    match["metadata_key"]=wifiMetadataKey(schemes.networks[home],schemes.networks[home].label.c_str(),false);
    WiFi.connected=WL_CONNECTED;WiFi.ipReady=true; // old network has recovered after trial
    auto ordinary=request("wifi_activate","ordinary-connect");ordinary["network_id"]="home";ordinary["version"]=schemes.networks[home].version;
    assert(!deserializeJson(result,wifiSetupCommand(ordinary,bond,1,2000,0,60000,true)));
    assert(result["stage"]=="accepted");const int ordinaryBegins=WiFi.begins;
    wifiSetupPoll(2001,bond,1,true);assert(WiFi.begins>ordinaryBegins&&wifiSetupBusy()); // BLE is still encrypted/live
    wifiDisconnectReason=202;wifiSetupPoll(27001,bond,1,true);
    assert(!wifiSetupBusy()&&matching()["error"]=="authentication_failed"&&matching()["reason"]==202);
    assert(WiFi.SSID()=="Device"); // failed activation restores old active network
    JsonDocument failedFrame;failedFrame.set(matching());
    String failedWire=bleBuildReply(failedFrame,match["request_id"].as<String>(),UINT32_MAX,UINT32_MAX);
    assert(failedWire.length()&&failedWire.length()<=512&&failedWire.indexOf("password")==-1);
    ordinary["request_id"]="ordinary-success";
    deserializeJson(result,wifiSetupCommand(ordinary,bond,1,30000,0,60000,true));
    wifiSetupPoll(30001,bond,1,true);WiFi.connected=WL_CONNECTED;WiFi.ipReady=false;
    wifiSetupPoll(31000,bond,1,true);assert(wifiSetupBusy()); // DHCP is part of proof
    WiFi.ipReady=true;wifiSetupPoll(32000,bond,1,true);
    assert(!wifiSetupBusy()&&matching()["connected"]==true&&matching()["error"].isNull());
    wifiSetupConnectionStarted(String(schemes.networks[home].ssid));wifiDisconnectReason=202;WiFi.connected=0;WiFi.ssid="";
    assert(matching()["error"]=="authentication_failed"&&matching()["stage"]=="failed");
    WiFi.connected=WL_CONNECTED;WiFi.ssid=String(schemes.networks[home].ssid);wifiDisconnectReason=0;
    auto wrongVersion=ordinary;wrongVersion["request_id"]="old-version";wrongVersion["version"]=0;
    deserializeJson(result,wifiSetupCommand(wrongVersion,bond,1,33000,0,60000,true));assert(result["error"]=="revision_conflict");
    auto otherMatch=match;deserializeJson(result,wifiSetupCommand(otherMatch,other,1,33000,0,60000,true));assert(result["matches"]==false);
    std::cout<<"BLE network feedback: matching, old-version/bond isolation, BLE-alive association, auth failure, DHCP and wire budget passed\n";
    wifiManagementResetSession(); assert(invoke(edit)["outcome"] == "committed");
    auto unauthorized = change("txn6",wifiStore.revision,permit);
    assert(invoke(unauthorized)["error"] == "window_required");
    assert(!deserializeJson(ack,wifiManagementAuthorize(authorize,bond,2000,122000,0)));
    auto fresh=ack["permit"].as<String>();
    auto protectedDelete=change("delete-active",wifiStore.revision,fresh);
    protectedDelete["changes"][0]["deleted"]=true;
    assert(invoke(protectedDelete,2000)["error"]=="active_protected");
    auto expired=change("expired",wifiStore.revision,fresh);
    assert(invoke(expired,122000)["error"]=="window_required");
    // Conservative durable result retention refuses new writes at capacity;
    // it never silently evicts a transaction whose ACK could have been lost.
    int free=8-wifiStore.receipts.size();
    for(int i=0;i<free;++i){auto fill=change(("fill"+std::to_string(i)).c_str(),wifiStore.revision,fresh);assert(invoke(fill,2000)["outcome"]=="committed");}
    auto fullLedger=change("ledger-full",wifiStore.revision,fresh);const auto finalBlob=testNvsBlob;
    assert(invoke(fullLedger,2000)["error"]=="result_capacity"&&finalBlob==testNvsBlob);
    std::cout << "W2 production: migration/corruption/atomic/replay/bond/legacy/active/trial/reboot/expiry/ledger passed; authorize frame " << frame.length() << " B\n";
    // Routine, owner-approved paired BLE writes borrow the formal window,
    // atomically commit two rows, and recover by receipt without a full read.
    testNvsBlob.clear();assert(wifiSchemesBegin());wifiManagementResetSession();
    auto batch=request("wifi_save","formal-batch");batch["expected_revision"]=0;
    auto rows=batch["changes"].to<JsonArray>();
    for(const char *id : {"home","travel"}) {auto n=rows.add<JsonObject>();n["network_id"]=id;n["host_ssid"]=String(id);n["ssid"]="Device";n["label"]="Network";n["credential"]="open";}
    auto bleInvoke=[&](JsonDocument &doc,bool allowed=true,uint64_t now=1000,uint64_t until=30000){JsonDocument r;assert(!deserializeJson(r,wifiSetupCommand(doc,bond,9,now,0,until,allowed)));return r;};
    assert(bleInvoke(batch,false)["error"]=="occupied" && wifiStore.revision==0);
    assert(bleInvoke(batch,true,1000,0)["error"]=="window_required" && wifiStore.revision==0);
    auto committed=bleInvoke(batch);assert(committed["outcome"]=="committed"&&committed["revision"]==1&&committed["bond"]==bond);
    assert(schemes.find(bond,"home")>=0&&schemes.find(bond,"travel")>=0);
    auto formalQuery=request("wifi_result","query");formalQuery["query_id"]="formal-batch";
    assert(bleInvoke(formalQuery)["outcome"]=="committed");
    JsonDocument foreign;deserializeJson(foreign,wifiSetupCommand(formalQuery,other,9,1000,0,30000,true));assert(foreign["outcome"]=="unknown");
    auto changed=batch;changed["changes"][0]["ssid"]="Different";assert(bleInvoke(changed)["error"]=="request_conflict");
    auto expiredBatch=batch;expiredBatch["request_id"]="expired-formal";expiredBatch["expected_revision"]=1;
    assert(bleInvoke(expiredBatch,true,30000,30000)["error"]=="window_required");
    std::cout<<"Formal BLE window: atomic batch, owner/expiry/bond rejection, durable receipt and replay passed\n";
    // Permanent sequence fence survives ACK retirement, channel/session reset
    // and atomic NVS failures; legacy arbitrary IDs cannot be safely retired.
    auto seq=change("w3_0000000000000001",wifiStore.revision,"");
    deserializeJson(ack,wifiManagementAuthorize(authorize,bond,1000,0,30000));seq["permit"]=ack["permit"];
    assert(invoke(seq)["outcome"]=="committed");
    auto confirm=request("result_ack");confirm["query_id"]=seq["request_id"];confirm["result_peer"]=bond;
    const auto beforeAck=testNvsBlob;
    testNvsFail=true;assert(invoke(confirm)["error"]=="storage_unknown"&&testNvsBlob==beforeAck);testNvsFail=false;
    assert(wifiSchemesBegin()&&invoke(seq)["outcome"]=="committed");
    testNvsCommitThenFail=true;assert(invoke(confirm)["error"]=="storage_unknown");testNvsCommitThenFail=false;
    assert(wifiSchemesBegin()&&invoke(confirm)["outcome"]=="retired");
    assert(invoke(confirm)["outcome"]=="retired");
    assert(invoke(seq)["error"]=="request_retired");
    seq["expected_revision"]=wifiStore.revision;seq["changes"][0]["ssid"]="Replay";
    assert(invoke(seq)["error"]=="request_retired");
    confirm["query_id"]="formal-batch";assert(invoke(confirm)["error"]=="legacy_receipt_retained");
    confirm["query_id"]="w3_0000000000000001";confirm["bridge_id"]="foreign";assert(invoke(confirm)["outcome"]=="unknown");
    seq["request_id"]="w3_0000000000000002";assert(invoke(seq)["outcome"]=="committed");
    const auto committedSeq=testNvsBlob;seq["changes"][0]["ssid"]="Conflict";
    assert(invoke(seq)["error"]=="request_conflict"&&testNvsBlob==committedSeq);
    wifiManagementResetSession();assert(wifiSchemesBegin());
    auto seqQuery=request("result");seqQuery["query_id"]="w3_0000000000000002";seqQuery["result_peer"]=bond;
    assert(invoke(seqQuery)["outcome"]=="committed");
    std::cout<<"WFS4 permanent fence: confirmation, retained legacy, cross-principal, replay and commit-then-error restart passed\n";

    // Both adapters call this exact production handler. USB continuous has no
    // permit/BLE/PowerPlan prerequisite; ownership still gates every side effect.
    testNvsBlob.clear();running=false;transactionUnified=false;wifiManagementResetSession();assert(wifiSchemesBegin());
    WifiControlContext ctx;ctx.principal="installation";ctx.nameSpace=bond;ctx.until=UINT64_MAX;ctx.claimed=true;
    auto unified=[&](JsonDocument &d){JsonDocument r;assert(!deserializeJson(r,wifiControlCommand(d,ctx,1000)));return r;};
    auto usb=change("w3_0000000000000010",0,"");ctx.claimed=false;
    assert(unified(usb)["error"]=="claim_required"&&wifiStore.revision==0);
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
    wifiSetupPoll(1001,"",99,false);assert(wifiSetupBusy()); // transport disconnect is not cancellation
    WiFi.connected=WL_CONNECTED;WiFi.ipReady=true;wifiSetupPoll(1002,"",99,false);
    assert(wifiSetupActivated()&&unified(queryActivation)["outcome"]=="committed");
    auto completedBytes=testNvsBlob;requestId="";transactionUnified=false;assert(wifiSchemesBegin());
    const int rfAfterCommit=WiFi.begins;ctx.claimed=false;ctx.sessionValid=false;
    assert(unified(activateUsb)["outcome"]=="committed"&&WiFi.begins==rfAfterCommit);
    testNvsBlob=pendingBytes;assert(wifiSchemesBegin());assert(unified(queryActivation)["outcome"]=="unknown");
    assert(unified(activateUsb)["outcome"]=="unknown"&&WiFi.begins==rfAfterCommit);
    testNvsBlob=completedBytes;assert(wifiSchemesBegin());ctx.claimed=true;ctx.sessionValid=true;
    auto usbTrial=request("trial","w3_0000000000000012");usbTrial["network_id"]="trial";usbTrial["host_ssid"]="Host";usbTrial["ssid"]="Candidate";usbTrial["password"]="";usbTrial["replace"]=false;
    assert(unified(usbTrial)["ok"]==true);WiFi.connected=WL_CONNECTED;WiFi.ipReady=true;
    wifiSetupPoll(1002,"",0,false);assert(!wifiSetupBusy());requestId="";transactionUnified=false;assert(wifiSchemesBegin());
    auto queryTrial=request("result");queryTrial["query_id"]=usbTrial["request_id"];assert(unified(queryTrial)["outcome"]=="unknown");
    usbTrial["request_id"]="w3_0000000000000013";ctx.until=1005;assert(unified(usbTrial)["ok"]==true);
    int beforeClose=WiFi.begins;wifiSetupConstrain(1005,0,true);assert(!wifiSetupBusy()&&WiFi.begins==beforeClose);
    std::cout<<"Unified HTTP/BLE: USB no-permit, strict claim, stale session replay, pending/active atomicity, reboot unknown trial and no RF after deadline passed\n";

}
