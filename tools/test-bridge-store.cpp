#include <cstdlib>
#include <new>
#include <cstddef>
#include <cassert>
#include <cstring>
#include <iostream>
#include <vector>
// Count requested C++ allocation bytes (allocator metadata is separate). Fail
// only nothrow allocations, which the embedded repository uses throughout.
static size_t liveBytes=0, peakBytes=0;
static long failAllocation=-1;
static bool allocationFaultInjected=false;
struct alignas(std::max_align_t) Allocation { size_t size; };
static void *allocate(size_t size) {
    auto *header=(Allocation *)std::malloc(sizeof(Allocation)+size);
    if(!header)throw std::bad_alloc(); header->size=size;
    liveBytes+=size; peakBytes=std::max(peakBytes,liveBytes); return header+1;
}
void *operator new(size_t size) { return allocate(size); }
void *operator new[](size_t size) { return allocate(size); }
void *operator new(size_t size,const std::nothrow_t &) noexcept { if(failAllocation==0){allocationFaultInjected=true;return nullptr;} if(failAllocation>0)--failAllocation; try{return allocate(size);}catch(...){return nullptr;} }
void *operator new[](size_t size,const std::nothrow_t &tag) noexcept { return ::operator new(size,tag); }
void operator delete(void *p) noexcept { if(p){auto *header=(Allocation *)p-1;liveBytes-=header->size;std::free(header);} }
void operator delete[](void *p) noexcept { ::operator delete(p); }
void operator delete(void *p,const std::nothrow_t &) noexcept { ::operator delete(p); }
void operator delete[](void *p,const std::nothrow_t &) noexcept { ::operator delete(p); }
#define CODEX_ENDPOINT_HOST_TEST
#include "../src/bridge_store.cpp"
#include <cassert>
#include <iostream>
static const char *peer="02:00:00:00:00:02/1";
static const char *raw="02:00:00:00:00:02";
static void legacy(const char *key,const char *text){endpointNvs[key]=std::vector<uint8_t>(text,text+strlen(text));}
int main(){
 assert(storeCount()==0);
 assert(endpointNvs.find("records2")==endpointNvs.end());std::cout<<"BRS2 empty store=0 bytes\n";
 assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"namespace-a",false),"physical_required"));
 // Damaged storage must never become an empty or guessed binding store.
 auto rejected=[] {
  const auto before=endpointNvs;EndpointRec ignored;
  assert(!storeAuthenticate("one",ignored));
  assert(!strcmp(storeCheckEndpoint("192.0.2.1",8765,"one","instance-a",peer),"storage"));
  for(bool physical:{false,true}) {
   const char *failure=storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"invented",physical);
   if(!failure || strcmp(failure,"storage")){std::cerr<<"legacy/incomplete binding accepted\n";std::exit(1);}
  }
  storeSetBssid(raw,"new-bssid");assert(endpointNvs==before);
 };
 endpointNvs["n"]={1};endpointNvs["p0"]={0x3d,0x22};legacy("m0",raw);legacy("h0","192.0.2.1");legacy("t0","one");
 {
  const auto old=endpointNvs;EndpointRec ignored;assert(storeCount()==1 && !storeGet(0,ignored));
  assert(!storeAuthenticate("one",ignored));
  assert(!strcmp(storeCheckEndpoint("192.0.2.1",8765,"one","instance-a",peer),"registration_mismatch"));
  assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"fresh-namespace",false),"physical_required"));
  assert(endpointNvs==old);
  endpointWriteFail=true;
  assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"fresh-namespace",true),"storage_unknown"));
  endpointWriteFail=false;assert(endpointNvs==old);
  assert(!storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"fresh-namespace",true));
  assert(storeAuthenticate("one",ignored)&&ignored.nameSpace=="fresh-namespace");
  assert(!endpointNvs.count("n")&&!endpointNvs.count("t0"));
 }
 for(const char *key:{"n","ctr","m0","h0","p0","t0","b0","r0","m7","t7"}) {
  endpointNvs.clear();endpointNvs[key]={};rejected();
 }
 // Multiple old rows with empty identities coexist; an unrelated registration
 // preserves all of them while making only the new binding available.
 endpointNvs.clear();endpointNvs["n"]={2};
 for(int i=0;i<2;++i){auto n=std::to_string(i);legacy(("m"+n).c_str(),raw);legacy(("h"+n).c_str(),"192.0.2.1");legacy(("t"+n).c_str(),i?"old-two":"old-one");endpointNvs["p"+n]={0x3d,0x22};}
 const auto oldPair=endpointNvs;Endpoints pending;assert(load(pending)&&pending.count==2);assert(endpointNvs==oldPair);
 assert(!storeUpsert(raw,"192.0.2.2",8765,"new-token","new-principal",peer,"new-ns",true));
 EndpointRec active;assert(storeCount()==3&&!storeGet(0,active)&&!storeGet(1,active)&&storeGet(2,active));
 assert(!storeAuthenticate("old-one",active)&&!storeAuthenticate("old-two",active));
 assert(load(pending)&&pending.rows[0].token=="old-one"&&pending.rows[1].token=="old-two");
 assert(!pending.rows[0].nameSpace.length()&&!pending.rows[1].principal.length());
 endpointNvs=oldPair;legacy("t1","old-one");rejected();
 endpointNvs=oldPair;legacy("h0",std::string(65,'x').c_str());rejected();
 endpointNvs=oldPair;endpointNvs["n"]={9};rejected();
 endpointNvs=oldPair;endpointNvs["p0"]={0};rejected();
 endpointNvs.clear();
 endpointWriteFail=true;assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,peer,true),"storage_unknown"));endpointWriteFail=false;
 assert(endpointNvs.empty());
 endpointCommitError=true;assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,peer,true),"storage_unknown"));endpointCommitError=false;
 EndpointRec a;assert(storeAuthenticate("one",a)&&a.principal=="instance-a"&&a.nameSpace==peer);
 const auto beforeCheck=endpointNvs;
 assert(!storeCheckEndpoint("192.0.2.1",8765,"one","instance-a",peer));
 for(auto mismatch:{storeCheckEndpoint("other-host",8765,"one","instance-a",peer),
     storeCheckEndpoint("192.0.2.1",8766,"one","instance-a",peer),
     storeCheckEndpoint("192.0.2.1",8765,"other-token","instance-a",peer),
     storeCheckEndpoint("192.0.2.1",8765,"one","other-principal",peer),
     storeCheckEndpoint("192.0.2.1",8765,"one","instance-a","other-peer")})
   assert(mismatch && !strcmp(mismatch,"registration_mismatch"));
 assert(endpointNvs==beforeCheck);
 const auto single=endpointNvs["records2"];
 size_t allocationFailures=0;
 for(const auto &initial:{endpointNvs,oldPair})for(long fault=0;fault<9;++fault){
  endpointNvs=initial;allocationFaultInjected=false;failAllocation=fault;
  const char *failure=storeUpsert(raw,"192.0.2.2",8765,"fault-token","fault-principal",peer,"fault-ns",true);
  failAllocation=-1;
  if(allocationFaultInjected){++allocationFailures;assert(failure);}
  Endpoints recovered;assert(load(recovered));
  EndpointRec bound;const bool committed=storeAuthenticate("fault-token",bound);
  if(!committed)assert(endpointNvs==initial);
  else assert(!failure || !strcmp(failure,"storage_unknown"));
 }
 std::cout<<"BRS allocation fault sites exercised="<<allocationFailures<<"\n";
 endpointNvs=beforeCheck;
 for(size_t missing:{size_t(1),size_t(2),size_t(3)}) {
  auto incomplete=single;size_t at=24;for(size_t i=0;i<missing;++i)at+=1+incomplete[at];
  size_t length=incomplete[at];incomplete.erase(incomplete.begin()+at+1,incomplete.begin()+at+1+length);incomplete[at]=0;
  size_t record=single[16]+256*single[17]-length;incomplete[16]=record;incomplete[17]=record>>8;
  for(size_t i=0;i<4;++i)incomplete[8+i]=incomplete.size()>>(8*i);
  memset(incomplete.data()+12,0,4);uint32_t crc=v2Crc32(incomplete.data(),incomplete.size());for(size_t i=0;i<4;++i)incomplete[12+i]=crc>>(8*i);
  endpointNvs["records2"]=incomplete;
  const auto before=endpointNvs;EndpointRec unavailable;
  assert(storeCount()==1 && !storeGet(0,unavailable) && !storeAuthenticate("one",unavailable));
  assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"fresh",false),"physical_required"));
  assert(endpointNvs==before);
  // Add a valid second endpoint without discarding or authorizing the old one.
  assert(!storeUpsert("other","192.0.2.2",8765,"two","instance-b","other-peer","other-namespace",true));
  assert(storeCount()==2 && storeAuthenticate("two",unavailable) && !storeAuthenticate("one",unavailable));
  const auto mixed=endpointNvs;
  assert(std::equal(incomplete.begin()+16,incomplete.end(),endpointNvs["records2"].begin()+16));
  assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-b",peer,"fresh",true),missing==1?"binding_conflict":"token_conflict"));
  if(missing!=1)assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","intruder",peer,"fresh",true),"token_conflict"));
  if(missing==1)assert(!strcmp(storeUpsert("wrong-mac","192.0.2.1",8765,"one","instance-a",peer,"fresh",true),"binding_conflict"));
  assert(endpointNvs==mixed);
  Endpoints loaded;assert(load(loaded) && loaded.count==2);
  assert(loaded.rows[0].principal.length()==(missing==1?0:strlen("instance-a")));
  assert(!storeCheckEndpoint("192.0.2.2",8765,"two","instance-b","other-peer"));
  assert(endpointNvs==mixed);
  assert(!storeUpsert("other","192.0.2.3",8765,"two","instance-b","other-peer","unused",false));
  Endpoints updated;assert(load(updated));
  assert(updated.rows[0].principal==loaded.rows[0].principal && updated.rows[0].identity==loaded.rows[0].identity && updated.rows[0].nameSpace==loaded.rows[0].nameSpace && updated.rows[0].token==loaded.rows[0].token);
  const auto beforeRepair=endpointNvs;
  endpointWriteFail=true;
  assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"fresh",true),"storage_unknown"));
  endpointWriteFail=false;assert(endpointNvs==beforeRepair);
  endpointCommitError=true;
  assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"fresh",true),"storage_unknown"));
  endpointCommitError=false;
  assert(storeAuthenticate("one",unavailable));
  assert(unavailable.nameSpace==(missing==3?"fresh":peer));
  const auto repaired=endpointNvs;
  assert(!storeCheckEndpoint("192.0.2.1",8765,"one","instance-a",peer));assert(endpointNvs==repaired);
 }
 endpointNvs["records2"]=single;
 // Current records remain usable alongside a private legacy backup; no deletion.
 endpointNvs["n"]={1};legacy("t0","old-backup-token");const auto backup=endpointNvs["t0"];
 assert(!storeUpsert(raw,"192.0.2.2",8775,"one","instance-a",peer,"unused",false));
 assert(endpointNvs["n"]==std::vector<uint8_t>{1}&&endpointNvs["t0"]==backup);
 std::cout<<"BRS2 one entry="<<endpointNvs["records2"].size()<<" bytes\n";
 assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-b",peer,"namespace-b",true),"token_conflict"));
 assert(!storeUpsert(raw,"192.0.2.1",8775,"two","instance-b",peer,"namespace-b",true));
 EndpointRec b;assert(storeAuthenticate("two",b)&&b.principal=="instance-b"&&b.identity==peer&&b.nameSpace!=a.nameSpace);
 assert(!strcmp(storeUpsert(raw,"192.0.2.1",8775,"two","instance-b","different-peer","unused",false),"binding_conflict"));
 assert(!storeUpsert(raw,"192.0.2.1",8775,"two","instance-b","different-peer","unused",true));
 assert(storeAuthenticate("two",b)&&b.nameSpace=="namespace-b"&&b.identity=="different-peer");
 assert(!strcmp(storeUpsert(raw,"192.0.2.1",8775,"three","instance-c",peer,peer,true),"binding_ambiguous"));
 const auto bytes=endpointNvs["records2"];
 for(size_t i=0;i<bytes.size();++i){endpointNvs["records2"]=bytes;endpointNvs["records2"][i]^=0x80;assert(!storeAuthenticate("one",a));assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"unused",false),"storage"));}
 for(size_t size=0;size<bytes.size();++size){
  endpointNvs["records2"]=std::vector<uint8_t>(bytes.begin(),bytes.begin()+size);
  assert(!storeAuthenticate("one",a));
  const char *failure=storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"new-namespace",true);
  if(!failure || strcmp(failure,"storage")){std::cerr<<"truncated BRS2 accepted at "<<size<<" bytes\n";return 1;}
  assert(endpointNvs["records2"]==std::vector<uint8_t>(bytes.begin(),bytes.begin()+size));
 }
 endpointNvs["records2"]=bytes;
 for(size_t i=2;i<8;++i){std::string n=std::to_string(i);assert(!storeUpsert(raw,"192.0.2.1",8765,("token"+n).c_str(),("instance"+n).c_str(),peer,("namespace"+n).c_str(),true));}
 assert(storeCount()==8);auto full=endpointNvs["records2"];assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"nine","instance9",peer,"namespace9",true),"capacity"));assert(endpointNvs["records2"]==full);
 endpointNvs.clear();
 for(size_t i=0;i<8;++i){std::string unique(1,char('a'+i));assert(!storeUpsert(std::string(21,'m').c_str(),std::string(64,'h').c_str(),8765,(std::string(127,'t')+unique).c_str(),(std::string(63,'p')+unique).c_str(),std::string(21,'i').c_str(),(std::string(20,'n')+unique).c_str(),true));storeSetBssid(std::string(21,'m').c_str(),std::string(21,'b').c_str());}
 size_t baseline=liveBytes;peakBytes=baseline;
 assert(!storeUpsert(std::string(21,'m').c_str(),std::string(64,'h').c_str(),8765,(std::string(127,'t')+"a").c_str(),(std::string(63,'p')+"a").c_str(),std::string(21,'i').c_str(),"unused",false));
 std::cout<<"BRS full update host peak heap delta="<<peakBytes-baseline<<" bytes (includes HAL)\n";
 assert(endpointNvs["records2"].size()==2856);std::cout<<"BRS2 full field upper bound="<<endpointNvs["records2"].size()<<" bytes\n";
 std::cout<<"BRS2: legacy/incomplete identity isolation, physical repair and byte preservation, peer+principal dual proof, same-peer instances, corruption/truncation, capacity no eviction passed; typical="<<bytes.size()<<" full="<<full.size()<<" bytes\n";
}
