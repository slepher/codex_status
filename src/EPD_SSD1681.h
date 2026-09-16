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

void EPD_SSD1681_Init(void);          // full-refresh waveform init
void EPD_SSD1681_Init_Partial(void);  // partial-refresh waveform init
void EPD_SSD1681_Clear(UBYTE color);  // fill RAM + full refresh (no framebuffer needed)
void EPD_SSD1681_Display(const UBYTE *Image);      // full refresh + seed previous RAM
void EPD_SSD1681_DisplayPart(const UBYTE *Image);  // partial refresh (~300ms, no flash)
void EPD_SSD1681_Sleep(void);

#endif
