#pragma once
#include <ArduinoJson.h>
#include <cstring>
#include <utility>

// Per-wake intent only. No credentials, repository copies, or new NVS record.
constexpr size_t PLAN_NETWORK_MAX = 384;
inline bool planNetworkText(JsonVariantConst v, size_t limit) {
    if (!v.is<const char *>()) return false;
    JsonString s = v.as<JsonString>();
    return s.size() && s.size() <= limit && strlen(s.c_str()) == s.size();
}
inline bool planNetworkHex(JsonVariantConst v) {
    if (!planNetworkText(v, 64) || v.as<JsonString>().size() != 64) return false;
    for (const char *p=v.as<const char *>(); *p; ++p)
        if (!(*p>='0' && *p<='9') && !(*p>='a' && *p<='f')) return false;
    return true;
}
inline bool planNetworkAddress(const char *s) {
    unsigned parts[4] = {};
    for (unsigned i=0; i<4; ++i) {
        const bool leadingZero=*s=='0';
        unsigned digits=0;
        while (*s>='0' && *s<='9') {
            if (++digits>3) return false;
            parts[i]=parts[i]*10+(*s++-'0');
        }
        if (!digits || (leadingZero && digits>1) || parts[i]>255 || (i<3 ? *s++!='.' : *s!=0)) return false;
    }
    return parts[0] && parts[0]!=127 && parts[0]<224;
}

class PlanActivation {
public:
    const char *prepare(uint64_t id, JsonVariantConst network, String &encoded) const {
        JsonVariantConst t=network["target"];
        if (!id || !network.is<JsonObjectConst>() || network.size()!=4 || !network["v"].is<uint8_t>() || network["v"]!=1 ||
            !planNetworkText(network["bridge_addr"],15) || !planNetworkAddress(network["bridge_addr"]) ||
            !network["bridge_port"].is<uint16_t>() || network["bridge_port"]==0 ||
            !t.is<JsonObjectConst>() || t.size()!=(t["credential_key"].isNull()?3u:4u) ||
            !planNetworkText(t["network_id"],32) || !t["open"].is<bool>() ||
            !planNetworkHex(t["metadata_key"]) || (!t["credential_key"].isNull() && !planNetworkHex(t["credential_key"])))
            return "plan_network_shape";
        JsonDocument canonical;
        canonical["v"]=1; canonical["bridge_addr"]=network["bridge_addr"];
        canonical["bridge_port"]=network["bridge_port"];
        canonical["target"]["network_id"]=t["network_id"];
        canonical["target"]["open"]=t["open"];
        canonical["target"]["metadata_key"]=t["metadata_key"];
        if (!t["credential_key"].isNull()) canonical["target"]["credential_key"]=t["credential_key"];
        const size_t length=measureJson(canonical);
        if (canonical.overflowed() || length>PLAN_NETWORK_MAX || !encoded.reserve(length)) return "memory";
        if (serializeJson(canonical,encoded)!=length) return "memory";
        if (planId_==id && encoded!=request_) return "plan_network_conflict";
        return nullptr;
    }
    void commit(uint64_t id, String &&encoded) { planId_=id; request_=std::move(encoded); }
    bool active() const { return planId_!=0; }
    uint64_t planId() const { return planId_; }
    const String &request() const { return request_; }
private:
    uint64_t planId_=0;
    String request_;
};
