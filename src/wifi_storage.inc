// Explicit WFS4 wire storage; no compiler layout or compression in new writes.
#include "wifi_legacy.h"
static constexpr size_t WIFI_STORAGE_MAX = 8192;
struct WifiWriter {
    uint8_t *bytes; size_t pos = 0;
    void integer(uint64_t value, size_t width) { for (size_t i = 0; i < width; ++i) { if (bytes) bytes[pos] = value >> (8*i); ++pos; } }
    void raw(const void *data, size_t size) { if (bytes) memcpy(bytes+pos, data, size); pos += size; }
    void text(const char *value) { size_t size = strlen(value); integer(size,1); raw(value,size); }
    void record(const WifiNetwork &n) {
        size_t size = 8+6+strlen(n.bond)+strlen(n.id)+strlen(n.host)+strlen(n.ssid)+strlen(n.password)+strlen(n.label);
        integer(size,2); integer(n.version,8);
        for (const char *value : {n.bond.c_str(),n.id.c_str(),n.host.c_str(),n.ssid.c_str(),n.password.c_str(),n.label.c_str()}) text(value);
    }
    void receipt(const WifiReceipt &r) {
        integer(8+32+4+10+strlen(r.bridge)+strlen(r.request)+strlen(r.bond)+strlen(r.error)+strlen(r.operation)+strlen(r.target),2);
        integer(r.revision,8); raw(r.hash,32);
        for (const char *value : {r.bridge.c_str(),r.request.c_str(),r.bond.c_str(),r.error.c_str()}) text(value);
        integer(r.targetVersion,8);text(r.operation);text(r.target);
    }
    void store(const WifiStore &s) {
        raw("WFS4",4); integer(4,1); integer(s.cursors.size(),1); integer(s.schemes.networks.size(),1); integer(s.receipts.size(),1);
        integer(0,4); integer(0,4); integer(s.revision,8); integer(s.activeVersion,8);
        int slot = s.schemes.find(s.active.bond,s.active.id);
        bool reference = slot >= 0 && s.schemes.networks[slot].version == s.activeVersion;
        integer(!*s.active.id ? 0 : reference ? 1 : 2,1);
        if (*s.active.id) { text(s.active.bond); text(s.active.id); if (!reference) { text(s.active.ssid); text(s.active.password); } }
        for (const auto &n : s.schemes.networks) record(n);
        for (const auto &r : s.receipts) receipt(r);
        for (const auto &c : s.cursors) {
            integer(10+strlen(c.bridge)+strlen(c.bond),2); integer(c.high,8); text(c.bridge); text(c.bond);
        }
    }
};
struct WifiReader {
    const uint8_t *bytes; size_t size, pos = 0; bool okay = true;
    uint64_t integer(size_t width) {
        if (width > size-pos) { okay = false; return 0; }
        uint64_t value = 0; for (size_t i = 0; i < width; ++i) value |= uint64_t(bytes[pos++]) << (8*i); return value;
    }
    bool raw(void *out, size_t length) { if (length > size-pos) { okay = false; return false; } memcpy(out,bytes+pos,length); pos += length; return true; }
    bool text(WifiText &out, size_t limit) {
        size_t length = integer(1);
        if (!okay || length > limit || length > size-pos || memchr(bytes+pos,0,length)) { okay = false; return false; }
        char field[65]; memcpy(field,bytes+pos,length); field[length]=0; pos += length;
        okay = out.set(field); wipe(field,sizeof(field)); return okay;
    }
    bool record(WifiNetwork &n) {
        size_t length = integer(2), start = pos; n.version = integer(8);
        if (!okay || length > size-start) return false;
        return text(n.bond,21) && text(n.id,32) && text(n.host,32) && text(n.ssid,32) && text(n.password,64) && text(n.label,64) && pos-start == length;
    }
    bool receipt(WifiReceipt &r, bool current) {
        size_t length = integer(2), start = pos; r.revision = integer(8);
        if (!okay || length > size-start) return false;
        if (!(raw(r.hash,32) && text(r.bridge,64) && text(r.request,64) && text(r.bond,21) && text(r.error,32))) return false;
        if(current){r.targetVersion=integer(8);if(!text(r.operation,16)||!text(r.target,32))return false;}
        return pos-start==length;
    }
};
static bool wifiStoreShape(const WifiStore &s) {
    if (!s.valid() || s.schemes.networks.size()>12 || s.receipts.size()>8 || s.cursors.size()>8) return false;
    for (size_t i = 0; i < s.schemes.networks.size(); ++i) {
        const auto &n = s.schemes.networks[i];
        if (!*n.bond || strlen(n.bond)>21 || !identifier(n.id,32) || !ssidValid(n.host) || !ssidValid(n.ssid) || !passwordValid(n.password) || strlen(n.label)>64 || n.version>s.revision) return false;
        for (size_t j = 0; j < s.schemes.networks.size(); ++j) {
            const auto &other = s.schemes.networks[j];
            if (!strcmp(n.bond,other.bond)) {
                if (j!=i && (!strcmp(n.id,other.id) || !strcmp(n.host,other.host))) return false; }
        }
    }
    if (s.activeVersion>s.revision) return false;
    if (*s.active.id) {
        const auto *n = s.activeCredentials();
        if (!*s.active.bond || strlen(s.active.bond)>21 || !identifier(s.active.id,32) || !ssidValid(n->ssid) || !passwordValid(n->password)) return false;
    } else if (*s.active.bond || *s.active.ssid || *s.active.password || s.activeVersion) return false;
    for (size_t i=0; i<s.receipts.size(); ++i) {
        const auto &r = s.receipts[i];
        if (!identifier(r.bridge,64) || !identifier(r.request,64) || !*r.bond || strlen(r.bond)>21 || strlen(r.error)>32 || r.revision>s.revision || strlen(r.operation)>16 || strlen(r.target)>32) return false;
        for (size_t j=0;j<i;++j) if (!strcmp(r.bridge,s.receipts[j].bridge) && !strcmp(r.request,s.receipts[j].request)) return false;
    }
    for (size_t i=0;i<s.cursors.size();++i) {
        const auto &c=s.cursors[i];
        if (!identifier(c.bridge,64) || !*c.bond || strlen(c.bond)>21 || !c.high) return false;
        for (size_t j=0;j<i;++j) if (!strcmp(c.bridge,s.cursors[j].bridge) && !strcmp(c.bond,s.cursors[j].bond)) return false;
    }
    for (const auto &receipt:s.receipts) if (uint64_t seq=wifiRequestSequence(receipt.request)) {
        bool found=false;
        for (const auto &c:s.cursors) if (!strcmp(c.bridge,receipt.bridge) && !strcmp(c.bond,receipt.bond) && c.high>=seq) found=true;
        if (!found) return false;
    }
    return true;
}
static bool wifiDecode3(uint8_t *bytes, size_t size, WifiStore &out) {
    if (size<33 || size>WIFI_STORAGE_MAX || (memcmp(bytes,"WFS3",4) && memcmp(bytes,"WFS4",4))) return false;
    WifiReader r{bytes,size,4};
    size_t version=r.integer(1), cursors=r.integer(1);
    if ((version!=3 && version!=4) || (version==3 && cursors) || cursors>8 || memcmp(bytes,version==3?"WFS3":"WFS4",4)) return false;
    size_t networks=r.integer(1), receipts=r.integer(1);
    if (networks>12 || receipts>8 || r.integer(4)!=size) return false;
    uint32_t crc=r.integer(4); memset(bytes+12,0,4);
    bool intact=crc==v2Crc32(bytes,size); for(size_t i=0;i<4;++i) bytes[12+i]=crc>>(8*i);
    if (!intact) return false;
    out.revision=r.integer(8); out.activeVersion=r.integer(8); size_t mode=r.integer(1);
    if (mode>2) return false;
    if (mode && !(r.text(out.active.bond,21) && r.text(out.active.id,32))) return false;
    if (mode==2 && !(r.text(out.active.ssid,32) && r.text(out.active.password,64))) return false;
    if (!out.schemes.networks.resize(networks) || !out.receipts.resize(receipts)) return false;
    for(auto &n:out.schemes.networks) if(!r.record(n)) return false;
    for(auto &receipt:out.receipts) if(!r.receipt(receipt,version==4)) return false;
    if (!out.cursors.resize(cursors)) return false;
    for (auto &c:out.cursors) {
        size_t length=r.integer(2), start=r.pos; c.high=r.integer(8);
        if (!r.okay || length>size-start || !r.text(c.bridge,64) || !r.text(c.bond,21) || r.pos-start!=length) return false;
    }
    int active=out.schemes.find(out.active.bond,out.active.id);
    if (mode==1 && (active<0 || out.schemes.networks[active].version!=out.activeVersion)) return false;
    if (mode==2 && active>=0 && out.schemes.networks[active].version==out.activeVersion) return false;
    out.schemes.selectedBond=out.active.bond; out.schemes.selectedId=out.active.id;
    return r.okay && r.pos==size && wifiStoreShape(out);
}
static bool wifiConvertLegacy(const WifiLegacyStoreV2 &old, WifiStore &out) {
    out.revision=old.revision; out.activeVersion=old.activeVersion;
    for(size_t i=0;i<12;++i) if(*old.schemes.networks[i].id) {
        const auto &o=old.schemes.networks[i]; WifiNetwork n;
        n.bond.set(o.bond); n.id.set(o.id); n.host.set(o.host); n.ssid.set(o.ssid); n.password.set(o.password); n.label.set(old.labels[i]); n.version=old.versions[i];
        if(!out.schemes.networks.append(n))return false;
    }
    out.active.bond.set(old.active.bond); out.active.id.set(old.active.id);
    int active=out.schemes.find(out.active.bond,out.active.id);
    if(active<0 || out.schemes.networks[active].version!=out.activeVersion) { out.active.ssid.set(old.active.ssid); out.active.password.set(old.active.password); }
    out.schemes.selectedBond=out.active.bond; out.schemes.selectedId=out.active.id;
    for(const auto &o:old.receipts) if(*o.request) {
        WifiReceipt receipt; receipt.bridge.set(o.bridge); receipt.request.set(o.request); receipt.bond.set(o.bond); receipt.error.set(o.error);
        memcpy(receipt.hash,o.hash,32); receipt.revision=o.revision; if(!out.receipts.append(receipt)) return false;
    }
    return wifiStoreShape(out);
}
static __attribute__((noinline)) bool readWifiStore(Preferences &prefs, WifiStore &out) {
    bool current=prefs.getBytesLength("store3"); const char *key=current?"store3":"scheme";
    size_t size=prefs.getBytesLength(key);
    if (!size || size>(current?WIFI_STORAGE_MAX:sizeof(WifiLegacyStoreV2)+64)) return false;
    std::unique_ptr<uint8_t[]> bytes(new(std::nothrow) uint8_t[size]); if(!bytes)return false;
    bool okay=prefs.getBytes(key,bytes.get(),size)==size;
    if(current) okay=okay && wifiDecode3(bytes.get(),size,out);
    else if(okay) {
        // Compatibility allocation exists only while loading a validated old blob.
        std::unique_ptr<WifiLegacyStoreV2> old(new(std::nothrow) WifiLegacyStoreV2);
        okay=bool(old);
        if(okay && size>4 && !memcmp(bytes.get(),"WFZ2",4)) okay=wifiInflate(old.get(),sizeof(*old),bytes.get()+4,size-4)==sizeof(*old) && old->valid();
        else if(okay && size==sizeof(*old)) { memcpy(old.get(),bytes.get(),size); okay=old->valid(); }
        else if(okay && size==sizeof(WifiLegacySchemes)) {
            memcpy(&old->schemes,bytes.get(),size); okay=old->schemes.valid();
            int active=old->schemes.find(old->schemes.selectedBond,old->schemes.selectedId);
            if(okay && active>=0) old->active=old->schemes.networks[active];
        } else okay=false;
        if(okay) okay=wifiConvertLegacy(*old,out);
        if(old) wipe((char *)old.get(),sizeof(*old));
    }
    if(okay && current)wifiStorageFormat=bytes[4];
    wipe((char *)bytes.get(),size); return okay;
}
static bool saveV2(WifiStore &next) {
    int active=next.schemes.find(next.active.bond,next.active.id);
    if(active>=0 && next.schemes.networks[active].version==next.activeVersion) { next.active.ssid.clear(); next.active.password.clear(); }
    if (!wifiStoreShape(next)) { wifiStorageFailure="storage_shape"; return false; }
    WifiWriter sizing{nullptr}; sizing.store(next); wifiStorageBytes=sizing.pos;
    if(sizing.pos>WIFI_STORAGE_MAX) { wifiStorageFailure="storage_capacity"; return false; }
    std::unique_ptr<uint8_t[]> bytes(new(std::nothrow) uint8_t[sizing.pos]);
    if(!bytes) { wifiStorageFailure="memory"; return false; }
    WifiWriter writer{bytes.get()}; writer.store(next);
    for(size_t i=0;i<4;++i)bytes[8+i]=writer.pos>>(8*i);
    uint32_t crc=v2Crc32(bytes.get(),writer.pos); for(size_t i=0;i<4;++i)bytes[12+i]=crc>>(8*i);
    Preferences prefs; bool okay=prefs.begin("wifi-bonds",false); wifiStorageFailure="storage_open";
    if(okay) {
        wifiStorageFree=prefs.freeEntries();
        // NVS can commit the new index then fail retiring old chunks. A zero
        // write result is also uncertain: reload before any next transaction.
        wifiStorageUncertain=true;
        okay=prefs.putBytes("store3",bytes.get(),writer.pos)==writer.pos; wifiStorageFailure="storage_write";
        if(okay) {
            std::unique_ptr<uint8_t[]> check(new(std::nothrow) uint8_t[writer.pos]);
            wifiStorageFailure="storage_readback";
            okay=check && prefs.getBytesLength("store3")==writer.pos && prefs.getBytes("store3",check.get(),writer.pos)==writer.pos && !memcmp(check.get(),bytes.get(),writer.pos);
            if(check) wipe((char *)check.get(),writer.pos);
        }
        if(okay && prefs.getBytesLength("scheme")) prefs.remove("scheme");
        wifiOldKeyPresent = prefs.getBytesLength("scheme") != 0;
        prefs.end();
    }
    if(okay) { wifiStorageUncertain=false; wifiVerifiedStorageBytes=writer.pos;wifiStorageFormat=4; }
    if(okay && &next!=&wifiStore) wifiStore=std::move(next);
    wipe((char *)bytes.get(),writer.pos); return okay;
}
