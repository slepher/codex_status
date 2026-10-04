#define CODEX_ENDPOINT_HOST_TEST
#include "../src/bridge_store.cpp"
#include <cassert>
#include <iostream>
static const char *peer="02:00:00:00:00:02/1";
static const char *raw="02:00:00:00:00:02";
static void legacy(const char *key,const char *text){endpointNvs[key]=std::vector<uint8_t>(text,text+strlen(text));}
int main(){
 assert(storeCount()==0);
 assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"namespace-a",false),"physical_required"));
 endpointNvs["n"]={1};endpointNvs["p0"]={0x3d,0x22};legacy("m0",raw);legacy("h0","192.0.2.1");legacy("t0","one");
 endpointWriteFail=true;assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"unused",false),"storage_unknown"));endpointWriteFail=false;
 assert(endpointNvs["n"].size()==1);
 endpointCommitError=true;assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"unused",false),"storage_unknown"));endpointCommitError=false;
 EndpointRec a;assert(storeAuthenticate("one",a)&&a.principal=="instance-a"&&a.nameSpace==peer);assert(endpointNvs["n"].size()==1);
 assert(!storeUpsert(raw,"192.0.2.2",8775,"one","instance-a",peer,"unused",false));assert(endpointNvs.find("n")==endpointNvs.end());
 assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-b",peer,"namespace-b",true),"token_conflict"));
 assert(!storeUpsert(raw,"192.0.2.1",8775,"two","instance-b",peer,"namespace-b",true));
 EndpointRec b;assert(storeAuthenticate("two",b)&&b.principal=="instance-b"&&b.identity==peer&&b.nameSpace!=a.nameSpace);
 assert(!strcmp(storeUpsert(raw,"192.0.2.1",8775,"two","instance-b","different-peer","unused",false),"binding_conflict"));
 assert(!storeUpsert(raw,"192.0.2.1",8775,"two","instance-b","different-peer","unused",true));
 assert(storeAuthenticate("two",b)&&b.nameSpace=="namespace-b"&&b.identity=="different-peer");
 assert(!strcmp(storeUpsert(raw,"192.0.2.1",8775,"three","instance-c",peer,peer,true),"binding_ambiguous"));
 const auto bytes=endpointNvs["records2"];
 for(size_t i=0;i<bytes.size();++i){endpointNvs["records2"]=bytes;endpointNvs["records2"][i]^=0x80;assert(!storeAuthenticate("one",a));assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"one","instance-a",peer,"unused",false),"storage"));}
 for(size_t size=1;size<bytes.size();++size){endpointNvs["records2"]=std::vector<uint8_t>(bytes.begin(),bytes.begin()+size);assert(!storeAuthenticate("one",a));}
 endpointNvs["records2"]=bytes;
 for(size_t i=2;i<8;++i){std::string n=std::to_string(i);assert(!storeUpsert(raw,"192.0.2.1",8765,("token"+n).c_str(),("instance"+n).c_str(),peer,("namespace"+n).c_str(),true));}
 assert(storeCount()==8);auto full=endpointNvs["records2"];assert(!strcmp(storeUpsert(raw,"192.0.2.1",8765,"nine","instance9",peer,"namespace9",true),"capacity"));assert(endpointNvs["records2"]==full);
 endpointNvs.clear();
 for(size_t i=0;i<8;++i){std::string unique(1,char('a'+i));assert(!storeUpsert(std::string(21,'m').c_str(),std::string(64,'h').c_str(),8765,(std::string(127,'t')+unique).c_str(),(std::string(63,'p')+unique).c_str(),std::string(21,'i').c_str(),(std::string(20,'n')+unique).c_str(),true));storeSetBssid(std::string(21,'m').c_str(),std::string(21,'b').c_str());}
 assert(endpointNvs["records2"].size()==2856);std::cout<<"BRS2 full field upper bound="<<endpointNvs["records2"].size()<<" bytes\n";
 std::cout<<"BRS2: legacy atomic migration, peer+principal dual proof, same-peer instances, corruption/truncation, capacity no eviction passed; typical="<<bytes.size()<<" full="<<full.size()<<" bytes\n";
}
