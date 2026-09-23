// ZecTrix Note4 SSD2683, monochrome full and partial refresh through the
// panel OTP waveform. Partial command sequence and transitions follow the
// MIT-licensed reference:
// https://github.com/itopinion/zectrix-note4-epd-demo/blob/main/components/zectrix_epd/zectrix_epd.cc
#include "platform_target.h"
#if defined(CODEX_TARGET_NOTE4)

#include "EPD_SSD2683.h"
#include "DEV_Config.h"
#include "dev_log.h"
#include <SPI.h>
#include <cstring>

namespace {
constexpr int kStride = EPD_SSD2683_WIDTH / 8;
constexpr size_t kFrameBytes = kStride * EPD_SSD2683_HEIGHT;
bool internalPowerOn = false;
bool shadowValid = false;
bool partialPrepared = false;
UBYTE shadow[kFrameBytes];

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
    // NOTE4 BUSY is active LOW.
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
    internalPowerOn = false;
    digitalWrite(EPD_RST_PIN, HIGH); delay(10);
    digitalWrite(EPD_RST_PIN, LOW);  delay(20);
    digitalWrite(EPD_RST_PIN, HIGH); delay(10);
    if (!waitReady("reset")) return false;
    command(0x00); data(0x2F); data(0x0E); // select OTP waveform
    command(0xE9); data(0x01);
    return waitReady("OTP init");
}

bool refresh(const UBYTE *frame) {
    partialPrepared = false;
    if (!prepare()) return false;
    // Write temperature configuration using the reference 25 C fallback.
    command(0x40);
    if (!waitReady("temperature read")) return false;
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

bool validWindow(int x0, int y0, int x1, int y1) {
    return x0 >= 0 && y0 >= 0 && x0 <= x1 && y0 <= y1 &&
           x1 < EPD_SSD2683_WIDTH && y1 < EPD_SSD2683_HEIGHT;
}

UBYTE pixel(const UBYTE *bits, int stride, int x, int y) {
    return (bits[y * stride + x / 8] >> (7 - (x & 7))) & 1U;
}

bool matchesShadow(int x0, int y0, int x1, int y1, const UBYTE *previous) {
    if (!previous) return false;
    const int width = x1 - x0 + 1;
    const int stride = (width + 7) / 8;
    for (int y = y0; y <= y1; ++y) {
        for (int x = x0; x <= x1; ++x) {
            if (pixel(previous, stride, x - x0, y - y0) !=
                pixel(shadow, kStride, x, y)) return false;
        }
    }
    return true;
}

void setPartialWindow(int x0, int y0, int x1, int y1) {
    command(0x83);
    const UBYTE values[] = {
        static_cast<UBYTE>((x0 >> 8) & 0x03), static_cast<UBYTE>(x0),
        static_cast<UBYTE>((x1 >> 8) & 0x03), static_cast<UBYTE>(x1),
        static_cast<UBYTE>((y0 >> 8) & 0x03), static_cast<UBYTE>(y0),
        static_cast<UBYTE>((y1 >> 8) & 0x03), static_cast<UBYTE>(y1), 0x01};
    for (UBYTE value : values) data(value);
}

bool partialWindow(int x0, int y0, int x1, int y1, const UBYTE *pixels) {
    const bool wasPrepared = partialPrepared;
    partialPrepared = false;
    if (!shadowValid || !validWindow(x0, y0, x1, y1) || !pixels) return false;
    const int requestedX0 = x0, requestedY0 = y0;
    const int requestedX1 = x1, requestedY1 = y1;
    const int sourceStride = (x1 - x0 + 8) / 8;
    x0 &= ~7;
    x1 = ((x1 + 8) & ~7) - 1;
    const int nativeStride = (x1 - x0 + 1) / 4;
    UBYTE row[EPD_SSD2683_WIDTH / 4];

    if (!wasPrepared) {
        if (!prepare()) {
            shadowValid = false;
            return false;
        }
    }
    command(0x50); data(0x77);
    command(0xE0); data(0x00);
    command(0xA5);
    if (!waitReady("partial temperature")) {
        shadowValid = false;
        return false;
    }
    delay(10);
    setPartialWindow(x0, y0, x1, y1);
    command(0x10);
    if (!waitReady("partial RAM write")) {
        shadowValid = false;
        return false;
    }

    for (int y = y0; y <= y1; ++y) {
        memset(row, 0, nativeStride);
        for (int x = x0; x <= x1; ++x) {
            const UBYTE oldPixel = pixel(shadow, kStride, x, y);
            UBYTE newPixel = oldPixel;
            if (x >= requestedX0 && x <= requestedX1 &&
                y >= requestedY0 && y <= requestedY1) {
                newPixel = pixel(pixels, sourceStride, x - requestedX0,
                                 y - requestedY0);
            }
            const UBYTE transition = (oldPixel << 1) | newPixel;
            const int relativeX = x - x0;
            row[relativeX / 4] |= transition << (6 - (relativeX & 3) * 2);
        }
        dataRow(row, nativeStride);
    }

    command(0x04);
    if (!waitReady("power on")) {
        internalPowerOn = false;
        shadowValid = false;
        return false;
    }
    internalPowerOn = true;
    command(0x12); data(0x00);
    if (!waitReady("partial refresh")) {
        internalPowerOn = false;
        shadowValid = false;
        return false;
    }
    command(0x02); data(0x00);
    if (!waitReady("power off")) {
        internalPowerOn = false;
        shadowValid = false;
        return false;
    }
    internalPowerOn = false;

    for (int y = requestedY0; y <= requestedY1; ++y) {
        for (int x = requestedX0; x <= requestedX1; ++x) {
            const int index = y * kStride + x / 8;
            const UBYTE mask = 1U << (7 - (x & 7));
            if (pixel(pixels, sourceStride, x - requestedX0, y - requestedY0))
                shadow[index] |= mask;
            else
                shadow[index] &= ~mask;
        }
    }
    return true;
}
} // namespace

bool EPD_SSD2683_Init(void) {
    partialPrepared = false;
    const bool ok = prepare();
    if (!ok) shadowValid = false;
    return ok;
}
bool EPD_SSD2683_Init_Partial(void) {
    const bool ok = prepare();
    partialPrepared = ok;
    if (!ok) shadowValid = false;
    return ok;
}
bool EPD_SSD2683_Clear(UBYTE color) {
    if (color != EPD_SSD2683_WHITE) return false;
    const bool ok = refresh(nullptr);
    if (ok) {
        memset(shadow, 0xFF, sizeof(shadow));
        shadowValid = true;
    } else shadowValid = false;
    return ok;
}
bool EPD_SSD2683_Display(const UBYTE *image) {
    if (!image) return false;
    const bool ok = refresh(image);
    if (ok) {
        memcpy(shadow, image, sizeof(shadow));
        shadowValid = true;
    } else shadowValid = false;
    return ok;
}
bool EPD_SSD2683_DisplayPart(const UBYTE *image) {
    if (!image || !shadowValid) return false;
    return partialWindow(0, 0, EPD_SSD2683_WIDTH - 1,
                         EPD_SSD2683_HEIGHT - 1, image);
}
bool EPD_SSD2683_WakePartial(const UBYTE *previousImage) {
    partialPrepared = false;
    if (!shadowValid || !previousImage ||
        memcmp(shadow, previousImage, sizeof(shadow)) != 0) return false;
    partialPrepared = prepare();
    if (!partialPrepared) shadowValid = false;
    return partialPrepared;
}
bool EPD_SSD2683_WakePartialWindow(int x0, int y0, int x1, int y1,
                                   const UBYTE *prev) {
    partialPrepared = false;
    if (!shadowValid || !validWindow(x0, y0, x1, y1) ||
        !matchesShadow(x0, y0, x1, y1, prev)) return false;
    partialPrepared = prepare();
    if (partialPrepared) return true;
    shadowValid = false;
    return false;
}
bool EPD_SSD2683_DisplayPartWindow(int x0, int y0, int x1, int y1,
                                   const UBYTE *data) {
    return partialWindow(x0, y0, x1, y1, data);
}

void EPD_SSD2683_Sleep(void) {
    if (internalPowerOn && digitalRead(EPD_BUSY_PIN) == HIGH) {
        command(0x02); data(0x00);
        if (waitReady("sleep")) internalPowerOn = false;
    }
    digitalWrite(EPD_PWR_PIN, LOW);
    partialPrepared = false;
}

#endif // CODEX_TARGET_NOTE4
