// WFS4 configuration and WTX1 transaction records have independent persistence.
// The WFS5 codec remains an offline archive reader/writer, never a device store.
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
        integer(8+32+4+10+8+strlen(r.bridge)+strlen(r.request)+strlen(r.bond)+strlen(r.error)+strlen(r.operation)+strlen(r.target),2);
        integer(r.revision,8); raw(r.hash,32);
        for (const char *value : {r.bridge.c_str(),r.request.c_str(),r.bond.c_str(),r.error.c_str()}) text(value);
        integer(r.targetVersion,8);text(r.operation);text(r.target);
        integer(r.executionGeneration,8);
    }
    void configuration(const WifiStore &s) {
        raw("WFS4",4); integer(4,1); integer(0,1); integer(s.schemes.networks.size(),1); integer(0,1);
        integer(0,4); integer(0,4); integer(s.revision,8); integer(s.activeVersion,8);
        int slot=s.schemes.find(s.active.bond,s.active.id);
        bool reference=slot>=0 && s.schemes.networks[slot].version==s.activeVersion;
        integer(!*s.active.id?0:reference?1:2,1);
        if(*s.active.id) { text(s.active.bond);text(s.active.id);if(!reference){text(s.active.ssid);text(s.active.password);} }
        for(const auto &n:s.schemes.networks)record(n);
    }
    void transactions(const WifiStore &s) {
        raw("WTX1",4);integer(1,1);integer(s.cursors.size(),1);integer(0,1);integer(s.receipts.size(),1);
        integer(0,4);integer(0,4);integer(s.executionGeneration,8);
        for(const auto &r:s.receipts)receipt(r);
        for(const auto &c:s.cursors){integer(10+strlen(c.bridge)+strlen(c.bond),2);integer(c.high,8);text(c.bridge);text(c.bond);}
    }
    void store(const WifiStore &s) {
        raw("WFS5",4); integer(5,1); integer(s.cursors.size(),1); integer(s.schemes.networks.size(),1); integer(s.receipts.size(),1);
        integer(0,4); integer(0,4); integer(s.revision,8); integer(s.activeVersion,8);
        integer(s.executionGeneration,8);
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
    bool receipt(WifiReceipt &r, bool generation=true) {
        size_t length = integer(2), start = pos; r.revision = integer(8);
        if (!okay || length > size-start) return false;
        if (!(raw(r.hash,32) && text(r.bridge,64) && text(r.request,64) && text(r.bond,21) && text(r.error,32))) return false;
        r.targetVersion=integer(8);if(!text(r.operation,16)||!text(r.target,32))return false;
        r.executionGeneration=generation?integer(8):0;
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
        if (r.executionGeneration > s.executionGeneration || (r.executionGeneration &&
            strcmp(r.operation,"trial") && strcmp(r.operation,"activate"))) return false;
        for (size_t j=0;j<i;++j) if (r.matches(s.receipts[j].bridge,s.receipts[j].bond,s.receipts[j].request)) return false;
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
static bool wifiDecodeStore(uint8_t *bytes, size_t size, WifiStore &out, bool generations) {
    if (size<(generations?41u:33u) || size>WIFI_STORAGE_MAX || memcmp(bytes,generations?"WFS5":"WFS4",4)) return false;
    WifiReader r{bytes,size,4};
    size_t version=r.integer(1), cursors=r.integer(1);
    if (version!=(generations?5u:4u) || cursors>8) return false;
    size_t networks=r.integer(1), receipts=r.integer(1);
    if (networks>12 || receipts>8 || r.integer(4)!=size) return false;
    uint32_t crc=r.integer(4); memset(bytes+12,0,4);
    bool intact=crc==v2Crc32(bytes,size); for(size_t i=0;i<4;++i) bytes[12+i]=crc>>(8*i);
    if (!intact) return false;
    out.revision=r.integer(8); out.activeVersion=r.integer(8);
    out.executionGeneration=generations?r.integer(8):0;
    size_t mode=r.integer(1);
    if (mode>2) return false;
    if (mode && !(r.text(out.active.bond,21) && r.text(out.active.id,32))) return false;
    if (mode==2 && !(r.text(out.active.ssid,32) && r.text(out.active.password,64))) return false;
    if (!out.schemes.networks.resize(networks) || !out.receipts.resize(receipts)) return false;
    for(auto &n:out.schemes.networks) if(!r.record(n)) return false;
    for(auto &receipt:out.receipts) if(!r.receipt(receipt,generations)) return false;
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
// Offline archive compatibility; runtime imports only deployed WFS4.
static bool wifiDecode5(uint8_t *bytes,size_t size,WifiStore &out){return wifiDecodeStore(bytes,size,out,true);}
static bool wifiDecode4(uint8_t *bytes,size_t size,WifiStore &out){return wifiDecodeStore(bytes,size,out,false);}
#include "wifi_storage_commit.inc"
