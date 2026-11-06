// Numeric entry point, durable replay and retired envelope regression.
#define CODEX_WIFI_SETUP_HOST_TEST
#include "../src/wifi_setup.cpp"
#include <cassert>
#include <iostream>

int main() {
    WifiControlContext ctx;ctx.principal="bridge";ctx.nameSpace="02:00:00:00:00:02/1";
    ctx.claimed=true;ctx.sessionValid=true;ctx.until=60000;ctx.physicalUntil=60000;
    assert(wifiSchemesBegin());
    JsonDocument body;body["device_mac"]="020000000001";body["bridge_id"]="bridge";
    body["request_id"]="w3_not_a_number";body["expected_revision"]=0;body["request_seq"]=1;
    JsonObject row=body["changes"].to<JsonArray>().add<JsonObject>();
    row["network_id"]="home";row["host_ssid"]="Host";row["ssid"]="Device";row["credential"]="open";
    auto invoke=[&](uint16_t op){JsonDocument reply;assert(!deserializeJson(reply,wifiControlCommand(op,body,ctx,1000)));return reply;};
    const int starts=WiFi.begins;
    assert(invoke(15)["outcome"]=="committed" && WiFi.begins==starts);
    const auto saved=testNvsBlob;
    body["request_seq"]="1";assert(invoke(15)["error"]=="shape");
    body["request_seq"]=-1;assert(invoke(15)["error"]=="shape");
    body["request_seq"]=2;assert(invoke(15)["error"]=="request_conflict");body["request_seq"]=1;
    assert(wifiSchemesBegin());
    assert(invoke(15)["outcome"]=="committed" && testNvsBlob==saved);
    body["changes"][0]["ssid"]="Other";
    assert(invoke(15)["error"]=="request_conflict" && testNvsBlob==saved);
    body.remove("changes");body["query_id"]="w3_not_a_number";
    assert(invoke(16)["outcome"]=="committed");
    assert(invoke(17)["outcome"]=="retired");
    assert(wifiSchemesBegin());
    assert(invoke(16)["outcome"]=="retired");
    body["request_id"]="new-valid-write";body["expected_revision"]=1;
    row=body["changes"].to<JsonArray>().add<JsonObject>();
    row["network_id"]="travel";row["host_ssid"]="Host2";row["ssid"]="Device2";row["credential"]="open";
    const auto retiredBytes=testNvsBlob;
    body["request_seq"]=0;assert(invoke(15)["error"]=="shape");
    body.remove("request_seq");assert(invoke(15)["error"]=="shape");
    body["request_seq"]=1;assert(invoke(15)["error"]=="request_retired");
    assert(testNvsBlob==retiredBytes);body.remove("changes");
    assert(wifiManagementHighWater(ctx.principal,ctx.nameSpace)==1);
    assert(invoke(14)["schema"]==2);
    assert(invoke(28)["ok"]==true);
    assert(invoke(34)["ok"]==true);
    assert(invoke(65535)["error"]=="unsupported");
    body["op"]="snapshot";assert(invoke(14)["error"]=="retired_protocol");body.remove("op");
    body["bridge_id"]="forged";assert(invoke(15)["error"]=="principal_mismatch");
    // Import old WTX1 bytes without changing original IDs or the high-watermark.
    WifiStore old=wifiStore;old.receipts.resize(0);old.cursors.resize(0);
    WifiReceipt legacy;legacy.bridge.set("bridge");legacy.bond.set(ctx.nameSpace.c_str());
    legacy.request.set("w3_0000000000000005");legacy.revision=old.revision;
    assert(old.receipts.append(legacy));
    WifiCursor cursor;cursor.bridge.set("bridge");cursor.bond.set(ctx.nameSpace.c_str());cursor.high=5;
    assert(old.cursors.append(cursor));
    auto encodeOld=[&](WifiWriter &w){
        w.raw("WTX1",4);w.integer(1,1);w.integer(1,1);w.integer(0,1);w.integer(1,1);
        w.integer(0,4);w.integer(0,4);w.integer(old.executionGeneration,8);
        w.receipt(old.receipts[0],false);
        w.integer(10+strlen(cursor.bridge)+strlen(cursor.bond),2);w.integer(cursor.high,8);
        w.text(cursor.bridge);w.text(cursor.bond);
    };
    WifiWriter sizing{nullptr};encodeOld(sizing);WifiBlob oldBlob;assert(oldBlob.allocate(sizing.pos));
    WifiWriter writer{oldBlob.data.get()};encodeOld(writer);oldBlob.seal();
    WifiStore imported=wifiStore;imported.receipts.resize(0);imported.cursors.resize(0);
    assert(wifiDecodeTransactions(oldBlob,imported));
    assert(imported.receipts[0].sequence==5 && !strcmp(imported.receipts[0].request,legacy.request));
    // Install genuine WTX1 bytes under its committed selector, then interrupt
    // each transaction-only migration boundary. Restart must recover the same ID.
    TestNvsKeys keys;assert(testNvsKeys(keys));auto &marker=keys["commit"];
    keys[wifiTransactionKey(marker[5])]={oldBlob.data.get(),oldBlob.data.get()+oldBlob.size};
    wifiSetWord(marker.data(),16,oldBlob.size);wifiSetWord(marker.data(),20,v2Crc32(oldBlob.data.get(),oldBlob.size));
    wifiSetWord(marker.data(),24,0);wifiSetWord(marker.data(),24,v2Crc32(marker.data(),marker.size()));
    assert(testNvsPublish(keys));const auto original=testNvsBlob;
    for(int fault=0;fault<2;++fault){
        testNvsBlob=original;assert(wifiSchemesBegin() && wifiTransactionFormat==1);
        auto next=wifiStore;testNvsFailAfter=fault;assert(!saveV2(next));testNvsFailAfter=-1;
        assert(wifiSchemesBegin() && wifiTransactionFormat==1);
        assert(wifiStore.receipts[0].sequence==5 && !strcmp(wifiStore.receipts[0].request,legacy.request));
        assert(wifiManagementHighWater(ctx.principal,ctx.nameSpace)==5);
    }
    for(bool committedError:{false,true}){
        testNvsBlob=original;assert(wifiSchemesBegin());auto next=wifiStore;
        if(committedError)testNvsCommitThenFailKey="commit";
        else {testNvsWrites.clear();testNvsFailReadAfterWriteKey="commit";}
        assert(!saveV2(next));testNvsCommitThenFailKey.clear();testNvsFailReadAfterWriteKey.clear();
        assert(wifiSchemesBegin() && wifiTransactionFormat==2);
        assert(wifiStore.receipts[0].sequence==5 && !strcmp(wifiStore.receipts[0].request,legacy.request));
    }
    testNvsBlob=original;assert(wifiSchemesBegin());auto next=wifiStore;
    assert(saveV2(next));assert(wifiSchemesBegin() && wifiTransactionFormat==2);
    body["bridge_id"]="bridge";body["query_id"]=legacy.request.c_str();body["request_seq"]=5;
    assert(invoke(16)["outcome"]=="committed");assert(invoke(17)["outcome"]=="retired");
    assert(wifiSchemesBegin());assert(invoke(16)["outcome"]=="retired");
    assert(wifiManagementHighWater(ctx.principal,ctx.nameSpace)==5);
    // Unsequenced historical receipts retain their original ID and cannot be
    // acknowledged away; new writes cannot impersonate them with a zero sequence.
    auto retained=wifiStore;legacy.request.set("historical-opaque-id");legacy.sequence=0;
    assert(retained.receipts.append(legacy));assert(saveV2(retained));assert(wifiSchemesBegin());
    body["query_id"]=legacy.request.c_str();body["request_seq"]=0;
    assert(invoke(16)["outcome"]=="committed");assert(invoke(17)["error"]=="legacy_receipt_retained");
    assert(wifiSchemesBegin() && wifiStore.receipts.size()==1);
    oldBlob.data[4]=9;oldBlob.seal();WifiStore invalid=wifiStore;
    assert(!wifiDecodeTransactions(oldBlob,invalid));
    std::cout<<"PASS: WTX1 import, WTX2 verified commit, original-ID query/retirement, failed save preservation and version rejection\n";
    std::cout<<"PASS: numeric Wi-Fi dispatch, durable replay/conflict/retirement, read pages and envelope rejection\n";
}
