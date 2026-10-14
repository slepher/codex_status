#include "offline_log.h"
#include "v2_sync.h"
#include <LittleFS.h>
#include <algorithm>
#include <mutex>
#include <string.h>

namespace {
constexpr size_t HEADER = 32, RECORD_MAX = 152, FILE_MAX = 16384;
constexpr size_t RESERVE = 49152; // Existing Bundle/sync reserve, never spent here.
constexpr uint32_t MAGIC = 0x314c464f; // OFL1
constexpr uint64_t MIN_UTC = 1600000001, MAX_UTC = 4102444799;
std::mutex mutex;
bool mounted = false;
uint64_t checkedHour = UINT64_MAX, lastSeq = 0, lastUtc = 0;
uint8_t lastGeneration[16] = {};
uint64_t nextSerial = 0;
uint64_t damagedHour = UINT64_MAX;
const char *error = nullptr;

void le(uint8_t *p, uint64_t value, size_t n) {
    for (size_t i=0;i<n;++i) p[i]=(uint8_t)(value>>(8*i));
}
uint64_t number(const uint8_t *p, size_t n) {
    uint64_t value=0;for(size_t i=0;i<n;++i)value|=(uint64_t)p[i]<<(8*i);return value;
}
bool known(uint64_t utc) { return utc >= MIN_UTC && utc <= MAX_UTC; }
void path(unsigned slot, char out[24]) { snprintf(out,24,"/offline/%u.bin",slot); }
bool header(File &f, uint64_t &hour, uint64_t *serial = nullptr) {
    uint8_t b[HEADER];
    if(f.size()<HEADER || f.size()>FILE_MAX || !f.seek(0) || f.read(b,HEADER)!=HEADER ||
       number(b,4)!=MAGIC || number(b+4,2)!=1 || number(b+6,2)!=HEADER ||
       !number(b+16,8) || number(b+24,4)!=FILE_MAX || number(b+28,4)!=syncCrc32(b,28)) return false;
    if(serial)*serial=number(b+16,8);
    hour=number(b+8,8);return hour>=MIN_UTC/3600 && hour<=MAX_UTC/3600;
}
bool rawValid(const uint8_t *b,size_t n) {
    return n>=24 && n<=120 && number(b,2)==n && b[2]>=1 && b[2]<=4 && !(b[3]&~3u) &&
        number(b+4,8) && number(b+n-4,4)==syncCrc32(b,n-4);
}
// A small atomic watermark prevents expired/rotated records still retained in
// RTC from being re-imported with a new save time. It contains no log payload.
bool readFloor(uint8_t b[44]) {
    File f=LittleFS.open("/offline/cursor","r");
    bool ok=f && f.size()==44 && f.read(b,44)==44;f.close();
    return ok && number(b,4)==0x3143464fu && number(b+4,2)==1 && number(b+6,2)==44 &&
        number(b+40,4)==syncCrc32(b,40);
}
bool saveFloor() {
    if(!lastSeq)return true;
    uint8_t b[44]={},check[44];
    if(readFloor(check) && !memcmp(check+8,lastGeneration,16) && number(check+24,8)>=lastSeq)return true;
    le(b,0x3143464fu,4);le(b+4,1,2);le(b+6,44,2);memcpy(b+8,lastGeneration,16);
    le(b+24,lastSeq,8);le(b+32,lastUtc,8);le(b+40,syncCrc32(b,40),4);
    File f=LittleFS.open("/offline/cursor.tmp","w");bool ok=f && f.write(b,44)==44;f.close();
    File verified=LittleFS.open("/offline/cursor.tmp","r");
    ok=ok && verified && verified.size()==44 && verified.read(check,44)==44 && !memcmp(check,b,44);verified.close();
    if(!ok || !LittleFS.rename("/offline/cursor.tmp","/offline/cursor")){error="cursor_write_failed";return false;}return true;
}
bool readRecord(File &f,uint32_t offset,uint8_t b[RECORD_MAX],size_t &n,uint64_t hour) {
    if(offset>f.size() || f.size()-offset<2 || !f.seek(offset) || f.read(b,2)!=2)return false;
    n=number(b,2);
    if(n<56 || n>RECORD_MAX || n>f.size()-offset || f.read(b+2,n-2)!=n-2)return false;
    return b[2]==1 && b[3]==0 && number(b+4,8)/3600==hour &&
        rawValid(b+28,n-32) && number(b+n-4,4)==syncCrc32(b,n-4);
}
// Validate a prefix with one bounded record; never allocate a file/ring copy.
uint32_t prefix(File &f,uint64_t hour,bool recoverCursor) {
    uint32_t offset=HEADER;uint8_t b[RECORD_MAX];size_t n;
    while(offset<f.size() && readRecord(f,offset,b,n,hour)) {
        const uint64_t utc=number(b+4,8),seq=number(b+32,8);
        if(recoverCursor && ((!memcmp(lastGeneration,b+12,16) && seq>lastSeq) || utc>=lastUtc)) {
            lastUtc=utc;lastSeq=seq;memcpy(lastGeneration,b+12,16);
        }
        offset+=(uint32_t)n;
    }
    return offset;
}
bool repair(const char *name,uint32_t bytes) {
    File source=LittleFS.open(name,"r"),out=LittleFS.open("/offline/pending","w");
    if(!source || !out)return false;
    uint8_t b[256];bool ok=true;
    for(uint32_t offset=0;offset<bytes;) {
        size_t n=std::min((size_t)(bytes-offset),sizeof(b));
        if(source.read(b,n)!=n || out.write(b,n)!=n){ok=false;break;}offset+=n;
    }
    source.close();out.close();
    File verified=LittleFS.open("/offline/pending","r");uint64_t hour=0;
    ok=ok && verified && verified.size()==bytes && header(verified,hour) && prefix(verified,hour,false)==bytes;
    verified.close();
    if(ok)ok=LittleFS.rename("/offline/pending",name);
    if(!ok)LittleFS.remove("/offline/pending");return ok;
}
bool prune(uint64_t utc) {
    if(!mounted)return false;
    const uint64_t now=known(utc)?utc/3600:UINT64_MAX;
    if(now==checkedHour && known(utc))return true;
    for(unsigned i=0;i<24;++i) {
        char name[24];path(i,name);if(!LittleFS.exists(name))continue;
        File f=LittleFS.open(name,"r");uint64_t hour=0;bool valid=f && header(f,hour);f.close();
        if(!known(utc) || (valid && (hour>now || now-hour>=24))) {
            if(!saveFloor())return false;
            if(!LittleFS.remove(name)){error="expiry_remove_failed";return false;}
        }
    }
    if(!known(utc)){error="time_unknown";return false;}checkedHour=now;return true;
}
bool room(size_t required,uint64_t currentHour) {
    while(LittleFS.totalBytes()<LittleFS.usedBytes()+RESERVE+required) {
        uint64_t oldest=UINT64_MAX;int selected=-1;
        for(unsigned i=0;i<24;++i) {
            char name[24];path(i,name);File f=LittleFS.open(name,"r");uint64_t h=0;
            if(f && header(f,h) && h!=currentHour && h<oldest){oldest=h;selected=(int)i;}f.close();
        }
        if(selected<0){error="storage_full";return false;}
        if(!saveFloor())return false;
        char name[24];path((unsigned)selected,name);
        if(!LittleFS.remove(name)){error="eviction_failed";return false;}
    }
    return true;
}
bool recoverTail() {
    if(damagedHour==UINT64_MAX)return true;
    char name[24];path((unsigned)(damagedHour%24),name);
    File f=LittleFS.open(name,"r");uint64_t hour=0;
    if(!f){damagedHour=UINT64_MAX;return true;}
    if(!header(f,hour) || hour!=damagedHour){f.close();error="recovery_failed";return false;}
    uint32_t valid=prefix(f,hour,false);f.close();
    if(!room(valid,hour) || !repair(name,valid)){error="recovery_failed";return false;}
    damagedHour=UINT64_MAX;error="torn_tail_recovered";return true;
}
}

bool offlineLogBegin(uint64_t utc) {
    std::lock_guard<std::mutex> guard(mutex);
    mounted=false;checkedHour=UINT64_MAX;lastSeq=lastUtc=0;memset(lastGeneration,0,16);error=nullptr;
    nextSerial=0;
    damagedHour=UINT64_MAX;
    if(!LittleFS.exists("/offline") && !LittleFS.mkdir("/offline")){error="storage_unavailable";return false;}
    uint8_t floor[44];
    if(LittleFS.exists("/offline/cursor")) {
        if(!readFloor(floor)){error="cursor_invalid";return false;}
        memcpy(lastGeneration,floor+8,16);lastSeq=number(floor+24,8);lastUtc=number(floor+32,8);
    }
    // Recover generation before expiry so a rotated file never reuses an old
    // client's cursor during this mount, including backward clock correction.
    for(unsigned i=0;i<24;++i) {
        char name[24];path(i,name);File f=LittleFS.open(name,"r");uint64_t hour=0,serial=0;
        if(f && header(f,hour,&serial)){nextSerial=std::max(nextSerial,serial);prefix(f,hour,true);}f.close();
    }
    mounted=true;if(!prune(utc))return false;
    for(unsigned i=0;i<24;++i) {
        char name[24];path(i,name);if(!LittleFS.exists(name))continue;
        File f=LittleFS.open(name,"r");uint64_t hour=0;
        if(!f || !header(f,hour)){f.close();error="format_invalid";continue;}
        uint32_t valid=prefix(f,hour,true);bool torn=valid<f.size();f.close();
        if(torn) {
            if(!room(valid,hour) || !repair(name,valid)){error="recovery_failed";return false;}
            error="torn_tail_recovered";
        }
    }
    return true;
}

bool offlineLogAppend(uint64_t utc,const uint8_t generation[16],const uint8_t *record,size_t length) {
    std::lock_guard<std::mutex> guard(mutex);
    if(!generation || !record || !rawValid(record,length)){error="record_invalid";return false;}
    if(!prune(utc))return false;
    if(!recoverTail())return false;
    uint64_t seq=number(record+4,8);
    if(!memcmp(generation,lastGeneration,16) && seq<=lastSeq)return true;
    uint64_t hour=utc/3600;char name[24];path((unsigned)(hour%24),name);
    File existing=LittleFS.open(name,"r");uint64_t stored=0;
    const bool present=LittleFS.exists(name);
    bool valid=existing && header(existing,stored);size_t bytes=existing?existing.size():0;existing.close();
    if(present && !valid){error="format_invalid";return false;}
    const size_t n=32+length;
    // Bounded hourly volume: rotate the oldest content in this hour when full.
    if(!valid || stored!=hour || bytes+n>FILE_MAX) {
        if(valid && !saveFloor())return false;
        if(!room(HEADER,hour))return false;
        uint8_t h[HEADER]={};le(h,MAGIC,4);le(h+4,1,2);le(h+6,HEADER,2);le(h+8,hour,8);
        if(nextSerial==UINT64_MAX){error="serial_exhausted";return false;}
        le(h+16,++nextSerial,8);le(h+24,FILE_MAX,4);le(h+28,syncCrc32(h,28),4);
        File f=LittleFS.open("/offline/pending","w");
        bool ok=f && f.write(h,HEADER)==HEADER;f.close();
        File check=LittleFS.open("/offline/pending","r");uint64_t verified=0;
        ok=ok && check && header(check,verified) && verified==hour;check.close();
        if(!ok || !LittleFS.rename("/offline/pending",name)){error="header_write_failed";return false;}
        bytes=HEADER;
    }
    if(!room(n,hour))return false;
    uint8_t b[RECORD_MAX]={};le(b,n,2);b[2]=1;le(b+4,utc,8);memcpy(b+12,generation,16);
    memcpy(b+28,record,length);le(b+n-4,syncCrc32(b,n-4),4);
    File f=LittleFS.open(name,"a");bool ok=f && f.write(b,n)==n;f.close();
    File check=LittleFS.open(name,"r");uint8_t verified[RECORD_MAX];size_t count=0;
    ok=ok && check && readRecord(check,(uint32_t)bytes,verified,count,hour) && count==n && !memcmp(b,verified,n);check.close();
    if(!ok){damagedHour=hour;error="record_write_failed";return false;}
    lastSeq=seq;lastUtc=utc;memcpy(lastGeneration,generation,16);return true;
}

void offlineLogExpire(uint64_t utc) { std::lock_guard<std::mutex> guard(mutex);prune(utc); }

bool offlineLogCapture(const SyncRtc &rtc,uint64_t savedUtc) {
    // Consult the durable watermark for this generation even when old and new
    // generations share a save timestamp (paused clock / cold recovery).
    uint8_t floor[44];
    if(readFloor(floor) && !memcmp(floor+8,rtc.generation,16) &&
       (memcmp(lastGeneration,rtc.generation,16) || number(floor+24,8)>lastSeq)) {
        memcpy(lastGeneration,rtc.generation,16);lastSeq=number(floor+24,8);
    }
    if(memcmp(lastGeneration,rtc.generation,16)) {
        memcpy(lastGeneration,rtc.generation,16);lastSeq=0;
        for(unsigned slot=0;slot<24;++slot) {
            char name[24];path(slot,name);File f=LittleFS.open(name,"r");uint64_t hour=0;
            if(!f || !header(f,hour)){f.close();continue;}
            uint8_t b[RECORD_MAX];size_t n;
            for(uint32_t offset=HEADER;offset<f.size() && readRecord(f,offset,b,n,hour);offset+=(uint32_t)n)
                if(!memcmp(b+12,rtc.generation,16))lastSeq=std::max(lastSeq,number(b+32,8));
            f.close();
        }
    }
    // Thin clock wakes don't mount Flash. The next full wake recovers retained
    // records before registration of the live sink. Never rewrite their bytes.
    for(uint16_t offset=0,next=0;offset<rtc.used;offset=next) {
        SyncRecord record;if(!syncReadAt(rtc,offset,record,next))return false;
        uint8_t raw[120];const uint16_t start=(rtc.head+4096-rtc.used+offset)%4096;
        const size_t length=next-offset;
        for(size_t i=0;i<length;++i)raw[i]=rtc.bytes[(start+i)%4096];
        if(!offlineLogAppend(savedUtc,rtc.generation,raw,length))return false;
    }
    return true;
}

void offlineLogStatus(uint64_t utc,JsonObject out) {
    std::lock_guard<std::mutex> guard(mutex);bool available=prune(utc);
    out["format"]="offline-log-1";out["available"]=available;out["retention_s"]=86400;
    out["max_bytes"]=OFFLINE_LOG_MAX_BYTES;out["error"]=error;
    size_t total=0;JsonArray files=out["files"].to<JsonArray>();
    if(available)for(unsigned i=0;i<24;++i) {
        char name[24];path(i,name);File f=LittleFS.open(name,"r");uint64_t hour=0,serial=0;
        if(f && header(f,hour,&serial)){JsonObject item=files.add<JsonObject>();item["hour"]=hour;item["serial"]=serial;item["bytes"]=f.size();total+=f.size();}f.close();
    }
    out["bytes"]=total;
}

int offlineLogPage(uint64_t utc,uint64_t hour,uint32_t offset,JsonObject out,uint64_t serial) {
    std::lock_guard<std::mutex> guard(mutex);
    auto fail=[&](int code,const char *why){out["result"]="rejected";out["error"]=why;out["stage"]="offline_log_read";return code;};
    if(!prune(utc))return fail(503,error?error:"storage_unavailable");
    if(!hour || hour>utc/3600 || utc/3600-hour>=24)return fail(410,"expired");
    char name[24];path((unsigned)(hour%24),name);File f=LittleFS.open(name,"r");uint64_t stored=0,currentSerial=0;
    if(!f)return fail(404,"not_found");
    if(!header(f,stored,&currentSerial))return fail(422,"format_invalid");
    if(stored!=hour || (serial && serial!=currentSerial))return fail(409,"archive_changed");
    if(offset && !serial)return fail(400,"serial_required");
    if(!offset)offset=HEADER;
    if(offset<HEADER || offset>f.size())return fail(400,"offset_invalid");
    // Check that a client cursor lies on a record boundary, not inside payload.
    uint32_t boundary=HEADER;uint8_t b[RECORD_MAX];size_t n;
    while(boundary<offset) {
        if(!readRecord(f,boundary,b,n,hour))return fail(422,"record_invalid");boundary+=(uint32_t)n;
    }
    if(boundary!=offset)return fail(400,"offset_invalid");
    JsonArray rows=out["records"].to<JsonArray>();
    for(unsigned i=0;i<6 && offset<f.size();++i) {
        if(!readRecord(f,offset,b,n,hour))return fail(422,"record_invalid");
        JsonObject row=rows.add<JsonObject>();row["utc"]=number(b+4,8);
        row["time_basis"]="saved";
        char generation[33],encoded[161],seq[24];syncHex(b+12,16,generation);
        snprintf(seq,sizeof(seq),"%llu",(unsigned long long)number(b+32,8));
        syncBase64(b+28,n-32,encoded,sizeof(encoded));
        row["generation"]=generation;row["seq"]=seq;row["record_b64"]=encoded;
        offset+=(uint32_t)n;
    }
    out["result"]="ok";out["hour"]=hour;out["serial"]=currentSerial;out["next_offset"]=offset;out["more"]=offset<f.size();return 200;
}
