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
 // Retired per-key storage cannot acquire guessed principal/namespace values.
 auto rejected=[] {
  const auto before=endpointNvs;EndpointRec ignored;
  assert(!storeAuthenticate("one",ignored));
  for(bool physical:{false,true}) {
   const char *failure=storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"invented",physical);
   if(!failure || strcmp(failure,"storage")){std::cerr<<"legacy/incomplete binding accepted\n";std::exit(1);}
  }
  storeSetBssid(raw,"new-bssid");assert(endpointNvs==before);
 };
 endpointNvs["n"]={1};endpointNvs["p0"]={0x3d,0x22};legacy("m0",raw);legacy("h0","192.0.2.1");legacy("t0","one");
 rejected();
 for(const char *key:{"n","ctr","m0","h0","p0","t0","b0","r0","m7","t7"}) {
  endpointNvs.clear();endpointNvs[key]={};rejected();
 }
 endpointNvs.clear();
 endpointWriteFail=true;assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,peer,true),"storage_unknown"));endpointWriteFail=false;
 assert(endpointNvs.empty());
 endpointCommitError=true;assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,peer,true),"storage_unknown"));endpointCommitError=false;
 EndpointRec a;assert(storeAuthenticate("one",a)&&a.principal=="instance-a"&&a.nameSpace==peer);
 const auto single=endpointNvs["records2"];
 for(size_t missing:{size_t(1),size_t(2),size_t(3)}) {
  auto incomplete=single;size_t at=24;for(size_t i=0;i<missing;++i)at+=1+incomplete[at];
  size_t length=incomplete[at];incomplete.erase(incomplete.begin()+at+1,incomplete.begin()+at+1+length);incomplete[at]=0;
  size_t record=single[16]+256*single[17]-length;incomplete[16]=record;incomplete[17]=record>>8;
  for(size_t i=0;i<4;++i)incomplete[8+i]=incomplete.size()>>(8*i);
  memset(incomplete.data()+12,0,4);uint32_t crc=v2Crc32(incomplete.data(),incomplete.size());for(size_t i=0;i<4;++i)incomplete[12+i]=crc>>(8*i);
  endpointNvs["records2"]=incomplete;rejected();
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
 assert(endpointNvs["records2"].size()==2856);std::cout<<"BRS2 full field upper bound="<<endpointNvs["records2"].size()<<" bytes\n";
 std::cout<<"BRS2: legacy/incomplete identity rejection and byte preservation, peer+principal dual proof, same-peer instances, corruption/truncation, capacity no eviction passed; typical="<<bytes.size()<<" full="<<full.size()<<" bytes\n";
}
