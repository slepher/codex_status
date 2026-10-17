/* SPDX-License-Identifier: GPL-2.0-or-later
 * Restricted BL616 shared TIMER/WDG functional aperture.
 * Fixed SDK41bf338a bflb_wdg.c + hardware/timer_reg.h. Counter/IRQ/reset
 * use QEMU virtual time; synthetic initial match=ffff, clocks80M/40M,
 * ideal32k=32000Hz/1k=1000Hz. External GPIO clock, align modes and zero WDG
 * match remain unsupported, never ready stubs. Timer match events include
 * the SDK's explicit two-tick compensation (bflb_timer_init writes val-2).
 */
#ifndef QEMU_AP01_TIMER_H
#define QEMU_AP01_TIMER_H

#include "qemu/timer.h"
#include "hw/irq.h"
#include "exec/memattrs.h"

typedef void (*AP01TimerResetRequest)(void *opaque);

typedef struct AP01TimerChannel {
    struct AP01Timer *owner;
    QEMUTimer *timer;
    qemu_irq irq;
    uint32_t source, divider, preload, match[3];
    uint64_t counter; /* Extended virtual ticks retain compare pipeline phase. */
    uint8_t pending, enable, trigger, next_matches;
    bool running, up;
    int64_t epoch_ns;
} AP01TimerChannel;

typedef struct AP01Timer {
    QEMUTimer *timer;
    qemu_irq irq;
    AP01TimerResetRequest reset_request;
    void *reset_opaque;
    uint32_t bclk_hz, xtal_hz;
    uint32_t clock_select, divider, mode;
    uint16_t match, counter;
    unsigned key_stage;
    bool pending, reset_status;
    int64_t epoch_ns;
    AP01TimerChannel channel[2];
} AP01Timer;

static uint32_t ap01_timer_source_hz(AP01Timer *s, unsigned source)
{
    switch (source) {
    case 0: return s->bclk_hz;
    case 1: return 32000;
    case 2: return 1000;
    case 3: return s->xtal_hz;
    default: return 0;
    }
}

static uint32_t ap01_timer_channel_hz(AP01TimerChannel *c)
{
    return c->running ? ap01_timer_source_hz(c->owner, c->source) /
                        (c->divider + 1) : 0;
}

static uint64_t ap01_timer_channel_ticks(AP01TimerChannel *c)
{
    return c->counter + muldiv64(
        qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) - c->epoch_ns,
        ap01_timer_channel_hz(c), NANOSECONDS_PER_SECOND);
}

static uint32_t ap01_timer_channel_counter(AP01TimerChannel *c)
{
    return ap01_timer_channel_ticks(c);
}

static void ap01_timer_channel_snapshot(AP01TimerChannel *c)
{
    c->counter = ap01_timer_channel_ticks(c);
    c->epoch_ns = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL);
}

static void ap01_timer_channel_rearm(AP01TimerChannel *c)
{
    uint32_t hz = ap01_timer_channel_hz(c);
    uint64_t counter = ap01_timer_channel_ticks(c);
    uint64_t nearest = UINT64_MAX;
    timer_del(c->timer);
    qemu_set_irq(c->irq, !!(c->pending & c->enable));
    c->next_matches = 0;
    if (!hz) { return; }
    for (unsigned i = 0; i < 3; i++) {
        uint64_t event = (uint64_t)c->match[i] + 2;
        uint64_t distance;
        if ((c->pending & (1 << i)) && (c->up || c->trigger != i + 1)) {
            continue;
        }
        if (event <= counter) {
            event += ((counter - event) / (1ULL << 32) + 1) * (1ULL << 32);
        }
        distance = event - counter;
        if (distance < nearest) {
            nearest = distance;
            c->next_matches = 1 << i;
        } else if (distance == nearest) {
            c->next_matches |= 1 << i;
        }
    }
    if (c->next_matches) {
        int64_t now = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL);
        uint64_t horizon = muldiv64(INT64_MAX - now, hz, NANOSECONDS_PER_SECOND);
        if (nearest < horizon) {
            timer_mod(c->timer, now + muldiv64(nearest, NANOSECONDS_PER_SECOND, hz) + 1);
        }
    }
}

static void ap01_timer_channel_expire(void *opaque)
{
    AP01TimerChannel *c = opaque;
    c->pending |= c->next_matches;
    if (!c->up && c->trigger && (c->next_matches & (1 << (c->trigger - 1)))) {
        c->counter = c->preload;
        c->epoch_ns = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL);
    }
    ap01_timer_channel_rearm(c);
}

static void ap01_timer_set_channel_irqs(AP01Timer *s, qemu_irq irq0,
                                       qemu_irq irq1)
{
    s->channel[0].irq = irq0;
    s->channel[1].irq = irq1;
    for (unsigned i = 0; i < 2; i++) {
        ap01_timer_channel_rearm(&s->channel[i]);
    }
}

static uint32_t ap01_timer_wdg_hz(AP01Timer *s)
{
    uint32_t source;
    switch (s->clock_select) {
    case 0: source = s->bclk_hz; break;
    case 1: source = 32000; break;
    case 2: source = 1000; break;
    case 3: source = s->xtal_hz; break;
    default: source = 0; break; /* source5 explicitly stops the clock. */
    }
    return (s->mode & 1) ? source / (s->divider + 1) : 0;
}

static uint16_t ap01_timer_wdg_counter(AP01Timer *s)
{
    return s->counter + muldiv64(
        qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) - s->epoch_ns,
        ap01_timer_wdg_hz(s), NANOSECONDS_PER_SECOND);
}

static void ap01_timer_wdg_snapshot(AP01Timer *s)
{
    s->counter = ap01_timer_wdg_counter(s);
    s->epoch_ns = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL);
}

static void ap01_timer_wdg_rearm(AP01Timer *s)
{
    uint32_t hz = ap01_timer_wdg_hz(s);
    uint32_t distance = (uint16_t)(s->match - ap01_timer_wdg_counter(s));
    timer_del(s->timer);
    qemu_set_irq(s->irq, s->pending && !(s->mode & 2));
    if (!hz || s->pending) {
        return;
    }
    /* An already-cleared equality is not a new match edge. */
    if (!distance) {
        distance = 65536;
    }
    timer_mod(s->timer, qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) +
              muldiv64(distance, NANOSECONDS_PER_SECOND, hz) + 1);
}

static void ap01_timer_wdg_expire(void *opaque)
{
    AP01Timer *s = opaque;
    s->pending = true;
    if (s->mode & 2) {
        /* The machine owns the actual reset request. Retain WSR evidence
         * through the subsequent warm reset; request at most once. */
        ap01_timer_wdg_snapshot(s);
        s->mode &= ~1U;
        s->reset_status = true;
        qemu_set_irq(s->irq, 0);
        s->reset_request(s->reset_opaque);
    } else {
        qemu_set_irq(s->irq, 1);
    }
}

static void ap01_timer_reset(AP01Timer *s)
{
    timer_del(s->timer);
    s->clock_select = s->divider = s->mode = s->key_stage = 0;
    s->match = UINT16_MAX;
    s->counter = 0;
    s->pending = false;
    s->epoch_ns = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL);
    qemu_set_irq(s->irq, 0);
    for (unsigned i = 0; i < 2; i++) {
        AP01TimerChannel *c = &s->channel[i];
        timer_del(c->timer);
        c->source = c->divider = c->counter = c->preload = 0;
        c->pending = c->enable = c->trigger = c->next_matches = 0;
        c->running = c->up = false;
        c->epoch_ns = s->epoch_ns;
        for (unsigned j = 0; j < 3; j++) { c->match[j] = UINT32_MAX; }
        qemu_set_irq(c->irq, 0);
    }
}

static void ap01_timer_init(AP01Timer *s, qemu_irq irq,
                            AP01TimerResetRequest reset_request, void *opaque)
{
    memset(s, 0, sizeof(*s));
    s->irq = irq;
    s->reset_request = reset_request;
    s->reset_opaque = opaque;
    s->bclk_hz = 80000000;
    s->xtal_hz = 40000000;
    s->timer = timer_new_ns(QEMU_CLOCK_VIRTUAL, ap01_timer_wdg_expire, s);
    for (unsigned i = 0; i < 2; i++) {
        s->channel[i].owner = s;
        s->channel[i].timer = timer_new_ns(QEMU_CLOCK_VIRTUAL,
                                         ap01_timer_channel_expire,
                                         &s->channel[i]);
    }
    ap01_timer_reset(s);
}

static void ap01_timer_set_clocks(AP01Timer *s, uint32_t bclk, uint32_t xtal)
{
    ap01_timer_wdg_snapshot(s);
    for (unsigned i = 0; i < 2; i++) {
        ap01_timer_channel_snapshot(&s->channel[i]);
    }
    s->bclk_hz = bclk;
    s->xtal_hz = xtal;
    ap01_timer_wdg_rearm(s);
    for (unsigned i = 0; i < 2; i++) {
        ap01_timer_channel_rearm(&s->channel[i]);
    }
}

static MemTxResult ap01_timer_channel_access(AP01Timer *s, hwaddr offset,
                                            uint64_t *value, bool write)
{
    if (offset == 0x84 || offset == 0x88) {
        uint32_t result = 0;
        if (write && (*value & ~(offset == 0x84 ? 0x66ULL : 6ULL))) {
            return MEMTX_ERROR;
        }
        for (unsigned i = 0; i < 2; i++) {
            AP01TimerChannel *c = &s->channel[i];
            if (write) {
                ap01_timer_channel_snapshot(c);
                if (offset == 0x84) {
                    bool running = *value & (2 << i);
                    if ((!c->running && running && !c->up) || (*value & (0x20 << i))) {
                        c->counter = (*value & (0x20 << i)) ? 0 : c->preload;
                    }
                    c->running = running;
                } else { c->up = *value & (2 << i); }
                ap01_timer_channel_rearm(c);
            }
            result |= (offset == 0x84 ? c->running : c->up) ? 2 << i : 0;
        }
        if (!write) { *value = result; }
        return MEMTX_OK;
    }
    for (unsigned i = 0; i < 2; i++) {
        AP01TimerChannel *c = &s->channel[i];
        if (offset >= 0x10 + 12 * i && offset < 0x1c + 12 * i) {
            unsigned match = (offset - 0x10 - 12 * i) / 4;
            if (write) { c->match[match] = *value; ap01_timer_channel_rearm(c); }
            else { *value = c->match[match]; }
            return MEMTX_OK;
        }
        if (offset == 0x2c + 4 * i || offset == 0x38 + 4 * i) {
            if (write) { return MEMTX_ERROR; }
            *value = offset == 0x2c + 4 * i ? ap01_timer_channel_counter(c) : c->pending;
            return MEMTX_OK;
        }
        if (offset == 0x44 + 4 * i || offset == 0x50 + 4 * i ||
            offset == 0x5c + 4 * i || offset == 0x78 + 4 * i) {
            if (write) {
                if ((offset == 0x44 + 4 * i || offset == 0x78 + 4 * i) && (*value & ~7ULL)) {
                    return MEMTX_ERROR;
                }
                if (offset == 0x5c + 4 * i && *value > 3) { return MEMTX_ERROR; }
                if (offset == 0x44 + 4 * i) { c->enable = *value; }
                else if (offset == 0x50 + 4 * i) { c->preload = *value; }
                else if (offset == 0x5c + 4 * i) { c->trigger = *value; }
                else { c->pending &= ~*value; }
                ap01_timer_channel_rearm(c);
            } else {
                *value = offset == 0x44 + 4 * i ? c->enable :
                         offset == 0x50 + 4 * i ? c->preload :
                         offset == 0x5c + 4 * i ? c->trigger : 0;
            }
            return MEMTX_OK;
        }
    }
    return MEMTX_ERROR;
}

static MemTxResult ap01_timer_access(AP01Timer *s, hwaddr offset,
                                    uint64_t *value, unsigned size, bool write)
{
    bool halfword = offset == 0x68 || offset == 0x6c ||
                    offset == 0x9c || offset == 0xa0;
    if ((size != 4 && !(halfword && size == 2)) || (offset & 3)) {
        return MEMTX_ERROR;
    }
    if ((offset >= 0x10 && offset < 0x64) || offset == 0x78 || offset == 0x7c ||
        offset == 0x84 || offset == 0x88) {
        return size == 4 ? ap01_timer_channel_access(s, offset, value, write) : MEMTX_ERROR;
    }
    if (offset == 0x9c || offset == 0xa0) {
        if (!write) {
            return MEMTX_ERROR; /* Keys are not a readable credential latch. */
        }
        if (offset == 0x9c && *value == 0xbaba) {
            s->key_stage = 1;
            return MEMTX_OK;
        }
        if (offset == 0xa0 && *value == 0xeb10 && s->key_stage == 1) {
            s->key_stage = 2;
            return MEMTX_OK;
        }
        s->key_stage = 0;
        return MEMTX_ERROR;
    }
    if (!write) {
        switch (offset) {
        case 0x00: *value = (s->clock_select << 8) | s->channel[0].source |
                            (s->channel[1].source << 4); break;
        case 0xbc: *value = (s->divider << 24) | (s->channel[0].divider << 8) |
                            (s->channel[1].divider << 16); break;
        case 0x64: *value = s->mode; break;
        case 0x68: *value = s->match; break;
        case 0x6c: *value = ap01_timer_wdg_counter(s); break;
        case 0x70: *value = s->reset_status; break;
        case 0x80: case 0x98: *value = 0; break; /* W1P actions, no latch. */
        default: return MEMTX_ERROR;
        }
        return MEMTX_OK;
    }
    if (offset == 0x00) {
        unsigned source = (*value >> 8) & 15;
        unsigned source0 = *value & 15, source1 = (*value >> 4) & 15;
        if ((*value & ~0xfffULL) || (source > 3 && source != 5) ||
            (source0 > 3 && source0 != 5) || (source1 > 3 && source1 != 5)) {
            return MEMTX_ERROR;
        }
        ap01_timer_wdg_snapshot(s);
        s->clock_select = source;
        for (unsigned i = 0; i < 2; i++) {
            ap01_timer_channel_snapshot(&s->channel[i]);
            s->channel[i].source = (*value >> (4 * i)) & 15;
        }
    } else if (offset == 0xbc) {
        if (*value & 0xff) {
            return MEMTX_ERROR;
        }
        ap01_timer_wdg_snapshot(s);
        s->divider = *value >> 24;
        for (unsigned i = 0; i < 2; i++) {
            ap01_timer_channel_snapshot(&s->channel[i]);
            s->channel[i].divider = (*value >> (8 + 8 * i)) & 255;
        }
    } else {
        if (s->key_stage != 2) {
            return MEMTX_ERROR;
        }
        switch (offset) {
        case 0x64:
            if ((*value & ~3ULL) || ((*value & 3) == 3 && !s->reset_request)) {
                return MEMTX_ERROR;
            }
            ap01_timer_wdg_snapshot(s);
            s->mode = *value;
            break;
        case 0x68:
            if (!*value || *value > UINT16_MAX) {
                return MEMTX_ERROR;
            }
            s->match = *value;
            break;
        case 0x80:
            if (*value & ~1ULL) {
                return MEMTX_ERROR;
            }
            if (*value & 1) { s->pending = false; }
            break;
        case 0x98:
            if (*value & ~1ULL) {
                return MEMTX_ERROR;
            }
            if (*value & 1) {
                s->counter = 0;
                s->epoch_ns = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL);
            }
            break;
        default: return MEMTX_ERROR;
        }
        /* Model access window: one protected write per ordered key pair;
         * reads do not consume it. No undocumented multi-write unlock. */
        s->key_stage = 0;
    }
    ap01_timer_wdg_rearm(s);
    for (unsigned i = 0; i < 2; i++) {
        ap01_timer_channel_rearm(&s->channel[i]);
    }
    return MEMTX_OK;
}
#endif
