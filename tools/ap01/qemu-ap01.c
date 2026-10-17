/*
 * AP01 application-image execution probe, not a complete BL616/BL618 board.
 * SPDX-License-Identifier: GPL-2.0-or-later
 *
 * Explicit synthetic BootROM leaves and bounded peripheral models are opt-in.
 * This is not a complete board or a physical BootROM/Boot2 reproduction.
 * RAM starts zero-filled; this is not a captured physical boot handoff.
 */

#include "qemu/osdep.h"
#include <glib/gstdio.h>
#include "hw/boards.h"
#include "hw/loader.h"
#include "hw/riscv/riscv_hart.h"
#include "qapi/error.h"
#include "qemu/error-report.h"
#include "qemu/module.h"
#include "system/address-spaces.h"
#include "system/reset.h"
#include "system/runstate.h"
#include "qemu/log.h"
#include "qemu/timer.h"
#include "hw/irq.h"
#include "qemu/bswap.h"
#include "system/system.h"
#include "qemu-ap01-uart.h"
#include "qemu-ap01-spi.h"
#include "qemu-ap01-dma.h"
#include "qemu-ap01-pwm.h"
#include "qemu-ap01-pec.h"
#include "qemu-ap01-flash.h"
#include "qemu-ap01-timer.h"
#include "qemu-ap01-rf.h"
#include "qemu-ap01-rf-cal.h"
#include "qemu-ap01-rf-acal.h"
#include "qemu-ap01-rf-pm.h"
#include "qemu-ap01-trng.h"
#include "qemu-ap01-mac.h"
#include "qemu-ap01-machw.h"

#define TYPE_AP01_MACHINE MACHINE_TYPE_NAME("ap01")
OBJECT_DECLARE_SIMPLE_TYPE(AP01State, AP01_MACHINE)

#define AP01_RAM_SIZE    0x78000
#define AP01_PSRAM_SIZE  0x400000
#define AP01_XIP_SIZE    0x1000000
#define AP01_XIP_BASE    0x9ffff000ULL
#define AP01_ENTRY       0xa0000000ULL

typedef struct AP01Register {
    hwaddr address;
    uint32_t reset;
    uint32_t mask;
} AP01Register;

/* Fixed SDK register-latch subset, plus explicit state machines below.
 * A register latch alone does not implement its downstream peripheral.
 * Unknown addresses/widths return a bus error, never a fabricated value. */
static const AP01Register ap01_boot_registers[] = {
    {0x2000e500, 0x21, 0x3f}, {0x2000e504, 0, 0x7f},
    {0x200053a0, 0xffff, 0x0f0fffff}, {0x200053a8, 0xffff, UINT32_MAX},
    {0x20005040, 0xffff, 0xffcfffff}, {0x2000504c, 0x3ff, 0x03ff03ff},
    {0x2000b070, 0, 0xfe0000fb},
    {0x2000b000, 0x1af2001c, 0xfffe0f1c}, /* SF_CTRL0 delay/format config. */
    {0x2000b034, 0, 0x00030303}, /* SF IO0 delay configuration. */
    {0x2000b004, 0xf3600000, 0xfbf6ffff}, /* SF_CTRL1 owner and AHB mux. */
    {0x20000154, 0x98543210, UINT32_MAX}, {0x20000158, 0xba76, 0xffff},
    {0x2000060c, 3, 0xf}, {0x20000090, 0xf, 0x00ffff0f},
    /* glb_reg.h proc_mon: refcount_div_onehot reset4, ready/frequency RO. */
    {0x200006f0, 0x400, 0xf3f},
    {0x20000620, 0x08000000, 0xd8000000}, /* PSRAM clock config only. */
    {0x200005c4, 0, UINT32_MAX}, /* Boot2 software scratch: cached JEDEC ID. */
    {0x2005200c, 0x1f082008, 0xff8f7fff}, /* PSRAM manual timing/address mask. */
    {0x20052000, 0xa080c002, 0xf0fd0777}, /* Config only; command pulses below. */
    {0x20052004, 0x00004800, 0x0000ff7f},
    {0x20052010, 0x40003782, 0xe31f37ff},
    {0x20052014, 0x40003382, 0},
    {0x20052108, 0, UINT32_MAX}, /* PSRAM rough delay configuration latches. */
    {0x2005210c, 0, UINT32_MAX},
    {0x20052114, 0, UINT32_MAX},
    {0x2000f014, 0x03010005, 0x0ff500ff},
    {0x2000f02c, 0xa, 0x1f}, {0x2000f030, 0xaa0a0020, 0xff0f803f},
    {0x2000f038, 0, 0xf0f7fc0f}, /* SDK HBN pad mux/config; GPIO16/17. */
    {0x2000f10c, UINT32_MAX, UINT32_MAX}, /* HBN_RSV3, SDK reset. */
    {0x2000e114, 0x130, 0x130}, /* PDS_CPU_CORE_CFG1: PLL mux + MCU1 enable. */
    /* SDK mcu_misc_reg.h MCU_E907_RTC: EN31/RST30/SRC29/DIV9:0. */
    {0x20009014, 0x8000000a, 0xe00003ff},
    {0x2000058c, 0x1e1fc, 0x1e1fc}, /* GLB_CGEN_CFG3, SDK reset/RW fields. */
    /* Fixed SDK glb_reg.h reset/RW fields: peripheral clock/config latches.
     * Their downstream buses are modeled separately, never inferred ready. */
    {0x20000584, 0x9b23cffd, 0xfffffffd},
    {0x20000588, 0x0fff0c11, 0x0fff0c11},
    {0x200001f0, 0x00000103, 0x0000031f},
    {0x20000180, 0x01ff0000, 0x03ff0000},
    {0x20000140, 0x008f0000, 0x00bf0000},
    {0x20000420, 0x00000000, 0xf8000000},
    {0x20000510, 0x00000b00, 0xe008bf3c},
    {0x20000250, 0x033e13e8, 0xb37f37ff},
    {0x20000254, 0, 0x00000f03}, /* SDK top/ISP PLL80M/160M mux selects. */
    {0x200001c0, 0x00000103, 0x0000031f},
    {0x200001b0, 0x00000103, 0x000f031f}, /* SDK SPI clock/divider/pad swap. */
    {0x20000838, 0x17228000, 0x3ff7ffff},
    {0x20000814, 0x01110214, 0x03330f7f}, /* SDK WiFi PLL_CFG1 dividers. */
    {0x20000848, 0x0000ff00, 0x1fffffff}, /* SDK WiFi PLL_CFG14 output control. */
    /* Fixed SDK WiFi PLL configuration bank used by RF XTAL setup.
     * These are storage fields; no analog lock/status is manufactured. */
    {0x20000810, 0x000003ff, 0x00000fff},
    {0x20000818, 0x00000621, 0x000007f1},
    {0x2000081c, 0x00036120, 0x0007f131},
    {0x20000820, 0x00000011, 0x00000133},
    {0x20000824, 0x0000001d, 0x0000003f},
    {0x20000828, 0x05800000, 0x87ffffff},
    {0x2000082c, 0x00000002, 0x00033133},
    {0x20000830, 0x00101000, 0x80001fff},
    {0x20000834, 0x00004000, 0x00007ff7},
    {0x2000083c, 0x000032f0, 0x000177ff},
    {0x20000840, 0x17228000, 0x3ff7ffff},
    {0x20000844, 0x000023e0, 0x000177ff},
    {0x20000884, 0x73b0c330, 0xfff3f777}, /* SDK LDO18IO config, OCP RO0. */
    {0x20000110, 0x00000102, 0x000001bf}, /* ADC clock config. */
    {0x20000548, 0x00000010, 0xf100003f}, /* PKA mux/shared reset config. */
    /* Explicit observed reset-control fixture, not full reset-domain hardware.
     * Native modem writes0xc00 after clearing0x300; 544 only clearsbit2. */
    {0x20000540, 0, 0x00000c10}, {0x20000544, 0, 0},
    {0x20000580, 0x1f, 3}, /* SDK CPU/SDU gates; other gate fields held. */
    {0x24c0c01c, 0, 0x03ff03ff}, {0x24c0c020, 0, 0x03ff03ff},
    {0x24c0c02c, 0, 0x000000ff}, {0x24c0c018, 0, 0x000003ff},
    /* SDK macsw_platform_init: CRM enable and SYSCTRL configuration;
     * explicit cold zero, no pending interrupt or network state implied. */
    {0x24940010, 0, 0x28000000}, {0x24900068, 0, 0x8000000c},
    {0x24940018, 0, 0x11}, /* Native crm_mdm_reset pulse, not self-clearing. */
    {0x24940008, 0, 0xf0}, /* Native crm PHY clock selector fields4..7. */
    /* Observed original phy_init configuration fields. Zero is a virtual
     * cold latch value, not recovered silicon reset data; no ready status. */
    {0x24c00820, 0, 0x13317377}, {0x24c00800, 0, 0x00000fff},
    {0x24c00824, 0, 0x03730177}, {0x24c00324, 0, 0x003f0000},
    {0x24c00870, 0, 0x01000000}, {0x24c008d4, 0, 0x03ff0000},
    {0x24c008d8, 0, 0x03ff03ff}, {0x24c008e0, 0, 0x03ff03ff},
    {0x24c00814, 0, 0x4daff1e8}, {0x24c00894, 0, 0x01100000},
    {0x24c00834, 0, 0xff000010}, {0x24c00848, 0, 0x10000000},
    {0x24c00844, 0, 0x10000000}, {0x24c008e4, 0, 0x00740000},
    {0x24c0b100, 0, 0x00007f7f}, {0x24c0b390, 0, 0x000117f3},
    {0x24c0b3a4, 0, 0x0000ffff}, {0x24c0b394, 0, 0x00ff0000},
    {0x24c0b398, 0, 0x0000ff00}, {0x24c0b3c4, 0, 0xffff00ff},
    {0x24c0b364, 0, 0x1f3f3f3f}, {0x24c0b368, 0, 0x003ff3ff},
    {0x24c0b36c, 0, 0x07ff07ff}, {0x24c0b370, 0, 0x007f0000},
    {0x24c0b3c0, 0, 0xff00ffff}, {0x24c0b3a0, 0, 0x000000ff},
    {0x24c0b380, 0, 0xfffffc00}, {0x24c0b384, 0, 0xfffffc00},
    {0x24c0b388, 0, 0xfffffc00}, {0x24c0b38c, 0, 0xffffdc00},
    {0x24c0c830, 0, 0xffffdc00}, {0x24c0c838, 0, 0x8007ffff},
    {0x24c0c83c, 0, 0x800fffff}, {0x24c0c840, 0, 0x803fffff},
    {0x24c0c834, 0, 0x43}, {0x24c0c814, 0, 0x000003ff},
    {0x24c0c040, 0, 0x01ff8000}, {0x24c0c044, 0, 0x0000ffff},
    {0x24c0b414, 0, 0x100}, {0x24c0b304, 0, 0x7f007f00},
    {0x24c0b004, 0, 1},
    {0x24c0b3bc, 0, 0x007a1200}, {0x24c0c864, 0, 0x0000c078},
    {0x24c0b440, 0, 1}, {0x24c0c860, 0, 0x1ec0},
    {0x24c0b3b4, 0, 0x00d406f8},
    {0x24c0c080, 0, UINT32_MAX}, {0x24c0c084, 0, UINT32_MAX},
    {0x24c0c088, 0, 0xff}, {0x24b00700, 0, 2},
    {0x24c0b3b0, 0, 0xaaaa0301}, {0x24c0b3a8, 0, 0x00550a0a},
    {0x24c0b3ac, 0, 0x1b5ff2ff},
    /* Observed phy_hw_set_channel BW0 / 2412MHz configuration. */
    {0x24c0084c, 0, 0x02007fff}, {0x24c008ec, 0, 0xffff},
    {0x24c0b080, 0, 0}, {0x24c00850, 0, 0},
    {0x24c00808, 0, 0xf7c0fbff}, {0x24c008f0, 0, 2},
    {0x24c00838, 0, 0x18c}, {0x24c0088c, 0, 0xff00},
    {0x24c0081c, 0, 0x02000f0a}, {0x24c00804, 0, 0xff000000},
    {0x24c0b374, 0, 0x03000000}, {0x24c0b340, 0, 0},
    {0x24c0b344, 0, 0}, {0x24c0b348, 0, 0},
    {0x24c00860, 0, 0x7f05}, {0x24c00828, 0, 0x01101928},
    {0x24c008f4, 0, 0x707}, {0x24c0c80c, 0, 0xff000000},
    /* Five packed coefficient words copied from original .data policy table
     * 62fc2d00 (ROM a0002814); all four bytes are programmed data. */
    {0x24c00330, 0, UINT32_MAX}, {0x24c00334, 0, UINT32_MAX},
    {0x24c00338, 0, UINT32_MAX}, {0x24c0033c, 0, UINT32_MAX},
    {0x24c00340, 0, UINT32_MAX},
    {0x249000e0, 0, 0x0001ff00},
    {0x24900084, 0, 1}, /* Native WiFi loop mirrors MACtime.bit19 to bit0. */
    /* Empty-radio MAC profile: original crm_get_mac_freq returns60MHz.
     * Initial timing fields are synthetic zero, not recovered silicon. */
    {0x24b000e4, 60, 0x0fffffff}, {0x24b000e8, 0, 0x00ffff00},
    {0x24b000ec, 0, 0x3ff003ff}, {0x24b000f4, 0, 0x00ffff00},
    {0x24b000f8, 0, 0x00ffff00}, {0x24b00104, 0, 0x3ff003ff},
    {0x24b000f0, 0, 3},
    {0x24b00400, 0, 0x69}, {0x24b00404, 0, 0x0024f637},
    {0x24b00410, 0, 1}, {0x24920004, 0, 0x5010001f},
    {0x24920028, 0, 0xa},
    /* Explicit minimal empty-radio capabilities: optional feature bits0;
     * RX buffer-profile byte9 satisfies ORIGINAL0041 threshold8. Neither
     * value claims the physical chip identity or its reset state. */
    {0x24b00004, 0, 0}, {0x24b00008, 0, 0},
    {0x24b000d8, 0x09020908, 0x00ffffff},
    {0x24b0004c, 0, 0x06083fc3}, {0x24b00054, 0, 0x0001003f},
    {0x24b00010, 0, UINT32_MAX}, {0x24b00014, 0, 0xffff},
    {0x24b080a4, 0, 0}, {0x24b080a8, 0, 0},
    {0x24b00324, 0, 0x400},
    {0x24b0002c, 0, 0xff00}, /* Native vif policy bitmap, a0160a82. */
    {0x24b00060, 0, 0x7fffffde}, {0x24b00064, 0, 0xff900064},
    {0x24b00150, 0, 0x00001000}, {0x24b00224, 0, 0},
    {0x24b000a0, 0, 0x00ffffff}, {0x24b00510, 0, 0x00001c25},
    {0x24b00310, 0, 0x00730080}, {0x24b0009c, 0, 0x1c000000},
    {0x24b00100, 0, 0x00ffffff}, {0x24b00360, 0, 2},
    {0x24b08354, 0, 0xf0000000}, /* Native txl_he_init high-nibble config. */
    /* Native rxl_hwdesc_init programs actual guest EM buffer pointers.
     * No incoming radio data or descriptor completion is manufactured. */
    {0x24b081c8, 0, UINT32_MAX}, {0x24b081cc, 0, UINT32_MAX},
    {0x24b081d0, 0, UINT32_MAX}, {0x24b081d4, 0, UINT32_MAX},
    {0x24b081d8, 0, UINT32_MAX}, {0x24b081dc, 0, UINT32_MAX},
    {0x24b081e0, 0, UINT32_MAX}, {0x24b081e4, 0, UINT32_MAX},
    {0x24b081e8, 0, 0x20001105}, {0x24b0010c, 0, 0x000fffff},
    /* Explicit zero-reset modem configuration latches, fixed SDK rfc.o
     * modem_init_core+2da..596. They do not imply PHY/MAC readiness. */
    {0x20002ca8, 0, 0x3fffffff}, {0x20002c0c, 0, 0x000003ff},
    {0x20002c60, 0, 0x000003ff}, {0x20002c3c, 0, 0x40000000},
    {0x20002c38, 0, 0x08000000}, {0x20002c68, 0, 0x0000000f},
    {0x20002c7c, 0, 0x0000000f}, {0x20002c6c, 0, 0x0000000f},
    {0x20002c80, 0, 0x0000000f}, {0x20002c20, 0, 0x0001ff00},
    {0x20002c70, 0, 0x0fffffff}, {0x20002c84, 0, 0x0fffffff},
    {0x20002d10, 0, 0x0fffffff}, {0x20002c78, 0, 0x0fffffff},
    {0x2000288c, 0, 0x007fc000}, {0x20002890, 0, 0x01fe0000},
    {0x2000282c, 0, 0x0003ffff}, {0x20002808, 0, 0x00800000},
    {0x200028a8, 0, 0x000000c8}, /* Only native literal0xc8 is accepted. */
    {0x20000150, 0x00000017, 0x00000017}, /* UART clock mux/divider. */
    {0x2000f818, 0x18180048, 0x3fff1f7f}, /* AON DCDC_TOP1, SDK reset/RW. */
    {0x2000f808, 3, 3}, /* AON_MISC software SoC/WB enable, SDK reset/RW. */
    {0x2000f814, 0x8258071b, 0x7fff1f1f}, /* SDK DCDC_TOP0 config fields. */
    {0x2000f824, 0x64444008, 0x6f777f3f}, /* SDK DCDC_TOP2 config fields. */
    {0x2000f884, 0xb410f8f0, 0xfffffbff}, /* AON XTAL_CFG, cap-in/out latch. */
    {0x20056800, 0x240003, 0xff6fff78},
    {0xe0800000, 0, UINT32_MAX}, {0xe0800004, (4 << 21) | 80, 0},
    {0xe0800008, 0, 0}, /* MINTTHRESH: restricted zero-threshold adapter. */
};

typedef struct AP01MMIO {
    MemoryRegion region;
    AP01State *state;
    hwaddr base;
} AP01MMIO;

struct AP01State {
    MachineState parent_obj;
    RISCVHartArrayState cpus;
    MemoryRegion ram_alias[4];
    MemoryRegion psram;
    MemoryRegion psram_alias[3];
    MemoryRegion xip;
    MemoryRegion hbn_ram;
    bool mhint_latch;
    uint32_t mhint;
    bool mtvt_latch;
    uint32_t mtvt;
    bool cache_csr_latches;
    uint32_t mxstatus;
    uint32_t mhcr;
    uint32_t mexstatus;
    bool boot_mmio;
    AP01MMIO mmio[7];
    AP01Register *registers;
    uint32_t *register_values;
    size_t register_count;
    bool efuse_shadow;
    uint8_t *shadow_bytes;
    uint8_t *shadow_written;
    char *otp_fixture_path;
    gchar *otp_fixture;
    unsigned otp_busy_reads;
    bool psram_mock;
    bool boot2_mock;
    bool clic_mock;
    AP01UART uart[2];
    AP01SPI spi;
    bool spi_initialized;
    AP01DMA dma;
    AP01PWM pwm;
    AP01Timer peripheral_timer;
    AP01RF rf;
    bool rf_calibration_mock;
    bool mac_empty_mock;
    bool trng_mock;
    bool gpio3_input_high;
    bool gpio24_input_high;
    AP01TRNG trng;
    MemoryRegion bt_software_heap;
    AP01MAC mac;
    AP01MACHW machw;
    uint32_t wifi_reset_generation;
    uint32_t phy_reset_generation;
    AP01MMIO mac_mmio[7];
    uint32_t phy_table[512]; /* Native phy_init copies exactly 2048 bytes. */
    AP01RFCal rf_cal;
    AP01RFACal rf_acal;
    AP01RFPM rf_pm;
    QEMUTimer *te_timer;
    bool te_level;
    AP01MMIO bootrom_api;
    AP01MMIO clock_mailbox;
    MemoryRegion mock_bootrom;
    char *rom_shim_path;
    MemoryRegion rom_shim;
    uint32_t rom_shim_table[512];
    bool pec_uart_mock;
    AP01PEC pec;
    AP01MMIO pec_api;
    AP01MMIO pec_mailbox;
    uint32_t pec_shim_table[128];
    uint8_t pec_header[16];
    char *flash_fixture_path;
    AP01Flash flash;
    AP01MMIO flash_mailbox;
    uint32_t clock_xtal_arg;
    uint32_t clock_pll_arg;
    unsigned clock_args_written;
    uint32_t clock_result;
    uint32_t virtual_xtal_hz;
    uint32_t virtual_wifi_pll_hz;
    uint32_t virtual_audio_pll_hz;
    uint32_t clock_request_count;
    uint32_t mcu_clock_arg;
    bool mcu_clock_pending;
    uint32_t mcu_clock_result;
    uint32_t virtual_mcu_hz;
    unsigned psram_grant_reads;
    unsigned psram_command_reads;
    uint32_t psram_command;
    AP01MMIO mtimer_mmio;
    AP01MMIO mac_time_mmio;
    int64_t mac_time_epoch_ns;
    int64_t sysctrl_time_epoch_ns;
    QEMUTimer *mtimer;
    qemu_irq mtimer_irq;
    uint64_t mtimer_ticks;
    uint64_t mtimer_compare;
    int64_t mtimer_epoch_ns;
    uint32_t mtimer_hz;
    bool mtimer_pending;
    bool clic_source[80];
    qemu_irq clic_wakeup;
    uint32_t *clic_regs;
    bool clic_any_pending;
};

static uint32_t *ap01_register_value(AP01State *s, hwaddr address)
{
    for (size_t i = 0; i < s->register_count; i++) {
        if (s->registers[i].address == address) {
            return &s->register_values[i];
        }
    }
    g_assert_not_reached();
}

static int ap01_clic_select(void *opaque)
{
    AP01State *s = opaque;
    int selected = RISCV_EXCP_NONE;
    unsigned best = 0;
    if (!s->clic_any_pending) {
        return selected;
    }
    for (unsigned i = 0; i < 80; i++) {
        uint32_t reg = s->clic_regs[i];
        if ((reg & 0x101) != 0x101 ||
            (i < 16 && !(s->cpus.harts[0].env.mie & (1ULL << i)))) {
            continue;
        }
        unsigned priority = reg >> 28;
        if (selected == RISCV_EXCP_NONE || priority >= best) {
            best = priority;
            selected = i;
        }
    }
    return selected;
}

static void ap01_clic_update(AP01State *s)
{
    bool pending = false;
    if (!s->clic_mock || !s->clic_wakeup) {
        return;
    }
    for (unsigned i = 0; i < 80; i++) {
        uint32_t reg = s->clic_regs[i];
        pending |= (reg & 0x101) == 0x101;
    }
    s->clic_any_pending = pending;
    /* Architectural MEIP is only the CPU wakeup wire. The opt-in callback
     * preserves the original CLIC ID and checks each architectural mie bit.
     * Keep wakeup high while individually masked CPU sources are pending. */
    qemu_set_irq(s->clic_wakeup, pending);
    if (s->mtimer_irq) {
        qemu_set_irq(s->mtimer_irq, s->mtimer_pending &&
                     (s->clic_regs[7] & 0x100));
    }
}

static void ap01_clic_input(void *opaque, int id, int level)
{
    AP01State *s = opaque;
    uint32_t *reg = &s->clic_regs[id];
    bool edge = *reg & 0x20000;
    if (!edge || (level && !s->clic_source[id])) {
        *reg = (*reg & ~1U) | !!level;
    }
    s->clic_source[id] = level;
    ap01_clic_update(s);
}

static void ap01_gpio_update(AP01State *s)
{
    bool pending = false;
    for (unsigned i = 0; i < 35; i++) {
        uint32_t *reg = ap01_register_value(s, 0x200008c4 + i * 4);
        unsigned mode = (*reg >> 16) & 15;
        bool input = *reg & (1U << 28);
        if ((*reg & 1) && (mode == 2 || mode == 3 || mode == 10 || mode == 11)) {
            bool active = (mode & 1) ? input : !input;
            *reg = (*reg & ~(1U << 21)) | (active ? 1U << 21 : 0);
        }
        pending |= (*reg & (1U << 21)) && !(*reg & (1U << 22));
    }
    ap01_clic_input(s, 60, pending);
}

static void ap01_te_sync(AP01State *s)
{
    if (ap01_spi_need_te(&s->spi)) {
        if (!timer_pending(s->te_timer)) {
            timer_mod(s->te_timer, qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) + 8333333);
        }
    } else {
        timer_del(s->te_timer);
    }
}

static void ap01_te_tick(void *opaque)
{
    AP01State *s = opaque;
    if (ap01_spi_need_te(&s->spi)) {
        uint32_t *reg = ap01_register_value(s, 0x200008c4 + 23 * 4);
        bool previous = *reg & (1U << 28);
        unsigned mode = (*reg >> 16) & 15;
        s->te_level = !s->te_level;
        *reg = (*reg & ~(1U << 28)) | (s->te_level ? 1U << 28 : 0);
        if ((*reg & 1) && ((mode == 4 && previous != s->te_level) ||
            ((mode == 0 || mode == 8) && previous && !s->te_level) ||
            ((mode == 1 || mode == 9) && !previous && s->te_level))) {
            *reg |= 1U << 21;
        }
        ap01_gpio_update(s);
    }
    /* Explicit ideal panel fixture: 60Hz TE, two half-period edges. */
    ap01_te_sync(s);
}

/* SDK 41bf338a bl616_clock.c CPU_Set/Get_MTimer_*: EN gates counting,
 * RST holds zero, source0=XCLK/source1=MCU, rate=source/(DIV+1).
 * core_rv32.h CORET: compare=e0004000, read-only time=e000bff8.
 * Ideal QEMU virtual time only. Optional CLIC uses a restricted flat selector
 * and common trap; SHV and nested priority state remain unsupported. */
static uint64_t ap01_mtimer_read_time(void *opaque)
{
    AP01State *s = opaque;
    return s->mtimer_ticks + muldiv64(
        qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) - s->mtimer_epoch_ns,
        s->mtimer_hz, NANOSECONDS_PER_SECOND);
}

static void ap01_mtimer_update(void *opaque)
{
    AP01State *s = opaque;
    int64_t now = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL);
    uint64_t ticks = ap01_mtimer_read_time(s);
    uint64_t diff, remaining;

    timer_del(s->mtimer);
    s->mtimer_pending = ticks >= s->mtimer_compare;
    qemu_set_irq(s->mtimer_irq, !s->clic_mock && s->mtimer_pending);
    if (s->clic_mock) {
        ap01_clic_input(s, 7, s->mtimer_pending);
    }
    if (!s->mtimer_hz || ticks >= s->mtimer_compare) {
        return;
    }
    diff = s->mtimer_compare - ticks;
    remaining = INT64_MAX - now;
    if (diff >= muldiv64(remaining, s->mtimer_hz,
                        NANOSECONDS_PER_SECOND)) {
        /* Unreachable before the virtual clock's signed-ns horizon.
         * Scheduling INT64_MAX would spin if icount warps to that horizon. */
        return;
    } else {
        /* Round upward; callback rechecks the counter at the deadline. */
        timer_mod(s->mtimer, now + muldiv64(diff, NANOSECONDS_PER_SECOND,
                                          s->mtimer_hz) + 1);
    }
}

static void ap01_mtimer_configure(AP01State *s, uint32_t config)
{
    uint32_t source = (config & (1U << 29)) ? s->virtual_mcu_hz :
                      s->virtual_xtal_hz;
    s->mtimer_ticks = (config & (1U << 30)) ? 0 : ap01_mtimer_read_time(s);
    s->mtimer_epoch_ns = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL);
    /* Explicit pre-clock handoff assumption matches SDK's 32MHz fallback. */
    s->mtimer_hz = (config & (1U << 31)) && !(config & (1U << 30)) ?
                  (source ? source : 32000000) / ((config & 0x3ff) + 1) : 0;
    ap01_mtimer_update(s);
}

/* Explicit synthetic peripheral: two CTRL reads grant a request, two more
 * complete a supported config transaction. No physical timing/DMA model.
 * SDK 41bf338a bl616_psram.c Winbond_Read/Write_Reg and bl616_psram.h enum:
 * vendor=1, WB_REG_SEL=2 selects CR0. ID0/ID1 read as explicit synthetic zero
 * identifiers (not real Winbond identity). Other protocols stay unknown.
 * The readback is the modeled controller status view (not a claimed external
 * PSRAM wire-register encoding); status changes only after a committed write. */
static MemTxResult ap01_psram_control(AP01State *s, uint64_t *value,
                                     unsigned size, unsigned shift, bool write)
{
    uint32_t *control = ap01_register_value(s, 0x20052000);
    uint32_t current = *control;
    if (write) {
        uint32_t mask = (UINT32_MAX >> ((4 - size) * 8)) << shift;
        uint32_t next = (current & ~mask) | (((uint32_t)*value << shift) & mask);
        uint32_t pulse = (next & (1 << 16)) ?
                         (((uint32_t)*value << shift) & mask & 0x3000) : 0;
        if (pulse && (size != 4 || shift || pulse == 0x3000 ||
                      !(current & (1 << 17)) || !(next & (1 << 16)) ||
                      (next & 7) != 1 ||
                      (pulse == 0x1000 && ((next >> 8) & 7) != 2) ||
                      (pulse == 0x2000 && ((next >> 8) & 7) > 2) ||
                      s->psram_command_reads)) {
            return MEMTX_ERROR;
        }
        if (s->psram_command_reads && (next & (1 << 16)) &&
            ((next ^ current) & 0xf0fd0777)) {
            return MEMTX_ERROR;
        }
        *control = (current & ~0xf0fd0777U) | (next & 0xf0fd0777U);
        if (!(next & (1 << 16))) {
            *control &= ~((1 << 17) | 0x3000);
            if (s->psram_command_reads) {
                *control &= ~(s->psram_command << 2);
            }
            s->psram_grant_reads = s->psram_command_reads = 0;
            s->psram_command = 0;
        } else if (!(current & (1 << 16))) {
            *control &= ~(1 << 17);
            s->psram_grant_reads = 2;
        }
        if (pulse) {
            *control = (*control | pulse) & ~(pulse << 2);
            s->psram_command = pulse;
            s->psram_command_reads = 2;
        }
        return MEMTX_OK;
    }
    if (s->psram_grant_reads && --s->psram_grant_reads == 0) {
        *control |= 1 << 17;
    }
    if (s->psram_command_reads && --s->psram_command_reads == 0) {
        uint32_t *status = ap01_register_value(s, 0x20052014);
        if (s->psram_command == 0x1000) {
            uint32_t config = *ap01_register_value(s, 0x20052010);
            *status = (*status & ~0x411f37ffU) | (config & 0x411f37ffU);
        } else {
            uint32_t *manual = ap01_register_value(s, 0x20052004);
            uint32_t result = ((*control >> 8) & 7) == 2 ? *status & 0xffff : 0;
            *manual = (*manual & 0xffff) | (result << 16);
        }
        *control = (*control & ~s->psram_command) | (s->psram_command << 2);
        s->psram_command = 0;
    }
    *value = (*control >> shift) & (UINT32_MAX >> ((4 - size) * 8));
    return MEMTX_OK;
}

static MemTxResult ap01_rf_acal_inputs_changed(AP01State *s)
{
    uint64_t mode = 0, channel = 0;
    if (ap01_rf_access(&s->rf, 0x14, &mode, 4, false) != MEMTX_OK ||
        ap01_rf_access(&s->rf, 0xc4, &channel, 4, false) != MEMTX_OK) {
        return MEMTX_ERROR;
    }
    ap01_rf_acal_update(&s->rf_acal, mode,
                        s->rf_cal.candidate | ((uint32_t)s->rf_cal.amplitude << 16),
                        channel);
    return MEMTX_OK;
}

static void ap01_machw_reset_complete(void *opaque)
{
    AP01State *s = opaque;
    for (size_t i = 0; i < s->register_count; i++) {
        hwaddr r = s->registers[i].address;
        if ((r >= 0x24b00000 && r < 0x24b01000) ||
            (r >= 0x24b08000 && r < 0x24b08400)) {
            s->register_values[i] = s->registers[i].reset;
        }
    }
    s->mac_time_epoch_ns = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL);
    ap01_rf_acal_set_epoch(&s->rf_acal, s->mac_time_epoch_ns);
}

static uint32_t ap01_mac_now_us(void *opaque)
{
    AP01State *s = opaque;
    return (uint32_t)((qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) -
                       s->mac_time_epoch_ns) / 1000);
}

static void ap01_mac_gen_level(void *opaque, bool level)
{
    AP01State *s = opaque;
    ap01_mac_set_source54(&s->mac, level);
}

static MemTxResult ap01_mmio_access(void *opaque, hwaddr offset,
                                   uint64_t *value, unsigned size, bool write)
{
    AP01MMIO *mmio = opaque;
    AP01State *s = mmio->state;
    hwaddr address = mmio->base + offset;
    unsigned shift = (address & 3) * 8;
    if (address >= 0x20004200 && address < 0x20004240) {
        return s->trng_mock ? ap01_trng_access(&s->trng, address - 0x20004200,
                                              value, size, write) : MEMTX_ERROR;
    }
    if (!s->mac_empty_mock &&
        (address == 0x24940018 || address == 0x24940008 ||
         (address >= 0x24c00000 && address < 0x24c0d000 &&
          address != 0x24c0c018 && address != 0x24c0c01c &&
          address != 0x24c0c020 && address != 0x24c0c02c))) {
        return MEMTX_ERROR;
    }
    if (s->mac_empty_mock && address == 0x24940018) {
        uint32_t *control = ap01_register_value(s, address);
        if (size != 4) { return MEMTX_ERROR; }
        if (!write) { *value = *control; return MEMTX_OK; }
        if (*value != 0 && *value != 0x11) { return MEMTX_ERROR; }
        if (*value == 0x11 && *control == 0) {
            /* Virtual processing reset pulse. Native channel setup pulses
             * again after configuration: retain programmed PHY latches/table.
             * No active PHY datapath is modeled; clocks/RF/RAM also remain. */
            s->phy_reset_generation++;
        }
        *control = *value;
        return MEMTX_OK;
    }
    if (s->mac_empty_mock && address >= 0x24c0a000 && address < 0x24c0a800) {
        if (size != 4 || (address & 3)) { return MEMTX_ERROR; }
        uint32_t *word = &s->phy_table[(address - 0x24c0a000) / 4];
        if (write) { *word = *value; } else { *value = *word; }
        return MEMTX_OK;
    }
    if (s->mac_empty_mock && address == 0x24900040) {
        /* Independent sysctrl_timer_get profile for native debug stats.
         * 1MHz is an explicit virtual rate; physical rate/gates unknown.
         * MAC-core soft-reset does not reset this clock domain. */
        if (size != 4 || write) { return MEMTX_ERROR; }
        *value = (uint32_t)((qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) -
                            s->sysctrl_time_epoch_ns) / 1000);
        return MEMTX_OK;
    }
    if (s->mac_empty_mock && address >= 0x24b00200 && address <= 0x24b0020c) {
        /* Explicit empty queue configuration. Original mm_env helper
         * reads bits27:12; zero selects its native default150. */
        if (size != 4 || (address & 3) || write) { return MEMTX_ERROR; }
        *value = 0;
        return MEMTX_OK;
    }
    if (s->mac_empty_mock && ap01_machw_handles(address)) {
        return ap01_machw_access(&s->machw, address, value, size, write);
    }
    if (s->mac_empty_mock && (address == 0x24c00000 || address == 0x24c0003c)) {
        /* Explicit unit-count virtual capability. Fields are
         * one-based; zero would underflow native getters to255. */
        if (size != 4 || write) { return MEMTX_ERROR; }
        /* Virtual version1.2 satisfies the native init constraint
         * minor+10*(major+2)==32; it is not recovered silicon identity.
         * Fourth count nibble is consumed by phy_init (a0010f8e). */
        *value = address == 0x24c00000 ? 0x1111 : 0x01020000;
        return MEMTX_OK;
    }
    if (s->mac_empty_mock && address >= 0x24910000 && address < 0x24910100) {
        return ap01_mac_access(&s->mac, address - 0x24910000, value, size, write);
    }
    if (s->mac_empty_mock && address >= 0x24a00000 && address < 0x24a00100) {
        return ap01_mac_dma_access(&s->mac, address - 0x24a00000, value, size, write);
    }
    if (s->mac_empty_mock && address == 0x24f10388) {
        /* Explicit original-platform zero-write fixture; physical role
         * unknown. No readback, reset-ready or completion is fabricated. */
        if (size != 4 || !write || *value != 0) { return MEMTX_ERROR; }
        s->mac.platform_zero_writes++;
        return MEMTX_OK;
    }
    if ((size != 1 && size != 2 && size != 4) || (address & 3) + size > 4) {
        return MEMTX_ERROR;
    }
    /* Original/SDK phyrf udelay uses a 32-bit microsecond counter here.
     * Independent ideal MAC time, not a radio-ready or TSF association signal. */
    if (s->boot2_mock && (address == 0x24b00120 ||
                         (s->mac_empty_mock && address == 0x24b00124))) {
        if (write || size != 4 || s->virtual_wifi_pll_hz != 960000000) {
            return MEMTX_ERROR;
        }
        uint64_t ticks = (qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) -
                          s->mac_time_epoch_ns) / 1000;
        /* Original a016785c..7878 reads high/low/high and masks high16. */
        *value = address == 0x24b00120 ? (uint32_t)ticks :
                                       (uint32_t)(ticks >> 32) & 0xffff;
        return MEMTX_OK;
    }
    if (s->pec_uart_mock && address >= 0xf1000080 && address < 0xf1000098) {
        return ap01_pec_access(&s->pec, address - 0xf1000080, value, size, write);
    }
    if (s->flash.bytes && address >= 0xf10000c0 && address < 0xf10000e0) {
        return ap01_flash_access(&s->flash, address - 0xf10000c0, value, size, write);
    }
    if (s->pec_uart_mock && address >= 0xa066e000 && address < 0xa066e200) {
        unsigned off = address - 0xa066e000;
        if (write) {
            return MEMTX_ERROR;
        }
        if (off < sizeof(s->pec_header)) {
            *value = 0;
            for (unsigned i = 0; i < size; i++) {
                *value |= (uint64_t)s->pec_header[off + i] << (i * 8);
            }
            return MEMTX_OK;
        }
        if (size != 4 || shift || !s->pec_shim_table[off / 4]) {
            qemu_log_mask(LOG_GUEST_ERROR, "AP01 unknown synthetic PEC API offset=%x\n", off);
            return MEMTX_ERROR;
        }
        /* Original callers add the table word to PEC's base in RV32.
         * This explicit API-binding overlay redirects only declared leaves. */
        *value = s->pec_shim_table[off / 4] - (uint32_t)0xa066e000;
        return MEMTX_OK;
    }
    if (s->boot2_mock && address >= 0x20001000 && address < 0x20002000) {
        if (s->rf_calibration_mock && (address == 0x20001620 || address == 0x20001624)) {
            uint32_t sampled;
            if (write || size != 4) { return MEMTX_ERROR; }
            MemTxResult result = address == 0x20001620 ?
                ap01_rf_pm_read_result(&s->rf_pm, &sampled) :
                ap01_rf_pm_read_result_q(&s->rf_pm, &sampled);
            if (result != MEMTX_OK) {
                return MEMTX_ERROR;
            }
            *value = sampled;
            return MEMTX_OK;
        }
        if (s->rf_calibration_mock && address == 0x20001618) {
            uint64_t config = 0;
            if (size != 4 || ap01_rf_access(&s->rf, 0x618, &config, 4, false) != MEMTX_OK) {
                return MEMTX_ERROR;
            }
            if (!write) {
                *value = config | (s->rf_pm.trigger ? 0x20000000 : 0) |
                         ap01_rf_pm_read_status(&s->rf_pm);
                return MEMTX_OK;
            }
            /* bit28 is sampled status, bit29 transaction control. */
            uint64_t requested = *value & ~0x30000000ULL;
            uint64_t snapshot[9] = {0};
            const uint32_t offsets[] = {0x14, 0xc4, 0x214, 0x218, 0x61c, 0x70, 0x614, 0x74, 0x600};
            for (unsigned i = 0; i < G_N_ELEMENTS(offsets); i++) {
                if (ap01_rf_access(&s->rf, offsets[i], &snapshot[i], 4, false) != MEMTX_OK) {
                    return MEMTX_ERROR;
                }
            }
            if ((requested ^ config) & ~0xc01fffffULL) { return MEMTX_ERROR; }
            if (ap01_rf_pm_trigger(&s->rf_pm, snapshot[0], snapshot[1],
                    snapshot[2], snapshot[3], snapshot[4], snapshot[5], *value, snapshot[6],
                    snapshot[7], snapshot[8],
                    (*value & 0x20000000) != 0) != MEMTX_OK) { return MEMTX_ERROR; }
            return ap01_rf_access(&s->rf, 0x618, &requested, 4, true);
        }
        /* Explicit virtual selection fixture, not a silicon reset claim.
         * Original full_cal reads bits8/10 to select ROSCAL/RCCAL. Neither
         * optional group is selected here; mandatory TX/RX calibration runs.
         * SDK evidence does not establish the physical meaning of this word. */
        if (s->rf_calibration_mock && address == 0x20001020) {
            if (write || size != 4) { return MEMTX_ERROR; }
            *value = 0;
            return MEMTX_OK;
        }
        if (s->rf_calibration_mock &&
            (address == 0x200010a0 || address == 0x200010a8 || address == 0x200010ac)) {
            if (s->virtual_xtal_hz != 40000000) { return MEMTX_ERROR; }
            MemTxResult result = ap01_rf_cal_access(&s->rf_cal, address - 0x20001000,
                                                    value, size, write);
            if (result == MEMTX_OK && write && address == 0x200010a0) {
                result = ap01_rf_acal_inputs_changed(s);
            }
            return result;
        }
        if (s->rf_calibration_mock && address == 0x200010a4) {
            uint64_t requested = *value;
            if (write) { *value &= ~0x1000ULL; } /* Read-only comparator. */
            MemTxResult result = ap01_rf_access(&s->rf, 0xa4, value, size, write);
            if (write) { *value = requested; }
            if (result == MEMTX_OK && !write && s->rf_acal.mode == 0x40) {
                uint32_t bits;
                if (s->virtual_xtal_hz != 40000000) { return MEMTX_ERROR; }
                if (ap01_rf_acal_measurement_valid(&s->rf_acal)) {
                    result = ap01_rf_acal_read_comparator(&s->rf_acal, &bits);
                } else {
                    /* Setup RMW reads see the held/reset digital output,
                     * not an invented qualified calibration sample. */
                    bits = ap01_rf_acal_output(&s->rf_acal);
                }
                if (result == MEMTX_OK) { *value |= bits; }
            }
            return result;
        }
        if (s->rf_calibration_mock && write && address == 0x200010c4 &&
            s->rf_acal.mode == 0x40 && *value != 0 && *value != 0x01000000 &&
            !ap01_rf_acal_channel_supported(*value)) {
            return MEMTX_ERROR;
        }
        MemTxResult result = ap01_rf_access(&s->rf, address - 0x20001000, value, size, write);
        if (result == MEMTX_OK && write && s->rf_calibration_mock &&
            (address == 0x20001014 || address == 0x200010c4)) {
            result = ap01_rf_acal_inputs_changed(s);
        }
        return result;
    }
    if (s->boot2_mock && address >= 0x2000a500 && address < 0x2000a600) {
        return ap01_timer_access(&s->peripheral_timer, address - 0x2000a500,
                                 value, size, write);
    }
    /* Fixed SDK GPIO_CFG136..141: shared output bank and SET/CLEAR strobes.
     * Same per-pad latch as CFG0..34; strobes must reach LCD CS/DC/reset too. */
    if (s->boot2_mock && address >= 0x20000ae4 && address <= 0x20000af8) {
        unsigned index = (address - 0x20000ae4) / 4;
        unsigned bank = index & 1;
        uint32_t valid = bank ? 7 : UINT32_MAX;
        if (size != 4 || shift || (write && (*value & ~valid))) {
            return MEMTX_ERROR;
        }
        if (!write) {
            if (index >= 2) { return MEMTX_ERROR; }
            *value = 0;
            for (unsigned bit = 0; bit < (bank ? 3 : 32); bit++) {
                *value |= (uint64_t)((*ap01_register_value(s,
                    0x200008c4 + 4 * (bank * 32 + bit)) >> 24) & 1) << bit;
            }
            return MEMTX_OK;
        }
        for (unsigned bit = 0; bit < (bank ? 3 : 32); bit++) {
            uint32_t *pad = ap01_register_value(s,
                0x200008c4 + 4 * (bank * 32 + bit));
            if (index < 2 || (*value & (1U << bit))) {
                if (index < 4 && (*value & (1U << bit))) { *pad |= 1U << 24; }
                else { *pad &= ~(1U << 24); }
            }
        }
        ap01_gpio_update(s);
        if (s->spi_initialized) {
            ap01_spi_lines_changed(&s->spi);
            ap01_te_sync(s);
        }
        return MEMTX_OK;
    }
    if (s->boot2_mock && address >= 0x2000a000 && address < 0x2000a200) {
        return ap01_uart_access(&s->uart[(address - 0x2000a000) / 0x100],
                                address & 0xff, value, size, write);
    }
    if (s->boot2_mock && address >= 0x2000a200 && address < 0x2000a300) {
        MemTxResult result = ap01_spi_access(&s->spi, address - 0x2000a200,
                                             value, size, write);
        ap01_te_sync(s);
        return result;
    }
    if (s->boot2_mock && address >= 0x2000c000 && address < 0x2000c900) {
        return ap01_dma_access(&s->dma, address - 0x2000c000, value, size, write);
    }
    if (s->boot2_mock && address >= 0x2000a400 && address < 0x2000a500) {
        return ap01_pwm_access(&s->pwm, address - 0x2000a400, value, size, write);
    }
    if (s->clic_mock && address >= 0xe0801000 && address < 0xe0801140) {
        unsigned id = (address - 0xe0801000) / 4;
        uint32_t *reg = &s->clic_regs[id];
        uint32_t mask = (UINT32_MAX >> ((4 - size) * 8)) << shift;
        if (write) {
            uint32_t next = (*reg & ~mask) | (((uint32_t)*value << shift) & mask);
            /* Non-SHV level and edge setup only. Four implemented CTL bits.
             * Fixed NLBITS4, zero threshold, non-nested common-trap subset. */
            if (((next >> 16) & 0xff) != 0 && ((next >> 16) & 0xff) != 2) {
                return MEMTX_ERROR;
            }
            if (id == 7 && ((next >> 16) & 0xff)) {
                return MEMTX_ERROR;
            }
            *reg = next & 0xf0020101;
            if (!(*reg & 0x20000)) {
                *reg = (*reg & ~1U) | s->clic_source[id];
            }
            ap01_clic_update(s);
        } else {
            uint32_t current = *reg;
            if (id == 7) {
                current = (current & ~1U) | s->mtimer_pending;
            }
            *value = (current >> shift) & (UINT32_MAX >> ((4 - size) * 8));
        }
        return MEMTX_OK;
    }
    if (s->clic_mock && write &&
        ((address & ~3ULL) == 0xe0800000 || (address & ~3ULL) == 0xe0800008)) {
        uint32_t current = *ap01_register_value(s, address & ~3ULL);
        uint32_t mask = (UINT32_MAX >> ((4 - size) * 8)) << shift;
        uint32_t next = (current & ~mask) | (((uint32_t)*value << shift) & mask);
        if (((address & ~3ULL) == 0xe0800000 && next != 0 && next != 8) ||
            ((address & ~3ULL) == 0xe0800008 && next != 0)) {
            return MEMTX_ERROR;
        }
    }
    if (s->boot2_mock && (address == 0xe0004000 || address == 0xe0004004 ||
                         address == 0xe000bff8 || address == 0xe000bffc)) {
        uint64_t current;
        unsigned half = (address & 4) ? 32 : 0;
        if (size != 4) {
            return MEMTX_ERROR;
        }
        if (address >= 0xe000bff8) {
            if (write) {
                return MEMTX_ERROR;
            }
            *value = (uint32_t)(ap01_mtimer_read_time(s) >> half);
        } else if (write) {
            current = s->mtimer_compare;
            s->mtimer_compare = (current & ~((uint64_t)UINT32_MAX << half)) |
                               ((uint64_t)(uint32_t)*value << half);
            ap01_mtimer_update(s);
        } else {
            *value = (uint32_t)(s->mtimer_compare >> half);
        }
        return MEMTX_OK;
    }
    if (s->boot2_mock && address == 0x90015d00 && size == 4 && !write) {
        /* ROM_APITABLE[320], signature int(uint32_t, flash_cfg*).
         * Our explicit unsupported result lets the ORIGINAL wrapper try
         * its own flash configuration table; no fabricated success. */
        *value = s->flash.bytes && s->rom_shim_table[320] ?
                 s->rom_shim_table[320] : 0x90000000;
        return MEMTX_OK;
    }
    if (s->boot2_mock && address == 0x900159cc && size == 4 && !write) {
        /* ROM_APITABLE[115]: BL_Err_Type(uint8_t xtal, uint8_t pll). */
        *value = 0x90000020;
        return MEMTX_OK;
    }
    if (s->boot2_mock && address == 0x90015a40 && size == 4 && !write) {
        /* ROM_APITABLE[144]: BL_Err_Type(uint8_t clkFreq). */
        *value = 0x90000040;
        return MEMTX_OK;
    }
    if (s->boot2_mock && address == 0x90015b8c && size == 4 && !write) {
        /* ROM_APITABLE[227]: BL_Err_Type(uint8_t *xtalType). */
        *value = 0x90000060;
        return MEMTX_OK;
    }
    if (s->rom_shim_path && address >= 0x90015800 &&
        address < 0x90016000 && size == 4 && !shift && !write) {
        uint32_t target = s->rom_shim_table[(address - 0x90015800) / 4];
        if (!target) {
            return MEMTX_ERROR;
        }
        *value = target;
        return MEMTX_OK;
    }
    if (s->boot2_mock && address >= 0xf1000000 && address < 0xf1000030) {
        if (size != 4 || shift) {
            return MEMTX_ERROR;
        }
        switch (address - 0xf1000000) {
        case 0:
            if (write) {
                s->clock_xtal_arg = *value;
                s->clock_args_written |= 1;
            } else {
                *value = s->clock_xtal_arg;
            }
            return MEMTX_OK;
        case 4:
            if (write) {
                s->clock_pll_arg = *value;
                s->clock_args_written |= 2;
            } else {
                *value = s->clock_pll_arg;
            }
            return MEMTX_OK;
        case 8:
            if (write) {
                return MEMTX_ERROR;
            }
            if (s->clock_args_written) {
                s->clock_result = 1; /* SDK BL_Err_Type ERROR, not int -1. */
                if (s->clock_args_written == 3 && s->clock_xtal_arg == 4 &&
                    s->clock_pll_arg == 3) {
                    /* Explicit ideal virtual environment. Fixed SDK 41bf338a
                     * bl616_glb.c:523 GLB_Power_On_XTAL_And_PLL_CLK:
                     * XTAL40M, WiFi960M, Audio491.52M; final root is XCLK,
                     * not PLL. bl616_hbn.c HBN_Set_Xtal_Type sets flag8/type4.
                     * We model visible selection/dividers and virtual rates,
                     * not analog power sequencing, physical lock or delays. */
                    uint32_t *rsv3 = ap01_register_value(s, 0x2000f10c);
                    uint32_t *hbn = ap01_register_value(s, 0x2000f030);
                    uint32_t *glb = ap01_register_value(s, 0x20000090);
                    *rsv3 = (*rsv3 & ~0xffU) | 0x84;
                    *hbn = (*hbn & ~3U) | 1;
                    *glb = (*glb & ~0x00ffffc0U) | 0x41;
                    s->virtual_xtal_hz = 40000000;
                    s->virtual_wifi_pll_hz = 960000000;
                    s->virtual_audio_pll_hz = 491520000;
                    s->virtual_mcu_hz = 40000000;
                    ap01_timer_set_clocks(&s->peripheral_timer, 40000000, 40000000);
                    ap01_mtimer_configure(s,
                        *ap01_register_value(s, 0x20009014));
                    s->clock_request_count++;
                    s->clock_result = 0;
                }
                s->clock_args_written = 0;
            }
            *value = s->clock_result;
            return MEMTX_OK;
        case 0xc: if (!write) { *value = s->virtual_xtal_hz; return MEMTX_OK; } break;
        case 0x10: if (!write) { *value = s->virtual_wifi_pll_hz; return MEMTX_OK; } break;
        case 0x14: if (!write) { *value = s->virtual_audio_pll_hz; return MEMTX_OK; } break;
        case 0x18: if (!write) { *value = s->clock_request_count; return MEMTX_OK; } break;
        case 0x20:
            if (write) {
                s->mcu_clock_arg = *value;
                s->mcu_clock_pending = true;
            } else {
                *value = s->mcu_clock_arg;
            }
            return MEMTX_OK;
        case 0x24:
            if (write) {
                return MEMTX_ERROR;
            }
            if (s->mcu_clock_pending) {
                s->mcu_clock_result = 1;
                if (s->mcu_clock_arg == 5 && s->virtual_xtal_hz == 40000000 &&
                    s->virtual_wifi_pll_hz == 960000000 &&
                    s->virtual_audio_pll_hz == 491520000) {
                    /* SDK 41bf338a bl616_glb.c GLB_Set_MCU_System_CLK:
                     * enum5 = TOP_WIFIPLL_320M. PDS PLL_SEL=3, HCLK_DIV=0,
                     * BCLK_DIV=3; ungate CGEN bit14, root=PLL, keep XCLK.
                     * This yields ideal MCU320MHz/BCLK80MHz; no analog or
                     * divider-protection timing is represented. */
                    uint32_t *pds = ap01_register_value(s, 0x2000e114);
                    uint32_t *glb = ap01_register_value(s, 0x20000090);
                    uint32_t *hbn = ap01_register_value(s, 0x2000f030);
                    uint32_t *cgen = ap01_register_value(s, 0x2000058c);
                    *pds = (*pds & ~0x30U) | 0x30;
                    *hbn |= 2;
                    *glb = (*glb & ~0x00ffffc0U) | 0x00030000 |
                           ((*hbn & 3) << 6);
                    *cgen |= 1 << 14;
                    s->virtual_mcu_hz = s->virtual_wifi_pll_hz / 3;
                    ap01_timer_set_clocks(&s->peripheral_timer, 80000000, 40000000);
                    ap01_mtimer_configure(s,
                        *ap01_register_value(s, 0x20009014));
                    s->mcu_clock_result = 0;
                }
                s->mcu_clock_pending = false;
            }
            *value = s->mcu_clock_result;
            return MEMTX_OK;
        case 0x28: if (!write) { *value = s->virtual_mcu_hz; return MEMTX_OK; } break;
        }
        return MEMTX_ERROR;
    }
    if (address >= 0x20056000 && address + size <= 0x20056800 &&
        s->efuse_shadow) {
        size_t index = address - 0x20056000;
        if (!write) {
            *value = 0;
            for (unsigned i = 0; i < size; i++) {
                if (!s->shadow_written[index + i]) {
                    qemu_log_mask(LOG_GUEST_ERROR,
                                  "AP01 unknown eFuse shadow read at 0x%"
                                  HWADDR_PRIx " (no OTP fixture)\n", address);
                    return MEMTX_ERROR;
                }
                *value |= (uint64_t)s->shadow_bytes[index + i] << (8 * i);
            }
        } else {
            for (unsigned i = 0; i < size; i++) {
                s->shadow_bytes[index + i] = *value >> (8 * i);
                s->shadow_written[index + i] = 1;
            }
        }
        return MEMTX_OK;
    }
    if (!s->otp_fixture && write && (address & ~3ULL) == 0x20056800 &&
        ((*value << shift) & (1 << 4))) {
        qemu_log_mask(LOG_GUEST_ERROR,
                      "AP01 eFuse OTP transaction trigger unsupported\n");
        return MEMTX_ERROR;
    }
    if (s->psram_mock && (address & ~3ULL) == 0x20052000) {
        return ap01_psram_control(s, value, size, shift, write);
    }
    if (s->psram_mock && write && (address & ~3ULL) == 0x20052010) {
        uint32_t current = *ap01_register_value(s, 0x20052010);
        uint32_t mask = (UINT32_MAX >> ((4 - size) * 8)) << shift;
        uint32_t next = (current & ~mask) | (((uint32_t)*value << shift) & mask);
        /* No reset, sleep or deep/input powerdown. Burst/linear selection is
         * retained as configuration; functional RAM has no bus-cycle model. */
        if ((next & ((1U << 31) | (1U << 29) | (1U << 24))) ||
            !(next & (1U << 13)) || s->psram_command_reads) {
            return MEMTX_ERROR;
        }
    }
    if (s->psram_mock && write && (address & ~3ULL) == 0x20052004) {
        uint32_t current = *ap01_register_value(s, 0x20052004);
        uint32_t mask = (UINT32_MAX >> ((4 - size) * 8)) << shift;
        uint32_t next = (current & ~mask) | (((uint32_t)*value << shift) & mask);
        if ((next & 0x3000) || !(next & 0x4000)) {
            return MEMTX_ERROR;
        }
    }
    if (!s->psram_mock && write && (address & ~3ULL) == 0x20052000 &&
        ((*value << shift) & 0x13000)) {
        qemu_log_mask(LOG_GUEST_ERROR,
                      "AP01 PSRAM config command/grant request not modeled\n");
        return MEMTX_ERROR;
    }
    if (s->boot2_mock && ((address & ~3ULL) == 0x20000540 ||
                         (address & ~3ULL) == 0x20000544) &&
        (size != 4 || (write && (address == 0x20000544 ? *value != 0 :
                                (*value & ~0xc10ULL) != 0)))) {
        return MEMTX_ERROR;
    }
    for (size_t i = 0; i < s->register_count; i++) {
        if (s->registers[i].address == (address & ~3ULL)) {
            uint32_t mask = (UINT32_MAX >> ((4 - size) * 8)) << shift;
            if (s->mac_empty_mock && write && address >= 0x24b081c8 &&
                address <= 0x24b081e4 &&
                (size != 4 || (address & 3) || (*value & 3) ||
                 *value < 0x22fc0000 || *value >= 0x23038000)) {
                return MEMTX_ERROR;
            }
            if (s->boot2_mock && ((address >= 0x20002800 && address < 0x20003000) ||
                                 (address >= 0x24c00000 && address < 0x24c0d000) ||
                                 (address >= 0x24940000 && address < 0x24940100) ||
                                 (address >= 0x24900000 && address < 0x24900100) ||
                                 (address >= 0x24920000 && address < 0x24920100) ||
                                 (address >= 0x24b00000 && address < 0x24b01000) ||
                                 (address >= 0x24b08000 && address < 0x24b08400))) {
                if (size != 4 ||
                    (write && address != 0x24b000d8 &&
                              (((uint32_t)*value ^ s->register_values[i]) &
                               ~s->registers[i].mask)) ||
                    (write && address == 0x200028a8 && *value != 0xc8)) {
                    return MEMTX_ERROR;
                }
            }
            if (s->otp_fixture && (address & ~3ULL) == 0x20056800) {
                uint32_t current = s->register_values[i];
                if (write) {
                    uint32_t merged = (current & ~mask) |
                                      (((uint32_t)*value << shift) & mask);
                    bool rising = (merged & (1 << 4)) && !(current & (1 << 4));
                    /* Poll-only automatic reads. Programming, manual mode and
                     * software interrupt assertion are never implemented.
                     * A rejected transaction cannot mutate any latch/shadow. */
                    if ((merged & ((1 << 3) | (1 << 5) | (1 << 22))) ||
                        (s->otp_busy_reads && merged != current) ||
                        (rising && (address != 0x20056800 || size != 4 ||
                                    ((merged >> 8) & 0xff) != 0xbf ||
                                    !(merged & (1 << 21))))) {
                        qemu_log_mask(LOG_GUEST_ERROR,
                                      "AP01 OTP mock rejected mode/trigger/write\n");
                        return MEMTX_ERROR;
                    }
                    if (rising) {
                        s->register_values[i] =
                            (((current & ~s->registers[i].mask) |
                              (merged & s->registers[i].mask)) & ~(1U << 1)) |
                            (1 << 2);
                        s->otp_busy_reads = 2;
                        return MEMTX_OK;
                    }
                } else if (s->otp_busy_reads) {
                    /* Deterministic functional latency: first CTRL0 read
                     * observes BUSY, second completes. No timing/IRQ claim.
                     * SDK commit 41bf338a: drivers/lhal/src/bflb_ef_ctrl.c
                     * EF_CTRL_DATA0_CLEAR/load_efuse_r0 uses 512-byte R0 on
                     * BL616; std/include/bl616_ef_ctrl.h agrees (128 * 4).
                     * ef_ctrl_reg.h defines BUSY2/RW3/TRIG4/MANUAL5/DONE1.
                     * Remaining fixture bytes are NOT implicitly loaded. */
                    if (--s->otp_busy_reads == 0) {
                        memcpy(s->shadow_bytes, s->otp_fixture, 512);
                        memset(s->shadow_written, 1, 512);
                        s->register_values[i] = (current & ~((1 << 2) |
                                                (1 << 4))) | (1 << 1);
                    }
                }
            }
            if (write) {
                if (s->boot2_mock && address == 0x20000540 &&
                    (*value & 0x10) && !(s->register_values[i] & 0x10)) {
                    /* Limited virtual WiFi domain: reset represented MAC
                     * configuration, masks and local time. RAM, RF analog
                     * calibration and unrelated peripherals are retained. */
                    ap01_mac_reset(&s->mac);
                    if (s->mac_empty_mock) { ap01_machw_reset(&s->machw); }
                    ap01_machw_reset_complete(s);
                    s->wifi_reset_generation++;
                    s->mac_time_epoch_ns = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL);
                    s->sysctrl_time_epoch_ns = s->mac_time_epoch_ns;
                    ap01_rf_acal_set_epoch(&s->rf_acal, s->mac_time_epoch_ns);
                    for (size_t j = 0; j < s->register_count; j++) {
                        hwaddr r = s->registers[j].address;
                        if ((r >= 0x24900000 && r < 0x24900100) ||
                            (r >= 0x24940000 && r < 0x24940100) ||
                            (r >= 0x24c0c000 && r < 0x24c0c100)) {
                            s->register_values[j] = s->registers[j].reset;
                        }
                    }
                }
                mask &= s->registers[i].mask;
                s->register_values[i] = (s->register_values[i] & ~mask) |
                                        (((uint32_t)*value << shift) & mask);
                if (address >= 0x200008c4 && address < 0x20000950) {
                    uint32_t pulses = (uint32_t)*value << shift;
                    if (pulses & (1U << 25)) {
                        s->register_values[i] |= 1U << 24;
                    }
                    if (pulses & (1U << 26)) {
                        s->register_values[i] &= ~(1U << 24);
                    }
                    if (pulses & (1U << 20)) {
                        s->register_values[i] &= ~(1U << 21);
                    }
                    if (s->boot2_mock) {
                        ap01_gpio_update(s);
                    }
                    if (s->spi_initialized) {
                        ap01_spi_lines_changed(&s->spi);
                        ap01_te_sync(s);
                    }
                }
                if (s->boot2_mock && s->registers[i].address == 0x20009014) {
                    ap01_mtimer_configure(s, s->register_values[i]);
                }
            } else {
                *value = (s->register_values[i] >> shift) &
                         (UINT32_MAX >> ((4 - size) * 8));
            }
            return MEMTX_OK;
        }
    }
    qemu_log_mask(LOG_GUEST_ERROR, "AP01 unsupported MMIO %s at 0x%" HWADDR_PRIx
                  " size=%u\n", write ? "write" : "read", address, size);
    return MEMTX_ERROR;
}

static MemTxResult ap01_mmio_read(void *opaque, hwaddr offset, uint64_t *value,
                                 unsigned size, MemTxAttrs attrs)
{
    return ap01_mmio_access(opaque, offset, value, size, false);
}

static MemTxResult ap01_mmio_write(void *opaque, hwaddr offset, uint64_t value,
                                  unsigned size, MemTxAttrs attrs)
{
    return ap01_mmio_access(opaque, offset, &value, size, true);
}

static const MemoryRegionOps ap01_mmio_ops = {
    .read_with_attrs = ap01_mmio_read,
    .write_with_attrs = ap01_mmio_write,
    .endianness = DEVICE_LITTLE_ENDIAN,
    .valid = {.min_access_size = 1, .max_access_size = 4, .unaligned = false},
    .impl = {.min_access_size = 1, .max_access_size = 4, .unaligned = false},
};

/* SDK identifies CSR 0x7c5 as MHINT. This optional functional latch only
 * preserves software RMW state in coherent memory; bit20/reset silicon
 * semantics are unknown. It does not enable unsupported CPU instructions.
 * Default off keeps the generic-CPU baseline reproducible. */
static RISCVException ap01_mhint_pred(CPURISCVState *env, int csrno)
{
    return RISCV_EXCP_NONE;
}

static RISCVException ap01_mhint_read(CPURISCVState *env, int csrno,
                                    target_ulong *value)
{
    AP01State *s = AP01_MACHINE(qdev_get_machine());
    switch (csrno) {
    case 0x307: *value = s->mtvt; break;
    case 0x7c0: *value = s->mxstatus; break;
    case 0x7c1: *value = s->mhcr; break;
    case 0x7e1: *value = s->mexstatus; break;
    default: *value = s->mhint; break;
    }
    return RISCV_EXCP_NONE;
}

static RISCVException ap01_mhint_write(CPURISCVState *env, int csrno,
                                     target_ulong value, uintptr_t ra)
{
    AP01State *s = AP01_MACHINE(qdev_get_machine());
    if (csrno == 0x307) {
        s->mtvt = value;
    } else if (csrno == 0x7c0) {
        s->mxstatus = value;
    } else if (csrno == 0x7c1) {
        s->mhcr = value;
    } else if (csrno == 0x7e1) {
        s->mexstatus = value;
    } else {
        s->mhint = value;
    }
    return RISCV_EXCP_NONE;
}

static void ap01_reset(void *opaque)
{
    AP01State *s = opaque;
    memset(s->phy_table, 0, sizeof(s->phy_table));
    if (s->trng_mock) { ap01_trng_reset(&s->trng); }
    s->mhint = 0; /* Explicit model initial state, not a measured reset value. */
    s->mtvt = 0;
    s->mxstatus = 0;
    s->mhcr = 0;
    s->mexstatus = 0;
    s->otp_busy_reads = 0;
    s->psram_grant_reads = s->psram_command_reads = 0;
    s->psram_command = 0;
    s->clock_xtal_arg = s->clock_pll_arg = s->clock_args_written = 0;
    s->clock_result = 1;
    s->virtual_xtal_hz = s->virtual_wifi_pll_hz = s->virtual_audio_pll_hz = 0;
    s->clock_request_count = 0;
    s->mcu_clock_arg = s->virtual_mcu_hz = 0;
    s->mcu_clock_pending = false;
    s->mcu_clock_result = 1;
    memset(s->clic_source, 0, sizeof(s->clic_source));
    if (s->shadow_written) {
        memset(s->shadow_written, 0, 0x800);
    }
    for (size_t i = 0; i < s->register_count; i++) {
        s->register_values[i] = s->registers[i].reset;
    }
    if (s->boot2_mock) {
        /* SDK GLB_Set_Flash_Id_Value: magic5a | JEDEC1740ef (8MiB).
         * Explicit synthetic Boot2 handoff; does not implement flash I/O. */
        *ap01_register_value(s, 0x200005c4) = s->flash.bytes ? 0x5a1840ef : 0x5a1740ef;
        if (s->flash.bytes) {
            ap01_flash_reset(&s->flash);
        }
        s->mtimer_ticks = 0;
        s->mtimer_hz = 0;
        s->mtimer_compare = UINT64_MAX; /* Explicit synthetic handoff. */
        s->mtimer_epoch_ns = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL);
        s->mac_time_epoch_ns = s->mtimer_epoch_ns;
        s->sysctrl_time_epoch_ns = s->mtimer_epoch_ns;
        ap01_mtimer_configure(s, *ap01_register_value(s, 0x20009014));
        for (size_t i = 0; i < G_N_ELEMENTS(s->uart); i++) {
            ap01_uart_reset(&s->uart[i]);
        }
        if (s->spi_initialized) {
            ap01_timer_reset(&s->peripheral_timer);
            ap01_rf_reset(&s->rf);
            if (s->mac_empty_mock) { ap01_mac_reset(&s->mac); }
            if (s->mac_empty_mock) { ap01_machw_reset(&s->machw); }
            if (s->rf_calibration_mock) {
                ap01_rf_cal_reset(&s->rf_cal);
                ap01_rf_acal_reset(&s->rf_acal);
                ap01_rf_pm_reset(&s->rf_pm);
                ap01_rf_acal_set_epoch(&s->rf_acal, s->mac_time_epoch_ns);
            }
            ap01_spi_reset(&s->spi);
            ap01_dma_reset(&s->dma);
            ap01_pwm_reset(&s->pwm);
            if (s->pec_uart_mock) {
                ap01_pec_reset(&s->pec);
            }
            s->te_level = false;
            timer_del(s->te_timer);
        }
        ap01_clic_update(s);
    }
}

static bool ap01_get_mhint(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->mhint_latch;
}

static void ap01_set_mhint(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->mhint_latch = value;
}

static bool ap01_get_mtvt(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->mtvt_latch;
}

static void ap01_set_mtvt(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->mtvt_latch = value;
}

static bool ap01_get_mmio(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->boot_mmio;
}

static void ap01_set_mmio(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->boot_mmio = value;
}

static bool ap01_get_cache_csrs(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->cache_csr_latches;
}

static void ap01_set_cache_csrs(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->cache_csr_latches = value;
}

static bool ap01_get_shadow(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->efuse_shadow;
}

static void ap01_set_shadow(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->efuse_shadow = value;
}

static bool ap01_get_boot2(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->boot2_mock;
}

static void ap01_set_boot2(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->boot2_mock = value;
}

static bool ap01_get_rf_calibration(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->rf_calibration_mock;
}

static bool ap01_get_mac_empty(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->mac_empty_mock;
}

static bool ap01_get_trng(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->trng_mock;
}

static bool ap01_get_gpio3_high(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->gpio3_input_high;
}

static void ap01_set_gpio3_high(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->gpio3_input_high = value;
}

static bool ap01_get_gpio24_high(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->gpio24_input_high;
}

static void ap01_set_gpio24_high(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->gpio24_input_high = value;
}

static void ap01_set_trng(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->trng_mock = value;
}

static void ap01_set_mac_empty(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->mac_empty_mock = value;
}

static char *ap01_get_mac_state(Object *obj, Error **errp)
{
    AP01State *s = AP01_MACHINE(obj);
    return g_strdup_printf("{\"enabled\":%s,\"pending_low\":%u,\"pending_high\":%u,"
                           "\"unmask_low\":%u,\"unmask_high\":%u,\"wifi_reset_generation\":%u,"
                           "\"dma_arbitration\":%u,\"dma_unmask\":%u,\"platform_zero_writes\":%u,"
                           "\"phy_reset_generation\":%u}",
                           s->mac_empty_mock ? "true" : "false",
                           s->mac.pending[0] & s->mac.unmask[0],
                           s->mac.pending[1] & s->mac.unmask[1],
                           s->mac.unmask[0], s->mac.unmask[1], s->wifi_reset_generation,
                           s->mac.dma_arbitration, s->mac.dma_unmask, s->mac.platform_zero_writes,
                           s->phy_reset_generation);
}

static char *ap01_get_machw_state(Object *obj, Error **errp)
{
    AP01State *s = AP01_MACHINE(obj);
    return g_strdup_printf("{\"enabled\":%s,\"busy\":%s,\"generation\":%u,"
                           "\"current\":%u,\"next\":%u,\"fsm_busy\":%s,"
                           "\"gen_pending\":%u,\"gen_enable\":%u,"
                           "\"timer_pending\":%u,\"timer_enable\":%u,"
                           "\"watchdog_deadline\":%u,\"kernel_deadline\":%u}",
                           s->mac_empty_mock ? "true" : "false",
                           s->machw.busy ? "true" : "false", s->machw.generation,
                           s->machw.current, s->machw.next,
                           s->machw.fsm_busy ? "true" : "false",
                           ap01_machw_gen_pending(&s->machw), s->machw.gen_enable,
                           s->machw.timer_pending, s->machw.timer_enable,
                           s->machw.deadlines[0].deadline, s->machw.deadlines[2].deadline);
}

static char *ap01_get_rf_calibration_state(Object *obj, Error **errp)
{
    AP01State *s = AP01_MACHINE(obj);
    uint64_t mode = 0, channel = 0, count = 0, ci = 0, cq = 0, control = 0;
    ap01_rf_access(&s->rf, 0x14, &mode, 4, false);
    ap01_rf_access(&s->rf, 0xc4, &channel, 4, false);
    ap01_rf_access(&s->rf, 0x61c, &count, 4, false);
    ap01_rf_access(&s->rf, 0x214, &ci, 4, false);
    ap01_rf_access(&s->rf, 0x218, &cq, 4, false);
    ap01_rf_access(&s->rf, 0x618, &control, 4, false);
    return g_strdup_printf("{\"enabled\":%s,\"candidate\":%u,\"sampled_candidate\":%u,"
                           "\"divisor\":%u,\"sampled_divisor\":%u,\"result\":%u,"
                           "\"trigger\":%s,\"busy\":%s,\"done\":%s,"
                           "\"amplitude\":%u,\"acal_mode\":%u,\"acal_channel\":%u,"
                           "\"acal_valid\":%s,\"acal_settled_ns\":%" PRId64
                           ",\"virtual_now_ns\":%" PRId64
                           ",\"pm_candidate\":%u,\"pm_result\":%u,\"pm_busy\":%s,\"pm_done\":%s,"
                           "\"mode014\":%u,\"channel_c4\":%u,\"count61c\":%u,"
                           "\"candidate214\":%u,\"candidate218\":%u,\"control618\":%u}",
                           s->rf_calibration_mock ? "true" : "false",
                           s->rf_cal.candidate, s->rf_cal.sampled_candidate,
                           s->rf_cal.divisor, s->rf_cal.sampled_divisor, s->rf_cal.result,
                           s->rf_cal.trigger ? "true" : "false",
                           s->rf_cal.busy ? "true" : "false", s->rf_cal.done ? "true" : "false",
                           s->rf_cal.amplitude, s->rf_acal.mode, s->rf_acal.channel_word,
                           s->rf_acal.valid ? "true" : "false", s->rf_acal.settled_ns,
                           qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL),
                           s->rf_pm.candidate, s->rf_pm.result,
                           s->rf_pm.busy ? "true" : "false", s->rf_pm.done ? "true" : "false",
                           (uint32_t)mode, (uint32_t)channel, (uint32_t)count,
                           (uint32_t)ci, (uint32_t)cq, (uint32_t)control);
}

static void ap01_set_rf_calibration(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->rf_calibration_mock = value;
}

static bool ap01_get_clic(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->clic_mock;
}

static void ap01_set_clic(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->clic_mock = value;
}

static bool ap01_get_pec(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->pec_uart_mock;
}

static void ap01_set_pec(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->pec_uart_mock = value;
}

static char *ap01_get_pec_state(Object *obj, Error **errp)
{
    AP01State *s = AP01_MACHINE(obj);
    return g_strdup_printf("{\"api_replacement_enabled\":%s,\"initialized\":%s,"
                           "\"ready\":%s,\"synthetic_clock_hz\":%u,"
                           "\"rx_object\":%u,\"tx_object\":%u,\"tx_pending\":%u,"
                           "\"accepted_words\":%" PRIu64 ",\"drained_bytes\":%" PRIu64 "}",
                           s->pec_uart_mock ? "true" : "false",
                           s->pec.initialized ? "true" : "false",
                           s->pec.ready ? "true" : "false", s->pec.synthetic_clock_hz,
                           s->pec.rx_object, s->pec.tx_object, s->pec.tx_count,
                           s->pec.accepted_words, s->pec.drained_bytes);
}

static char *ap01_get_flash_state(Object *obj, Error **errp)
{
    AP01Flash *s = &AP01_MACHINE(obj)->flash;
    return g_strdup_printf("{\"enabled\":%s,\"owner\":%u,\"xip_offset\":%u,"
                           "\"state_saved\":%s,\"continuous_read\":%s,"
                           "\"burst_wrap\":%s,\"qspi_enabled\":%s,"
                           "\"address_32bit\":%s,\"idbus_enabled\":%s,"
                           "\"read_count\":%" PRIu64 ",\"program_count\":%" PRIu64
                           ",\"erase_count\":%" PRIu64 ",\"ota_writable\":%s,\"fw1_active\":%s}",
                           s->bytes ? "true" : "false", s->owner, s->xip_offset,
                           s->state_saved ? "true" : "false",
                           s->continuous_read ? "true" : "false",
                           s->burst_wrap ? "true" : "false",
                           s->qspi_enabled ? "true" : "false",
                           s->address_32bit ? "true" : "false",
                           s->idbus_enabled ? "true" : "false",
                           s->read_count, s->program_count, s->erase_count,
                           s->ota_writable ? "true" : "false", s->fw1_active ? "true" : "false");
}

static char *ap01_get_lcd_state(Object *obj, Error **errp)
{
    AP01State *s = AP01_MACHINE(obj);
    return g_strdup_printf("{\"enabled\":%s,\"tx_bytes\":%" PRIu64
                           ",\"pixel_writes\":%" PRIu64
                           ",\"commands\":%" PRIu64
                           ",\"display_on\":%s,\"sleep_out\":%s,\"te\":%s,"
                           "\"madctl\":%u,\"pixel_format\":%u,"
                           "\"landscape_clockwise_assumption\":true}",
                           s->spi_initialized ? "true" : "false",
                           s->spi.tx_bytes, s->spi.pixel_writes, s->spi.command_count,
                           s->spi.display_on ? "true" : "false",
                           s->spi.sleep_out ? "true" : "false",
                           s->spi.te_enabled ? "true" : "false",
                           s->spi.madctl, s->spi.pixel_format);
}

static bool ap01_spi_line_read(void *opaque, AP01SPILine line)
{
    AP01State *s = opaque;
    static const unsigned pins[] = {28, 30, 32};
    uint32_t config = *ap01_register_value(s, 0x200008c4 + 4 * pins[line]);
    /* Undriven CS/reset are pulled high in this explicit board fixture. */
    return (config & (1U << 6)) ? !!(config & (1U << 24)) : line != AP01_SPI_DC;
}

static bool ap01_get_psram(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->psram_mock;
}

static void ap01_set_psram(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->psram_mock = value;
}

static char *ap01_get_otp(Object *obj, Error **errp)
{
    return g_strdup(AP01_MACHINE(obj)->otp_fixture_path);
}

static void ap01_set_otp(Object *obj, const char *value, Error **errp)
{
    AP01State *s = AP01_MACHINE(obj);
    g_free(s->otp_fixture_path);
    s->otp_fixture_path = g_strdup(value);
}

static char *ap01_get_rom_shim(Object *obj, Error **errp)
{
    return g_strdup(AP01_MACHINE(obj)->rom_shim_path);
}

static void ap01_set_rom_shim(Object *obj, const char *value, Error **errp)
{
    AP01State *s = AP01_MACHINE(obj);
    g_free(s->rom_shim_path);
    s->rom_shim_path = g_strdup(value);
}

static char *ap01_get_flash(Object *obj, Error **errp)
{
    return g_strdup(AP01_MACHINE(obj)->flash_fixture_path);
}

static void ap01_set_flash(Object *obj, const char *value, Error **errp)
{
    AP01State *s = AP01_MACHINE(obj);
    g_free(s->flash_fixture_path);
    s->flash_fixture_path = g_strdup(value);
}

static bool ap01_get_ota_flash(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->flash.ota_writable;
}

static void ap01_set_ota_flash(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->flash.ota_writable = value;
}

static bool ap01_get_fw1_active(Object *obj, Error **errp)
{
    return AP01_MACHINE(obj)->flash.fw1_active;
}

static void ap01_set_fw1_active(Object *obj, bool value, Error **errp)
{
    AP01_MACHINE(obj)->flash.fw1_active = value;
}

static void ap01_set_flash_snapshot(Object *obj, const char *path, Error **errp)
{
    AP01Flash *f = &AP01_MACHINE(obj)->flash;
    g_autofree char *parent = g_path_get_dirname(path);
    if (!f->bytes || !g_path_is_absolute(path) || !g_file_test(parent, G_FILE_TEST_IS_DIR)) {
        error_setg(errp, "snapshot requires an initialized fixture and an existing absolute parent directory");
        return;
    }
    int fd = g_open(path, O_WRONLY | O_CREAT | O_EXCL, 0600);
    if (fd < 0) { error_setg_errno(errp, errno, "cannot create new NOR snapshot"); return; }
    size_t done = 0;
    while (done < f->size) {
        ssize_t n = write(fd, f->bytes + done, f->size - done);
        if (n < 0 && errno == EINTR) { continue; }
        if (n <= 0) {
            int saved = errno;
            close(fd);
            g_unlink(path);
            error_setg_errno(errp, saved, "incomplete NOR snapshot");
            return;
        }
        done += n;
    }
    if (close(fd)) { error_setg_errno(errp, errno, "cannot close NOR snapshot"); }
}

static void ap01_watchdog_reset_request(void *opaque)
{
    qemu_system_reset_request(SHUTDOWN_CAUSE_GUEST_RESET);
}

static void ap01_init(MachineState *machine)
{
    static const hwaddr ram_addr[] = {
        0x20fc0000, 0x22fc0000, 0x60fc0000, 0x62fc0000,
    };
    static const hwaddr psram_addr[] = {
        0x88000000, 0xa8000000, 0xd8000000,
    };
    AP01State *s = AP01_MACHINE(machine);
    MemoryRegion *sysmem = get_system_memory();
    g_autofree gchar *image = NULL;
    g_autoptr(GError) read_error = NULL;
    gsize image_size;

    if (s->psram_mock && !s->boot_mmio) {
        error_report("AP01 psram-mock: stage=configuration; requires boot-mmio=on");
        exit(EXIT_FAILURE);
    }
    if (s->pec_uart_mock && (!s->boot2_mock || !s->rom_shim_path)) {
        error_report("pec-uart-mock requires boot2-mock and native v3 rom-shim");
        exit(EXIT_FAILURE);
    }
    if (s->flash_fixture_path) {
        gchar *data = NULL;
        gsize size = 0;
        if (!s->boot2_mock || !g_path_is_absolute(s->flash_fixture_path) ||
            !g_file_get_contents(s->flash_fixture_path, &data, &size, &read_error)) {
            error_report("flash-fixture requires boot2-mock, an absolute readable file");
            exit(EXIT_FAILURE);
        }
        if (size != 0x1000000) {
            error_report("flash-fixture must contain exactly16MiB");
            exit(EXIT_FAILURE);
        }
        s->flash.bytes = (uint8_t *)data;
        s->flash.size = size;
        s->flash_mailbox.state = s;
        s->flash_mailbox.base = 0xf10000c0;
        memory_region_init_io(&s->flash_mailbox.region, OBJECT(machine), &ap01_mmio_ops,
                              &s->flash_mailbox, "ap01.synthetic-nor-mailbox", 0x20);
        memory_region_add_subregion(sysmem, 0xf10000c0, &s->flash_mailbox.region);
        warn_report("Explicit synthetic16MiB NOR fixture; JEDEC1840ef; "
                    "virtual data/resource writable; inactive FW/PT writes require ota-flash-mock; "
                    "fixture file immutable");
    }
    if ((s->flash.ota_writable || s->flash.fw1_active) && !s->flash.bytes) {
        error_report("OTA flash profile requires an explicit synthetic NOR fixture");
        exit(EXIT_FAILURE);
    }
    if (s->psram_mock) {
        warn_report("AP01 synthetic PSRAM request/config mock enabled: "
                    "poll-count completion, zero synthetic ID0/ID1, CR0 status-view; "
                    "no physical timing, sleep/reset or full-boot claim");
    }
    if (s->boot2_mock && !s->boot_mmio) {
        error_report("boot2-mock requires boot-mmio=on");
        exit(EXIT_FAILURE);
    }
    if (s->clic_mock && !s->boot2_mock) {
        error_report("clic-mock requires boot2-mock");
        exit(EXIT_FAILURE);
    }
    if (s->rf_calibration_mock && !s->boot2_mock) {
        error_report("rf-calibration-mock requires boot2-mock");
        exit(EXIT_FAILURE);
    }
    if (s->mac_empty_mock && !s->boot2_mock) {
        error_report("mac-empty-mock requires boot2-mock");
        exit(EXIT_FAILURE);
    }
    if (s->trng_mock && !s->boot2_mock) {
        error_report("trng-mock requires boot2-mock");
        exit(EXIT_FAILURE);
    }
    if (s->boot2_mock) {
        warn_report("AP01 synthetic Boot2 cached JEDEC=%s; physical identity unverified",
                    s->flash.bytes ? "1840ef (16MiB fixture)" : "1740ef (8MiB)");
        warn_report("AP01 ideal virtual clock mailbox enabled: only XTAL=4/"
                    "PLL=3 supports stateful 40MHz/960MHz/491.52MHz setup; "
                    "MCU enum5 selects ideal 320MHz after PLL setup; "
                    "no physical PLL lock or timing claim");
    }

    if (machine->ram_size != AP01_RAM_SIZE) {
        error_report("ap01 requires exactly 480 KiB of internal RAM");
        exit(EXIT_FAILURE);
    }
    if (!machine->kernel_filename || machine->firmware ||
        machine->initrd_filename) {
        error_report("ap01 requires a raw application BIN via -kernel; "
                     "-bios and -initrd are unsupported");
        exit(EXIT_FAILURE);
    }
    if (!g_file_get_contents(machine->kernel_filename, &image, &image_size,
                             &read_error)) {
        error_report("cannot read AP01 image: %s", read_error->message);
        exit(EXIT_FAILURE);
    }
    if (image_size <= AP01_ENTRY - AP01_XIP_BASE ||
        image_size > AP01_XIP_SIZE ||
        (image_size >= 4 && memcmp(image, "\177ELF", 4) == 0)) {
        error_report("ap01 requires a raw BIN larger than 4096 bytes "
                     "and no larger than 16 MiB (ELF is unsupported)");
        exit(EXIT_FAILURE);
    }
    if (s->otp_fixture_path) {
        gsize fixture_size = 0;
        if (!s->efuse_shadow || !s->boot_mmio ||
            !g_path_is_absolute(s->otp_fixture_path)) {
            error_report("AP01 otp-fixture: stage=configuration; requires "
                         "an absolute path, efuse-shadow=on and boot-mmio=on");
            exit(EXIT_FAILURE);
        }
        if (!g_file_get_contents(s->otp_fixture_path, &s->otp_fixture,
                                 &fixture_size, &read_error)) {
            error_report("AP01 otp-fixture: stage=read; %s", read_error->message);
            exit(EXIT_FAILURE);
        }
        if (fixture_size != 0x800) {
            error_report("AP01 otp-fixture: stage=validate; expected 2048 bytes, "
                         "got %zu", fixture_size);
            exit(EXIT_FAILURE);
        }
        warn_report("AP01 synthetic OTP fixture enabled: immutable source, "
                    "bank0 512-byte automatic reads only; deterministic "
                    "two-CTRL0-read completion, no physical timing or IRQ; "
                    "no real device identity or full-boot claim");
    }

    for (size_t i = 0; i < G_N_ELEMENTS(ram_addr); i++) {
        g_autofree char *name = g_strdup_printf("ap01.ram-alias%zu", i);
        memory_region_init_alias(&s->ram_alias[i], OBJECT(machine), name,
                                 machine->ram, 0, AP01_RAM_SIZE);
        memory_region_add_subregion(sysmem, ram_addr[i], &s->ram_alias[i]);
    }
    memory_region_init_ram(&s->psram, NULL, "ap01.psram",
                           AP01_PSRAM_SIZE, &error_fatal);
    for (size_t i = 0; i < G_N_ELEMENTS(psram_addr); i++) {
        g_autofree char *name = g_strdup_printf("ap01.psram-alias%zu", i);
        memory_region_init_alias(&s->psram_alias[i], OBJECT(machine), name,
                                 &s->psram, 0, AP01_PSRAM_SIZE);
        memory_region_add_subregion(sysmem, psram_addr[i],
                                    &s->psram_alias[i]);
    }
    memory_region_init_rom(&s->xip, NULL, "ap01.xip",
                           AP01_XIP_SIZE, &error_fatal);
    memory_region_add_subregion(sysmem, AP01_XIP_BASE, &s->xip);
    rom_add_blob_fixed("ap01.application", image, image_size, AP01_XIP_BASE);
    if (s->rom_shim_path) {
        g_autofree gchar *shim = NULL;
        gsize shim_size = 0;
        if (!s->boot2_mock || !g_path_is_absolute(s->rom_shim_path)) {
            error_report("AP01 rom-shim requires boot2-mock and an absolute path");
            exit(EXIT_FAILURE);
        }
        if (!g_file_get_contents(s->rom_shim_path, &shim, &shim_size,
                                 &read_error)) {
            error_report("AP01 rom-shim: %s", read_error->message);
            exit(EXIT_FAILURE);
        }
        if (shim_size <= 0x800 || shim_size > 0x10000) {
            error_report("AP01 rom-shim requires 2049..65536 bytes");
            exit(EXIT_FAILURE);
        }
        for (size_t i = 0; i < G_N_ELEMENTS(s->rom_shim_table); i++) {
            uint32_t target = ldl_le_p(shim + i * 4);
            if (target && ((target & 1) || target < 0x90001800 ||
                           target >= 0x90001000 + shim_size)) {
                error_report("AP01 rom-shim invalid function pointer at slot%zu", i);
                exit(EXIT_FAILURE);
            }
            s->rom_shim_table[i] = target;
        }
        if (s->pec_uart_mock) {
            if (shim_size < 0x1000 || ldl_le_p(shim + 0x800) != 0x43455041 ||
                ldl_le_p(shim + 0x804) != 1 || ldl_le_p(shim + 0x808) != 128 ||
                ldl_le_p(shim + 0x80c)) {
                error_report("PEC mock requires APEC v1,128 native API header");
                exit(EXIT_FAILURE);
            }
            for (size_t i = 0; i < G_N_ELEMENTS(s->pec_shim_table); i++) {
                uint32_t target = ldl_le_p(shim + 0x810 + i * 4);
                if (target && ((target & 1) || target < 0x90002000 ||
                               target >= 0x90001000 + shim_size)) {
                    error_report("invalid PEC shim function pointer at slot%zu", i);
                    exit(EXIT_FAILURE);
                }
                s->pec_shim_table[i] = target;
            }
            if (image_size < 0x66f010 || memcmp(image + 0x66f000, "PEC:", 4)) {
                error_report("PEC mock requires a PEC header at fixed0041 offset66f000");
                exit(EXIT_FAILURE);
            }
            memcpy(s->pec_header, image + 0x66f000, sizeof(s->pec_header));
            s->pec_api.state = s;
            s->pec_api.base = 0xa066e000;
            memory_region_init_io(&s->pec_api.region, OBJECT(machine), &ap01_mmio_ops,
                                  &s->pec_api, "ap01.synthetic-pec-api-binding", 0x200);
            memory_region_add_subregion_overlap(sysmem, 0xa066e000, &s->pec_api.region, 1);
            s->pec_mailbox.state = s;
            s->pec_mailbox.base = 0xf1000080;
            memory_region_init_io(&s->pec_mailbox.region, OBJECT(machine), &ap01_mmio_ops,
                                  &s->pec_mailbox, "ap01.synthetic-pec-mailbox", 0x18);
            memory_region_add_subregion(sysmem, 0xf1000080, &s->pec_mailbox.region);
            warn_report("PEC UART API binding replacement enabled: synthetic stateful "
                        "UART2, not original PEC decryption or device-equivalent code");
        }
        memory_region_init_rom(&s->rom_shim, NULL, "ap01.native-rom-shim",
                               0x10000, &error_fatal);
        memory_region_add_subregion(sysmem, 0x90001000, &s->rom_shim);
        rom_add_blob_fixed("ap01.native-rom-shim", shim, shim_size, 0x90001000);
        warn_report("AP01 synthetic native BootROM shim enabled; "
                    "only explicit table entries supported, original image unchanged");
    }

    if (s->boot2_mock) {
        static const uint8_t unsupported[] = {
            0x13, 0x05, 0xf0, 0xff, /* addi a0,zero,-1 */
            0x67, 0x80, 0x00, 0x00, /* jalr zero,ra,0 */
        };
        static const uint8_t clock_call[] = {
            0xb7, 0x02, 0x00, 0xf1, /* lui t0,0xf1000 */
            0x23, 0xa0, 0xa2, 0x00, /* sw a0,0(t0) */
            0x23, 0xa2, 0xb2, 0x00, /* sw a1,4(t0) */
            0x03, 0xa5, 0x82, 0x00, /* lw a0,8(t0): validated commit */
            0x67, 0x80, 0x00, 0x00, /* ret */
        };
        static const uint8_t mcu_clock_call[] = {
            0xb7, 0x02, 0x00, 0xf1, /* lui t0,0xf1000 */
            0x23, 0xa0, 0xa2, 0x02, /* sw a0,0x20(t0) */
            0x03, 0xa5, 0x42, 0x02, /* lw a0,0x24(t0): validated commit */
            0x67, 0x80, 0x00, 0x00, /* ret */
        };
        /* SDK hbn.c HBN_Get_Xtal_Type: null or bad marker returns ERROR=1
         * without touching output; valid flag8 returns low-nibble XTAL type.
         * Actual guest loads/store preserve bus faults for invalid pointers.
         * bl616_common.h:78..79 defines SUCCESS=0, ERROR=1. */
        static const uint8_t xtal_getter[] = {
            0x63, 0x06, 0x05, 0x02, /* beq a0,zero,error (+44) */
            0xb7, 0xf2, 0x00, 0x20, /* lui t0,0x2000f */
            0x83, 0xa2, 0xc2, 0x10, /* lw t0,0x10c(t0) */
            0x13, 0xd3, 0x42, 0x00, /* srli t1,t0,4 */
            0x13, 0x73, 0xf3, 0x00, /* andi t1,t1,15 */
            0x93, 0x03, 0x80, 0x00, /* li t2,8 */
            0x63, 0x1a, 0x73, 0x00, /* bne t1,t2,error (+20) */
            0x93, 0xf2, 0xf2, 0x00, /* andi t0,t0,15 */
            0x23, 0x00, 0x55, 0x00, /* sb t0,0(a0) */
            0x13, 0x05, 0x00, 0x00, /* li a0,0 */
            0x67, 0x80, 0x00, 0x00, /* ret */
            0x13, 0x05, 0x10, 0x00, /* error: li a0,1 */
            0x67, 0x80, 0x00, 0x00, /* ret */
        };
        memory_region_init_rom(&s->mock_bootrom, NULL, "ap01.mock-bootrom",
                               0x1000, &error_fatal);
        memory_region_add_subregion(sysmem, 0x90000000, &s->mock_bootrom);
        rom_add_blob_fixed("ap01.mock-unsupported-flash-config", unsupported,
                           sizeof(unsupported), 0x90000000);
        rom_add_blob_fixed("ap01.mock-stateful-clock-call", clock_call,
                           sizeof(clock_call), 0x90000020);
        rom_add_blob_fixed("ap01.mock-stateful-mcu-clock-call", mcu_clock_call,
                           sizeof(mcu_clock_call), 0x90000040);
        rom_add_blob_fixed("ap01.mock-xtal-getter", xtal_getter,
                           sizeof(xtal_getter), 0x90000060);
        s->bootrom_api.state = s;
        s->bootrom_api.base = 0x90015800;
        memory_region_init_io(&s->bootrom_api.region, OBJECT(machine),
                              &ap01_mmio_ops, &s->bootrom_api,
                              "ap01.mock-rom-api-table", 0x800);
        memory_region_add_subregion(sysmem, 0x90015800, &s->bootrom_api.region);
        s->clock_mailbox.state = s;
        s->clock_mailbox.base = 0xf1000000;
        memory_region_init_io(&s->clock_mailbox.region, OBJECT(machine),
                              &ap01_mmio_ops, &s->clock_mailbox,
                              "ap01.mock-virtual-clock-mailbox", 0x30);
        memory_region_add_subregion(sysmem, 0xf1000000,
                                    &s->clock_mailbox.region);
    }

    if (s->boot_mmio) {
        static const hwaddr base[] = {0x20000000, 0xe0800000, 0xeffff000, 0x24c0c000,
                                      0x24940000, 0x24900000, 0x24920000};
        static const uint64_t size[] = {0x60000, 0x2000, 0x1000, 0x1000, 0x100, 0x100, 0x100};
        size_t n = G_N_ELEMENTS(ap01_boot_registers);
        s->register_count = n + 35 + 12 + 80 + 16;
        s->registers = g_new0(AP01Register, s->register_count);
        s->register_values = g_new0(uint32_t, s->register_count);
        memcpy(s->registers, ap01_boot_registers, sizeof(ap01_boot_registers));
        for (size_t i = 0; i < 35; i++) {
            s->registers[n++] = (AP01Register){0x200008c4 + 4 * i,
                                             0x00400b02 | (((i == 3 && s->gpio3_input_high) ||
                                                 (i == 24 && s->gpio24_input_high)) ? 1u << 28 : 0),
                                             0xc15f1f7f};
        }
        for (size_t i = 0; i < 12; i++) {
            s->registers[n++] = (AP01Register){0x20000968 + 4 * i, 2, 0x3f};
        }
        s->clic_regs = &s->register_values[n];
        for (size_t i = 0; i < 80; i++) {
            s->registers[n++] = (AP01Register){0xe0801000 + 4 * i, 0, UINT32_MAX};
        }
        for (size_t i = 0; i < 16; i++) {
            s->registers[n++] = (AP01Register){0xeffff000 + 4 * i, 0, UINT32_MAX};
        }
        for (size_t i = 0; i < G_N_ELEMENTS(base); i++) {
            g_autofree char *name = g_strdup_printf("ap01.boot-mmio%zu", i);
            s->mmio[i].state = s;
            s->mmio[i].base = base[i];
            memory_region_init_io(&s->mmio[i].region, OBJECT(machine),
                                  &ap01_mmio_ops, &s->mmio[i], name, size[i]);
            memory_region_add_subregion(sysmem, base[i], &s->mmio[i].region);
        }
        /* SDK HBN_RAM_BASE/SIZE: real retention storage, not ready latches.
         * Synthetic cold start is zero; ordinary system reset retains bytes. */
        memory_region_init_ram(&s->hbn_ram, NULL, "ap01.hbn-retention",
                               0x1000, &error_fatal);
        memory_region_add_subregion_overlap(sysmem, 0x20010000,
                                           &s->hbn_ram, 1);
        warn_report("AP01 boot register subset enabled; unknown registers fail; "
                    "functional peripherals require the explicit mock bundle");
    }
    if (s->efuse_shadow) {
        if (!s->boot_mmio) {
            error_report("efuse-shadow requires boot-mmio=on");
            exit(EXIT_FAILURE);
        }
        s->shadow_bytes = g_malloc0(0x800);
        s->shadow_written = g_malloc0(0x800);
        warn_report("AP01 eFuse shadow storage enabled; unknown reads rejected; "
                    "OTP triggers require an explicit synthetic fixture");
    }

    if (s->mhint_latch) {
        const riscv_csr_operations ops = {
            .name = "ap01.mhint-functional-latch",
            .predicate = ap01_mhint_pred,
            .read = ap01_mhint_read,
            .write = ap01_mhint_write,
        };
        riscv_set_csr_ops(0x7c5, &ops);
        warn_report("AP01 MHINT latch enabled: reset=0 and coherent cache "
                    "are model assumptions; bit20 semantics unverified");
    }
    if (s->mtvt_latch) {
        const riscv_csr_operations ops = {
            .name = "ap01.mtvt-functional-latch",
            .predicate = ap01_mhint_pred,
            .read = ap01_mhint_read,
            .write = ap01_mhint_write,
        };
        riscv_set_csr_ops(0x307, &ops);
        warn_report("AP01 MTVT storage enabled; SHV vector dispatch unsupported; "
                    "CLIC common-trap adaptation requires clic-mock and CPU flag");
    }
    if (s->cache_csr_latches) {
        const riscv_csr_operations ops = {
            .name = "ap01.cache-csr-functional-latch",
            .predicate = ap01_mhint_pred,
            .read = ap01_mhint_read,
            .write = ap01_mhint_write,
        };
        riscv_set_csr_ops(0x7c0, &ops);
        riscv_set_csr_ops(0x7c1, &ops);
        riscv_set_csr_ops(0x7e1, &ops);
        warn_report("AP01 MXSTATUS/MHCR/MEXSTATUS latches enabled: no "
                    "privilege/cache/reset/sleep side effects; ISA via -cpu");
    }
    qemu_register_reset(ap01_reset, s);

    object_initialize_child(OBJECT(machine), "cpus", &s->cpus,
                            TYPE_RISCV_HART_ARRAY);
    object_property_set_str(OBJECT(&s->cpus), "cpu-type", machine->cpu_type,
                            &error_fatal);
    object_property_set_int(OBJECT(&s->cpus), "num-harts", 1, &error_fatal);
    object_property_set_int(OBJECT(&s->cpus), "resetvec", AP01_ENTRY,
                            &error_fatal);
    sysbus_realize(SYS_BUS_DEVICE(&s->cpus), &error_fatal);
    if (s->clic_mock) {
        if (!s->cpus.harts[0].cfg.ext_xap01clic) {
            error_report("clic-mock requires CPU x-ap01-clic=on");
            exit(EXIT_FAILURE);
        }
        s->clic_wakeup = qdev_get_gpio_in(DEVICE(&s->cpus.harts[0]), IRQ_M_EXT);
        s->cpus.harts[0].env.ap01_clic_pending = ap01_clic_select;
        s->cpus.harts[0].env.ap01_clic_pending_arg = s;
    }
    if (s->boot2_mock) {
        for (size_t i = 0; i < G_N_ELEMENTS(s->uart); i++) {
            qemu_chr_fe_init(&s->uart[i].chr, serial_hd(i), &error_fatal);
            s->uart[i].irq = qemu_allocate_irq(ap01_clic_input, s, 44 + i);
        }
        /* Original a013c180..1a8 and fixed SDK btble_ke_mem_init agree:
         * header magic/size 3724a55a, two software links, aligned pool end.
         * Separate synthetic RAM backing; physical alias/retention unknown. */
        memory_region_init_ram(&s->bt_software_heap, NULL,
                               "ap01.bt-software-heap", 0x3724, &error_fatal);
        memory_region_add_subregion(sysmem, 0x28012c24, &s->bt_software_heap);
        ap01_spi_init(&s->spi, ap01_spi_line_read, s, true);
        if (s->trng_mock) { ap01_trng_init(&s->trng); }
        s->spi_initialized = true;
        if (s->rf_calibration_mock) {
            ap01_rf_cal_init(&s->rf_cal);
            ap01_rf_acal_init(&s->rf_acal);
            ap01_rf_pm_init(&s->rf_pm);
            ap01_rf_acal_set_epoch(&s->rf_acal, s->mac_time_epoch_ns);
        }
        ap01_timer_init(&s->peripheral_timer,
                         qemu_allocate_irq(ap01_clic_input, s, 54),
                         ap01_watchdog_reset_request, s);
        ap01_timer_set_channel_irqs(&s->peripheral_timer,
                                   qemu_allocate_irq(ap01_clic_input, s, 52),
                                   qemu_allocate_irq(ap01_clic_input, s, 53));
        s->dma.spi = &s->spi;
        s->dma.uart[0] = &s->uart[0];
        s->dma.uart[1] = &s->uart[1];
        s->dma.irq = qemu_allocate_irq(ap01_clic_input, s, 31);
        s->te_timer = timer_new_ns(QEMU_CLOCK_VIRTUAL, ap01_te_tick, s);
        if (s->pec_uart_mock) {
            qemu_chr_fe_init(&s->pec.chr, serial_hd(2), &error_fatal);
        }
        s->mtimer_mmio.state = s;
        s->mac_time_mmio.state = s;
        if (s->mac_empty_mock) {
            ap01_mac_init(&s->mac);
            ap01_mac_set_irq(&s->mac, qemu_allocate_irq(ap01_clic_input, s, 70));
            ap01_machw_init(&s->machw, ap01_machw_reset_complete, s);
            ap01_machw_configure(&s->machw, ap01_mac_now_us, ap01_mac_gen_level, s);
            const hwaddr mac_bases[] = {0x24910000, 0x24a00000, 0x24f10380, 0x24b08000, 0x24c00000, 0x24c0a000, 0x24c0b000};
            const uint64_t mac_sizes[] = {0x100, 0x100, 0x10, 0x400, 0x1000, 0x800, 0x1000};
            for (unsigned i = 0; i < G_N_ELEMENTS(mac_bases); i++) {
                g_autofree char *name = g_strdup_printf("ap01.empty-radio%u", i);
                s->mac_mmio[i].state = s;
                s->mac_mmio[i].base = mac_bases[i];
                memory_region_init_io(&s->mac_mmio[i].region, OBJECT(machine),
                                      &ap01_mmio_ops, &s->mac_mmio[i], name, mac_sizes[i]);
                memory_region_add_subregion(sysmem, mac_bases[i], &s->mac_mmio[i].region);
            }
        }
        s->mac_time_mmio.base = s->mac_empty_mock ? 0x24b00000 : 0x24b00120;
        memory_region_init_io(&s->mac_time_mmio.region, OBJECT(machine),
                              &ap01_mmio_ops, &s->mac_time_mmio,
                              "ap01.mac-clock-registers", s->mac_empty_mock ? 0x1000 : 4);
        memory_region_add_subregion(sysmem, s->mac_time_mmio.base, &s->mac_time_mmio.region);
        s->mtimer_mmio.base = 0xe0004000;
        memory_region_init_io(&s->mtimer_mmio.region, OBJECT(machine),
                              &ap01_mmio_ops, &s->mtimer_mmio,
                              "ap01.mock-mtimer", 0x8000);
        memory_region_add_subregion(sysmem, 0xe0004000,
                                    &s->mtimer_mmio.region);
        s->mtimer_irq = qdev_get_gpio_in(DEVICE(&s->cpus.harts[0]), IRQ_M_TIMER);
        s->mtimer = timer_new_ns(QEMU_CLOCK_VIRTUAL, ap01_mtimer_update, s);
        riscv_cpu_set_rdtime_fn(&s->cpus.harts[0].env,
                               ap01_mtimer_read_time, s);
        warn_report("AP01 ideal virtual MTIMER: gated/divided counter and "
                    "MTIP compare; initial count0/compare-max/32MHz source "
                    "are synthetic handoff assumptions");
        warn_report("AP01 functional UART/SPI/DMA and ideal 60Hz GPIO23 TE enabled; "
                    "clockwise LCD output rotation is a model assumption; "
                    "wireless is unsupported; PEC/flash require explicit functional fixtures");
    }
}

static void ap01_class_init(ObjectClass *oc, const void *data)
{
    MachineClass *mc = MACHINE_CLASS(oc);

    mc->desc = "AP01 raw application probe (memory and generic RV32 only)";
    mc->init = ap01_init;
    mc->default_cpu_type = TYPE_RISCV_CPU_BASE32;
    mc->default_cpus = 1;
    mc->min_cpus = 1;
    mc->max_cpus = 1;
    mc->default_ram_id = "ap01.ram";
    mc->default_ram_size = AP01_RAM_SIZE;
    object_class_property_add_bool(oc, "mhint-latch", ap01_get_mhint,
                                   ap01_set_mhint);
    object_class_property_add_bool(oc, "mtvt-latch", ap01_get_mtvt,
                                   ap01_set_mtvt);
    object_class_property_add_bool(oc, "boot-mmio", ap01_get_mmio,
                                   ap01_set_mmio);
    object_class_property_add_bool(oc, "cache-csr-latches", ap01_get_cache_csrs,
                                   ap01_set_cache_csrs);
    object_class_property_add_bool(oc, "efuse-shadow", ap01_get_shadow,
                                   ap01_set_shadow);
    object_class_property_add_str(oc, "otp-fixture", ap01_get_otp,
                                  ap01_set_otp);
    object_class_property_add_bool(oc, "psram-mock", ap01_get_psram,
                                   ap01_set_psram);
    object_class_property_add_bool(oc, "boot2-mock", ap01_get_boot2,
                                   ap01_set_boot2);
    object_class_property_add_str(oc, "rom-shim", ap01_get_rom_shim,
                                  ap01_set_rom_shim);
    object_class_property_add_bool(oc, "clic-mock", ap01_get_clic,
                                   ap01_set_clic);
    object_class_property_add_str(oc, "lcd-state", ap01_get_lcd_state, NULL);
    object_class_property_add_bool(oc, "pec-uart-mock", ap01_get_pec, ap01_set_pec);
    object_class_property_add_str(oc, "pec-state", ap01_get_pec_state, NULL);
    object_class_property_add_str(oc, "flash-fixture", ap01_get_flash, ap01_set_flash);
    object_class_property_add_str(oc, "flash-state", ap01_get_flash_state, NULL);
    object_class_property_add_bool(oc, "ota-flash-mock", ap01_get_ota_flash, ap01_set_ota_flash);
    object_class_property_add_bool(oc, "ota-fw1-active", ap01_get_fw1_active, ap01_set_fw1_active);
    object_class_property_add_str(oc, "flash-snapshot", NULL, ap01_set_flash_snapshot);
    object_class_property_add_bool(oc, "rf-calibration-mock", ap01_get_rf_calibration,
                                   ap01_set_rf_calibration);
    object_class_property_add_str(oc, "rf-calibration-state", ap01_get_rf_calibration_state, NULL);
    object_class_property_add_bool(oc, "mac-empty-mock", ap01_get_mac_empty, ap01_set_mac_empty);
    object_class_property_add_bool(oc, "trng-mock", ap01_get_trng, ap01_set_trng);
    object_class_property_add_bool(oc, "gpio3-input-high", ap01_get_gpio3_high, ap01_set_gpio3_high);
    object_class_property_add_bool(oc, "gpio24-input-high", ap01_get_gpio24_high, ap01_set_gpio24_high);
    object_class_property_add_str(oc, "mac-state", ap01_get_mac_state, NULL);
    object_class_property_add_str(oc, "machw-state", ap01_get_machw_state, NULL);
}

static const TypeInfo ap01_type = {
    .name = TYPE_AP01_MACHINE,
    .parent = TYPE_MACHINE,
    .instance_size = sizeof(AP01State),
    .class_init = ap01_class_init,
};

static void ap01_register_types(void)
{
    type_register_static(&ap01_type);
}

type_init(ap01_register_types)
