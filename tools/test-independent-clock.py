#!/usr/bin/env python3
"""Production minute service + shared wake selection; fake panel/monotonic HAL.
Device-sim bootstrap additionally checks real compiled-template pixels across RF.
"""
from pathlib import Path
import subprocess
root = Path(__file__).resolve().parents[1]
out = root / 'artifacts/bridge-macos'
src = (root/'src/firmware/display.cpp').read_text()
a=src.index('void serviceMinuteClock() {'); b=src.index('\n#ifdef CODEX_CLK_WINDOW_TEST\nvoid clkTestTick()',a)
service=src[a:b]
app=(root/'src/firmware/application.cpp').read_text()
tick=app[app.index('void tick()'):]
assert tick.index('display::serviceMinuteClock()') < tick.index('transport::state.server.handleClient()') < tick.index('if (radio::state.v2InRendezvous)')
setup=app[:app.index('void tick()')]
assert setup.index('display::clockTickWake(); // failure') < setup.index('radio::v2RendezvousBegin(')
assert setup.index('display::serviceMinuteClock()') < setup.index('radio::v2RendezvousBegin(')
power=(root/'src/firmware/power.cpp').read_text()
assert 'nextDisplayWakeSeconds(now,' in power and 'if (sec < 1) sec = 1;' in power
code=r'''
#include "minute_clock.h"
#include "rendezvous_protocol.h"
#include <cassert>
#include <cstddef>
#include <cstdio>
static int64_t wall=1800000058, mono=0;
static int64_t fakeTime(void *) { return wall; }
#define time fakeTime
namespace clock { uint64_t v2NowMs() {return mono;} }
namespace platform {struct {bool activeTplHasNow=true,bundleMaintenance=false;} state;}
struct {int64_t displayedMinute=1800000058/60;} retained;
struct {int epdPartialCount=0;bool forceCleanRefresh=false;} state;
static int draws=0,failures=0;
static bool clockTickWake() {
 if(failures) {--failures;return false;}
 ++draws;retained.displayedMinute=wall/60;return true;
}
static void renderCurrent() {clockTickWake();}
'''+service+r'''
int main() {
 assert(nextDisplayWakeSeconds(1800000058,true,1800000077)==2);
 assert(nextDisplayWakeSeconds(1800000061,true,1800000077)==16);
 assert(nextDisplayWakeSeconds(1800000059,true,1800000077)==1);
 assert(nextDisplayWakeSeconds(1800000077,true,1800000077)==1);
 assert(nextDisplayWakeSeconds(1800000061,false,1800000077)==16);
 assert(displaySleepMicros(60000000,58750000)==1250000);
 assert(displaySleepMicros(60000000,60250000)==1000);
 assert(!minuteClockDue(0,-1));
 RendezvousProtocol rv;rv.begin(0,4000);rv.connected(0);
 const auto deadline=rv.deadline();
 serviceMinuteClock();assert(draws==0);
 mono=2000;wall+=2;serviceMinuteClock();
 assert(draws==1 && rv.active() && rv.deadline()==deadline);
 mono=3000;serviceMinuteClock();assert(draws==1);
 rv.tick(deadline);mono=deadline;serviceMinuteClock();assert(draws==1);
 wall-=60;mono+=60000;serviceMinuteClock();assert(draws==2); // correction
 wall+=60;mono+=60000;failures=2;serviceMinuteClock();assert(draws==2);
 serviceMinuteClock();assert(draws==2); // failed write not acknowledged, bounded retry
 mono+=1000;serviceMinuteClock();assert(draws==3);
 wall+=60;platform::state.activeTplHasNow=false;mono+=60000;serviceMinuteClock();assert(draws==3);
 puts("PASS: independent minute, :58 RF crossover, :17 contact, :59 sleep, correction, failure retry, no binding; production wiring checked");
}
'''
p=out/'independent-clock-host.cpp';p.write_text(code)
exe=out/'independent-clock-host'
subprocess.run(['c++','-std=c++17','-I'+str(root/'src'),str(p),str(root/'src/rendezvous_protocol.cpp'),'-o',str(exe)],check=True)
subprocess.run([str(exe)],check=True)
