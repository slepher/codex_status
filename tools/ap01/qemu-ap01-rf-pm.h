/* SPDX-License-Identifier: GPL-2.0-or-later
 * Explicit bounded TX/RXCAL PM stimulus, not an analog RF measurement.
 * Original first PM transaction: MIX014[23:20]=5, 214/218 low11 equal,
 * 61c high16=1024; 618 bit29 rising starts, bit28 done, 620 signed25.
 * SDK41bf338a phyrf archive rf_pri.o supplies the matched TXCAL sequence.
 * Ideal mean=112+candidate-256-2*(trim-11)^2, raw=mean*count,
 * count is explicitly1024 or4096 for scalar TX only.
 * candidate domain0..2047, trim=MIX070 low4 and 2us latency
 * are declared synthetic hardware behavior. The supported profile is selected
 * by recovered 40MHz channel words, independently of clock-initialization
 * latches. All21 words share this law; no wireless link or silicon
 * calibration is claimed.
 * RX helper original a0015fd4: mode014[25:24]=1, count61c=20480,
 * control618 bit30/bit20 enabled, low20=f0000, trigger29/done28.
 * Original a0016056 writes601f0000 (rf-rxcal-58); earlier fixture
 * transcription f000 was incorrect and is explicitly unsupported.
 * Synthetic RX I=sign10(614.low10), Q=614[22:12]-1024, each times512;
 * disabled614 bits10/23 produce zero on their corresponding component.
 * TX IQ: mode5/RXdomain0,count4096,request20=1,low20=9200 or12400.
 * I=sign6(074[29:24])+enabled sign10(600.low10),
 * Q=sign6(074[21:16])+enabled(600[22:12]-1024); raw=component*512.
 * This independent linear residual is synthetic, not recovered RF physics.
 * Caller owns config latches, size checks and read-only write rejection.
 */
#ifndef QEMU_AP01_RF_PM_H
#define QEMU_AP01_RF_PM_H

#include "qemu/timer.h"
#include "exec/memattrs.h"

typedef struct AP01RFPM {
    QEMUTimer *timer;
    uint16_t candidate, count;
    int16_t iq_i, iq_q;
    uint8_t trim;
    uint32_t channel, result, result_q, rx_candidate;
    bool trigger, busy, done, rx, iq;
} AP01RFPM;

static void ap01_rf_pm_complete(void *opaque)
{
    AP01RFPM *s = opaque;

    if (s->trigger && s->busy) {
        if (s->rx) {
            int32_t i = s->rx_candidate & 0x3ff;
            int32_t q = ((s->rx_candidate >> 12) & 0x7ff) - 1024;
            if (i & 0x200) {
                i -= 1024;
            }
            s->result = (s->rx_candidate & 0x400) ?
                ((uint32_t)(i * 512) & 0x01ffffff) : 0;
            s->result_q = (s->rx_candidate & 0x800000) ?
                ((uint32_t)(q * 512) & 0x01ffffff) : 0;
        } else if (s->iq) {
            s->result = (uint32_t)((int32_t)s->iq_i * 512) & 0x01ffffff;
            s->result_q = (uint32_t)((int32_t)s->iq_q * 512) & 0x01ffffff;
        } else {
            int32_t delta = (int32_t)s->trim - 11;
            int32_t raw = (112 + (int32_t)s->candidate - 256 -
                           2 * delta * delta) * s->count;
            s->result = (uint32_t)raw & 0x01ffffff;
            s->result_q = 0;
        }
        s->busy = false;
        s->done = true;
    }
}

static void ap01_rf_pm_reset(AP01RFPM *s)
{
    if (s->timer) {
        timer_del(s->timer);
    }
    s->candidate = s->count = 0;
    s->iq_i = s->iq_q = 0;
    s->trim = 0;
    s->channel = s->result = s->result_q = s->rx_candidate = 0;
    s->trigger = s->busy = s->done = s->rx = s->iq = false;
}

static void ap01_rf_pm_init(AP01RFPM *s)
{
    s->timer = timer_new_ns(QEMU_CLOCK_VIRTUAL, ap01_rf_pm_complete, s);
    ap01_rf_pm_reset(s);
}

static bool ap01_rf_pm_channel_supported(uint32_t word)
{
    static const uint32_t channels[] = {
        0x14088889, 0x14111111, 0x1419999a, 0x14222222, 0x142aaaab,
        0x14333333, 0x143bbbbc, 0x14444444, 0x144ccccd, 0x14555555,
        0x145dddde, 0x14666666, 0x146eeeef, 0x14777777, 0x14800000,
        0x14888889, 0x14911111, 0x1499999a, 0x14a22222, 0x14aaaaab,
        0x14b33333,
    };
    for (size_t i = 0; i < sizeof(channels) / sizeof(channels[0]); i++) {
        if (word == channels[i]) {
            return true;
        }
    }
    return false;
}

static int32_t ap01_rf_pm_signed(uint32_t value, unsigned bits)
{
    int32_t result = value & ((1U << bits) - 1);
    return result & (1U << (bits - 1)) ? result - (int32_t)(1U << bits) : result;
}

/* Validate before changing state: failed rising edges are atomic.
 * Full register snapshots are supplied; unrelated fields belong to base.
 * Holding the trigger high does not restart or replace sampled inputs.
 */
static MemTxResult ap01_rf_pm_trigger(AP01RFPM *s, uint32_t mode014,
                                     uint32_t channel_c4, uint32_t candidate214,
                                     uint32_t candidate218, uint32_t count61c,
                                     uint32_t trim070, uint32_t control618,
                                     uint32_t candidate614, uint32_t coefficient074,
                                     uint32_t coefficient600, bool trigger)
{
    if (!trigger) {
        timer_del(s->timer);
        s->trigger = s->busy = s->done = false;
        return MEMTX_OK;
    }
    if (s->trigger) {
        return MEMTX_OK;
    }
    bool rx = (mode014 & 0x03000000) == 0x01000000;
    bool iq = !rx && (control618 & 0x00100000);
    bool valid = ap01_rf_pm_channel_supported(channel_c4);
    if (rx) {
        valid &= (count61c >> 16) == 20480 &&
                 (control618 & 0x401fffff) == 0x401f0000;
    } else {
        valid &= !(mode014 & 0x03000000) &&
                 (mode014 & 0x00f00000) == 0x00500000 &&
                 ((count61c >> 16) == 1024 || (count61c >> 16) == 4096);
        if (iq) {
            valid &= (count61c >> 16) == 4096 &&
                     ((control618 & 0xfffff) == 0x9200 ||
                      (control618 & 0xfffff) == 0x12400);
        } else {
            valid &= (candidate214 & 0x7ff) == (candidate218 & 0x7ff);
        }
    }
    if (!valid) {
        return MEMTX_ERROR;
    }
    s->candidate = candidate214 & 0x7ff;
    s->count = count61c >> 16;
    s->trim = trim070 & 0xf;
    s->rx = rx;
    s->iq = iq;
    s->rx_candidate = candidate614;
    s->iq_i = ap01_rf_pm_signed(coefficient074 >> 24, 6) +
              ((coefficient600 & 0x400) ? ap01_rf_pm_signed(coefficient600, 10) : 0);
    s->iq_q = ap01_rf_pm_signed(coefficient074 >> 16, 6) +
              ((coefficient600 & 0x800000) ?
               (int32_t)((coefficient600 >> 12) & 0x7ff) - 1024 : 0);
    s->channel = channel_c4;
    s->trigger = s->busy = true;
    s->done = false;
    timer_mod(s->timer, qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) + 2000);
    return MEMTX_OK;
}

static uint32_t ap01_rf_pm_read_status(const AP01RFPM *s)
{
    return s->done ? 0x10000000 : 0;
}

static MemTxResult ap01_rf_pm_read_result(const AP01RFPM *s, uint32_t *value)
{
    if (!s->done) {
        return MEMTX_ERROR;
    }
    *value = s->result;
    return MEMTX_OK;
}

static MemTxResult ap01_rf_pm_read_result_q(const AP01RFPM *s, uint32_t *value)
{
    if (!s->done || (!s->rx && !s->iq)) {
        return MEMTX_ERROR;
    }
    *value = s->result_q;
    return MEMTX_OK;
}
#endif
