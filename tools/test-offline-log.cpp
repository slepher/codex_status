// Persistence/retention logic over the production codec/store and existing FS shim.
#include "../src/offline_log.h"
#include "../src/v2_sync.h"
#include <LittleFS.h>
#include <cassert>
#include <memory>
#include <iostream>

static uint64_t utc=1791986400;
static bool online=false;
static void sink(const uint8_t g[16],const uint8_t *record,size_t size) {
    if(!online)offlineLogAppend(utc,g,record,size);
}
static JsonDocument status() { JsonDocument d;offlineLogStatus(utc,d.to<JsonObject>());return d; }
int main() {
    auto rtc=std::make_unique<SyncRtc>();uint8_t generation[16]={1};syncRecover(*rtc,generation);
    LittleFS.files.clear();LittleFS.dirs.clear();LittleFS.root.clear();LittleFS.capacity=512*1024;
    assert(offlineLogBegin(utc));syncSetRecordSink(sink);
    assert(status()["bytes"]==0);
    auto emit=[&](int i) {
        char text[97];snprintf(text,sizeof(text),"[net] witness %d",i);
        assert(syncAppend(*rtc,SYNC_TEXT,0,1,i,(const uint8_t *)text,strlen(text)));
    };
    emit(0);auto one=status();assert(one["bytes"].as<size_t>()==32+32+24+strlen("[net] witness 0"));
    auto hour=utc/3600;auto serial=one["files"][0]["serial"].as<uint64_t>();
    JsonDocument page;assert(offlineLogPage(utc,hour,0,page.to<JsonObject>(),serial)==200);
    auto next=page["next_offset"].as<uint32_t>();assert(page["records"].size()==1);
    online=true;emit(1);assert(status()["bytes"]==one["bytes"]);online=false;
    for(int i=2;i<600;++i)emit(i);
    assert(syncEarliestSeq(*rtc)>1);auto full=status();assert(full["bytes"].as<size_t>()<=16384);
    page.clear();assert(offlineLogPage(utc,hour,next,page.to<JsonObject>(),serial)==409); // rotated cursor
    std::string file;
    for(const auto &item:LittleFS.files)if(item.first.find(".bin")!=std::string::npos)file=item.first;
    assert(!file.empty());auto validBytes=LittleFS.files[file].size();
    LittleFS.files[file].insert(LittleFS.files[file].end(),{56,0,1}); // incomplete final append
    assert(offlineLogBegin(utc));assert(LittleFS.files[file].size()==validBytes);
    assert(status()["error"]=="torn_tail_recovered");emit(600);
    auto stable=LittleFS.files[file].size();
    LittleFS.writeBudget=10;emit(601);LittleFS.writeBudget=-1;
    assert(status()["error"]=="record_write_failed");emit(602); // next append repairs; no reboot required
    assert(LittleFS.files[file].size()>stable);assert(status()["error"]=="torn_tail_recovered");
    assert(offlineLogBegin(utc));
    for(int h=1;h<24;++h){utc+=3600;emit(700+h);}
    auto daily=status();assert(daily["files"].size()==24);assert(daily["bytes"].as<size_t>()<=OFFLINE_LOG_MAX_BYTES);
    utc+=3600;emit(900);auto aged=status();assert(aged["files"].size()==24);
    page.clear();assert(offlineLogPage(utc,hour,0,page.to<JsonObject>())==410);
    // Log space pressure evicts only log files, preserving unrelated content.
    LittleFS.files["/bundle/witness"]={1,2,3};LittleFS.capacity=LittleFS.usedBytes()+49152;
    emit(901);assert(LittleFS.files["/bundle/witness"]==std::vector<uint8_t>({1,2,3}));
    assert(status()["files"].size()<24);
    // A current segment plus no spare space is a visible failure, not deletion of configuration.
    LittleFS.capacity=LittleFS.usedBytes();emit(902);assert(status()["error"]=="storage_full" || status()["error"]=="cursor_write_failed");
    LittleFS.capacity=512*1024;utc=0;offlineLogExpire(utc);assert(status()["available"]==false);
    assert(LittleFS.files.count("/bundle/witness")==1);
    utc=1791986400;assert(offlineLogBegin(utc));assert(status()["files"].size()==0);
    // Unknown format is rejected and retained for diagnosis.
    char bad[24];snprintf(bad,sizeof(bad),"/offline/%u.bin",(unsigned)((utc/3600)%24));
    LittleFS.files[bad]=std::vector<uint8_t>(32,0);assert(offlineLogBegin(utc));emit(903);
    assert(status()["error"]=="format_invalid");assert(LittleFS.files[bad].size()==32);
    syncSetRecordSink(nullptr);
    // Older wake entries survived only in RTC while Flash was not mounted.
    // Recover the entire ring, not just the current wake's text, and dedupe.
    LittleFS.files.clear();LittleFS.dirs.clear();assert(offlineLogBegin(utc));
    memset(rtc.get(),0,sizeof(*rtc));generation[0]=2;syncRecover(*rtc,generation);
    for(unsigned wake:{1u,2u,3u}) {
        const uint8_t text[]="[net] retained wake";
        assert(syncAppend(*rtc,SYNC_TEXT,0,wake,100,text,sizeof(text)-1));
    }
    assert(status()["bytes"]==0);assert(offlineLogCapture(*rtc,utc));
    page.clear();assert(offlineLogPage(utc,utc/3600,0,page.to<JsonObject>())==200);
    assert(page["records"].size()==3);auto saved=status()["bytes"].as<size_t>();
    assert(offlineLogCapture(*rtc,utc));assert(status()["bytes"].as<size_t>()==saved);
    utc+=86400;offlineLogExpire(utc);assert(status()["bytes"]==0);
    assert(offlineLogCapture(*rtc,utc));assert(status()["bytes"]==0 && "expired RTC records resurrected");
    assert(offlineLogBegin(utc));assert(offlineLogCapture(*rtc,utc));assert(status()["bytes"]==0);
    std::cout<<"offline log: empty/single/typical/capacity, rotation cursor, torn write recovery, 24h expiry, space reserve, unknown time/version passed\n";
}
