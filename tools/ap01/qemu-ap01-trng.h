/* SPDX-License-Identifier: GPL-2.0-or-later
 * Deterministic simulation fixture, NOT physical or cryptographic entropy.
 * Fixed SDK hardware/sec_eng_reg.h: CTRL bits0..4,8..11; DOUT0..7.
 * bflb_sec_trng.c and original0041 a005208a poll BUSY and read each word
 * repeatedly. IRQ/manual/test modes are deliberately unsupported.
 */
#ifndef QEMU_AP01_TRNG_H
#define QEMU_AP01_TRNG_H
#include "qemu/timer.h"
#include "exec/memattrs.h"

typedef struct AP01TRNG {
    uint32_t words[8], state, generation;
    int64_t complete_ns;
    bool enabled, trigger, busy, interrupt, valid;
} AP01TRNG;

static void ap01_trng_reset(AP01TRNG *s)
{
    memset(s, 0, sizeof(*s));
    s->state = 0x41503031; /* Public, fixed fixture seed. Never a secret. */
}
static void ap01_trng_init(AP01TRNG *s)
{
    ap01_trng_reset(s);
}
static void ap01_trng_update(AP01TRNG *s)
{
    if (!s->busy || qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) < s->complete_ns) {
        return;
    }
    for (unsigned i = 0; i < 8; i++) {
        uint32_t x = s->state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        s->words[i] = s->state = x;
    }
    s->generation++;
    s->busy = false;
    s->interrupt = s->valid = true;
}
static MemTxResult ap01_trng_access(AP01TRNG *s, hwaddr offset,
                                    uint64_t *value, unsigned size, bool write)
{
    if (size != 4 || (offset & 3) ||
        (offset != 0 && (offset < 8 || offset > 0x24)) ||
        (write && offset != 0)) {
        return MEMTX_ERROR;
    }
    if (write) {
        /* Readback RO bits may be present in native RMW. INT_MASK must
         * remain one: this fixture has no interrupt output. */
        if ((*value & ~UINT64_C(0xb1f)) || !(*value & 0x800)) {
            return MEMTX_ERROR;
        }
        bool enable = *value & 4, trigger = *value & 2;
        bool start = trigger && !s->trigger;
        if (start && (!enable || s->busy || (*value & 8))) {
            return MEMTX_ERROR;
        }
        if (s->busy && enable && (trigger != s->trigger || (*value & 8))) {
            return MEMTX_ERROR;
        }
        ap01_trng_update(s);
        if (*value & 0x200) s->interrupt = false;
        if (!enable || (*value & 8)) {
            memset(s->words, 0, sizeof(s->words));
            s->valid = false;
        }
        if (!enable) {
            s->busy = false;
            s->complete_ns = 0;
        }
        s->enabled = enable;
        s->trigger = enable && trigger;
        if (start) {
            s->valid = s->interrupt = false;
            s->busy = true;
            s->complete_ns = qemu_clock_get_ns(QEMU_CLOCK_VIRTUAL) + 2000;
        }
        return MEMTX_OK;
    }
    ap01_trng_update(s);
    if (!offset) {
        *value = 0x800 | (s->enabled ? 4 : 0) | (s->trigger ? 2 : 0) |
                 (s->busy ? 1 : 0) | (s->interrupt ? 0x100 : 0);
    } else {
        *value = s->words[(offset - 8) / 4];
    }
    return MEMTX_OK;
}
#endif
