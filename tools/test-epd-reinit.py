#!/usr/bin/env python3
"""Replay thin -> full init with production GPIO/SPI detach code and a lock HAL.

No panel/RF simulation: a recursive take of the SPI bus mutex raises instead
of waiting forever. Run --expect-red to replay the original misplaced guard.
"""
import argparse
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--expect-red', action='store_true')
args = parser.parse_args()
core = ROOT / '.pio-pkgs/note4/framework-arduinoespressif32/cores/esp32'


def function(source, name):
    match = re.search(r'(?:static )?(?:void|bool|UBYTE)\s+' + name + r'\([^;{}]*\)\s*\{', source)
    assert match, name
    end, depth = match.end(), 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[match.start():end]


dev = (ROOT / 'src/DEV_Config.cpp').read_text()
init = function(dev, 'DEV_Module_Init')
if args.expect_red:
    guard = 'static bool spiReady = false;\n    if (spiReady) return 0;'
    assert guard in init
    init = init.replace(guard, '')
    init = init.replace('\tspiReady = true;', guard + '\n\tspiReady = true;')
spi = (core / 'esp32-hal-spi.c').read_text()
gpio = (core / 'esp32-hal-gpio.c').read_text()
# Keep the real pinMode ownership/deinit prefix; register programming is HAL.
pin_mode = gpio[gpio.index('extern void ARDUINO_ISR_ATTR __pinMode('):]
pin_mode = pin_mode[:pin_mode.index('  gpio_hal_context_t gpiohal;')] + '}\n'
cpp = (core.parent.parent / 'libraries/SPI/src/SPI.cpp').read_text()
assert 'SPI_MUTEX_LOCK();' in function(spi, 'spiTransaction')
assert 'SPI_PARAM_LOCK();' in function(cpp, 'SPIClass::beginTransaction')
code = r'''
#include <cassert>
#include <cstdint>
#include <cstdio>
#include <stdexcept>
#define ARDUINO_ISR_ATTR
#define SOC_GPIO_PIN_COUNT 49
#define D_9PIN 1
#define EPD_PWR_PIN 6
#define EPD_BUSY_PIN 8
#define EPD_RST_PIN 9
#define EPD_DC_PIN 10
#define EPD_CS_PIN 11
#define EPD_SCK_PIN 12
#define EPD_MOSI_PIN 13
#define INPUT 0
#define OUTPUT 1
#define HIGH 1
#define LOW 0
#define GPIO_PIN_RESET 0
#define GPIO_PIN_SET 1
#define SPI_MODE0 0
#define MSBFIRST 1
#define log_e(...) ((void)0)
#define log_d(...) ((void)0)
#define SPI_MISO_IDX(x) (x)
using UBYTE = uint8_t;
struct Device { struct { int val=1; } clock; } device;
struct spi_t { Device *dev=&device; int sck=-1,miso=-1,mosi=-1,ss=-1,num=0; };
static spi_t _spi_bus_array[1];
static bool locked=false;
static int stopCalls=0, serialBegins=0, gpioCalls=0, transfers=0;
enum { ESP32_BUS_TYPE_GPIO, ESP32_BUS_TYPE_SPI_MASTER_SCK, ESP32_BUS_TYPE_SPI_MASTER_MOSI };
static int owner[49];
static bool (*deinit[3])(void *);
static bool spiDetachSCK(spi_t *),spiDetachMISO(spi_t *),spiDetachMOSI(spi_t *),spiDetachSS(spi_t *);
static bool gpioDetachBus(void *) {return true;}
static void perimanSetBusDeinit(int type,bool (*cb)(void *)) {deinit[type]=cb;}
static void *perimanGetPinBus(int pin,int type) {return owner[pin]==type ? (void *)1 : nullptr;}
static bool perimanClearPinBus(int pin) {
  if(owner[pin]!=ESP32_BUS_TYPE_GPIO) deinit[owner[pin]]((void *)1);
  owner[pin]=ESP32_BUS_TYPE_GPIO;
  return true;
}
static void pinMatrixOutDetach(int,bool,bool) {}
static void pinMatrixInDetach(int,bool,bool) {}
static void removeApbChangeCallback(spi_t *,int) {}
static int _on_apb_change;
static void takeLock() {
  if(locked) throw std::runtime_error("SPI bus mutex self-deadlock in spiStopBus");
  locked=true;
}
#define SPI_MUTEX_LOCK() takeLock()
#define SPI_MUTEX_UNLOCK() (locked=false)
static void spiInitBus(spi_t *spi) {++stopCalls;spi->dev->clock.val=0;}
''' + function(spi, 'spiStopBus') + '\n'
for name in ['spiDetachBus', 'spiDetachBus_SCK', 'spiDetachBus_MOSI',
             'spiDetachSCK', 'spiDetachMISO', 'spiDetachMOSI', 'spiDetachSS']:
    # ESP32 has 32-bit pointers; preserve the bus-index cast on a 64-bit host.
    code += function(spi, name).replace('(int)bus', '(intptr_t)bus') + '\n'
code += pin_mode + r'''
static void pinMode(int pin,int mode) {++gpioCalls;__pinMode(pin,mode);}
static void digitalWrite(int,int) {}
static void delay(int) {}
struct {void begin(int) {++serialBegins;}} Serial;
struct SPISettings {SPISettings(int,int,int) {}};
struct {
  void begin(int sck,int,int mosi,int) {
    auto &bus=_spi_bus_array[0];bus.sck=sck;bus.mosi=mosi;
    deinit[ESP32_BUS_TYPE_SPI_MASTER_SCK]=spiDetachBus_SCK;
    deinit[ESP32_BUS_TYPE_SPI_MASTER_MOSI]=spiDetachBus_MOSI;
    owner[sck]=ESP32_BUS_TYPE_SPI_MASTER_SCK;owner[mosi]=ESP32_BUS_TYPE_SPI_MASTER_MOSI;
  }
  void beginTransaction(SPISettings) {takeLock();}
  void transfer(uint8_t) {
    assert(locked && owner[EPD_SCK_PIN]==ESP32_BUS_TYPE_SPI_MASTER_SCK);
    assert(owner[EPD_MOSI_PIN]==ESP32_BUS_TYPE_SPI_MASTER_MOSI);++transfers;
  }
} SPI;
''' + function(dev, 'GPIO_Config') + '\n' + init + '\n' + function(dev, 'DEV_SPI_WriteByte') + r'''
int main() {
  try {
    assert(DEV_Module_Init()==0); // epdThinBegin, before the minute write
    DEV_SPI_WriteByte(0x24);
    int configured=gpioCalls;
    assert(DEV_Module_Init()==0); // epdBegin, same network-due wake
    assert(gpioCalls==configured && serialBegins==1 && stopCalls==0);
    for(int i=0;i<100;i++) {assert(DEV_Module_Init()==0);DEV_SPI_WriteByte(i);}
    assert(transfers==101 && stopCalls==0);
    puts("PASS: thin -> full -> repeated init keeps SPI pin ownership and held transaction");
  } catch(const std::runtime_error &error) {puts(error.what());return 2;}
}
'''
out = ROOT / 'artifacts/bridge-macos'
source = out / 'epd-reinit-host.cpp'
exe = out / 'epd-reinit-host'
source.write_text(code)
subprocess.run(['c++', '-std=c++17', '-fsanitize=address,undefined', str(source), '-o', str(exe)], check=True)
result = subprocess.run([str(exe)])
if args.expect_red:
    assert result.returncode == 2, result.returncode
    print('PASS: original initialization reproduces the mutex self-deadlock')
else:
    assert result.returncode == 0, result.returncode
