#include <Arduino.h>
#include <SPI.h>

#define PIN_PWR  6
#define PIN_BUSY 8
#define PIN_RST  9
#define PIN_DC   10
#define PIN_CS   11
#define PIN_SCK  12
#define PIN_MOSI 13

#define C_BLACK  0
#define C_WHITE  1
#define C_YELLOW 2
#define C_RED    3

static void wcmd(uint8_t c) {
    digitalWrite(PIN_DC, 0);
    digitalWrite(PIN_CS, 0);
    SPI.transfer(c);
    digitalWrite(PIN_CS, 1);
}
static void wdat(uint8_t d) {
    digitalWrite(PIN_DC, 1);
    digitalWrite(PIN_CS, 0);
    SPI.transfer(d);
    digitalWrite(PIN_CS, 1);
}
static void waitBusyHigh() {
    uint32_t t0 = millis();
    while (digitalRead(PIN_BUSY) == 0) {
        delay(10);
        if (millis() - t0 > 30000) { Serial.println("[epd2] busy timeout"); return; }
    }
}
static void resetPanel() {
    digitalWrite(PIN_RST, 1);
    delay(200);
    digitalWrite(PIN_RST, 0);
    delay(20);
    digitalWrite(PIN_RST, 1);
    delay(200);
}
static void initPanel() {
    resetPanel();
    wcmd(0x4D); wdat(0x78);
    wcmd(0x00); wdat(0x0F); wdat(0x29);
    wcmd(0x06); wdat(0x0D); wdat(0x12); wdat(0x30); wdat(0x20); wdat(0x19); wdat(0x2A); wdat(0x22);
    wcmd(0x50); wdat(0x37);
    wcmd(0x61); wdat(200 / 256); wdat(200 % 256); wdat(200 / 256); wdat(200 % 256);
    wcmd(0xE9); wdat(0x01);
    wcmd(0x30); wdat(0x08);
    wcmd(0x04); waitBusyHigh();
}
static void displayBuf(const uint8_t *buf, size_t len) {
    wcmd(0x10);
    digitalWrite(PIN_DC, 1);
    digitalWrite(PIN_CS, 0);
    size_t off = 0;
    while (off < len) {
        size_t chunk = (len - off) > 1024 ? 1024 : (len - off);
        SPI.transferBytes(buf + off, nullptr, chunk);
        off += chunk;
    }
    digitalWrite(PIN_CS, 1);
    wcmd(0x12); wdat(0x00); waitBusyHigh();
}

void setup() {
    pinMode(PIN_PWR, OUTPUT);
    digitalWrite(PIN_PWR, HIGH);
    delay(500);
    digitalWrite(PIN_PWR, LOW);
    delay(200);
    pinMode(42, OUTPUT);       digitalWrite(42, LOW);
    pinMode(17, OUTPUT);       digitalWrite(17, HIGH);
    pinMode(PIN_RST, OUTPUT);
    pinMode(PIN_DC, OUTPUT);
    pinMode(PIN_CS, OUTPUT);
    pinMode(PIN_BUSY, INPUT);
    digitalWrite(PIN_CS, 1);
    delay(20);

    Serial.begin(115200);
    delay(300);
    Serial.println();
    Serial.println("[epd2] start (hw spi, 20MHz, reset 20ms)");

    SPI.begin(PIN_SCK, -1, PIN_MOSI, -1);
    SPI.beginTransaction(SPISettings(20000000, MSBFIRST, SPI_MODE0));

    initPanel();
    Serial.println("[epd2] init done");

    static uint8_t buf[200 * 50];
    for (int y = 0; y < 200; y++) {
        uint8_t c = (y < 50) ? C_RED : (y < 100 ? C_YELLOW : (y < 150 ? C_BLACK : C_WHITE));
        uint8_t packed = (c << 6) | (c << 4) | (c << 2) | c;
        memset(buf + y * 50, packed, 50);
    }
    Serial.println("[epd2] display 4 bars");
    displayBuf(buf, sizeof(buf));
    Serial.println("[epd2] done");
}

void loop() {
    delay(1000);
}
