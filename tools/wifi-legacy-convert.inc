// Retired explicit formats are decoded only by the offline converter.
static bool wifiDecodeExplicitOffline(uint8_t *bytes, size_t size, WifiStore &out) {
    if(size>=4 && !memcmp(bytes,"WFS5",4))return wifiDecode5(bytes,size,out);
    if (size<33 || size>WIFI_STORAGE_MAX || (memcmp(bytes,"WFS3",4) && memcmp(bytes,"WFS4",4))) return false;
    WifiReader r{bytes,size,4};
    size_t version=r.integer(1), cursors=r.integer(1);
    if ((version!=3 && version!=4) || (version==3 && cursors) || cursors>8 ||
        memcmp(bytes,version==3?"WFS3":"WFS4",4)) return false;
    size_t networks=r.integer(1), receipts=r.integer(1);
    if (networks>12 || receipts>8 || r.integer(4)!=size) return false;
    uint32_t crc=r.integer(4); memset(bytes+12,0,4);
    bool intact=crc==v2Crc32(bytes,size); for(size_t i=0;i<4;++i) bytes[12+i]=crc>>(8*i);
    if (!intact) return false;
    out.revision=r.integer(8); out.activeVersion=r.integer(8);
    size_t mode=r.integer(1);
    if (mode>2) return false;
    if (mode && !(r.text(out.active.bond,21) && r.text(out.active.id,32))) return false;
    if (mode==2 && !(r.text(out.active.ssid,32) && r.text(out.active.password,64))) return false;
    if (!out.schemes.networks.resize(networks) || !out.receipts.resize(receipts)) return false;
    for(auto &n:out.schemes.networks) if(!r.record(n)) return false;
    for(auto &receipt:out.receipts) {
        size_t length=r.integer(2),start=r.pos;receipt.revision=r.integer(8);
        if(!r.okay || length>size-start || !r.raw(receipt.hash,32) ||
            !r.text(receipt.bridge,64) || !r.text(receipt.request,64) ||
            !r.text(receipt.bond,21) || !r.text(receipt.error,32))return false;
        if(version>=4) {receipt.targetVersion=r.integer(8);if(!r.text(receipt.operation,16)||!r.text(receipt.target,32))return false;}
        if(!r.okay || r.pos-start!=length)return false;
    }
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
// Offline Note4 legacy ABI decoder. Offsets were extracted from the matching
// Xtensa ELF; never cast input to a host C++ struct. See conversion evidence.
static uint64_t legacyInteger(const uint8_t *bytes,size_t offset,size_t width) {
    uint64_t result=0;for(size_t i=0;i<width;++i)result|=uint64_t(bytes[offset+i])<<(8*i);return result;
}
static bool legacyText(WifiText &out,const uint8_t *bytes,size_t offset,size_t width) {
    return !bytes[offset+width-1] && out.set(reinterpret_cast<const char *>(bytes+offset));
}
static bool legacyNetwork(WifiNetwork &out,const uint8_t *bytes) {
    return legacyText(out.bond,bytes,0,22) && legacyText(out.id,bytes,22,33) &&
        legacyText(out.host,bytes,55,33) && legacyText(out.ssid,bytes,88,33) && legacyText(out.password,bytes,121,65);
}
static bool wifiDecodeLegacyOffline(const uint8_t *input,size_t size,WifiStore &out) {
    std::unique_ptr<uint8_t[]> inflated;
    if(size>4 && !memcmp(input,"WFZ2",4)) {
        inflated.reset(new(std::nothrow) uint8_t[5248]);if(!inflated)return false;
        if(wifiInflate(inflated.get(),5248,input+4,size-4)!=5248)return false;
        input=inflated.get();size=5248;
    }
    bool v2=size==5248;
    if(!v2 && size!=2296)return false;
    if(v2 && (legacyInteger(input,0,4)!=2 || legacyInteger(input,5240,4)!=v2Crc32(input,5240)))return false;
    const uint8_t *schemes=input+(v2?4:0);
    if(legacyInteger(schemes,0,4)!=1 || legacyInteger(schemes,2292,4)!=v2Crc32(schemes,2292))return false;
    WifiText selectedBond,selectedId;
    if(!legacyText(selectedBond,schemes,2236,22) || !legacyText(selectedId,schemes,2258,33))return false;
    out.revision=v2?legacyInteger(input,2488,8):0;
    out.activeVersion=v2?legacyInteger(input,2496,8):0;
    for(size_t i=0;i<12;++i) {
        WifiNetwork n;if(!legacyNetwork(n,schemes+4+i*186))return false;
        if(v2 && !legacyText(n.label,input,2600+i*65,65))return false;
        if(!*n.id)continue;
        n.version=v2?legacyInteger(input,2504+i*8,8):0;
        if(!out.schemes.networks.append(n))return false;
    }
    int selected=out.schemes.find(selectedBond,selectedId);
    if(*selectedId && selected<0)return false;
    WifiNetwork active;
    if(v2) {if(!legacyNetwork(active,input+2300))return false;}
    const WifiNetwork &source=!v2 && selected>=0?out.schemes.networks[selected]:active;
    if(!out.active.bond.set(source.bond) || !out.active.id.set(source.id))return false;
    int slot=out.schemes.find(source.bond,source.id);
    if(slot<0 || out.schemes.networks[slot].version!=out.activeVersion) {
        if(!out.active.ssid.set(source.ssid) || !out.active.password.set(source.password))return false;
    }
    out.schemes.selectedBond=out.active.bond;out.schemes.selectedId=out.active.id;
    if(v2) for(size_t i=0;i<8;++i) {
        const uint8_t *record=input+3384+i*232;WifiReceipt receipt;
        if(!legacyText(receipt.bridge,record,0,65) || !legacyText(receipt.request,record,65,65) ||
            !legacyText(receipt.bond,record,130,22) || !legacyText(receipt.error,record,184,33))return false;
        if(!*receipt.request)continue;
        memcpy(receipt.hash,record+152,32);receipt.revision=legacyInteger(record,224,8);
        if(!out.receipts.append(receipt))return false;
    }
    // Legacy formats have no high-watermark/execution fields. Never invent
    // them: the current shape validator rejects a sequenced receipt without
    // its cursor instead of turning an incomplete fence into an empty store.
    return wifiStoreShape(out);
}
