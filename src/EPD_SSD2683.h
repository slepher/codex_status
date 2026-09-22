/*****************************************************************************
* | File      	:   EPD_SSD2683.h
* | Function    :   4.2" 400x300 B/W e-Paper - SSD2683 controller (ZecTrix
* |                 Note4 V1.0 target). Driver structure and BUSY/window/plane
* |                 handling follow the verified SSD1681 adaptation; the panel
* |                 waveform LUTs and the board GPIO map are NOT yet verified
* |                 and must be supplied explicitly (see platform_target.h).
* | Status      :   blocked_by_hardware_arrival - compiles and is host-tested
* |                 for geometry; must not be flashed as a verified panel.
******************************************************************************/
#ifndef __EPD_SSD2683_H_
#define __EPD_SSD2683_H_

#include "platform_target.h"
#include "DEV_Config.h"

// Display resolution (confirmed panel fact: 4.2", 400x300, B/W, SSD2683).
#define EPD_SSD2683_WIDTH   400
#define EPD_SSD2683_HEIGHT  300

// 1bpp color (fill byte: 0 = black, 1 = white)
#define EPD_SSD2683_BLACK   0
#define EPD_SSD2683_WHITE   1

// True = every panel BUSY wait completed inside the 5 s timeout. A false
// return means the waveform state is unknown: the caller must not treat the
// pixels as displayed or update its old-frame baseline (design §8.4).
bool EPD_SSD2683_Init(void);          // full-refresh waveform init
bool EPD_SSD2683_Init_Partial(void);  // partial-refresh waveform init
bool EPD_SSD2683_WakePartial(const UBYTE *PreviousImage);
bool EPD_SSD2683_Clear(UBYTE color);
bool EPD_SSD2683_Display(const UBYTE *Image);
bool EPD_SSD2683_DisplayPart(const UBYTE *Image);
bool EPD_SSD2683_WakePartialWindow(int x0, int y0, int x1, int y1,
                                   const UBYTE *prev);
bool EPD_SSD2683_DisplayPartWindow(int x0, int y0, int x1, int y1,
                                   const UBYTE *data);
void EPD_SSD2683_Sleep(void);

#endif
