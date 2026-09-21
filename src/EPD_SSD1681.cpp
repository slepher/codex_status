/*****************************************************************************
* | File      	:   EPD_SSD1681.cpp
* | Function    :   Waveshare 1.54" e-Paper (B) - SSD1681 driver
* | Info        :   Waveform LUTs and init sequences vendored verbatim from
* |                 waveshareteam/ESP32-S3-ePaper-1.54 via
* |                 clawdmeter-epaper (firmware/src/boards/waveshare_epaper_154).
******************************************************************************/
#include "EPD_SSD1681.h"
#include "dev_log.h"
#include "DEV_Config.h"

// 1bpp framebuffer size: 200 x 200 / 8 = 5000 bytes
#define FB_BYTES  ((EPD_SSD1681_WIDTH * EPD_SSD1681_HEIGHT) / 8)

// 159-byte waveform tables: first 153 bytes via cmd 0x32, then 6 trailer
// bytes for cmds 0x3F, 0x03, 0x04 (x3) and 0x2C.
static const UBYTE WF_FULL[159] = {
    0x80,0x48,0x40,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x40,0x48,0x80,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x80,0x48,0x40,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x40,0x48,0x80,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x0A,0x00,0x00,0x00,0x00,0x00,0x00,
    0x08,0x01,0x00,0x08,0x01,0x00,0x02,
    0x0A,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x22,0x22,0x22,0x22,0x22,0x22,0x00,0x00,0x00,
    0x22,0x17,0x41,0x00,0x32,0x20
};

static const UBYTE WF_PARTIAL[159] = {
    0x00,0x40,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x80,0x80,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x40,0x40,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x80,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x0F,0x00,0x00,0x00,0x00,0x00,0x00,
    0x01,0x01,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x22,0x22,0x22,0x22,0x22,0x22,0x00,0x00,0x00,
    0x02,0x17,0x41,0xB0,0x32,0x28
};

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

// SSD1681 BUSY: HIGH while busy; wait for LOW. Timeout 5s.
// Returns false on timeout so the caller can refuse to trust the panel state
// (design §8.4: no software baseline update after an unknown waveform).
static bool readBusy(void)
{
    UDOUBLE t0 = millis();
    while (digitalRead(EPD_BUSY_PIN) == HIGH) {
        if (millis() - t0 > 5000) {
            DevLog.println("[epd] busy timeout");
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

// Reference EPD_SetWindows: X in byte units, Y as 16-bit pixel address.
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

bool EPD_SSD1681_Init(void)
{
    bool ok = reset();

    sendCmd(0x12);              // SW reset
    ok = readBusy() && ok;

    // Driver output control: 200 gates, TB=1
    sendCmd(0x01);
    sendData((EPD_SSD1681_HEIGHT - 1) & 0xFF);
    sendData(((EPD_SSD1681_HEIGHT - 1) >> 8) & 0xFF);
    sendData(0x01);

    sendCmd(0x11);              // data entry: X+, Y-
    sendData(0x01);

    setWindow(0, EPD_SSD1681_HEIGHT - 1, EPD_SSD1681_WIDTH - 1, 0);

    sendCmd(0x3C);              // border waveform (full)
    sendData(0x01);

    sendCmd(0x18);              // internal temperature sensor
    sendData(0x80);

    sendCmd(0x22);              // load temp + OTP waveform, then activate
    sendData(0xB1);
    sendCmd(0x20);

    setCursor(0, EPD_SSD1681_HEIGHT - 1);
    ok = readBusy() && ok;

    return loadLut(WF_FULL) && ok;
}

bool EPD_SSD1681_Init_Partial(void)
{
    bool ok = reset();
    ok = loadLut(WF_PARTIAL) && ok;

    sendCmd(0x37);              // partial-mode parameters (vendored)
    sendData(0x00); sendData(0x00); sendData(0x00); sendData(0x00); sendData(0x00);
    sendData(0x40); sendData(0x00); sendData(0x00); sendData(0x00); sendData(0x00);

    sendCmd(0x3C);              // border waveform (partial)
    sendData(0x80);

    sendCmd(0x22);              // enable clock+analog, arm partial mode
    sendData(0xC0);
    sendCmd(0x20);
    return readBusy() && ok;
}

// Wake the controller from deep-sleep mode 1 and prepare a partial refresh.
// Mode 1 retains RAM, but a reset after wake may not be trusted to keep the
// "previous" RAM (cmd 0x26) baseline, so it is re-seeded from the frame that
// is known to be on screen.
bool EPD_SSD1681_WakePartial(const UBYTE *PreviousImage)
{
    bool ok = EPD_SSD1681_Init_Partial();
    if (!PreviousImage) return ok;
    setWindow(0, EPD_SSD1681_HEIGHT - 1, EPD_SSD1681_WIDTH - 1, 0);
    setCursor(0, EPD_SSD1681_HEIGHT - 1);
    sendCmd(0x26);
    sendDataN(PreviousImage, FB_BYTES);
    return ok;
}

bool EPD_SSD1681_Clear(UBYTE color)
{
    static UBYTE chunk[500];
    UBYTE fill = (color == EPD_SSD1681_WHITE) ? 0xFF : 0x00;
    memset(chunk, fill, sizeof(chunk));

    setWindow(0, EPD_SSD1681_HEIGHT - 1, EPD_SSD1681_WIDTH - 1, 0);
    setCursor(0, EPD_SSD1681_HEIGHT - 1);

    sendCmd(0x24);
    for (int done = 0; done < FB_BYTES; done += sizeof(chunk))
        sendDataN(chunk, sizeof(chunk));

    setCursor(0, EPD_SSD1681_HEIGHT - 1);
    sendCmd(0x26);
    for (int done = 0; done < FB_BYTES; done += sizeof(chunk))
        sendDataN(chunk, sizeof(chunk));

    sendCmd(0x22);
    sendData(0xC7);
    sendCmd(0x20);
    return readBusy();
}

bool EPD_SSD1681_Display(const UBYTE *Image)
{
    setWindow(0, EPD_SSD1681_HEIGHT - 1, EPD_SSD1681_WIDTH - 1, 0);
    setCursor(0, EPD_SSD1681_HEIGHT - 1);

    sendCmd(0x24);              // visible RAM
    sendDataN(Image, FB_BYTES);

    setCursor(0, EPD_SSD1681_HEIGHT - 1);
    sendCmd(0x26);              // seed "previous" RAM for later partials
    sendDataN(Image, FB_BYTES);

    sendCmd(0x22);
    sendData(0xC7);             // full refresh
    sendCmd(0x20);
    return readBusy();
}

bool EPD_SSD1681_DisplayPart(const UBYTE *Image)
{
    setWindow(0, EPD_SSD1681_HEIGHT - 1, EPD_SSD1681_WIDTH - 1, 0);
    setCursor(0, EPD_SSD1681_HEIGHT - 1);

    sendCmd(0x24);
    sendDataN(Image, FB_BYTES);

    sendCmd(0x22);
    sendData(0xCF);             // partial refresh vs previous RAM
    sendCmd(0x20);
    return readBusy();
}

void EPD_SSD1681_Sleep(void)
{
    sendCmd(0x10);
    sendData(0x01);             // deep sleep mode 1: retain RAM
}

// ---- sub-window partial refresh (clock region) ----
// The full-screen calls use window Y = (HEIGHT-1 .. 0) with the cursor at
// HEIGHT-1, so screen y maps to RAM row (HEIGHT-1 - y); the data rows are
// written top-to-bottom. Mirror that mapping for an arbitrary sub-window.
static void setWindowRegion(int x0, int y0, int x1, int y1)
{
    setWindow((UBYTE)x0, (UWORD)(EPD_SSD1681_HEIGHT - 1 - y0),
              (UBYTE)x1, (UWORD)(EPD_SSD1681_HEIGHT - 1 - y1));
}

static void setCursorRegion(int x0, int y0)
{
    setCursor((UBYTE)x0, (UWORD)(EPD_SSD1681_HEIGHT - 1 - y0));
}

bool EPD_SSD1681_WakePartialWindow(int x0, int y0, int x1, int y1,
                                   const UBYTE *prev)
{
    bool ok = EPD_SSD1681_Init_Partial();
    if (!prev) return ok;
    const UDOUBLE bytes = (UDOUBLE)((x1 >> 3) - (x0 >> 3) + 1) * (y1 - y0 + 1);
    setWindowRegion(x0, y0, x1, y1);
    setCursorRegion(x0, y0);
    sendCmd(0x26);
    sendDataN(prev, bytes);
    return ok;
}

bool EPD_SSD1681_DisplayPartWindow(int x0, int y0, int x1, int y1,
                                   const UBYTE *data)
{
    const UDOUBLE bytes = (UDOUBLE)((x1 >> 3) - (x0 >> 3) + 1) * (y1 - y0 + 1);
    setWindowRegion(x0, y0, x1, y1);
    setCursorRegion(x0, y0);
    sendCmd(0x24);
    sendDataN(data, bytes);
    sendCmd(0x22);
    sendData(0xCF);             // partial refresh vs previous RAM
    sendCmd(0x20);
    return readBusy();
}
