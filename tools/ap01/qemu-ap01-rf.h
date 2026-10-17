/* SPDX-License-Identifier: GPL-2.0-or-later
 * Explicit synthetic RF configuration storage, NOT a radio/calibration model.
 * Original0041 modem_init_core a00102b4:
 *   a0010318..034c MIX+220 clearbit26/setbit27;
 *   a0010360..03be MIX+1c0/c4/c8/cc frequency-table field writes.
 * Fixed SDK41bf338a phyrf/lib-gcc_10.2.0-toolchain_V2.6.1/libbl616_phyrf.a,
 * rfc.o .text.modem_init_core +60/+64 matches MIX+220 access; relocations
 * identify rf_pri_input_xtalfreq and rf_pri_init, followed by calibration
 * table getters. Dynamic calibration feedback is separately opt-in in
 * qemu-ap01-rf-cal/acal/pm.h; this file only stores programmed fields.
 */
#ifndef QEMU_AP01_RF_H
#define QEMU_AP01_RF_H
#include "exec/memattrs.h"

typedef struct AP01RF {
    uint32_t config[130];
} AP01RF;

static void ap01_rf_reset(AP01RF *s)
{
    /* Unknown silicon reset: declared synthetic cold/warm zero latches only. */
    memset(s->config, 0, sizeof(s->config));
}

static MemTxResult ap01_rf_access(AP01RF *s, hwaddr offset, uint64_t *value,
                                 unsigned size, bool write)
{
    static const uint32_t addresses[] = {
        0x220, 0x1c0, 0x1c4, 0x1c8, 0x1cc,
        0x21c, 0x094, 0x608, 0x63c, 0x090, 0x618, 0x504,
        0x05c, 0x070,
        0x780, 0x7b4,
        0x788, 0x78c, 0x790, 0x794, 0x798, 0x79c, 0x7a0, 0x7a4, 0x7a8,
        0x784, 0x7b8,
        0x704, 0x7ac,
        0x078, 0x0a4, 0x030, 0x0cc, 0x0b8,
        0x138, 0x0b0, 0x0b4, 0x01c, 0x02c,
        0x004, 0x10c, 0x07c, 0x088, 0x1b0, 0x068, 0x0d4,
        0x7bc, 0x7c0, 0x7c4, 0x7c8, 0x7cc, 0x7d0, 0x7d4, 0x7d8,
        0x0d0,
        0x014, 0x0c0, 0x0c4,
        0x600, 0x60c, 0x61c, 0x048, 0x20c, 0x214, 0x218,
        0x23c, 0x240, 0x244, 0x0f0, 0x064, 0x058, 0x074,
        0x060,
        0x0bc,
        0x614,
        0x70c, 0x710, 0x714, 0x718, 0x71c, 0x720, 0x724, 0x728,
        0x72c, 0x730, 0x734, 0x738, 0x73c, 0x740, 0x744, 0x748,
        0x74c, 0x750, 0x754, 0x758, 0x75c, 0x760, 0x764, 0x768,
        0x76c, 0x770,
        0x170, 0x174, 0x178, 0x17c,
        0x13c, 0x140, 0x144, 0x148, 0x14c, 0x150,
        0x154, 0x158, 0x15c, 0x160, 0x164, 0x26c, 0x268,
        0x708, 0x700,
        0x400,
        0x080, 0x2c4, 0x514,
        0x08c, 0x50c,
        0x228, 0x264,
        0x680, 0x684,
    };
    static const uint32_t masks[] = {
        0x0d001ff3, 0x00000fff, 0x1fffffff, 0x000fffff, 0x000fffff,
        /* Original a0012c7c..2cf4 consecutive configuration sequence:
         * +21c clear10001000; +094 OR30010000; +220 OR600;
         * +608 clear20000000; +63c store0, later a001324c OR8080;
         * +090 replace low3 with4; +618 clearc0000000; +504 set100000.
         * No calibration completion/status is synthesized by these latches.
         */
        0x10001000, 0x30010000, 0x20000000, 0x00008080,
        0x00000037, 0xc01fffff, 0x00100010,
        /* a0016fde..6ff8 cap-code RMW maskffe0fc0f (defaultOR100200).
         * a00132ee..3304 replaces MIX+070 low4 with calibration-policy
         * table value; this is a programmed field, not a measured status.
         */
        0x001f03f0, 0x0000077f,
        /* Original a0016544..661e, rf_pri_set_bz_channel_pwr_comp:
         * each register packs five programmed six-bit channel power
         * compensation fields from local/RAM parameter tables. SDK
         * rf_pri.o same symbol +166/+1e8 accesses MIX+780/+7b4.
         * Upper two bits preserved; no RF measurement/status inputs.
         */
        0x3fffffff, 0x3fffffff,
        /* Original a0011d06..20aa = SDK rf_pri_bz_pwr_table_w2reg.
         * +788 gets four 6-bit programmed powers. Following four
         * register pairs pack policy-table fields in bits11..29 then
         * bits24..27. Original stores: a0011e12/1e7c/1eb0/1f1a/1f5c/
         * 1fc0/1ffa/2062/2092. No sampled RF data or ready flags.
         */
        /* BZ TXCAL additionally stores low11 / packed bits23:2 into
         * these four existing power-table pairs (SDK w2reg+22..18e). */
        0x00ffffff, 0x3fffffff, 0x0ffffffc,
        0x3fffffff, 0x0ffffffc, 0x3fffffff, 0x0ffffffc,
        0x3fffffff, 0x0ffffffc,
        /* Original rf_pri_set_bz_temp_comp a00166a0:
         * a001684a..68e8 / a00168f8..6962 pack five 6-bit deltas
         * computed from RAM temperature-compensation parameter tables.
         * SDK same symbol +1c2/+270 reads MIX+784/+7b8. These
         * programmed offsets are not a thermal sensor measurement.
         */
        0x3fffffff, 0x3fffffff,
        /* Original rf_pri_efuse_init a0012bf6..2c36; fixed SDK same
         * symbol +e0/+fc accesses +704/+7ac. RAM policy value57c2
         * is programmed into +704 highbyte and (value-4) into +7ac
         * lowbyte. Both preserve other fields; neither reads RF status.
         */
        0xffffffff, 0x000000ff,
        /* Original a0012d48..2e6c configuration sequence, including
         * +220 clearbit4: +078 clears30000000; +0a4 clearsbit29,
         * sets low3=5 and configured clock bits8..11; +030 OR1003;
         * +0cc OR200000; +0b8 sets/clearsbit4 from RAM policy;
         * +138 OR3; +0b0 clearbit0/setbit2/clearc0;
         * +0b4 clears33800 then ORc000; +01c clear80; +02c OR40.
         * These are mode/config RMW fields, not completion indicators.
         */
        0x30000000, 0x20000f07, 0xffff7f03, 0x00200000, 0x01013010,
        0x0337733f, 0x100000c5, 0x0003f800, 0x00003098, 0x00004267,
        /* Finite remainder a0012e6e..304c (both RAM-policy branches):
         * +004 replacebits2:3 with4; +10c lowbyte22/66;
         * +07c bits4:6=10/30; +088 bits12:14=0/4000;
         * +070 clears700/OR270; +138 clears0337700c/OR11330;
         * +1b0 store0fffffff; +068 AND0c00c088/OR e17e0244;
         * +0d4 ANDfff0f00f/OR00f013c1; +7bc..7d8 low22-bit
         * literal policy values, preserving upper10. No a0/a8/ac
         * calibration-handshake ownership here and no sampled results.
         */
        0x0000080e, 0x000000ff, 0x00000170, 0x93007000,
        0x0fffffff, 0xf3ff3f77, 0x00ff1ff1,
        0x003fffff, 0x003fffff, 0x003fffff, 0x003fffff,
        0x003fffff, 0x003fffff, 0x003fffff, 0x003fffff,
        /* Original finite XTAL/WiFiPLL config routine a0018b74..8d74:
         * a0018bb4..8bba clears low4; alternative a0018c0e..8c18
         * clearsbit0 then OR0xe. All other MMIO in this routine is
         * GLB WiFiPLL +818/+81c/+828/+82c/+830, owned by main model.
         */
        0x0000000f,
        /* Fixed SDK rf_pri_lo_fcal +20/+3ec: MIX+014 mode bits5:4;
         * +4a clear +004.bit1; +58 +02c=0 (only previously setbit6);
         * +5c..84 +030 AND0cf090ff/OR0cf00000 and bit1;
         * +a2..bc +0b8 clear3000/bit4, later togglebit16;
         * +dc..f0/+108..120 +0c0 bit12/16 toggles; +0c4=01000000;
         * +12c..136 +01c bit3. Cleanup restores saved registers then
         * ORs +014 with30. These are configuration, never ready flags.
         * rf_pri_save_state_before_cal (original a00162bc) additionally
         * reads the fourteen registers below; restore writes their saved
         * values unchanged. No LOFCAL mutation of these fields is known,
         * so accept reads and unchanged restores only (mask0), rather than
         * inventing broad writable masks or downstream peripheral effects.
         */
        /* ACAL original a0013538..354c / a00136c6 also uses +014
         * bits7:6; a0013592 sets +01c.bit4. Shared fields above retain
         * their LOFCAL behavior; comparator +0a4.bit12 is not a latch. */
        /* a0018f8a..96 selects 40MHz table a000d4a0 via RAM62fc5690.
         * ACAL a00135e6 th.lrw loads one of 21 full channel words
         * 14088889..14b33333, then a00135ea writes +0c4. Their union
         * 14ffffff plus LOFCAL literal01000000 gives 15ffffff.
         * This is programmed tuning stimulus, not a measured result. */
        0x03f000f0, 0x00011000, 0x3fffffff,
        /* TXCAL SDK +94..282 / original a00173ba onward: programmed
         * mixer stimulus and candidates only, no PM ready/result bits. */
        0x00fff7ff, 0, 0xffff0000, 0x31f7731f, 0x83ff0000, 0xffc007ff, 0xffc007ff,
        0x00040000, 0, 0, 0, 0xf7fc0000, 0x00070000, 0x3f3f0000,
        0x00000003,
        /* rf_pri_config_channel: programmed four-bit CW field. */
        0x20f00000,
        /* RXIQ candidate stimulus: low signed10+enable10,
         * high unsigned11 at12+enable23, bit11 remains unknown. */
        0x00fff7ff,
        /* Original a0018642 / SDK txcal_w2reg: thirteen packed pairs
         * retain real firmware-computed calibration words per channel. */
        0xffffffff, 0x0ffffffc, 0xffffffff, 0x0ffffffc,
        0xffffffff, 0x0ffffffc, 0xffffffff, 0x0ffffffc,
        0xffffffff, 0x0ffffffc, 0xffffffff, 0x0ffffffc,
        0xffffffff, 0x0ffffffc, 0xffffffff, 0x0ffffffc,
        0xffffffff, 0x0ffffffc, 0xffffffff, 0x0ffffffc,
        0xffffffff, 0x0ffffffc, 0xffffffff, 0x0ffffffc,
        0xffffffff, 0x0ffffffc,
        /* RXCAL original a0015eca..5f20: computed gain/phase words. */
        0x07ff03ff, 0x07ff03ff, 0x07ff03ff, 0x07ff03ff,
        /* modem_init_core a0010408..0462 writes native LO calibration
         * bytes for 21channels (two freq/idac pairs per word, final pair
         * in low16), then selects its programmed baseband config. */
        0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff,
        0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff,
        0x0000ffff, 0x3ffffc0f, 0x3ff2ffff,
        /* Native pwr_table_w2reg a0013e92: programmed power fields;
         * above thirteen A/B pairs also retain their packed power bits. */
        0x3fffffff, 0x00ffffff,
        /* modem_init_core a001051c..056a: programmed mux/enable bits. */
        0x0080c300,
        /* Fixed SDK modem_init_core final configuration writes. */
        0x40000000, 0x00000003, 0x00000010,
        /* Original phy_init a00117ea and bba_init f750/f75c:
         * programmed low nibble and gain-zero clears, no sampled status. */
        0x0000000f, 0x00000300,
        /* Native BW0/channel1 ordered configuration with native udelay. */
        0x0000000c, 0x00000fff,
        /* Native channel1 policy-disabled path stores0, then bit31. */
        0, 0x80000000,
    };
    G_STATIC_ASSERT(G_N_ELEMENTS(addresses) == G_N_ELEMENTS(masks));
    G_STATIC_ASSERT(G_N_ELEMENTS(addresses) == G_N_ELEMENTS(s->config));
    if (size != 4 || (offset & 3)) return MEMTX_ERROR;
    for (unsigned i = 0; i < G_N_ELEMENTS(addresses); i++) {
        if (offset != addresses[i]) continue;
        if (write) {
            uint32_t requested = *value;
            /* RMW-preserved bits are tolerated, but unknown changed bits
             * fail rather than being silently discarded or made ready. */
            if ((requested ^ s->config[i]) & ~masks[i]) return MEMTX_ERROR;
            s->config[i] = requested;
        } else {
            *value = s->config[i];
        }
        return MEMTX_OK;
    }
    return MEMTX_ERROR;
}
#endif
