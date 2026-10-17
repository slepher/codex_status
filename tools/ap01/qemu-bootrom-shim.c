/* Functional BootROM subset, derived from Bouffalo SDK commit
 * 41bf338a57be53919d99f6c6715a96100428c97d:
 * drivers/soc/bl616/std/src/bl616_glb.c and bl616_glb_gpio.c.
 * Explicitly synthetic QEMU firmware, never an installable device image.
 * Bounds are stricter than release SDK CHECK_PARAM; rejected inputs do no IO.
 */
#include <stdint.h>
#define REG(off) (*(volatile uint32_t *)(0x20000000u + (off)))
static void field(uint32_t off, uint32_t mask, uint32_t value)
{
    REG(off) = (REG(off) & ~mask) | (value & mask);
}
static int gpio(uint32_t fun, const uint8_t *pins, uint32_t count)
{
    if (fun > 31 || count > 35 || (count && !pins)) return 1;
    for (uint32_t i = 0; i < count; i++) if (pins[i] >= 35) return 1;
    /* Synthetic QFN56 fixture exposes all 35 pads. No package OTP access. */
    for (uint32_t i = 0; i < count; i++) {
        uint32_t off = 0x8c4 + 4u * pins[i];
        REG(off) &= ~64u;
        field(off, 0xc0001f7f, (fun << 8) | (fun == 10 ? 6u : 23u));
    }
    return 0;
}
static int dig_select(uint32_t sel)
{
    if (sel > 2) return 1;
    uint32_t enabled = REG(0x250) & 0x01001000;
    field(0x250, 0x01001000, 0);
    field(0x250, 0x30000000, sel << 28);
    field(0x250, 0x01001000, enabled);
    return 0;
}
static int dig512(uint32_t enable, uint32_t compensation, uint32_t div)
{
    if (enable > 1 || compensation > 1 || div > 127) return 1;
    field(0x250, 0x027f0000, (compensation << 25) | (div << 16));
    field(0x250, 0x01000000, enable << 24);
    return 0;
}
static int pwm(uint32_t sel)
{
    if (sel > 1) return 1;
    /* Preserve fixed SDK's whole-register right shift, even if surprising. */
    REG(0x1c0) = ((REG(0x1c0) >> 1) & ~1u) | sel;
    return 0;
}
static int pka(uint32_t sel)
{
    if (sel > 1) return 1;
    field(0x548, 0x01000000, sel << 24);
    return 0;
}
static int em_select(uint32_t type)
{
    /* SDK GLB_Set_EM_Sel: enum0=WRAM160/EM0,1=128/32,2=96/64 KiB.
     * Configuration view only: existing shared RAM aliases are not carved,
     * moved or cleared. This is not a physical SRAM allocator simulation. */
    static const uint8_t selection[] = {0, 3, 15};
    if (type > 2) return 1;
    field(0x60c, 15, selection[type]);
    return 0;
}
/* Observed Wi-Fi startup subset of GLB_PER_Clock_UnGate(uint64_t).
 * SDK DMA1/2 requests (bits4/5) are explicitly no-ops on BL616. */
static int per_ungate(uint32_t low, uint32_t high)
{
    if (high || (low & ~0x33u)) return 1;
    uint32_t cfg0 = REG(0x580), cfg1 = REG(0x584), cfg2 = REG(0x588);
    REG(0x580) = cfg0 | (low & 3u);
    REG(0x584) = cfg1;
    REG(0x588) = cfg2;
    return 0;
}
static int wifi_reset(uint32_t type)
{
    if (type != 4) return 1;
    /* SDK clear/set/clear pulse. The board owns Wi-Fi reset side effects. */
    field(0x540, 16, 0);
    __asm__ volatile ("nop; nop; nop; nop");
    field(0x540, 16, 16);
    __asm__ volatile ("nop; nop; nop; nop");
    field(0x540, 16, 0);
    return 0;
}
static uint32_t ticks(void)
{
    uint32_t value;
    __asm__ volatile ("rdtime %0" : "=r"(value));
    return value;
}
static void delay(uint32_t tick_count)
{
    uint32_t begin = ticks();
    while ((uint32_t)(ticks() - begin) < tick_count) { }
}
static int usb(uint32_t enable)
{
    uint32_t cfg = *(volatile uint32_t *)0x20009014u;
    if (enable > 1 || !(cfg & 0x80000000u) || (cfg & 0x40000000u)) return 1;
    /* Model contract: XCLK40MHz or MCU320MHz, integer divider+1. */
    uint32_t divisor = (cfg & 1023u) + 1;
    uint32_t mhz = (cfg & 0x20000000u) ? 320 : 40;
    uint32_t t3 = (3 * mhz + divisor - 1) / divisor;
    uint32_t t2 = (2 * mhz + divisor - 1) / divisor;
    field(0x838, 0x20000000, enable << 29);
    delay(t3);
    field(0x838, 0x10000000, 0x10000000);
    delay(t2);
    field(0x838, 0x10000000, 0);
    delay(t2);
    field(0x838, 0x10000000, 0x10000000);
    return 0;
}
static int xtal_cap(uint32_t input, uint32_t output)
{
    uint32_t cfg = *(volatile uint32_t *)0x20009014u;
    if (input > 63 || output > 63 || !(cfg & 0x80000000u) ||
        (cfg & 0x40000000u)) return 1;
    field(0xf884, 0x0fff0000u, (input << 22) | (output << 16));
    uint32_t divisor = (cfg & 1023u) + 1;
    uint32_t mhz = (cfg & 0x20000000u) ? 320 : 40;
    delay((100 * mhz + divisor - 1) / divisor);
    return 0;
}
static uint32_t xtal_cap_get(void)
{
    return (REG(0xf884) >> 22) & 63;
}
static int spi_swap(uint32_t state)
{
    if (state > 1) return 1;
    field(0x510, 0x2000, state << 13);
    return 0;
}
static int uart(uint32_t enable, uint32_t sel, uint32_t div)
{
    if (enable > 1 || sel > 2 || div > 7) return 1;
    /* bl616_glb.c:1536 -> bl616_hbn.c:593, preserve write ordering. */
    field(0x150, 0x10, 0);
    field(0x150, 7, div);
    field(0xf030, 0x8004, sel == 2 ? 0x8000 : sel << 2);
    field(0x150, 0x10, enable << 4);
    return 0;
}
/* Flash transport slots, exact fixed-SDK identities:
 * 358 Sbus2_Replace(u8 pad), 359 Sbus2_Revoke_replace(void),
 * 379 SFlash_Read(cfg*,u8 io,u8 cont,u32 addr,u8 *data,u32 len),
 * 440 XIP_Erase_Need_Lock(cfg*,u32 start,int len,u8 group,u8 bank),
 * 444 XIP_Read_Need_Lock(cfg*,u32 addr,u8 *data,u32 len,u8 group,u8 bank),
 * 446 State_Restore(cfg*,u32 offset,u8 group,u8 bank),
 * 447 State_Save(cfg*,u32 *offset,u8 group,u8 bank),
 * 448 XIP_Write_Need_Lock(cfg*,u32 addr,u8 *data,u32 len,u8 group,u8 bank),
 * 449 Opt_Enter(u8 *aesEnable), 450 Opt_Exit(u8 aesEnable).
 * Capture all six integer argument registers, including unused registers.
 * Host must validate only the actual SDK arguments for each operation.
 */
static uint32_t flash_call(uint32_t slot, uint32_t a0, uint32_t a1,
                           uint32_t a2, uint32_t a3, uint32_t a4, uint32_t a5)
{
    volatile uint32_t *mailbox = (volatile uint32_t *)0xf10000c0u;
    mailbox[0] = a0;
    mailbox[1] = a1;
    mailbox[2] = a2;
    mailbox[3] = a3;
    mailbox[4] = a4;
    mailbox[5] = a5;
    mailbox[6] = slot;
    return mailbox[7];
}
#define FLASH_WRAPPER(slot) \
    static uint32_t flash_##slot(uint32_t a0, uint32_t a1, uint32_t a2, \
                                 uint32_t a3, uint32_t a4, uint32_t a5) \
    { return flash_call(slot, a0, a1, a2, a3, a4, a5); }
FLASH_WRAPPER(320)
FLASH_WRAPPER(368)
FLASH_WRAPPER(369)
FLASH_WRAPPER(371)
FLASH_WRAPPER(374)
FLASH_WRAPPER(384)
FLASH_WRAPPER(389)
FLASH_WRAPPER(392)
FLASH_WRAPPER(329)
FLASH_WRAPPER(365)
FLASH_WRAPPER(367)
FLASH_WRAPPER(377)
FLASH_WRAPPER(385)
FLASH_WRAPPER(387)
FLASH_WRAPPER(393)
FLASH_WRAPPER(402)
FLASH_WRAPPER(358)
FLASH_WRAPPER(359)
FLASH_WRAPPER(379)
FLASH_WRAPPER(401)
FLASH_WRAPPER(440)
FLASH_WRAPPER(444)
FLASH_WRAPPER(446)
FLASH_WRAPPER(447)
FLASH_WRAPPER(448)
FLASH_WRAPPER(449)
FLASH_WRAPPER(450)
#undef FLASH_WRAPPER
typedef void (*entry)(void);
__attribute__((section(".api"), used))
const entry ap01_bootrom_api[512] = {
    [14] = (entry)xtal_cap, [20] = (entry)xtal_cap_get,
    [185] = (entry)gpio, [132] = (entry)dig_select,
    [131] = (entry)dig512, [149] = (entry)pwm,
    [147] = (entry)pka, [162] = (entry)usb, [163] = (entry)spi_swap,
    [134] = (entry)em_select,
    [89] = (entry)wifi_reset, [107] = (entry)per_ungate,
    [161] = (entry)uart,
    [358] = (entry)flash_358, [359] = (entry)flash_359,
    [320] = (entry)flash_320,
    [368] = (entry)flash_368, [369] = (entry)flash_369,
    [371] = (entry)flash_371, [374] = (entry)flash_374,
    [384] = (entry)flash_384, [389] = (entry)flash_389,
    [392] = (entry)flash_392,
    [367] = (entry)flash_367,
    [329] = (entry)flash_329, [365] = (entry)flash_365,
    [377] = (entry)flash_377, [385] = (entry)flash_385,
    [387] = (entry)flash_387, [393] = (entry)flash_393,
    [402] = (entry)flash_402,
    [379] = (entry)flash_379, [440] = (entry)flash_440,
    [401] = (entry)flash_401,
    [444] = (entry)flash_444, [446] = (entry)flash_446,
    [447] = (entry)flash_447, [448] = (entry)flash_448,
    [449] = (entry)flash_449, [450] = (entry)flash_450,
};

/* Explicit PEC API transport, not a recovered/decrypted Boot2 image.
 * Each entry preserves the four-register integer ABI and records the actual
 * original header byte offset. The host validates and executes the operation;
 * no wrapper substitutes a fixed success value for an unavailable service.
 */
typedef uint32_t (*pec_entry)(uint32_t, uint32_t, uint32_t, uint32_t);
static uint32_t pec_call(uint32_t operation, uint32_t a0, uint32_t a1,
                         uint32_t a2, uint32_t a3)
{
    volatile uint32_t *mailbox = (volatile uint32_t *)0xf1000080u;
    mailbox[0] = a0;
    mailbox[1] = a1;
    mailbox[2] = a2;
    mailbox[3] = a3;
    mailbox[4] = operation;
    return mailbox[5];
}
#define PEC_WRAPPER(name, offset) \
    static uint32_t name(uint32_t a0, uint32_t a1, uint32_t a2, uint32_t a3) \
    { return pec_call(offset, a0, a1, a2, a3); }
PEC_WRAPPER(pec_30, 0x30)
PEC_WRAPPER(pec_88, 0x88)
PEC_WRAPPER(pec_40, 0x40)
PEC_WRAPPER(pec_44, 0x44)
PEC_WRAPPER(pec_48, 0x48)
PEC_WRAPPER(pec_4c, 0x4c)
PEC_WRAPPER(pec_50, 0x50)
PEC_WRAPPER(pec_e8, 0xe8)
#undef PEC_WRAPPER
struct pec_api {
    uint32_t magic, version, count, reserved;
    pec_entry table[128];
};
_Static_assert(sizeof(struct pec_api) == 0x210, "PEC table requires RV32 pointers");
__attribute__((section(".pec_api"), used))
const struct pec_api ap01_pec_api = {
    .magic = 0x43455041, .version = 1, .count = 128, .reserved = 0,
    .table = {
        [0x30 / 4] = pec_30, [0x88 / 4] = pec_88,
        [0x40 / 4] = pec_40, [0x44 / 4] = pec_44,
        [0x48 / 4] = pec_48, [0x4c / 4] = pec_4c,
        [0x50 / 4] = pec_50, [0xe8 / 4] = pec_e8,
    },
};
