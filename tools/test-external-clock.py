#!/usr/bin/env python3
"""Test exact firmware clock helpers + production driver at a controlled bus seam.
This covers helper ordering, not Arduino's scheduler/SNTP/Wi-Fi or real RF.
"""
from pathlib import Path
import subprocess
import os
import sys
repo = Path(__file__).resolve().parents[1]
out = repo / 'artifacts/external-rtc'
out.mkdir(parents=True, exist_ok=True)
source = (repo / 'src/firmware/clock.cpp').read_text()
start = source.index('static void writeExternalRtcFromSystem() {')
end = source.index('void writeClockStatus(', start)
helpers = source[start:end]
# Negative control models the old pending-only handoff without production edits.
if '--pending-only' in sys.argv:
    start = helpers.index('void ntpTimeSynced(')
    end = helpers.index('bool readExternalRtc(', start)
    helpers = helpers[:start] + '''void ntpTimeSynced(struct timeval *) { ntpRtcPending=true; }
void flushNtpRtcSync() {
    if(!ntpRtcPending) return;
    ntpRtcPending=false;timeSource=TIME_NTP;writeExternalRtcFromSystem();
}
''' + helpers[end:]
preamble = r'''
#include "external_rtc.h"
#include <Wire.h>
#include <sys/time.h>
#include <cassert>
#include <mutex>
#include <cstdio>
#include <iostream>
TwoWire Wire;
static uint64_t virtualNow=0, wallUs=1700000000000000ULL, wallAt=0;
static time_t testTime(time_t *) {return (wallUs+virtualNow-wallAt)/1000000ULL;}
static int testSettimeofday(const timeval *tv, const void *) {
    wallUs=(uint64_t)tv->tv_sec*1000000ULL+tv->tv_usec;wallAt=virtualNow;return 0;
}
static int64_t esp_timer_get_time() {return virtualNow;}
static bool timeKnown() {return testTime(nullptr)>1600000000;}
struct Log {void printf(const char *,...) {}} DevLog;
enum {TIME_NONE=0,TIME_BLE=1,TIME_RTC=2,TIME_EXTERNAL_RTC=3,TIME_NTP=4};
struct ClockState {
    uint8_t timeSource=TIME_NONE;
    ExternalRtc externalRtc;
    uint64_t externalRtcLastPollMs=0;
} state;
static uint8_t &timeSource=state.timeSource;
static ExternalRtc &externalRtc=state.externalRtc;
static std::mutex ntpRtcMux;
static bool ntpRtcPending=false;
static uint64_t ntpRtcEpochUs=0,ntpRtcMonotonicUs=0,externalRtcLastPollMs=0;
static void portENTER_CRITICAL(std::mutex *m) {m->lock();}
static void portEXIT_CRITICAL(std::mutex *m) {m->unlock();}
#define time testTime
#define settimeofday testSettimeofday
'''
tests = r'''
static timeval ntpSample{1700003600,123456};
static void ntpDuringRead() {
    testSettimeofday(&ntpSample,nullptr); // lwIP sets time before callback
    int operations=Wire.operation;ntpTimeSynced(&ntpSample);
    assert(Wire.operation==operations); // no I2C on TCP/IP callback
}
int main() {
    assert(externalRtc.write(1700000000));
    timeval wrong{1700000000-3600,0};testSettimeofday(&wrong,nullptr);
    assert(readExternalRtc(true)&&testTime(nullptr)==1700000000);
    assert(timeSource==TIME_EXTERNAL_RTC);
    timeval phase{1700000000,654321};testSettimeofday(&phase,nullptr);
    assert(readExternalRtc(false)&&wallUs%1000000==654321);
    // New NTP arrives during RTC read; the older RTC corrects system time,
    // then rendering/network work delays loop's NTP handoff by 30 seconds.
    Wire.onRead=ntpDuringRead;assert(readExternalRtc(false));
    assert(testTime(nullptr)==1700000000);
    virtualNow+=30000000;flushNtpRtcSync();
    assert(testTime(nullptr)==1700003630);
    assert(wallUs%1000000==123456 && timeSource==TIME_NTP);
    time_t chip=0;assert(externalRtc.read(chip)&&chip==1700003630);
    int operations=Wire.operation;flushNtpRtcSync();assert(Wire.operation==operations);
    Wire.registers[2]|=0x80;time_t before=testTime(nullptr);
    assert(!readExternalRtc(true)&&testTime(nullptr)==before);
    auto mono=virtualNow;Wire.registers[2]&=0x7f;
    assert(readExternalRtc(true)&&virtualNow==mono);
    std::cout<<"firmware clock helpers: cold restore/phase/fallback/NTP race+30s delay/monotonic isolation passed\n";
}
'''
name = 'test-clock-pending-only' if '--pending-only' in sys.argv else 'test-clock'
cpp=out/(name+'.cpp');cpp.write_text(preamble+helpers+tests)
subprocess.run([os.environ.get('CXX','clang++'),'-std=c++17','-Wall','-Wextra','-Werror',
    '-Wno-unused-variable','-Wno-unused-function','-g','-fsanitize=address,undefined','-DCODEX_TARGET_NOTE4=1',
    '-I'+str(repo/'src'),'-I'+str(repo/'tools/rtc-test-stubs'),str(cpp),
    str(repo/'src/external_rtc.cpp'),'-o',str(out/name)],check=True)
result=subprocess.run([str(out/name)])
raise SystemExit(result.returncode)
