/* SPDX-License-Identifier: GPL-2.0-or-later
 * Bounded MAC INTC with local GEN54 IRQ; no external radio event sources.
 * Fixed SDK intc.c.obj matches original intc_init: base24910000,
 * status low/high +00/+04; UNMASK_SET low/high +10/+14 are W1S writes.
 * No mask-readback contract is established; reads of SET aliases reject.
 * Handler table has64 entries. All64 mask inputs are explicitly supported
 * virtual latches; no physical implemented-source mask is claimed.
 * INDEX+40 has no established empty encoding and rejects empty reads.
 */
#ifndef QEMU_AP01_MAC_H
#define QEMU_AP01_MAC_H

#include "exec/memattrs.h"
#include "hw/irq.h"

typedef struct AP01MAC {
    uint32_t unmask[2], pending[2];
    qemu_irq irq;
    uint32_t dma_arbitration, dma_unmask, platform_zero_writes;
} AP01MAC;

static void ap01_mac_update_irq(AP01MAC *s)
{
    if (s->irq) qemu_set_irq(s->irq, !!((s->pending[0] & s->unmask[0]) |
                                      (s->pending[1] & s->unmask[1])));
}
static void ap01_mac_set_irq(AP01MAC *s, qemu_irq irq)
{
    s->irq = irq;
    ap01_mac_update_irq(s);
}
static void ap01_mac_set_source54(AP01MAC *s, bool level)
{
    if (level) s->pending[1] |= 1u << 22;
    else s->pending[1] &= ~(1u << 22);
    ap01_mac_update_irq(s);
}
static void ap01_mac_reset(AP01MAC *s)
{
    /* Explicit synthetic reset, with no incoming events or radio backend. */
    s->unmask[0] = s->unmask[1] = 0;
    s->pending[0] = s->pending[1] = 0;
    ap01_mac_update_irq(s);
    s->dma_arbitration = s->dma_unmask = s->platform_zero_writes = 0;
}

static void ap01_mac_init(AP01MAC *s)
{
    ap01_mac_reset(s);
}

static MemTxResult ap01_mac_access(AP01MAC *s, hwaddr offset,
                                  uint64_t *value, unsigned size, bool write)
{
    if (size != 4) {
        return MEMTX_ERROR;
    }
    switch (offset) {
    case 0:
    case 4:
        if (write) {
            return MEMTX_ERROR;
        }
        *value = s->pending[offset / 4] & s->unmask[offset / 4];
        return MEMTX_OK;
    case 0x10:
    case 0x14: {
        unsigned bank = (offset - 0x10) / 4;
        if (!write) {
            return MEMTX_ERROR;
        }
        s->unmask[bank] |= (uint32_t)*value;
        ap01_mac_update_irq(s);
        return MEMTX_OK;
    }
    case 0x40:
        if (write || !(s->pending[1] & s->unmask[1] & (1u << 22))) return MEMTX_ERROR;
        *value = 54;
        return MEMTX_OK;
    default:
        /* Includes INDEX+40: no fabricated index for an empty controller. */
        return MEMTX_ERROR;
    }
}

/* SDK dma_init: arbitration+34=0xc, int_unmask_set+18=0x1ffff.
 * Empty virtual DMA has no submitted transfer or pending completion.
 * Only observed configuration requests are supported; no start/ACK aliases. */
static MemTxResult ap01_mac_dma_access(AP01MAC *s, hwaddr offset,
                                      uint64_t *value, unsigned size, bool write)
{
    if (size != 4) return MEMTX_ERROR;
    if (offset == 0x10) {
        /* Original a01605e2 / SDK dma_dma_status_oft_free_getf wait for
         * low16==ffff. No descriptor/start is accepted by this empty
         * profile, so all16 OFT_FREE bits remain set; not packet status. */
        if (write) return MEMTX_ERROR;
        *value = 0xffff;
        return MEMTX_OK;
    }
    if (offset == 0x80 || offset == 0x84 || offset == 0x88 ||
        offset == 0x8c || offset == 0x94) {
        /* Original a015ce04..ce52 / SDK dma_lli_counter_get: no accepted
         * descriptor/start operation exists, therefore completed LLI count0. */
        if (write) return MEMTX_ERROR;
        *value = 0;
        return MEMTX_OK;
    }
    if (!write) return MEMTX_ERROR;
    if (offset == 0x20 && !(*value & ~0x1ffffULL)) {
        /* Original rxl_reset a0161afe and txl_reset a0160624 use ACK W1C.
         * Virtual17-bit source domain matches observed initialization mask.
         * No submitted RX descriptors or incoming DMA IRQ exist, so clear
         * leaves pending empty; it does not manufacture a completion. */
        return MEMTX_OK;
    }
    if (offset == 0x34 && *value == 0xc) {
        s->dma_arbitration = *value;
        return MEMTX_OK;
    }
    if (offset == 0x18 && *value == 0x1ffff) {
        /* Only the full initialization word is known; W1S vs RW has
         * not been established for this DMA register. */
        s->dma_unmask = *value;
        return MEMTX_OK;
    }
    return MEMTX_ERROR;
}
#endif
