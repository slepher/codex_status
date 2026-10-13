// A single 32-byte selector commits independently stored configuration and
// transactions together. Prepared slots are invisible until this record changes.
static constexpr size_t WIFI_TRANSACTION_MAX=4096, WIFI_COMMIT_BYTES=32;
static bool wifiStorageSplit=false, wifiStorageCleanupPending=false;
static size_t wifiConfigurationBytes=0, wifiTransactionBytes=0;
static const char *wifiConfigKey(unsigned slot){return slot?"config1":"config0";}
static const char *wifiTransactionKey(unsigned slot){return slot?"txn1":"txn0";}
static uint32_t wifiWord(const uint8_t *b,size_t at){uint32_t v=0;for(size_t i=0;i<4;++i)v|=uint32_t(b[at+i])<<(8*i);return v;}
static void wifiSetWord(uint8_t *b,size_t at,uint32_t v){for(size_t i=0;i<4;++i)b[at+i]=v>>(8*i);}
struct WifiBlob {
    std::unique_ptr<uint8_t[]> data;size_t size=0;
    ~WifiBlob(){if(data)wipe((char *)data.get(),size);}
    bool allocate(size_t n){size=n;data.reset(new(std::nothrow) uint8_t[n]);return bool(data);}
    bool read(Preferences &p,const char *key,size_t maximum){
        size_t n=p.getBytesLength(key);return n>=16 && n<=maximum && allocate(n) && p.getBytes(key,data.get(),n)==n;
    }
    void seal(){wifiSetWord(data.get(),8,size);wifiSetWord(data.get(),12,0);wifiSetWord(data.get(),12,v2Crc32(data.get(),size));}
};
static bool wifiPreparedKeys(Preferences &p){return p.isKey("config0") || p.isKey("config1") || p.isKey("txn0") || p.isKey("txn1");}
static bool wifiCommitRead(Preferences &p,uint8_t *record){
    if(p.getBytesLength("commit")!=WIFI_COMMIT_BYTES || p.getBytes("commit",record,WIFI_COMMIT_BYTES)!=WIFI_COMMIT_BYTES)return false;
    uint32_t crc=wifiWord(record,24);wifiSetWord(record,24,0);bool okay=crc==v2Crc32(record,WIFI_COMMIT_BYTES);wifiSetWord(record,24,crc);
    if(!okay || memcmp(record,"WCM1",4) || record[6] || record[7] || wifiWord(record,28))return false;
    if(record[4]>=2)return record[4]==record[5] && !wifiWord(record,16) && !wifiWord(record,20) &&
        (record[4]==2 ? wifiWord(record,8)>=33 && wifiWord(record,8)<=WIFI_STORAGE_MAX :
         record[4]==3 && !wifiWord(record,8) && !wifiWord(record,12));
    return record[5]<=1 && wifiWord(record,8)>=33 && wifiWord(record,8)<=WIFI_STORAGE_MAX &&
        wifiWord(record,16)>=24 && wifiWord(record,16)<=WIFI_TRANSACTION_MAX;
}
static bool wifiDecodeTransactions(WifiBlob &blob,WifiStore &out){
    auto *b=blob.data.get();if(blob.size<24 || memcmp(b,"WTX1",4) || b[4]!=1 || b[5]>8 || b[6] || b[7]>8 || wifiWord(b,8)!=blob.size)return false;
    uint32_t crc=wifiWord(b,12);wifiSetWord(b,12,0);bool intact=crc==v2Crc32(b,blob.size);wifiSetWord(b,12,crc);if(!intact)return false;
    WifiReader r{b,blob.size,16};out.executionGeneration=r.integer(8);
    if(!out.receipts.resize(b[7]) || !out.cursors.resize(b[5]))return false;
    for(auto &receipt:out.receipts)if(!r.receipt(receipt))return false;
    for(auto &c:out.cursors){size_t length=r.integer(2),start=r.pos;c.high=r.integer(8);
        if(!r.okay || length>r.size-start || !r.text(c.bridge,64) || !r.text(c.bond,21) || r.pos-start!=length)return false;}
    return r.okay && r.pos==r.size && wifiStoreShape(out);
}
static __attribute__((noinline)) bool readWifiStore(Preferences &p,WifiStore &out){
    wifiStorageSplit=false;wifiConfigurationBytes=wifiTransactionBytes=wifiStorageBytes=0;
    if(!p.isKey("commit")){
        if(wifiPreparedKeys(p))return false; // Missing selector must never roll back to an older ledger.
        WifiBlob old;if(!old.read(p,"store3",WIFI_STORAGE_MAX) || !wifiDecode4(old.data.get(),old.size,out))return false;
        // Legacy receipts remain authoritative until the first verified split commit.
        wifiStorageFormat=4;wifiStorageBytes=wifiConfigurationBytes=old.size;return true;
    }
    uint8_t record[WIFI_COMMIT_BYTES];if(!wifiCommitRead(p,record))return false;
    // Bootstrap selectors make first-save interruption distinguishable from a
    // lost/corrupt commit key. 2 references the original WFS4; 3 commits empty.
    if(record[4]>=2){
        if(record[4]==2){WifiBlob old;if(!old.read(p,"store3",WIFI_STORAGE_MAX) || old.size!=wifiWord(record,8) ||
            v2Crc32(old.data.get(),old.size)!=wifiWord(record,12) || !wifiDecode4(old.data.get(),old.size,out))return false;
            wifiStorageFormat=4;wifiConfigurationBytes=old.size;
        }else{out=WifiStore{};wifiStorageFormat=0;}
        wifiStorageBytes=wifiConfigurationBytes+WIFI_COMMIT_BYTES;return true;
    }
    {
        WifiBlob cfg;if(!cfg.read(p,wifiConfigKey(record[4]),WIFI_STORAGE_MAX) || cfg.size!=wifiWord(record,8) ||
            v2Crc32(cfg.data.get(),cfg.size)!=wifiWord(record,12) || cfg.data[5] || cfg.data[7] || !wifiDecode4(cfg.data.get(),cfg.size,out))return false;
    }
    {
        WifiBlob txn;if(!txn.read(p,wifiTransactionKey(record[5]),WIFI_TRANSACTION_MAX) || txn.size!=wifiWord(record,16) ||
            v2Crc32(txn.data.get(),txn.size)!=wifiWord(record,20) || !wifiDecodeTransactions(txn,out))return false;
    }
    wifiStorageSplit=true;wifiStorageFormat=4;wifiConfigurationBytes=wifiWord(record,8);wifiTransactionBytes=wifiWord(record,16);
    wifiStorageBytes=wifiConfigurationBytes+wifiTransactionBytes+WIFI_COMMIT_BYTES;
    wifiStorageCleanupPending=p.isKey("store3") || p.isKey(wifiConfigKey(1-record[4])) || p.isKey(wifiTransactionKey(1-record[5]));return true;
}
static bool wifiWriteVerified(Preferences &p,const char *key,const uint8_t *bytes,size_t size){
    wifiStorageFailure="storage_write";if(p.putBytes(key,bytes,size)!=size)return false;
    wifiStorageFailure="storage_readback";WifiBlob check;
    return check.read(p,key,size) && check.size==size && !memcmp(check.data.get(),bytes,size);
}
static bool wifiPreparePart(Preferences &p,const WifiStore &next,bool transaction,bool existing,uint8_t *record){
    WifiWriter sizing{nullptr};if(transaction)sizing.transactions(next);else sizing.configuration(next);
    size_t limit=transaction?WIFI_TRANSACTION_MAX:WIFI_STORAGE_MAX;
    wifiStorageFailure="storage_capacity";if(sizing.pos>limit)return false;
    WifiBlob blob;wifiStorageFailure="memory";if(!blob.allocate(sizing.pos))return false;
    WifiWriter writer{blob.data.get()};if(transaction)writer.transactions(next);else writer.configuration(next);blob.seal();
    size_t slotAt=transaction?5:4,lenAt=transaction?16:8,crcAt=lenAt+4;
    const char *oldKey=transaction?wifiTransactionKey(record[slotAt]):wifiConfigKey(record[slotAt]);
    if(existing){
        WifiBlob old;wifiStorageFailure="storage_read";
        if(!old.read(p,oldKey,limit) || old.size!=wifiWord(record,lenAt) || v2Crc32(old.data.get(),old.size)!=wifiWord(record,crcAt))return false;
        if(old.size==blob.size && !memcmp(old.data.get(),blob.data.get(),blob.size))return true;
    }
    unsigned slot=existing?1-record[slotAt]:0;const char *key=transaction?wifiTransactionKey(slot):wifiConfigKey(slot);
    if(!wifiWriteVerified(p,key,blob.data.get(),blob.size))return false;
    record[slotAt]=slot;wifiSetWord(record,lenAt,blob.size);wifiSetWord(record,crcAt,v2Crc32(blob.data.get(),blob.size));return true;
}
static void wifiRetirePrepared(Preferences &p,const uint8_t *record){
    wifiStorageCleanupPending=false;
    for(const char *key:{"store3",wifiConfigKey(1-record[4]),wifiTransactionKey(1-record[5])})
        if(p.isKey(key) && !p.remove(key))wifiStorageCleanupPending=true;
}
static bool saveV2(WifiStore &next){
    int active=next.schemes.find(next.active.bond,next.active.id);
    if(active>=0 && next.schemes.networks[active].version==next.activeVersion){next.active.ssid.clear();next.active.password.clear();}
    wifiStorageFailure="storage_shape";if(!wifiStoreShape(next))return false;
    Preferences p;wifiStorageFailure="storage_open";if(!p.begin("wifi-bonds",false))return false;
    uint8_t record[WIFI_COMMIT_BYTES]={};memcpy(record,"WCM1",4);
    bool existing=p.isKey("commit"),okay=!existing || wifiCommitRead(p,record);
    wifiStorageFree=p.freeEntries();wifiStorageUncertain=true;
    if(!existing){
        // Establish an authoritative old/empty root before any prepared key.
        // Without it a lost selector could silently resurrect an older receipt.
        okay=!wifiPreparedKeys(p);
        if(okay && p.isKey("store3")){
            WifiBlob old;okay=old.read(p,"store3",WIFI_STORAGE_MAX);
            if(okay){record[4]=record[5]=2;wifiSetWord(record,8,old.size);wifiSetWord(record,12,v2Crc32(old.data.get(),old.size));}
        }else if(okay){okay=!p.isKey("scheme");record[4]=record[5]=3;}
        if(okay){wifiSetWord(record,24,v2Crc32(record,sizeof(record)));okay=wifiWriteVerified(p,"commit",record,sizeof(record));}
    }
    uint8_t before[WIFI_COMMIT_BYTES];memcpy(before,record,sizeof(record));
    bool split=record[4]<2;
    if(okay)okay=wifiPreparePart(p,next,false,split,record) && wifiPreparePart(p,next,true,split,record);
    if(okay && (!existing || memcmp(before,record,sizeof(record)))){
        wifiSetWord(record,24,0);wifiSetWord(record,24,v2Crc32(record,sizeof(record)));
        okay=wifiWriteVerified(p,"commit",record,sizeof(record));
    }
    if(okay){
        // Preparation readback and the selector readback prove the pair. Failed
        // garbage collection cannot undo an already confirmed transaction.
        wifiRetirePrepared(p,record);wifiStorageUncertain=false;wifiStorageSplit=true;wifiStorageFormat=4;
        wifiConfigurationBytes=wifiWord(record,8);wifiTransactionBytes=wifiWord(record,16);
        wifiStorageBytes=wifiVerifiedStorageBytes=wifiConfigurationBytes+wifiTransactionBytes+WIFI_COMMIT_BYTES;
        if(&next!=&wifiStore)wifiStore=std::move(next);
    }
    p.end();return okay;
}
