#pragma once
#include <ArduinoJson.h>
#include "v2_sync.h"

// The retained production record, also used by Fake's diagnostic HAL.
#pragma pack(push, 1)
struct WakeTraceRec {
    uint32_t wake_generation, seq;
    uint8_t wake_type, wake_cause, ext1, valid_start, valid_end, furthest, result, error;
    uint16_t request_crc;
    uint32_t awake_ms;
    uint16_t timing[9];
    uint16_t stage_mask;
    uint8_t transport, reserved;
    uint32_t crc;
};
#pragma pack(pop)
static_assert(sizeof(WakeTraceRec)==48,"wake trace record must stay 48 bytes");
inline uint32_t wakeTraceCrc(const WakeTraceRec &r) {
    const auto *p=(const uint8_t *)&r;uint32_t h=2166136261u;
    for(size_t i=0;i<offsetof(WakeTraceRec,crc);++i){h^=p[i];h*=16777619u;}return h;
}
inline bool wakeTraceValid(const WakeTraceRec &r){return r.valid_start==0xa5&&r.valid_end==0x5a&&r.crc==wakeTraceCrc(r);}
inline void wakeTraceCompactRow(JsonArray list,const WakeTraceRec &r,bool complete) {
    static const char *types[]={"boot","timer","button","thin","other"};
    static const char *results[]={"active","thin","answered","no_connection","no_command","wifi_no_ip","http_failed","interrupted"};
    auto item=list.add<JsonObject>();item["format"]=2;item["wake_generation"]=r.wake_generation;item["seq"]=r.seq;
    item["wake_type"]=types[r.wake_type<4?r.wake_type:4];item["awake_ms"]=r.awake_ms;
    item["result"]=results[r.result<8?r.result:0];item["complete"]=complete;item["stage"]=r.furthest;
    auto timing=item["timing"].to<JsonArray>();for(auto t:r.timing)timing.add((uint32_t)t*100u);
    item["transport"]=(r.transport&3)==3?"ble+wifi":(r.transport&1)?"ble":(r.transport&2)?"wifi":"none";
}
