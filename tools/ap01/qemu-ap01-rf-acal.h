/* SPDX-License-Identifier: GPL-2.0-or-later
 * Explicit ideal ACAL comparator; not measured RF or a wireless connection.
 * SDK41bf338a libbl616_phyrf.a rf_pri.o .text.rf_pri_lo_acal matches
 * original0041 a0013518. a001354c selects MIX+014[7:6]=01;
 * a0013604 sets MIX+a0[21:16], delays 1us, a001360e reads +a4 bit12.
 * +c4 is loaded through pointer RAM62fc5690, indexed by channel<<2.
 * a0018f8a selects ROMa000d4a0's 21 words for XTAL40MHz/div855.
 * Only those recovered channel words are supported, not channel indices.
 * Channel-independent threshold24 and settling across one MAC microsecond
 * tick are an explicit synthetic profile, not silicon characteristics.
 * Original udelay1 waits for floor((now-MACepoch)/1000) to increase; it does
 * not guarantee 1000 elapsed ns. Caller supplies the same epoch as MACtime.
 * Caller gates XTAL40MHz/opt-in and merges comparator into +a4 config.
 */
#ifndef QEMU_AP01_RF_ACAL_H
#define QEMU_AP01_RF_ACAL_H

#include "qemu/timer.h"
#include "exec/memattrs.h"

typedef struct AP01RFACal {
    uint32_t mode, candidate_word, channel_word, comparator_bits;
    int64_t settled_ns, epoch_ns;
    bool valid;
} AP01RFACal;

static void ap01_rf_acal_reset(AP01RFACal *s)
{
    /* Synthetic empty state; no undocumented hardware reset is claimed. */
    memset(s, 0, sizeof(*s));
    s->epoch_ns = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL);
}

static void ap01_rf_acal_init(AP01RFACal *s)
{
    ap01_rf_acal_reset(s);
}

/* Bind the MAC counter epoch after init and every VM reset. Epoch changes
 * invalidate samples and clear published output, including warm resets.
 */
static void ap01_rf_acal_set_epoch(AP01RFACal *s, int64_t epoch_ns)
{
    ap01_rf_acal_reset(s);
    s->epoch_ns = epoch_ns;
}

static bool ap01_rf_acal_channel_supported(uint32_t channel_word)
{
    static const uint32_t channels[] = {
        0x14088889, 0x14111111, 0x1419999a, 0x14222222, 0x142aaaab,
        0x14333333, 0x143bbbbc, 0x14444444, 0x144ccccd, 0x14555555,
        0x145dddde, 0x14666666, 0x146eeeef, 0x14777777, 0x14800000,
        0x14888889, 0x14911111, 0x1499999a, 0x14a22222, 0x14aaaaab,
        0x14b33333,
    };
    for (size_t i = 0; i < sizeof(channels) / sizeof(channels[0]); i++) {
        if (channel_word == channels[i]) {
            return true;
        }
    }
    return false;
}

/* Call after a successful write of mode, candidate or channel config. */
static void ap01_rf_acal_update(AP01RFACal *s, uint32_t mode_register,
                                uint32_t a0_register, uint32_t c4_register)
{
    uint32_t mode = mode_register & 0xc0;
    int64_t now = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL);

    if (!s->valid || s->mode != mode ||
        s->candidate_word != a0_register || s->channel_word != c4_register) {
        /* Next MAC tick boundary, never the current tick or zero waiting. */
        s->settled_ns = now >= s->epoch_ns ?
            s->epoch_ns + ((now - s->epoch_ns) / 1000 + 1) * 1000 :
            INT64_MAX;
    }
    s->mode = mode;
    s->candidate_word = a0_register;
    s->channel_word = c4_register;
    s->valid = mode == 0x40 && ap01_rf_acal_channel_supported(c4_register);
}

/* Input qualification only; settling is checked by strict sampling below. */
static bool ap01_rf_acal_measurement_valid(const AP01RFACal *s)
{
    return s->valid;
}

/* Configuration snapshots may read this even before a measurement exists.
 * Synthetic reset/unqualified output is low, never a successful sample.
 */
static uint32_t ap01_rf_acal_output(const AP01RFACal *s)
{
    return s->valid ? s->comparator_bits : 0;
}

/* Only returns the RO comparator field; never changes caller value on error. */
static MemTxResult ap01_rf_acal_read_comparator(AP01RFACal *s, uint32_t *bits)
{
    if (!s->valid || s->mode != 0x40 ||
        qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) < s->settled_ns) {
        return MEMTX_ERROR;
    }
    s->comparator_bits = ((s->candidate_word >> 16) & 0x3f) >= 24 ? 0x1000 : 0;
    *bits = s->comparator_bits;
    return MEMTX_OK;
}
#endif
