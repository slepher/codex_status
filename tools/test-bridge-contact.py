"""Exercise the production owner-contact hook without hardware or wall-clock waits."""
import argparse
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--out', type=Path, required=True)
args = parser.parse_args()
assert args.out.is_dir()
root = Path(__file__).resolve().parents[1]
source = (root / 'src/firmware/radio.cpp').read_text()
start = source.index('void noteBridgeContact(')
end = source.index('\n}\n', start) + 2
code = r'''
#include "network_display_state.h"
#include <cassert>
#include <string>
namespace firmware {
using String = std::string;
struct OwnerRec { String id; };
bool owned = true, knownTime = true;
bool ownerGet(OwnerRec &owner) { owner.id="owner"; return owned; }
namespace clock { bool timeKnown() { return knownTime; } }
uint32_t now = 1700000000;
#define time(unused) now
struct { BridgeContactState bridgeContact; uint32_t rtcLastSyncEpoch=1700000000; } retained;
'''
code += source[start:end]
code += r'''
}
int main() {
    using namespace firmware;
    retained.bridgeContact.failed = true;
    noteBridgeContact("other");
    assert(retained.bridgeContact.epoch == 0 && retained.bridgeContact.failed);
    owned=false; noteBridgeContact("owner");
    assert(retained.bridgeContact.epoch == 0 && retained.bridgeContact.failed);
    owned=true;
    for (unsigned i=0; i<60; ++i) {
        now+=30; noteBridgeContact("owner");
        assert(retained.bridgeContact.offlineMinutes(now,retained.rtcLastSyncEpoch,true,6,-1)==-1);
        assert(retained.rtcLastSyncEpoch==1700000000);
    }
    now+=420;
    assert(retained.bridgeContact.offlineMinutes(now,retained.rtcLastSyncEpoch,true,6,-1)==7);
    retained.bridgeContact.failed=true; noteBridgeContact("owner");
    assert(!retained.bridgeContact.failed);
    const uint32_t previous=retained.bridgeContact.epoch;
    knownTime=false; noteBridgeContact("owner");
    assert(retained.bridgeContact.epoch==previous);
}
'''
path = args.out / 'bridge-contact-test.cpp'
path.write_text(code)
binary = args.out / 'bridge-contact-test'
subprocess.run(['c++', '-std=c++17', '-Wall', '-Wextra', '-I'+str(root/'src'), str(path), '-o', str(binary)], check=True)
subprocess.run([str(binary)], check=True)
print('Production contact hook: owner isolation, idle freshness, true loss, recovery and sync clock preservation passed')
