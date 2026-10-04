#include "external_rtc.h"
#include <Arduino.h>
#include <Wire.h>
#include <cassert>
#include <cstdlib>
#include <cstring>
#include <iostream>
TwoWire Wire;
#if defined(CODEX_TARGET_NOTE4)
constexpr int secondsReg=2;
#else
constexpr int secondsReg=4;
#endif
static time_t epoch(int year, int month, int day, int h=0, int m=0, int s=0) {
    struct tm value={}; value.tm_year=year-1900; value.tm_mon=month-1;
    value.tm_mday=day; value.tm_hour=h; value.tm_min=m; value.tm_sec=s;
    return timegm(&value);
}
static void reset() { Wire=TwoWire(); }
static void seed(time_t utc) { ExternalRtc rtc; assert(rtc.write(utc)); Wire.operation=0; }
int main() {
#if defined(CODEX_TARGET_NOTE4)
    for (int previous : {LOW, HIGH}) {
        rtcTestPowerLevel=previous; reset(); ExternalRtc powered;
        assert(powered.write(epoch(2026,10,4)) && rtcTestPowerLevel==previous);
        time_t observed=0; assert(powered.read(observed) && rtcTestPowerLevel==previous);
        Wire.shortOperation=Wire.operation+2;
        assert(!powered.read(observed) && rtcTestPowerLevel==previous);
    }
    rtcTestPowerLevel=LOW;
#endif
    const time_t now=epoch(2026,10,4,23,59,58);
    for (const char *tz : {"UTC0", "CST-8", "EST5EDT"}) {
        setenv("TZ",tz,1);tzset();reset();ExternalRtc rtc;
        assert(rtc.write(now)); time_t observed=0;assert(rtc.read(observed)&&observed==now);
        assert(Wire.sda==47 && Wire.scl==48 && Wire.frequency==100000);
        assert(Wire.timeout==25 && Wire.bufferSize==32);
        assert(Wire.registers[secondsReg]==0x58);
        assert(Wire.registers[secondsReg+2]==0x23); // UTC, never timezone-adjusted
        // A new boot reads chip state, independent of CPU uptime/RTC RAM.
        ExternalRtc rebooted;assert(rebooted.read(observed)&&observed==now);
        // Chip advances while CPU sleeps: no old internal-RC delta can win.
        Wire.registers[secondsReg]=0x59;assert(rebooted.read(observed)&&observed==now+1);
    }
    for (time_t utc : {epoch(2024,2,29,23,59,59),epoch(2024,3,1),
                      epoch(2026,12,31,23,59,59),epoch(2027,1,1),
                      epoch(2038,1,19,3,14,8),epoch(2099,12,31,23,59,59)}) {
        reset();ExternalRtc rtc;assert(rtc.write(utc));time_t observed=0;
        assert(rtc.read(observed)&&observed==utc);
    }
    // Invalid read must preserve the caller's fallback time.
    for (int invalid=0;invalid<12;++invalid) {
        reset();seed(now);
        switch(invalid) {
        case 0: Wire.registers[secondsReg]|=0x80;break; // OS/VL
        case 1: Wire.registers[0]|=0x20;break; // STOP
        case 2: Wire.registers[0]|=0x80;break; // TEST
        case 3: Wire.registers[secondsReg]=0x6A;break; // bad BCD
        case 4: Wire.registers[secondsReg+1]=0x60;break;
        case 5: Wire.registers[secondsReg+2]=0x24;break;
        case 6: Wire.registers[secondsReg+3]=0;break;
        case 7: Wire.registers[secondsReg+3]=0x31;Wire.registers[secondsReg+5]=0x04;break;
        case 8: Wire.registers[secondsReg+3]=0x29;Wire.registers[secondsReg+5]=0x02;
                Wire.registers[secondsReg+6]=0x23;break; // non-leap February
        case 9: Wire.registers[secondsReg+4]=7;break;
        case 10: Wire.registers[secondsReg+5]=0x13;break;
        case 11: Wire.registers[secondsReg+6]=0x00;break; // uninitialized year
        }
        ExternalRtc rtc;time_t fallback=123;assert(!rtc.read(fallback)&&fallback==123&&!rtc.valid());
    }
#if defined(CODEX_TARGET_NOTE4)
    reset();seed(now);Wire.registers[secondsReg+5]|=0x80;
    ExternalRtc century;time_t fallback=123;assert(!century.read(fallback));
    reset();seed(now);Wire.registers[0]|=0x08;ExternalRtc testc;assert(!testc.read(fallback));
#else
    reset();seed(now);Wire.registers[0]|=0x02;ExternalRtc hour12;time_t fallback=123;
    assert(!hour12.read(fallback)); // not silently interpreting 12h as 24h
#endif
    reset();Wire.initOkay=false;ExternalRtc init;assert(!init.read(fallback));
    reset();Wire.bufferOkay=false;ExternalRtc allocation;assert(!allocation.read(fallback));
    for (int op : {1,2}) {reset();seed(now);Wire.failOperation=op;
        ExternalRtc rtc;assert(!rtc.read(fallback)&&fallback==123);}
    reset();seed(now);Wire.shortOperation=2;ExternalRtc shortRead;
    assert(!shortRead.read(fallback)&&fallback==123);
    // NACK/lost ACK at every transaction: no success, old clock cannot undo a
    // trusted correction, and a later trusted retry restores a running clock.
    for (int op=1;op<=9;++op) for (bool lostAck : {false,true}) {
        reset();seed(now);Wire.failOperation=op;Wire.commitOnFailure=lostAck;
        ExternalRtc rtc;assert(!rtc.write(now+60)&&!rtc.valid());
        int operations=Wire.operation;assert(!rtc.read(fallback));assert(Wire.operation==operations);
        Wire.failOperation=0;assert(rtc.write(now+60));time_t observed=0;
        assert(rtc.read(observed)&&observed==now+60);
    }
    reset();seed(now);Wire.shortWrite=true;ExternalRtc shortWrite;
    assert(!shortWrite.write(now+60)&&!shortWrite.valid());
    ExternalRtc afterShortWrite;assert(!afterShortWrite.read(fallback));
    Wire.shortWrite=false;assert(shortWrite.write(now+60));
    for (bool partial : {false,true}) {
        reset();seed(now);Wire.corruptCalendar=!partial;Wire.partialCalendar=partial;
        ExternalRtc rtc;assert(!rtc.write(now+60));assert(Wire.registers[0]&0x20);
        ExternalRtc rebooted;assert(!rebooted.read(fallback)); // torn write remains stopped
        Wire.corruptCalendar=Wire.partialCalendar=false;assert(rtc.write(now+60));
    }
    // Differential sweep against libc UTC: every day in the supported range,
    // including all leap/year/month boundaries, not a copy of the decoder.
    reset();ExternalRtc sweep;
    for (time_t utc=epoch(2020,9,14);utc<epoch(2100,1,1);utc+=86400) {
        assert(sweep.write(utc));time_t observed=0;assert(sweep.read(observed)&&observed==utc);
    }
    reset();seed(now);Wire.registers[0]|=0xA0;Wire.registers[secondsReg]|=0x80;
    ExternalRtc repair;assert(repair.write(now)); // known time repairs STOP/OS/VL/test
    assert((Wire.registers[0]&0xA0)==0);
    assert(repair.write(1600000001));time_t boundary=0;assert(repair.read(boundary)&&boundary==1600000001);
    assert(!repair.write(1600000000));
    assert(!repair.write(epoch(2000,1,1)));assert(!repair.write(epoch(2100,1,1)));
    std::cout<<repair.model()<<": UTC/calendar/integrity/cold+sleep restore/9-stage faults passed\n";
}
