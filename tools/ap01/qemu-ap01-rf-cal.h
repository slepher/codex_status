/* SPDX-License-Identifier: GPL-2.0-or-later
 * Explicit synthetic LOFCAL counter, not an analog RF/PLL implementation.
 * Fixed SDK41bf338a libbl616_phyrf.a rf_pri.o .text.rf_pri_lo_fcal:
 * +8e/+150/+23e/+2a8 replace MIX+a0 low8 (candidate);
 * +c0..d0 replace MIX+a8 low16 (reference divisor);
 * +16e..196/+260..292/+2ce..2f2 set/clear MIX+ac bit4, poll bit20,
 * then read MIX+a8 high16. The poll/read sequence matches original0041
 * a00138b8 byte-for-byte; function entry matches a001375c.
 * Original 40MHz threshold table a000d248 = {42656,42720,44256}.
 * Only this reference-divisor profile is supported. The declared linear
 * counter law and 2us latency are virtual stimuli, not recovered silicon
 * transfer functions, oscillator frequencies or measured hardware timing.
 * Caller must additionally gate this aperture on XTAL40MHz and opt-in.
 */
#ifndef QEMU_AP01_RF_CAL_H
#define QEMU_AP01_RF_CAL_H

#include "qemu/timer.h"
#include "exec/memattrs.h"

typedef struct AP01RFCal {
    QEMUTimer *timer;
    uint8_t candidate, sampled_candidate, amplitude;
    uint16_t divisor, sampled_divisor, result;
    bool trigger, busy, done;
} AP01RFCal;

static void ap01_rf_cal_complete(void *opaque)
{
    AP01RFCal *s = opaque;

    if (s->busy && s->trigger) {
        /* All uint8 candidates yield a positive, bounded uint16 result. */
        s->result = 42688 + (128 - (int)s->sampled_candidate) * 64;
        s->busy = false;
        s->done = true;
    }
}

static void ap01_rf_cal_reset(AP01RFCal *s)
{
    if (s->timer) {
        timer_del(s->timer);
    }
    /* Synthetic reset values; no undocumented silicon reset is claimed. */
    s->candidate = s->sampled_candidate = s->amplitude = 0;
    s->divisor = s->sampled_divisor = s->result = 0;
    s->trigger = s->busy = s->done = false;
}

static void ap01_rf_cal_init(AP01RFCal *s)
{
    s->timer = timer_new_ns(QEMU_CLOCK_VIRTUAL, ap01_rf_cal_complete, s);
    ap01_rf_cal_reset(s);
}

static MemTxResult ap01_rf_cal_access(AP01RFCal *s, hwaddr offset,
                                     uint64_t *value, unsigned size, bool write)
{
    uint32_t v = *value;

    if (size != 4 || (offset != 0xa0 && offset != 0xa8 && offset != 0xac)) {
        return MEMTX_ERROR;
    }
    if (!write) {
        switch (offset) {
        case 0xa0: *value = s->candidate | ((uint32_t)s->amplitude << 16); break;
        case 0xa8: *value = s->divisor | ((uint32_t)s->result << 16); break;
        case 0xac: *value = (s->trigger ? 0x10 : 0) |
                           (s->done ? 0x100000 : 0); break;
        }
        return MEMTX_OK;
    }
    switch (offset) {
    case 0xa0:
        /* SDK rf_pri_lo_acal (original a0013518) independently RMWs
         * bits21:16 for amplitude; this does not alter LOFCAL feedback. */
        if (v & ~0x003f00ffU) {
            return MEMTX_ERROR;
        }
        s->candidate = v;
        s->amplitude = (v >> 16) & 0x3f;
        break;
    case 0xa8:
        /* High16 is read-only; allow native RMW preserving previous result. */
        s->divisor = v;
        break;
    case 0xac:
        /* Bit20 is read-only, including native clear-trigger RMW writes. */
        if (v & ~0x100010U) {
            return MEMTX_ERROR;
        }
        if ((v & 0x10) && !s->trigger) {
            if (s->divisor != 0x855) {
                return MEMTX_ERROR;
            }
            s->sampled_candidate = s->candidate;
            s->sampled_divisor = s->divisor;
            s->trigger = s->busy = true;
            s->done = false;
            timer_mod(s->timer, qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) + 2000);
        } else if (!(v & 0x10)) {
            timer_del(s->timer);
            s->trigger = s->busy = s->done = false;
        }
        break;
    }
    return MEMTX_OK;
}
#endif
