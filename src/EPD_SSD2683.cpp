// ZecTrix Note4 SSD2683, monochrome full refresh through the panel OTP waveform.
// Sequence and 2-bit panel RAM encoding follow the MIT-licensed reference:
// https://github.com/itopinion/zectrix-note4-epd-demo/blob/main/components/zectrix_epd/zectrix_epd.cc
#include "platform_target.h"
#if defined(CODEX_TARGET_NOTE4)

#include "EPD_SSD2683.h"
#include "DEV_Config.h"
#include "dev_log.h"
#include <SPI.h>

namespace {
constexpr int kStride = EPD_SSD2683_WIDTH / 8;
bool internalPowerOn = false;

void command(UBYTE value) {
    digitalWrite(EPD_DC_PIN, LOW);
    DEV_SPI_WriteByte(value);
}

void data(UBYTE value) {
    digitalWrite(EPD_DC_PIN, HIGH);
    DEV_SPI_WriteByte(value);
}

void dataRow(const UBYTE *row, size_t length) {
    digitalWrite(EPD_DC_PIN, HIGH);
    digitalWrite(EPD_CS_PIN, LOW);
    SPI.writeBytes(row, length);
    digitalWrite(EPD_CS_PIN, HIGH);
}

bool waitReady(const char *stage) {
    const uint32_t start = millis();
    // NOTE4 BUSY is active LOW. The earlier SSD1681-derived skeleton inverted it.
    while (digitalRead(EPD_BUSY_PIN) == LOW) {
        if (millis() - start >= 5000) {
            DevLog.printf("[epd2683] BUSY timeout: %s\n", stage);
            return false;
        }
        delay(10);
    }
    return true;
}

bool prepare() {
    digitalWrite(EPD_PWR_PIN, HIGH); // NOTE4 panel rail is active HIGH.
    delay(10);
    digitalWrite(EPD_RST_PIN, HIGH); delay(10);
    digitalWrite(EPD_RST_PIN, LOW);  delay(20);
    digitalWrite(EPD_RST_PIN, HIGH); delay(10);
    if (!waitReady("reset")) return false;
    command(0x00); data(0x2F); data(0x0E); // select OTP waveform
    command(0xE9); data(0x01);
    return waitReady("OTP init");
}

bool refresh(const UBYTE *frame) {
    if (!prepare()) return false;
    // Start the controller's temperature read even with the reference 25 C
    // fallback: the OTP setup sequence expects this stage before E0/E6/A5.
    command(0x40);
    if (!waitReady("temperature read")) return false;
    // The Note4 adapter has write-only SPI, so use the reference 25 C fallback.
    command(0xE0); data(0x02);
    command(0xE6); data(241);
    command(0xA5);
    if (!waitReady("temperature")) return false;
    delay(10);

    command(0x10); // SSD2683 RAM is two bits per pixel: 00 black, 01 white.
    UBYTE row[EPD_SSD2683_WIDTH / 4];
    for (int y = 0; y < EPD_SSD2683_HEIGHT; ++y) {
        for (int x = 0; x < kStride; ++x) {
            const UBYTE bits = frame ? frame[y * kStride + x] : 0xFF;
            for (int half = 0; half < 2; ++half) {
                UBYTE packed = 0;
                for (int pixel = 0; pixel < 4; ++pixel) {
                    const int bit = 7 - half * 4 - pixel;
                    packed |= ((bits >> bit) & 1U) << (6 - pixel * 2);
                }
                row[x * 2 + half] = packed;
            }
        }
        dataRow(row, sizeof(row));
    }
    command(0x04); // internal power on
    if (!waitReady("power on")) return false;
    internalPowerOn = true;
    command(0x12); data(0x00); // refresh from OTP waveform
    if (!waitReady("refresh")) return false;
    command(0x02); data(0x00); // internal power off
    if (!waitReady("power off")) return false;
    internalPowerOn = false;
    return true;
}
} // namespace

bool EPD_SSD2683_Init(void) { return prepare(); }
bool EPD_SSD2683_Clear(UBYTE color) {
    if (color != EPD_SSD2683_WHITE) return false;
    return refresh(nullptr);
}
bool EPD_SSD2683_Display(const UBYTE *image) { return image && refresh(image); }

// Partial waveform and window writes are deliberately unavailable in this
// bring-up ROM. TARGET_PARTIAL=0 prevents their use by the display policy.
bool EPD_SSD2683_Init_Partial(void) { return false; }
bool EPD_SSD2683_DisplayPart(const UBYTE *) { return false; }
bool EPD_SSD2683_WakePartial(const UBYTE *) { return false; }
bool EPD_SSD2683_WakePartialWindow(int, int, int, int, const UBYTE *) { return false; }
bool EPD_SSD2683_DisplayPartWindow(int, int, int, int, const UBYTE *) { return false; }

void EPD_SSD2683_Sleep(void) {
    if (internalPowerOn && digitalRead(EPD_BUSY_PIN) == HIGH) {
        command(0x02); data(0x00);
        waitReady("sleep");
    }
    internalPowerOn = false;
    digitalWrite(EPD_PWR_PIN, LOW);
}

#endif // CODEX_TARGET_NOTE4
