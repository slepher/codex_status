#include "bridge_store.h"
#include "v2_state.h"
#ifdef CODEX_ENDPOINT_HOST_TEST
#include "../tools/bridge-store-test-hal.h"
#else
#include <Preferences.h>
#endif
#include <memory>
#include <new>

static constexpr size_t STORE_MAX=8, STORE_BYTES=4096;
static String k(const char *prefix,int i) { return String(prefix)+String(i); }
struct Endpoints {
    std::unique_ptr<EndpointRec[]> rows;
    size_t count=0;
    bool legacy=false;
};
static bool valid(const EndpointRec &r) {
    return r.mac.length()<=21 && r.host.length()>0 && r.host.length()<=64 && r.port &&
        r.token.length()>0 && r.token.length()<=128 && r.principal.length()<=64 &&
        r.identity.length()<=21 && r.nameSpace.length()<=21 && r.bssid.length()<=21;
}
static bool authorized(const EndpointRec &r) {
    return r.principal.length() && r.identity.length() && r.nameSpace.length();
}
static bool legacyString(Preferences &p,const String &key,String &out,size_t limit,bool required) {
    if(!p.isKey(key.c_str()))return !required;
    char text[129]{};
    size_t n=p.getString(key.c_str(),text,limit+1);
    if(!n || n>limit+1 || strlen(text)!=n-1)return false;
    out=text;memset(text,0,sizeof(text));return out.length()==n-1;
}
// Reading old records never grants an identity or rewrites storage. Incomplete
// records are retained, but only complete bindings can authenticate or route.
static bool load(Endpoints &out) {
    Preferences p; if (!p.begin("brg",true)) return false;
    size_t size=p.getBytesLength("records2");
    if (!size) {
        // An existing empty/wrong-type current record is corruption, not a
        // legacy store. Never recreate authorization identities over it.
        if (p.isKey("records2")) { p.end(); return false; }
        if(p.isKey("n")) {
            out.legacy=true;out.count=p.getUChar("n",255);
            if(out.count>STORE_MAX){p.end();return false;}
            out.rows.reset(out.count?new(std::nothrow) EndpointRec[out.count]:nullptr);
            if(out.count&&!out.rows){p.end();return false;}
            bool okay=true;
            for(size_t i=0;i<out.count && okay;++i){auto &r=out.rows[i];
                okay=legacyString(p,k("m",i),r.mac,21,true) &&
                    legacyString(p,k("h",i),r.host,64,true) &&
                    legacyString(p,k("t",i),r.token,128,true) &&
                    legacyString(p,k("b",i),r.bssid,21,false);
                r.port=p.getUShort(k("p",i).c_str(),0);r.mru=p.getULong(k("r",i).c_str(),0);
                okay=okay && valid(r);
                for(size_t j=0;j<i;++j)if(r.token==out.rows[j].token)okay=false;
            }
            for(size_t i=out.count;i<STORE_MAX;++i)for(const char *prefix:{"m","h","p","t","b","r"})
                if(p.isKey(k(prefix,i).c_str()))okay=false;
            p.end();return okay;
        }
        if(p.isKey("ctr")){p.end();return false;}
        // A missing legacy count cannot turn orphaned credentials into empty storage.
        for(size_t i=0;i<STORE_MAX;++i)for(const char *prefix:{"m","h","p","t","b","r"})
            if(p.isKey(k(prefix,i).c_str())) { p.end();return false; }
        p.end();return true;
    }
    if(size<16 || size>STORE_BYTES) { p.end();return false; }
    std::unique_ptr<uint8_t[]> bytes(new(std::nothrow) uint8_t[size]);
    bool okay=bytes && p.getBytes("records2",bytes.get(),size)==size;p.end();if(!okay)return false;
    auto *b=bytes.get();
    auto integer=[&](size_t at,size_t width){uint64_t v=0;for(size_t i=0;i<width;++i)v|=uint64_t(b[at+i])<<(8*i);return v;};
    uint32_t crc=integer(12,4);memset(b+12,0,4);
    if(memcmp(b,"BRS2",4)||b[4]!=2||b[5]>STORE_MAX||b[6]||b[7]||integer(8,4)!=size||crc!=v2Crc32(b,size))return false;
    out.count=b[5];out.rows.reset(out.count?new(std::nothrow) EndpointRec[out.count]:nullptr);if(out.count&&!out.rows)return false;
    size_t pos=16;
    for(size_t i=0;i<out.count;++i) {
        if(size-pos<8)return false;size_t length=integer(pos,2);pos+=2;size_t start=pos;if(length<13 || length>size-pos)return false;
        auto &r=out.rows[i];r.port=integer(pos,2);pos+=2;r.mru=integer(pos,4);pos+=4;
        for(String *field:{&r.mac,&r.principal,&r.identity,&r.nameSpace,&r.host,&r.token,&r.bssid}) {
            if(pos>=size)return false;size_t n=b[pos++];if(n>size-pos||memchr(b+pos,0,n))return false;
            char text[129];if(n>128)return false;memcpy(text,b+pos,n);text[n]=0;*field=text;memset(text,0,sizeof(text));if(field->length()!=n)return false;pos+=n;
        }
        if(pos-start!=length||!valid(r))return false;
        for(size_t j=0;j<i;++j) if((r.principal.length() && r.principal==out.rows[j].principal)||r.token==out.rows[j].token ||
            (r.nameSpace.length() && r.nameSpace==out.rows[j].nameSpace))return false;
    }
    memset(b,0,size);return pos==size;
}
static bool save(const Endpoints &all) {
    size_t size=16;
    for(size_t i=0;i<all.count;++i){const auto &r=all.rows[i];if(!valid(r))return false;
        size+=15+r.mac.length()+r.principal.length()+r.identity.length()+r.nameSpace.length()+r.host.length()+r.token.length()+r.bssid.length();}
    if(size>STORE_BYTES)return false;
    std::unique_ptr<uint8_t[]> bytes(new(std::nothrow) uint8_t[size]{});if(!bytes)return false;
    auto *b=bytes.get();memcpy(b,"BRS2",4);b[4]=2;b[5]=all.count;
    auto integer=[&](size_t at,uint64_t v,size_t width){for(size_t i=0;i<width;++i)b[at+i]=v>>(8*i);};integer(8,size,4);
    size_t pos=16;
    for(size_t i=0;i<all.count;++i){const auto &r=all.rows[i];size_t start=pos;pos+=2;integer(pos,r.port,2);pos+=2;integer(pos,r.mru,4);pos+=4;
        for(const String *field:{&r.mac,&r.principal,&r.identity,&r.nameSpace,&r.host,&r.token,&r.bssid}){b[pos++]=field->length();memcpy(b+pos,field->c_str(),field->length());pos+=field->length();}
        integer(start,pos-start-2,2);}
    integer(12,v2Crc32(b,size),4);Preferences p;bool okay=p.begin("brg",false);
    if(okay){okay=p.putBytes("records2",b,size)==size;
        if(okay){std::unique_ptr<uint8_t[]> check(new(std::nothrow) uint8_t[size]);okay=check&&p.getBytesLength("records2")==size&&p.getBytes("records2",check.get(),size)==size&&!memcmp(check.get(),b,size);if(check)memset(check.get(),0,size);}
        if(okay && all.legacy){
            for(size_t i=0;i<all.count;++i)for(const char *prefix:{"m","h","p","t","b","r"})p.remove(k(prefix,i).c_str());
            p.remove("n");p.remove("ctr");
        }
        p.end();}
    memset(b,0,size);return okay;
}
int storeCount(){Endpoints all;return load(all)?all.count:0;}
bool storeGet(int i,EndpointRec &rec){Endpoints all;if(!load(all)||i<0||size_t(i)>=all.count||!authorized(all.rows[i]))return false;rec=all.rows[i];return true;}
bool storeAuthenticate(const String &token,EndpointRec &rec){Endpoints all;if(!token.length()||!load(all))return false;int found=-1;
    for(size_t i=0;i<all.count;++i)if(token==all.rows[i].token && authorized(all.rows[i])){if(found>=0)return false;found=i;}
    if(found<0)return false;rec=all.rows[found];return true;}
const char *storeCheckEndpoint(const String &host,uint16_t port,const String &token,
    const String &principal,const String &identity) {
    Endpoints all;if(!load(all))return "storage";
    for(size_t i=0;i<all.count;++i){const auto &r=all.rows[i];
        if(authorized(r) && r.principal==principal && r.identity==identity && r.token==token && r.host==host && r.port==port)
            return nullptr;
    }
    return "registration_mismatch";
}
const char *storeUpsert(const String &mac,const String &host,uint16_t port,const String &token,
    const String &principal,const String &identity,const String &newNamespace,bool physical){
    if(!principal.length()||!identity.length()||!token.length())return "binding_required";
    Endpoints old;if(!load(old))return "storage";int slot=-1;
    for(size_t i=0;i<old.count;++i){const auto &r=old.rows[i];
        if(r.principal==principal){if(r.token!=token)return "binding_conflict";slot=i;}
        if(r.token==token){
            if(r.principal.length() && r.principal!=principal)return "token_conflict";
            if(!r.principal.length()){
                if(r.mac!=mac)return "binding_conflict";
                slot=i;
            }
        }}
    String ns;
    if(slot>=0){const auto &r=old.rows[slot];
        if(!authorized(r) && !physical)return "physical_required";
        if(r.token!=token || (r.identity!=identity && !physical))return "binding_conflict";
        // Physical repair proves the new identity; it does not infer an old
        // namespace from the current peer or give old transactions a new owner.
        ns=r.nameSpace.length()?r.nameSpace:newNamespace;}
    else {if(!physical)return "physical_required";if(old.count>=STORE_MAX)return "capacity";ns=newNamespace;slot=old.count;}
    if(!ns.length())return "shape";
    Endpoints next;next.legacy=old.legacy;next.count=old.count+(size_t(slot)==old.count);next.rows.reset(new(std::nothrow) EndpointRec[next.count]);if(!next.rows)return "memory";
    for(size_t i=0;i<old.count;++i){next.rows[i]=old.rows[i];const auto &a=old.rows[i],&b=next.rows[i];
        if(a.mac!=b.mac || a.principal!=b.principal || a.identity!=b.identity || a.nameSpace!=b.nameSpace || a.host!=b.host || a.token!=b.token || a.bssid!=b.bssid)return "memory";
    }
    auto &r=next.rows[slot];r.mac=mac;r.host=host;r.port=port;r.token=token;r.principal=principal;r.identity=identity;r.nameSpace=ns;
    if(!authorized(r) || r.mac!=mac || r.host!=host || r.token!=token || r.principal!=principal || r.identity!=identity || r.nameSpace!=ns)return "memory";
    for(size_t i=0;i<next.count;++i)if(i!=size_t(slot)&&(next.rows[i].token==r.token||next.rows[i].nameSpace==r.nameSpace))return "binding_ambiguous";
    return save(next)?nullptr:"storage_unknown";
}
void storeTouch(const String &) {} // routing recency never changes authorization lifetime
void storeSetBssid(const String &mac,const String &bssid){Endpoints all;if(!load(all))return;for(size_t i=0;i<all.count;++i)if(authorized(all.rows[i]) && all.rows[i].mac==mac && all.rows[i].bssid!=bssid){all.rows[i].bssid=bssid;save(all);}}
void storeClear(){Preferences p;if(p.begin("brg",false)){p.clear();p.end();}}
