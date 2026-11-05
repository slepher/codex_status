"""Exercise production BLE reassembly and compact header code on the host.

Run with --out pointing to a pre-created directory and --arduinojson src path.
Hardware notification delivery and allocator high-water marks remain device checks.
"""
import argparse
from pathlib import Path
import re
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--out', type=Path, required=True)
parser.add_argument('--arduinojson', type=Path, required=True)
args = parser.parse_args()
assert args.out.is_dir()
root = Path(__file__).resolve().parents[1]
source = (root / 'src/ble_bridge.cpp').read_text()

def function(name):
    match = re.search(r'(?:static )?(?:void|bool) ' + name + r'\([^;]*?\)\s*\{', source)
    assert match, name
    end, depth = match.end(), 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[match.start():end]

code = r'''
#include "Arduino.h"
#include "v2_command_envelope.h"
#include <cassert>
#include <mutex>
#include <vector>
static uint32_t nowMs=1, tplCtrlStartedMs=0, connectionGeneration=7;
static String tplCtrlBuf;
static std::recursive_mutex infoMutex;
static std::vector<String> replies, queue;
uint32_t millis() {return nowMs;}
void bleNotifyStatusQuiet(const String &bytes) {replies.push_back(bytes);}
void dispatchTemplateCtrl(const String &bytes) {queue.push_back(bytes);}
'''
code += '\n'.join(function(name) for name in (
    'bleReplyTransportError', 'appendCommandFrame', 'blePollCommandTimeout'))
code += r'''
#include <new>
struct V2BleMessage { String body, peer, identity; uint32_t generation; };
namespace firmware {
struct { void *v2BleQueue=(void*)1; } state;
static std::vector<V2BleMessage*> pending;
static bool secure=true;
bool blePeerIsBonded(){return secure;} bool blePeerIsEncrypted(){return secure;}
String blePeerAddress(){return "peer";} String blePeerIdentity(){return "identity";}
uint32_t bleConnectionGeneration(){return connectionGeneration;}
size_t uxQueueSpacesAvailable(void*){return 4-pending.size();}
constexpr int pdTRUE=1;
int xQueueSend(void*,V2BleMessage **message,int){if(pending.size()==4)return 0;pending.push_back(*message);return pdTRUE;}
'''
source = (root / 'src/firmware/transport.cpp').read_text()
code += function('handleBleV2Ctrl') + '\n}\n'
code += r'''
std::string frame(uint64_t number) {
    CompactFrameHeader h; h.op=2;h.requestNo=number;h.payloadLen=2;h.session[0]=9;
    uint8_t bytes[48];assert(compactFrameEncode(h,bytes));
    return std::string((char*)bytes,48)+"{}";
}
void rejected(uint64_t number) {
    assert(!replies.empty()); CompactFrameHeader h;
    auto &r=replies.back();
    assert(compactFrameDecode((const uint8_t*)r.c_str(),r.length(),h,512));
    assert(h.kind==1 && h.result==2 && h.requestNo==number);
    JsonDocument json;assert(!deserializeJson(json,r.c_str()+48));
    assert(json["error"]=="frame_invalid" && json["stage"]=="reassembly");
    assert(connectionGeneration==7);
}
int main() {
    auto valid=frame(1);
    assert(appendCommandFrame(tplCtrlBuf,valid)); assert(queue.size()==1);
    assert(!appendCommandFrame(tplCtrlBuf,frame(2).substr(0,48)));
    nowMs+=2001;blePollCommandTimeout();rejected(2);assert(!tplCtrlBuf.length());
    auto broken=frame(3);broken[0]=0;
    assert(!appendCommandFrame(tplCtrlBuf,broken));rejected(0);
    assert(!appendCommandFrame(tplCtrlBuf,frame(4).substr(0,48)));
    assert(!appendCommandFrame(tplCtrlBuf,std::string(8193,'x')));rejected(4);
    assert(appendCommandFrame(tplCtrlBuf,frame(5)));assert(queue.size()==2);
    // A frame timeout does not retire an already queued valid command.
    assert(queue.front()==String(valid.c_str(),valid.size()));
    for (unsigned i=0;i<4;++i) firmware::handleBleV2Ctrl(String(valid.c_str(),valid.size()));
    assert(firmware::pending.size()==4);
    firmware::handleBleV2Ctrl(String(valid.c_str(),valid.size()));
    CompactFrameHeader busy;
    auto &reply=replies.back();
    assert(compactFrameDecode((const uint8_t*)reply.c_str(),reply.length(),busy,512));
    JsonDocument doc;assert(!deserializeJson(doc,reply.c_str()+48));
    assert(doc["error"]=="device_busy" && doc["stage"]=="queue");
    assert(firmware::pending.size()==4);
    firmware::secure=false;firmware::handleBleV2Ctrl(String(valid.c_str(),valid.size()));
    doc.clear();assert(!deserializeJson(doc,replies.back().c_str()+48));assert(doc["error"]=="ble_auth_required");
    for(auto *message:firmware::pending)delete message;
    for (uint16_t op=1;op<=35;++op) {
        bool expected=op==1||op==7||op==14||op==16||op==21||op==25||op==26||op==27||op==28||op==31||op==33||op==34||op==35;
        assert(isIdempotentReadOp(op)==expected);
    }
}
'''
test = args.out / 'protocol-p0.cpp'
test.write_text(code)
binary = args.out / 'protocol-p0'
subprocess.run(['c++', '-std=c++17', '-DARDUINOJSON_ENABLE_ARDUINO_STRING=1',
                '-I'+str(root/'bridge/crates/render/shim'), '-I'+str(root/'src'),
                '-I'+str(args.arduinojson), str(test), str(root/'src/v2_command_envelope.cpp'),
                '-o', str(binary)], check=True)
subprocess.run([str(binary)], check=True)
print('PASS: production BLE timeout/overflow/corrupt-header rejection, queued-frame preservation and read-op classification')
