#pragma once
#include <Arduino.h>
#include <stdint.h>
#include <stddef.h>
#include <array>
#include <vector>
#include <algorithm>
#include <cassert>

// Fake only the bus. Tests compile the same external_rtc.cpp as the ROM.
class TwoWire {
public:
    std::array<uint8_t, 256> registers{};
    std::vector<uint8_t> tx, rx;
    size_t bufferSize = 0, rxAt = 0;
    uint8_t pointer = 0;
    int operation = 0, failOperation = 0, shortOperation = 0;
    bool initOkay = true, bufferOkay = true, corruptCalendar = false;
    bool commitOnFailure = false, partialCalendar = false, shortWrite = false;
    int sda = -1, scl = -1, timeout = 0, frequency = 0;
    void (*onRead)() = nullptr;
    // Match the installed ESP32 Arduino Wire implementation: FIFO minimum is 32.
    size_t setBufferSize(size_t size) { return bufferSize = bufferOkay && size >= 32 ? size : 0; }
    void setTimeOut(uint16_t ms) { timeout = ms; }
    bool begin(int a, int b, uint32_t hz) { sda=a; scl=b; frequency=hz; return initOkay; }
    void beginTransmission(uint8_t address) { assert(address == 0x51); tx.clear(); }
    size_t write(uint8_t byte) { tx.push_back(byte); return 1; }
    size_t write(const uint8_t *bytes, size_t n) {
        if (shortWrite && n==7) --n;
        tx.insert(tx.end(), bytes, bytes+n); return n;
    }
    uint8_t endTransmission(bool = true) {
        ++operation;
#if defined(CODEX_TARGET_NOTE4)
        if (!rtcTestPowerLevel) return 2; // manufacturer requires GPIO42 high on I2C access
#endif
        bool failed = operation == failOperation;
        if (failed && !commitOnFailure) return 2;
        assert(!tx.empty()); pointer=tx[0];
        size_t count = tx.size()-1;
        if (count == 7 && partialCalendar) count=3;
        for (size_t i=0;i<count;++i) registers[pointer+i]=tx[i+1];
        if (tx.size()==8 && corruptCalendar) registers[pointer+1]^=1;
        return failed ? 2 : 0;
    }
    uint8_t requestFrom(uint8_t address, uint8_t count, bool) {
        assert(address == 0x51); ++operation; rx.clear(); rxAt=0;
#if defined(CODEX_TARGET_NOTE4)
        if (!rtcTestPowerLevel) return 0;
#endif
        if (onRead) { auto callback=onRead; onRead=nullptr; callback(); }
        if (operation == failOperation) return 0;
        if (operation == shortOperation) --count;
        for (int i=0;i<count;++i) rx.push_back(registers[pointer+i]);
        return count;
    }
    int available() { return (int)(rx.size()-rxAt); }
    int read() { return rxAt<rx.size() ? rx[rxAt++] : -1; }
};
extern TwoWire Wire;
