// Target driver selection (v2 §5): the firmware compiles against generic EPD_*
// aliases so one code base serves multiple firmware targets, each with its own
// controller/panel adaptation and independent ROM.
#pragma once

#include "platform_target.h"

#if defined(CODEX_TARGET_NOTE4)
#include "EPD_SSD2683.h"
#define EPD_TGT_WIDTH            EPD_SSD2683_WIDTH
#define EPD_TGT_HEIGHT           EPD_SSD2683_HEIGHT
#define EPD_TGT_BLACK            EPD_SSD2683_BLACK
#define EPD_TGT_WHITE            EPD_SSD2683_WHITE
#define EPD_TGT_Init             EPD_SSD2683_Init
#define EPD_TGT_Init_Partial     EPD_SSD2683_Init_Partial
#define EPD_TGT_WakePartial      EPD_SSD2683_WakePartial
#define EPD_TGT_WakePartialWindow EPD_SSD2683_WakePartialWindow
#define EPD_TGT_Clear            EPD_SSD2683_Clear
#define EPD_TGT_Display          EPD_SSD2683_Display
#define EPD_TGT_DisplayPart      EPD_SSD2683_DisplayPart
#define EPD_TGT_DisplayPartWindow EPD_SSD2683_DisplayPartWindow
#define EPD_TGT_Sleep            EPD_SSD2683_Sleep
#else
#include "EPD_SSD1681.h"
#define EPD_TGT_WIDTH            EPD_SSD1681_WIDTH
#define EPD_TGT_HEIGHT           EPD_SSD1681_HEIGHT
#define EPD_TGT_BLACK            EPD_SSD1681_BLACK
#define EPD_TGT_WHITE            EPD_SSD1681_WHITE
#define EPD_TGT_Init             EPD_SSD1681_Init
#define EPD_TGT_Init_Partial     EPD_SSD1681_Init_Partial
#define EPD_TGT_WakePartial      EPD_SSD1681_WakePartial
#define EPD_TGT_WakePartialWindow EPD_SSD1681_WakePartialWindow
#define EPD_TGT_Clear            EPD_SSD1681_Clear
#define EPD_TGT_Display          EPD_SSD1681_Display
#define EPD_TGT_DisplayPart      EPD_SSD1681_DisplayPart
#define EPD_TGT_DisplayPartWindow EPD_SSD1681_DisplayPartWindow
#define EPD_TGT_Sleep            EPD_SSD1681_Sleep
#endif
