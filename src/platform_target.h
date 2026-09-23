// Hardware/render target descriptor (v2 §5). Every build declares exactly one
// (firmware_target, render_target) pair; both the Bridge and the device check it
// and OTA refuses an image built for another target.
//
// Target status:
//   codex-status-154g       : Waveshare 1.54" 200x200 SSD1681 B/W - verified
//   codex-status-154g-gray4 : software render target (2 bpp / 4 gray). The panel
//                             combination is NOT hardware-verified; the build
//                             compiles and is host-tested but refuses normal
//                             operation until the hardware facts are confirmed
//                             (blocked_by_hardware_arrival).
//   zectrix-note4-400x300   : ZecTrix Note4 V1.0 board (ESP32-S3) with a 4.2"
//                             400x300 B/W panel and an SSD2683 controller.
//                             Official V1.0 pin map and OTP full/partial driver
//                             paths are used; partial hardware validation remains.
#pragma once

#if defined(CODEX_TARGET_NOTE4)
#define FW_TARGET_ID       "zectrix-note4-400x300"
#define RENDER_TARGET_ID   "epd-ssd2683-400x300-1bpp"
#define TARGET_PIXEL_FORMAT "1bpp"
#define TARGET_COLORS      "bw"
#define TARGET_PARTIAL     1   // OTP driver ported; hardware verification pending
#define TARGET_VERIFIED    1   // Note4 V1.0 pin map and OTP full-refresh path
#define TARGET_WIDTH  400
#define TARGET_HEIGHT 300
#elif defined(CODEX_TARGET_GRAY4)
#define FW_TARGET_ID       "codex-status-154g-gray4"
#define RENDER_TARGET_ID   "epd-200x200-2bpp-gray4"
#define TARGET_PIXEL_FORMAT "2bpp"
#define TARGET_COLORS      "gray4"
#define TARGET_PARTIAL     0
#define TARGET_VERIFIED    0
#define TARGET_WIDTH  200
#define TARGET_HEIGHT 200
#else
#define FW_TARGET_ID       "codex-status-154g"
#define RENDER_TARGET_ID   "epd-ssd1681-200x200-1bpp"
#define TARGET_PIXEL_FORMAT "1bpp"
#define TARGET_COLORS      "bw"
#define TARGET_PARTIAL     1
#define TARGET_VERIFIED    1
#define TARGET_WIDTH  200
#define TARGET_HEIGHT 200
#endif

#define TARGET_ROW_BYTES ((TARGET_WIDTH + 7) / 8)
#define TARGET_FB_BYTES  (TARGET_ROW_BYTES * TARGET_HEIGHT)

// ---------------------------------------------------------------------------
// ZecTrix Note4 board facts. Known from the supplied schematic sheets:
//   - EPD nets: EPD_SCK/EPD_CS/EPD_DC/EPD_RST/EPD_BUSY/EPD_SDA + EPD3V3_EN
//   - KEY nets: KEY_PGUP (side), KEY_PGDN (side), KEY_ENTER (front)
//   - I2C: PCF8563 RTC (0x51), ES8311 codec (0x18), NFC (0x55)
// The V1.0 GPIO numbers are explicit in the Note4 PlatformIO env. Full
// refresh uses the panel OTP waveform; no external waveform table is needed.
// ---------------------------------------------------------------------------
#if defined(CODEX_TARGET_NOTE4)
#if !defined(NOTE4_EPD_SCK) || !defined(NOTE4_EPD_MOSI) || !defined(NOTE4_EPD_CS) || \
    !defined(NOTE4_EPD_DC) || !defined(NOTE4_EPD_RST) || !defined(NOTE4_EPD_BUSY) || \
    !defined(NOTE4_EPD_PWR)
#error "target zectrix-note4-400x300: missing EPD GPIO map. Provide NOTE4_EPD_SCK/MOSI/CS/DC/RST/BUSY/PWR (schematic net -> GPIO) in build_flags."
#endif
#if !defined(NOTE4_KEY_PGUP) || !defined(NOTE4_KEY_PGDN) || !defined(NOTE4_KEY_ENTER)
#error "target zectrix-note4-400x300: missing KEY GPIO map. Provide NOTE4_KEY_PGUP/PGDN/ENTER in build_flags."
#endif
#endif
