"""Replay production INFO updates with the installed Arduino String allocator.

Run on Linux/WSL with g++; pre-create --out with the repository ACL policy.
Only JSON/GATT are stubs; refreshInfo, setters and WString are production code.
"""
import argparse
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--out', type=Path, required=True)
parser.add_argument('--expect-red', action='store_true')
args = parser.parse_args()
assert args.out.is_dir(), 'pre-create output directory'
src = (ROOT / 'src/ble_bridge.cpp').read_text()

def function(name):
    m = re.search(r'(?:static )?(?:void|bool) ' + name + r'\([^;]*?\)\s*\{', src)
    assert m, name
    i = m.end(); depth = 1
    while depth:
        depth += (src[i] == '{') - (src[i] == '}')
        i += 1
    return src[m.start():i]

core = ROOT / '.pio-pkgs/note4/framework-arduinoespressif32/cores/esp32'
for name in ('WString.h', 'pgmspace.h', 'stdlib_noniso.h'):
    (args.out / name).write_text((core / name).read_text())
wstring = (core / 'WString.cpp').read_text()
wstring = wstring.replace('#include "Arduino.h"', '#include <stdio.h>')
wstring = wstring.replace('#include "esp32-hal-log.h"', '#define log_e(...)\n#define log_w(...)')
(args.out / 'WString.cpp').write_text(wstring)
declarations = re.search(r'static String\s+infoExtra;', src).group()
mutex = re.search(r'static std::recursive_mutex infoMutex;', src)
if mutex:
    declarations += '\n' + mutex.group()
snapshot = (ROOT / 'src/ble_wire.h').read_text()
snapshot = snapshot[snapshot.index('struct BleInfoSnapshot'):snapshot.index('struct BleStatusSnapshot')]
prefix = r'''
#include "WString.h"
#include <atomic>
#include <cassert>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <mutex>
#include <string>
#include <thread>
#include <vector>
static std::atomic<void*> watched{nullptr};
static std::atomic<int> watchedFrees{0};
static std::atomic<bool> secondStarted{false};
extern "C" void __real_free(void*);
extern "C" void __wrap_free(void* p) {
    if (p && p == watched.load()) {
        if (watchedFrees.fetch_add(1)) {
            std::puts("RED: two production INFO writers free the same Arduino String buffer");
            std::fflush(stdout); std::_Exit(86);
        }
        while (!secondStarted.load()) std::this_thread::yield();
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }
    __real_free(p);
}
struct JsonField {
    template<class T> T operator|(T fallback) { return fallback; }
    const char* operator|(const char* fallback) { return fallback; }
};
struct JsonDocument { JsonField operator[](const char*) { return {}; } };
static bool deserializeJson(JsonDocument&, const String& s) { return !s.length(); }
struct NimBLECharacteristic { int writes=0; };
static NimBLECharacteristic* infoChr=nullptr;
static String fwVersion="test";
static bool peerBonded=true, peerEncrypted=true;
static bool blePairingWindowOpen() { return false; }
static bool writeAttribute(NimBLECharacteristic* p,const String&,const char*) {
    if (!p) return false;
    ++p->writes; return true;
}
static void refreshInfo();
static bool advertising, connected, disconnecting;
static uint16_t peerConnHandle;
constexpr uint16_t BLE_HS_CONN_HANDLE_NONE=65535;
static NimBLECharacteristic* statusChr=nullptr;
static void* bleServer=nullptr;
struct Logger { void println(const char*) {} };
static Logger DevLog;
struct NimBLEDevice {
    static bool initialized;
    static bool isInitialized() { return initialized; }
    static void deinit(bool) {
        // Host shutdown drains a final callback on its own thread. Holding the
        // INFO lock across this join would deadlock the actual stop sequence.
        std::thread callback([] {refreshInfo();});callback.join();
        initialized=false;
    }
};
bool NimBLEDevice::initialized=true;
static void clearReceiveBuffers() { assert(!NimBLEDevice::initialized); }
'''
suffix = r'''
static std::atomic<int> seq{0};
static String provider() {
    std::string s(512 + (seq.fetch_add(1) % 4)*128, 'x');
    return String(s.c_str());
}
int main() {
    NimBLECharacteristic first;
    infoChr=&first;
    bleSetInfoProvider(provider);
    infoExtra=String(std::string(64,'a').c_str());
    watched=const_cast<char*>(infoExtra.c_str());
    std::thread auth([] {refreshInfo();});
    while (!watchedFrees.load()) std::this_thread::yield();
    std::thread reader([] {secondStarted=true;refreshInfo();});
    auth.join(); reader.join(); watched=nullptr;
    assert(watchedFrees==1);
    std::vector<std::thread> workers;
    for (int i=0;i<4;i++) workers.emplace_back([i] {
        for (int j=0;j<2000;j++) {
            if (i==0) bleSetInfoExtra(String(std::string(32+j%160,'q').c_str()));
            else refreshInfo();
        }
    });
    for (auto& t:workers) t.join();
    assert(first.writes>=8002);
    bleDeinit();
    int writes=first.writes;refreshInfo();assert(first.writes==writes);
    NimBLECharacteristic next;
    { std::lock_guard<std::recursive_mutex> lock(infoMutex); infoChr=&next; }
    refreshInfo();assert(next.writes==1);
    puts("PASS: production INFO auth/read/set interleaving, 8000 concurrent updates, detached and republished GATT");
}
'''
if not mutex:
    suffix = suffix.replace('std::lock_guard<std::recursive_mutex> lock(infoMutex);', '')
parts = [function(n) for n in ('refreshInfo', 'bleSetInfoProvider', 'bleSetInfoExtra')]
if args.expect_red:
    parts = [re.sub(r'\s*std::lock_guard<std::recursive_mutex> lock\(infoMutex\);', '', p) for p in parts]
source = prefix + snapshot + '\n' + declarations + '\nstatic String (*infoProvider)()=nullptr;\n'
source += 'static String bleBuildInfo(const BleInfoSnapshot&) {return String("{} ");}\n'
source += '\n'.join(parts) + function('bleDeinit') + suffix
(args.out / 'replay.cpp').write_text(source)
binary = args.out / 'replay'
subprocess.run(['g++', '-std=c++17', '-g', '-O1', '-pthread', '-fsanitize=address,undefined',
                '-ffunction-sections', '-fdata-sections', '-I'+str(args.out),
                str(args.out/'WString.cpp'), str(args.out/'replay.cpp'),
                '-Wl,--gc-sections', '-Wl,--wrap=free', '-o', str(binary)], check=True)
result = subprocess.run([str(binary)], capture_output=True, text=True, timeout=45)
(args.out / ('red.log' if args.expect_red else 'green.log')).write_text(result.stdout+result.stderr)
if args.expect_red:
    assert result.returncode==86, result.stdout+result.stderr
    print(result.stdout, end='')
else:
    assert result.returncode==0, result.stdout+result.stderr
    close = function('bleDeinit')
    assert close.index('infoChr = nullptr') < close.index('NimBLEDevice::deinit(true)')
    assert close.index('NimBLEDevice::deinit(true)') < close.index('clearReceiveBuffers()')
    assert re.search(r'infoChr = nullptr;[^}]*}\s*[^}]*NimBLEDevice::deinit', close, re.S), 'host stop holds INFO mutex'
    print(result.stdout, end='')
