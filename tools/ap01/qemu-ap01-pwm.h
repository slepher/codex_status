/* SPDX-License-Identifier: GPL-2.0-or-later
 * Bounded PWMv2 configuration model, fixed SDK41bf338a:
 * drivers/lhal/include/hardware/pwm_v2_reg.h and src/bflb_pwm_v2.c.
 * Two group banks at40/80 (stride40), four threshold channels per group.
 * Synthetic reset is stopped with other config zero. STOP_STS reflects a
 * finite2us virtual-clock stop/start transition, never a constant-ready bit.
 * Clock/break/repeat/channel fields are configuration latches only: no PWM
 * waveforms, period counters, break inputs, ADC triggering or IRQ events.
 */
#ifndef QEMU_AP01_PWM_H
#define QEMU_AP01_PWM_H
#include "qemu/timer.h"
#include "exec/memattrs.h"

typedef struct AP01PWMGroup {
    QEMUTimer *timer;
    uint32_t config, channels, period, dead_time, threshold[4], mask, enable;
    bool stopped;
} AP01PWMGroup;
typedef struct AP01PWM { AP01PWMGroup group[2]; } AP01PWM;

static void ap01_pwm_complete(void *opaque)
{
    AP01PWMGroup *g = opaque;
    g->stopped = !!(g->config & (1u << 27));
}
static void ap01_pwm_reset(AP01PWM *s)
{
    for (unsigned i = 0; i < 2; ++i) {
        AP01PWMGroup *g = &s->group[i];
        if (!g->timer) g->timer = timer_new_ns(QEMU_CLOCK_VIRTUAL, ap01_pwm_complete, g);
        timer_del(g->timer);
        g->config = 1u << 27;
        g->stopped = true;
        g->channels = g->period = g->dead_time = g->mask = g->enable = 0;
        memset(g->threshold, 0, sizeof(g->threshold));
    }
}
static MemTxResult ap01_pwm_access(AP01PWM *s, hwaddr offset,
                                  uint64_t *value, unsigned size, bool write)
{
    uint32_t v = *value;
    if (size != 4 || (offset & 3)) return MEMTX_ERROR;
    if (offset == 0) {
        if (write && (v & ~0x303u)) return MEMTX_ERROR;
        if (!write) *value = 0; /* No modeled source asserts global status. */
        return MEMTX_OK;
    }
    if (offset < 0x40 || offset >= 0xc0) return MEMTX_ERROR;
    AP01PWMGroup *g = &s->group[(offset - 0x40) / 0x40];
    unsigned reg = (offset - 0x40) & 0x3f;
    uint32_t *latch = NULL;
    switch (reg) {
    case 0:
        if (!write) {
            *value = g->config | (g->stopped ? 1u << 29 : 0);
            return MEMTX_OK;
        }
        /* Bit29 RO is tolerated in SDK read/modify/write, never stored. */
        if ((v & ~0xfff8ffffu) || (v >> 30) == 3) return MEMTX_ERROR;
        bool old_target = !!(g->config & (1u << 27));
        bool target = !!(v & (1u << 27));
        g->config = v & 0xdff8ffffu;
        if (target == g->stopped) timer_del(g->timer);
        else if (old_target != target || !timer_pending(g->timer))
            timer_mod(g->timer, qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) + 2000);
        return MEMTX_OK;
    case 4: latch = &g->channels; break;
    case 8: latch = &g->period; break;
    case 12: latch = &g->dead_time; break;
    case 16: case 20: case 24: case 28:
        latch = &g->threshold[(reg - 16) / 4]; break;
    case 32:
        if (write) return MEMTX_ERROR;
        *value = 0; return MEMTX_OK;
    case 36: latch = &g->mask; break;
    case 40:
        if (!write || (v & ~0x7ffu)) return MEMTX_ERROR;
        return MEMTX_OK; /* W1C of empty local status. */
    case 44: latch = &g->enable; break;
    default: return MEMTX_ERROR;
    }
    if (write) {
        if ((latch == &g->mask || latch == &g->enable) && (v & ~0x7ffu)) return MEMTX_ERROR;
        *latch = v;
    } else *value = *latch;
    return MEMTX_OK;
}
#endif
