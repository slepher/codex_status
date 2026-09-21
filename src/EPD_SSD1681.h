/*****************************************************************************
* | File      	:   EPD_SSD1681.h
* | Function    :   Waveshare 1.54" e-Paper (B) - SSD1681, 200x200, 1bpp
* | Info        :   Ported from clawdmeter-epaper waveshare_epaper_154 board
* |                 (full + partial refresh, panel waveform LUTs)
******************************************************************************/
#ifndef __EPD_SSD1681_H_
#define __EPD_SSD1681_H_

#include "DEV_Config.h"

// Display resolution
#define EPD_SSD1681_WIDTH       200
#define EPD_SSD1681_HEIGHT      200

// 1bpp color (fill byte: 0 = black, 1 = white)
#define EPD_SSD1681_BLACK   0
#define EPD_SSD1681_WHITE   1

// True = every panel BUSY wait completed inside the 5 s timeout. A false
// return means the waveform state is unknown: the caller must not treat the
// pixels as displayed or update its old-frame baseline (design §8.4).
bool EPD_SSD1681_Init(void);          // full-refresh waveform init
bool EPD_SSD1681_Init_Partial(void);  // partial-refresh waveform init
bool EPD_SSD1681_WakePartial(const UBYTE *PreviousImage);  // wake from sleep + seed prev RAM
bool EPD_SSD1681_Clear(UBYTE color);  // fill RAM + full refresh (no framebuffer needed)
bool EPD_SSD1681_Display(const UBYTE *Image);      // full refresh + seed previous RAM
bool EPD_SSD1681_DisplayPart(const UBYTE *Image);  // partial refresh (~300ms, no flash)
// Sub-window variants: X coords are start/end pixels (byte-aligned cells),
// data is row-major top-to-bottom, (xEnd-xStart)/8+1 bytes per row.
bool EPD_SSD1681_WakePartialWindow(int x0, int y0, int x1, int y1,
                                   const UBYTE *prev);
bool EPD_SSD1681_DisplayPartWindow(int x0, int y0, int x1, int y1,
                                   const UBYTE *data);
void EPD_SSD1681_Sleep(void);

#endif
