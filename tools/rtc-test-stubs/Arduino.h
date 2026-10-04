#pragma once
#include <stdint.h>
constexpr int OUTPUT=1, HIGH=1, LOW=0;
inline int rtcTestPowerLevel=0;
inline void pinMode(int, int) {}
inline int digitalRead(int pin) { return pin==42 ? rtcTestPowerLevel : 0; }
inline void digitalWrite(int pin,int level) { if(pin==42) rtcTestPowerLevel=level; }
