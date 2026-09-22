/*****************************************************************************
* | File      	:   EPD_SSD2683.cpp
* | Function    :   4.2" 400x300 B/W e-paper, SSD2683 controller.
* |
* | The command set (0x12 reset, 0x01 output, 0x11 data entry, 0x44/0x45
* | window, 0x4E/0x4F cursor, 0x24/0x26 planes, 0x22/0x20 update, 0x18 temp,
* | 0x3C border, 0x32/0x3F/0x03/0x04/0x2C LUT, 0x10 sleep) is the E Ink
* | SSD16xx/SSD26xx family protocol. Panel-specific values (gate count,
* | orientation, border, temperature curve and both waveform LUTs) require the
* | panel datasheet/vendor sample and are supplied through
* | `ssd2683_luts.h` + the NOTE4_* build flags. Until they exist this target is
* | `TARGET_VERIFIED=0`: it builds only when the facts are provided and refuses
* | normal operation otherwise.
******************************************************************************/
#include "platform_target.h"

// This driver is part of the ZecTrix Note4 target only. The default 154g env
// must not compile it (its vendor LUT header does not exist yet), so the whole
// translation unit is gated on the target macro.
#if defined(CODEX_TARGET_NOTE4)

#include "EPD_SSD2683.h"
#include "dev_log.h"
#include "DEV_Config.h"

// 1bpp framebuffer size: 400 x 300 / 8 = 15000 bytes
#define FB_BYTES  ((EPD_SSD2683_WIDTH * EPD_SSD2683_HEIGHT) / 8)
#define ROW_BYTES (EPD_SSD2683_WIDTH / 8)

// Panel waveform tables (vendor data required; not guessed here).
#include "ssd2683_luts.h"
static const UBYTE *WF_FULL = SSD2683_WF_FULL;
static const UBYTE *WF_PARTIAL = SSD2683_WF_PARTIAL;

static void sendCmd(UBYTE cmd)
{
    digitalWrite(EPD_DC_PIN, LOW);
    DEV_SPI_WriteByte(cmd);
}

static void sendData(UBYTE data)
{
    digitalWrite(EPD_DC_PIN, HIGH);
    DEV_SPI_WriteByte(data);
}

static void sendDataN(const UBYTE *buf, UDOUBLE len)
{
    if (!buf || !len) return;
    digitalWrite(EPD_DC_PIN, HIGH);
    DEV_SPI_Write_nByte((UBYTE *)buf, len);
}

// SSD26xx BUSY: HIGH while busy; wait for LOW. Timeout 5 s.
// Returns false on timeout so the caller refuses to trust the panel state.
static bool readBusy(void)
{
    UDOUBLE t0 = millis();
    while (digitalRead(EPD_BUSY_PIN) == HIGH) {
        if (millis() - t0 > 5000) {
            DevLog.println("[epd2683] busy timeout");
            return false;
        }
        delay(1);
    }
    return true;
}

static bool reset(void)
{
    digitalWrite(EPD_RST_PIN, HIGH); delay(50);
    digitalWrite(EPD_RST_PIN, LOW);  delay(20);
    digitalWrite(EPD_RST_PIN, HIGH); delay(50);
    return readBusy();
}

static void setWindow(UBYTE xStartPx, UWORD yStartPx, UBYTE xEndPx, UWORD yEndPx)
{
    sendCmd(0x44);
    sendData((xStartPx >> 3) & 0xFF);
    sendData((xEndPx   >> 3) & 0xFF);

    sendCmd(0x45);
    sendData( yStartPx       & 0xFF);
    sendData((yStartPx >> 8) & 0xFF);
    sendData( yEndPx         & 0xFF);
    sendData((yEndPx   >> 8) & 0xFF);
}

static void setCursor(UBYTE xPx, UWORD yPx)
{
    sendCmd(0x4E);
    sendData((xPx >> 3) & 0xFF);

    sendCmd(0x4F);
    sendData( yPx       & 0xFF);
    sendData((yPx >> 8) & 0xFF);
}

static bool loadLut(const UBYTE *lut)
{
    sendCmd(0x32);
    sendDataN(lut, 153);
    bool ok = readBusy();

    sendCmd(0x3F); sendData(lut[153]);
    sendCmd(0x03); sendData(lut[154]);
    sendCmd(0x04); sendData(lut[155]); sendData(lut[156]); sendData(lut[157]);
    sendCmd(0x2C); sendData(lut[158]);
    return ok;
}

bool EPD_SSD2683_Init(void)
{
    bool ok = reset();

    sendCmd(0x12);              // SW reset
    ok = readBusy() && ok;

    sendCmd(0x01);              // driver output control: 300 gates
    sendData((EPD_SSD2683_HEIGHT - 1) & 0xFF);
    sendData(((EPD_SSD2683_HEIGHT - 1) >> 8) & 0xFF);
    sendData(0x00);             // SM=0, TB=0 (orientation per panel datasheet)

    sendCmd(0x11);              // data entry: X+, Y-
    sendData(0x03);

    setWindow(0, 0, EPD_SSD2683_WIDTH - 1, EPD_SSD2683_HEIGHT - 1);
    setCursor(0, 0);

    sendCmd(0x3C);              // border waveform
    sendData(0x05);
    sendCmd(0x18);              // internal temperature sensor
    sendData(0x80);
    sendCmd(0x22);              // display update control 1
    sendData(0xB1);
    sendCmd(0x20);              // activate
    ok = readBusy() && ok;
    ok = loadLut(WF_FULL) && ok;
    return ok;
}

bool EPD_SSD2683_Init_Partial(void)
{
    bool ok = reset();
    ok = loadLut(WF_PARTIAL) && ok;
    sendCmd(0x37);              // partial in
    sendData(0x00); sendData(0x00);
    sendData(0x00); sendData(0x00);
    sendData(0x00); sendData(0x40);
    sendData(0x00); sendData(0x00);
    sendData(0x00); sendData(0x00);
    sendCmd(0x3C);
    sendData(0x80);
    sendCmd(0x22);
    sendData(0xC0);
    sendCmd(0x20);
    return readBusy() && ok;
}

bool EPD_SSD2683_Clear(UBYTE color)
{
    UBYTE fill = color ? 0xFF : 0x00;
    sendCmd(0x24);
    for (int i = 0; i < FB_BYTES; i++) sendData(fill);
    sendCmd(0x26);
    for (int i = 0; i < FB_BYTES; i++) sendData(fill);
    sendCmd(0x22);
    sendData(0xC7);
    sendCmd(0x20);
    return readBusy();
}

bool EPD_SSD2683_Display(const UBYTE *Image)
{
    if (!Image) return false;
    sendCmd(0x24);
    sendDataN(Image, FB_BYTES);
    sendCmd(0x26);
    sendDataN(Image, FB_BYTES);
    sendCmd(0x22);
    sendData(0xC7);
    sendCmd(0x20);
    return readBusy();
}

bool EPD_SSD2683_DisplayPart(const UBYTE *Image)
{
    if (!Image) return false;
    sendCmd(0x24);
    sendDataN(Image, FB_BYTES);
    sendCmd(0x22);
    sendData(0xCF);
    sendCmd(0x20);
    return readBusy();
}

bool EPD_SSD2683_WakePartial(const UBYTE *PreviousImage)
{
    if (!EPD_SSD2683_Init_Partial()) return false;
    if (PreviousImage) {
        sendCmd(0x26);
        sendDataN(PreviousImage, FB_BYTES);
    }
    return true;
}

// Y is mirrored like the verified 1.54" path: data rows are top-to-bottom while
// the controller scans Y-, so a window row maps to (HEIGHT-1-screen_y).
static void setWindowMirrored(int x0, int y0, int x1, int y1)
{
    const int yTop = EPD_SSD2683_HEIGHT - 1 - y0;
    const int yBot = EPD_SSD2683_HEIGHT - 1 - y1;
    setWindow((UBYTE)x0, (UWORD)yTop, (UBYTE)x1, (UWORD)yBot);
    setCursor((UBYTE)x0, (UWORD)yTop);
}

bool EPD_SSD2683_WakePartialWindow(int x0, int y0, int x1, int y1,
                                   const UBYTE *prev)
{
    if (!prev) return false;
    if (!EPD_SSD2683_Init_Partial()) return false;
    setWindowMirrored(x0, y0, x1, y1);
    const int bw = (x1 >> 3) - (x0 >> 3) + 1;
    const int rows = y1 - y0 + 1;
    sendCmd(0x26);
    for (int r = 0; r < rows; r++) {
        const UBYTE *src = prev + (size_t)(y0 + r) * ROW_BYTES + (x0 >> 3);
        sendDataN(src, bw);
    }
    return readBusy();
}

bool EPD_SSD2683_DisplayPartWindow(int x0, int y0, int x1, int y1,
                                   const UBYTE *data)
{
    if (!data) return false;
    setWindowMirrored(x0, y0, x1, y1);
    const int bw = (x1 >> 3) - (x0 >> 3) + 1;
    const int rows = y1 - y0 + 1;
    sendCmd(0x24);
    for (int r = 0; r < rows; r++) {
        const UBYTE *src = data + (size_t)(y0 + r) * ROW_BYTES + (x0 >> 3);
        sendDataN(src, bw);
    }
    sendCmd(0x22);
    sendData(0xCF);
    sendCmd(0x20);
    return readBusy();
}

void EPD_SSD2683_Sleep(void)
{
    sendCmd(0x10);
    sendData(0x01);
}

#endif  // CODEX_TARGET_NOTE4
