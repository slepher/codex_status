"""Run production builtin-page drawing with recorded paint calls (WSL g++).

Run elevated; --out must already exist under the project's directory policy.
"""
import argparse
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--out', type=Path, required=True)
args = parser.parse_args()
repo = Path(__file__).resolve().parents[1]
assert args.out.is_dir()
source = (repo / 'src/firmware/display.cpp').read_text(encoding='utf-8')

def function(signature):
    start = source.index(signature)
    brace = source.index('{', start)
    depth = 1
    end = brace + 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[start:end]

code = r'''
#include "Arduino.h"
#include "network_display_state.h"
#include <cassert>
#include <vector>
#include <iostream>
using UBYTE = unsigned char;
constexpr int WHITE=1, BLACK=0, DOT_PIXEL_1X1=1, DRAW_FILL_EMPTY=0, MODE_DEEP=1;
int EPD_W=200, EPD_H=200;
struct Font { int Width, Height; } Font12{7,12}, Font16{11,16};
struct Draw { int x,y; std::string text; Font font; };
std::vector<Draw> calls;
struct { UBYTE *frame = (UBYTE*)1; bool builtinScreen=false; } state;
namespace radio {
struct { bool bleOn=false, v2InRendezvous=false; } state;
WifiPhase phase=WifiPhase::Idle;
WifiPhase wifiPhase() { return phase; }
}
namespace power { struct { int rtcMode=0; } retained; }
void Paint_SelectImage(UBYTE*) {}
void Paint_Clear(int) { calls.clear(); }
void Paint_DrawRectangle(int,int,int,int,int,int,int) {}
void Paint_DrawString_EN(int x,int y,const char *text,Font *font,int,int) {
    assert(x>=0 && y>=0 && x+int(strlen(text))*font->Width<=EPD_W);
    assert(y+font->Height<=EPD_H);
    calls.push_back({x,y,text,*font});
}
void epdFlush() {}
'''
code += function('static std::vector<String> wrapText(const String &s, size_t maxChars)')
code += '\n'
# The production declaration supplies this default.
code = code.replace('size_t maxChars)', 'size_t maxChars = 17)')
for signature in ('static bool builtinBleOn()', 'static void drawBuiltinRadioStatus(int x, int y)',
                  'void screen(const std::vector<String> &lines, UBYTE color)'):
    code += '\n' + function(signature)
code += r'''
int main() {
    for (int width : {200,400}) {
        EPD_W=width; EPD_H=width==200?200:300;
        for (auto phase : {WifiPhase::Idle,WifiPhase::Connecting,WifiPhase::Connected,WifiPhase::Off}) {
            radio::phase=phase;
            for (bool ble : {false,true}) {
                radio::state.bleOn=ble;
                screen({"CONTENT UPDATE","ct_bind_unsupported",String(std::string(500,'x'))},BLACK);
                assert(state.builtinScreen && calls.size()>=3);
                auto wifi=calls[calls.size()-2], bt=calls.back();
                assert(wifi.text.rfind("WIFI ",0)==0 && bt.text==(ble?"BLE on":"BLE off"));
                for (size_t i=0;i+2<calls.size();++i) assert(calls[i].y+calls[i].font.Height<=wifi.y);
                // The two builtin usage-page status rows fit beside the hero block.
                calls.clear(); drawBuiltinRadioStatus(EPD_W-4-12*Font12.Width,64);
                assert(calls.front().x>108 && calls.back().y+Font12.Height<=94);
            }
        }
        power::retained.rtcMode=MODE_DEEP;
        radio::state.bleOn=true; radio::phase=WifiPhase::Connected;
        screen({"CONTENT REQUIRED"},BLACK);
        assert(calls[calls.size()-2].text=="WIFI sleep" && calls.back().text=="BLE off");
        radio::state.v2InRendezvous=true;
        screen({"CONTENT REQUIRED"},BLACK);
        assert(calls[calls.size()-2].text=="WIFI online" && calls.back().text=="BLE on");
        radio::state.v2InRendezvous=false; power::retained.rtcMode=0;
    }
    std::cout << "PASS builtin error/default: radio phases, automatic BLE, sleep/wake, text bounds 200/400\n";
}
'''
cpp = args.out / 'builtin-radio-status.cpp'
cpp.write_text(code, encoding='utf-8')

def linux(path):
    path = path.resolve()
    return '/mnt/' + path.drive[0].lower() + path.as_posix()[2:]

for name, source_path, includes in (
    ('builtin-radio-status', cpp, [repo/'src', repo/'bridge/crates/render/shim']),
    ('wifi-metadata', repo/'tools/test-wifi-metadata.cpp', [repo/'src']),
):
    binary = args.out / name
    subprocess.run(['wsl','-d','Ubuntu-24.04','--','g++','-std=c++17','-Wall','-Wextra','-Werror',
                    *['-I'+linux(p) for p in includes],linux(source_path),'-o',linux(binary)],check=True)
    subprocess.run(['wsl','-d','Ubuntu-24.04','--',linux(binary)],check=True)
