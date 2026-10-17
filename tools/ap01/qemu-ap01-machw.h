/* SPDX-License-Identifier: GPL-2.0-or-later
 * Observed local MACHW reset/FSM and three absolute timers. No radio backend,
 * external RX/TX events, physical timing, or independent raw77 IRQ claim.
 * Original GEN handler a015feaa ACKs806c&8074 via8070; INTC54 dispatches
 * through the original OS86/raw70 Wi-Fi ISR a0008bc4.
 */
#ifndef QEMU_AP01_MACHW_H
#define QEMU_AP01_MACHW_H
#include "qemu/timer.h"
#include "exec/memattrs.h"

typedef struct AP01MACHW AP01MACHW;
typedef struct AP01MACDeadline {
    QEMUTimer *timer;
    AP01MACHW *owner;
    uint32_t deadline, bit;
    bool valid;
} AP01MACDeadline;
struct AP01MACHW {
    QEMUTimer *timer, *fsm_timer;
    AP01MACDeadline deadlines[3];
    void (*reset_cb)(void *opaque);
    void *opaque;
    uint32_t (*now_us)(void *opaque);
    void (*gen_level)(void *opaque, bool level);
    void *local_opaque;
    uint32_t generation, current, next, gen_latched, gen_enable;
    uint32_t timer_pending, timer_enable, txrx_enable;
    bool busy, fsm_busy;
};
static uint32_t ap01_machw_gen_pending(AP01MACHW *s)
{
    return s->gen_latched | ((s->timer_pending & s->timer_enable) ? 8u : 0);
}
static void ap01_machw_update_irq(AP01MACHW *s)
{
    if (s->gen_level) s->gen_level(s->local_opaque,
        (s->gen_enable & 0x80000000u) &&
        (ap01_machw_gen_pending(s) & s->gen_enable & 0x7fffffffu));
}
static void ap01_machw_local_reset(AP01MACHW *s)
{
    timer_del(s->fsm_timer);
    for (unsigned i = 0; i < 3; i++) {
        timer_del(s->deadlines[i].timer);
        s->deadlines[i].deadline = 0;
        s->deadlines[i].valid = false;
    }
    s->current = s->next = 0;
    s->fsm_busy = false;
    s->gen_latched = s->gen_enable = 0;
    s->timer_pending = s->timer_enable = s->txrx_enable = 0;
    ap01_machw_update_irq(s);
}
static void ap01_machw_complete(void *opaque)
{
    AP01MACHW *s = opaque;
    if (!s->busy) return;
    s->busy = false;
    s->generation++;
    ap01_machw_local_reset(s);
    if (s->reset_cb) s->reset_cb(s->opaque);
}
static void ap01_machw_fsm_complete(void *opaque)
{
    AP01MACHW *s = opaque;
    if (!s->fsm_busy) return;
    bool idle = s->current == 3 && s->next == 0;
    s->current = s->next;
    s->fsm_busy = false;
    if (idle) s->gen_latched |= 4;
    ap01_machw_update_irq(s);
}
static void ap01_machw_deadline_complete(void *opaque)
{
    AP01MACDeadline *d = opaque;
    d->owner->timer_pending |= d->bit;
    ap01_machw_update_irq(d->owner);
}
static void ap01_machw_reset(AP01MACHW *s)
{
    timer_del(s->timer);
    s->busy = false;
    ap01_machw_local_reset(s);
    /* Diagnostic generation counts completions, not canceled resets. */
}
static void ap01_machw_init(AP01MACHW *s, void (*reset_cb)(void *), void *opaque)
{
    s->reset_cb = reset_cb;
    s->opaque = opaque;
    s->generation = 0;
    s->timer = timer_new_ns(QEMU_CLOCK_VIRTUAL, ap01_machw_complete, s);
    s->fsm_timer = timer_new_ns(QEMU_CLOCK_VIRTUAL, ap01_machw_fsm_complete, s);
    for (unsigned i = 0; i < 3; i++) {
        s->deadlines[i].owner = s;
        s->deadlines[i].bit = 1u << (6 + i);
        s->deadlines[i].timer = timer_new_ns(QEMU_CLOCK_VIRTUAL,
            ap01_machw_deadline_complete, &s->deadlines[i]);
    }
    ap01_machw_reset(s);
}
static void ap01_machw_configure(AP01MACHW *s, uint32_t (*now_us)(void *),
                                 void (*gen_level)(void *, bool), void *opaque)
{
    s->now_us = now_us;
    s->gen_level = gen_level;
    s->local_opaque = opaque;
    ap01_machw_update_irq(s);
}
static bool ap01_machw_handles(hwaddr address)
{
    switch (address) {
    case 0x24b00038: case 0x24b00140: case 0x24b00144: case 0x24b00148:
    case 0x24b08050: case 0x24b0806c: case 0x24b08070:
    case 0x24b08074: case 0x24b08078: case 0x24b0807c:
    case 0x24b08080: case 0x24b08084: case 0x24b08088:
    case 0x24b0808c: return true;
    default: return false;
    }
}
static void ap01_machw_schedule(AP01MACHW *s, AP01MACDeadline *d)
{
    if (!d->valid || !(s->timer_enable & d->bit)) return;
    int32_t delta = (int32_t)(d->deadline - s->now_us(s->local_opaque));
    timer_mod(d->timer, qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) +
              (delta > 0 ? (int64_t)delta * 1000 : 1));
}
static MemTxResult ap01_machw_access(AP01MACHW *s, hwaddr address,
                                    uint64_t *value, unsigned size, bool write)
{
    if (size != 4 || !ap01_machw_handles(address)) return MEMTX_ERROR;
    if (address == 0x24b08050) {
        if (!write) { *value = s->busy; return MEMTX_OK; }
        if (*value != 1) return MEMTX_ERROR;
        if (!s->busy) {
            s->busy = true;
            timer_mod(s->timer, qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) + 2000);
        }
        return MEMTX_OK;
    }
    if (address == 0x24b00038) {
        if (!write) { *value = s->current | s->next << 4; return MEMTX_OK; }
        uint32_t target = (*value >> 4) & 15;
        if ((*value & ~0xffu) || (target != 0 && target != 3) || s->busy ||
            (s->fsm_busy && target != s->next)) return MEMTX_ERROR;
        if (!s->fsm_busy && target != s->current) {
            s->next = target;
            if (target == 3) {
                /* Virtual ACTIVE acceptance is synchronous: original
                 * mm_active immediately calls idle_req, which asserts that
                 * CURRENT is not IDLE. No ACTIVE completion IRQ exists. */
                s->current = target;
            } else {
                s->fsm_busy = true;
                timer_mod(s->fsm_timer, qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) + 2000);
            }
        }
        return MEMTX_OK;
    }
    if (address >= 0x24b00140 && address <= 0x24b00148) {
        AP01MACDeadline *d = &s->deadlines[(address - 0x24b00140) / 4];
        if (!write) { *value = d->deadline; return MEMTX_OK; }
        if (!s->now_us || s->busy) return MEMTX_ERROR;
        d->deadline = *value;
        d->valid = true;
        ap01_machw_schedule(s, d);
        return MEMTX_OK;
    }
    uint32_t *reg = NULL, mask = 0;
    switch (address) {
    case 0x24b0806c:
        if (write) return MEMTX_ERROR;
        *value = ap01_machw_gen_pending(s); return MEMTX_OK;
    case 0x24b08070:
        if (!write || (*value & ~0x8373f14cu)) return MEMTX_ERROR;
        s->gen_latched &= ~(uint32_t)*value; break;
    case 0x24b08074: reg = &s->gen_enable; mask = 0x8373f14c; break;
    case 0x24b08078:
        if (write) return MEMTX_ERROR;
        *value = 0; return MEMTX_OK;
    case 0x24b0807c:
        if (!write || (*value != UINT32_MAX && (*value & ~0x80f107c0u))) return MEMTX_ERROR;
        break; /* Empty TX/RX backend: ACK cannot create an event. */
    case 0x24b08080: reg = &s->txrx_enable; mask = 0x80f107c0; break;
    case 0x24b08084:
        if (write) return MEMTX_ERROR;
        *value = s->timer_pending; return MEMTX_OK;
    case 0x24b08088:
        if (!write || (*value & ~0x1c0u)) return MEMTX_ERROR;
        s->timer_pending &= ~(uint32_t)*value; break;
    case 0x24b0808c: reg = &s->timer_enable; mask = 0x1c0; break;
    default: return MEMTX_ERROR;
    }
    if (reg) {
        if (!write) { *value = *reg; return MEMTX_OK; }
        if (*value & ~mask) return MEMTX_ERROR;
        uint32_t old = *reg;
        if (reg == &s->timer_enable && !s->now_us) return MEMTX_ERROR;
        *reg = *value;
        if (reg == &s->timer_enable) {
            for (unsigned i = 0; i < 3; i++) {
                AP01MACDeadline *d = &s->deadlines[i];
                if ((old & d->bit) && !(*reg & d->bit)) timer_del(d->timer);
                else if (!(old & d->bit) && (*reg & d->bit)) ap01_machw_schedule(s, d);
            }
        }
    }
    ap01_machw_update_irq(s);
    return MEMTX_OK;
}
#endif
