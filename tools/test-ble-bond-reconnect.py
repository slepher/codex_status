"""Exercise actual connection/authentication/poll callbacks with a bond-store HAL."""
import pathlib,re,subprocess
root=pathlib.Path(__file__).resolve().parents[1]
s=(root/'src/ble_bridge.cpp').read_text()
def method(signature):
 a=s.index(signature);i=s.index('{',a)+1;depth=1
 while depth:
  depth+=(s[i]=='{')-(s[i]=='}');i+=1
 return s[a:i].replace(' override','')
assert 'NimBLEDevice::isBonded(info_->getIdAddress())' in s
source=r'''
#include <cassert>
#include <mutex>
#include <string>
#include <cstdio>
using String=std::string;
struct Peer {bool liveBond, stored, encrypted;};
struct PeerRef {
 Peer &p; PeerRef(Peer& v):p(v){} bool bonded(){return p.liveBond;}
 bool storedBond(){return p.stored;} bool encrypted(){return p.encrypted;}
 bool secure(){return p.liveBond&&p.encrypted;} int handle(){return 1;}
 String address(){return "test-peer";} String identity(){return "test-id/0";}
};
#define PEER_ARG Peer &desc
struct NimBLEServer {int drops=0; void disconnect(int){++drops;}} server;
static NimBLEServer *bleServer=&server;
static bool connected,peerBonded,peerEncrypted,disconnecting,windowOpen;
static int peerConnHandle,connectionGeneration;
static String peerAddress,peerIdentity;
static std::recursive_mutex infoMutex;
static bool blePairingWindowOpen(){return windowOpen;}
static void clearReceiveBuffers(){} static void refreshInfo(){}
struct Logger {void println(const char*){} template<class...T>void printf(const char*,T...) {}} DevLog;
struct Callbacks {
'''+method('void onConnect(NimBLEServer *server, PEER_ARG)')+'\n'+method('void onAuthenticationComplete(PEER_ARG)')+'\n};\n'+method('void blePoll()')+r'''
int main(){
 Callbacks c;
 for(bool known:{false,true})for(bool window:{false,true}){
  server.drops=0;windowOpen=window;Peer peer{false,known,false};
  c.onConnect(&server,peer);
  assert(server.drops==(!known&&!window));
  assert(!peerEncrypted);
  blePoll();assert(server.drops==(!known&&!window));
  if(known||window){
   peer.liveBond=true;peer.encrypted=true;c.onAuthenticationComplete(peer);
   assert(peerBonded&&peerEncrypted&&!disconnecting);
   windowOpen=false;blePoll();assert(server.drops==0);
  }
 }
 // Persisted identity never substitutes for successful encrypted authentication.
 windowOpen=false;server.drops=0;Peer bad{false,true,false};
 c.onConnect(&server,bad);c.onAuthenticationComplete(bad);assert(server.drops==1);
 windowOpen=true;server.drops=0;Peer pending{false,false,false};
 c.onConnect(&server,pending);windowOpen=false;blePoll();assert(server.drops==1);
 puts("PASS: stored-bond reconnect, new-peer window and encrypted-auth rejection");
}
'''
out=root/'artifacts/bridge-macos';cpp=out/'ble-bond-reconnect.cpp';exe=out/'ble-bond-reconnect';cpp.write_text(source)
subprocess.run(['c++','-std=c++17',str(cpp),'-o',str(exe)],check=True)
subprocess.run([str(exe)],check=True)
