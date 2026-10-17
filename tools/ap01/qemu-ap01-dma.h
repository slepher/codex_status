/* SPDX-License-Identifier: GPL-2.0-or-later
 * Bounded synchronous BL616 DMA0 model. Fixed SDK 41bf338a57be53919d99f6c6715a96100428c97d:
 * drivers/lhal/include/hardware/dma_reg.h and src/bflb_dma.c.
 * Eight register banks, 1024 LLIs and 4MiB per activation maximum.
 * Error can follow partial transfers; ERR never means atomic rollback.
 * No bus timing, DMA arbitration, software requests or peripheral flow control.
 */
#ifndef QEMU_AP01_DMA_H
#define QEMU_AP01_DMA_H
#include "system/address-spaces.h"
#include "qemu/bswap.h"
#include "hw/irq.h"

typedef struct AP01DMAChannel {
    uint32_t src, dst, lli, control, config;
} AP01DMAChannel;
typedef struct AP01DMA {
    AP01SPI *spi;
    AP01UART *uart[2];
    qemu_irq irq;
    uint32_t config, sync, raw_tc, raw_error;
    AP01DMAChannel channel[8];
} AP01DMA;

static uint32_t ap01_dma_masked(AP01DMA *s, bool error)
{
    uint32_t result = 0;
    for (unsigned i = 0; i < 8; i++) {
        if (s->channel[i].config & (error ? 1u << 14 : 1u << 15)) {
            result |= (error ? s->raw_error : s->raw_tc) & (1u << i);
        }
    }
    return result;
}
static void ap01_dma_irq(AP01DMA *s)
{
    if (s->irq) {
        qemu_set_irq(s->irq, !!(ap01_dma_masked(s, false) |
                              ap01_dma_masked(s, true)));
    }
}
static void ap01_dma_reset(AP01DMA *s)
{
    s->config = s->sync = s->raw_tc = s->raw_error = 0;
    memset(s->channel, 0, sizeof(s->channel));
    ap01_dma_irq(s);
}

static bool ap01_dma_range(uint32_t addr, uint32_t length, bool write)
{
    static const uint32_t ram[] = {0x20fc0000, 0x22fc0000, 0x60fc0000, 0x62fc0000};
    static const uint32_t psram[] = {0x88000000, 0xa8000000, 0xd8000000};
    uint64_t end = (uint64_t)addr + length;
    if (!length || end > UINT64_C(0x100000000)) return false;
    for (unsigned i = 0; i < G_N_ELEMENTS(ram); i++) {
        if (addr >= ram[i] && end <= (uint64_t)ram[i] + 0x78000) return true;
    }
    for (unsigned i = 0; i < G_N_ELEMENTS(psram); i++) {
        if (addr >= psram[i] && end <= (uint64_t)psram[i] + 0x400000) return true;
    }
    if (addr >= 0x20010000 && end <= 0x20011000) return true;
    return !write && addr >= 0x9ffff000 && end <= UINT64_C(0xa0fff000);
}

static MemTxResult ap01_dma_memory(uint32_t addr, uint8_t *bytes,
                                  unsigned size, bool write)
{
    if (!ap01_dma_range(addr, size, write)) return MEMTX_ERROR;
    if (write) {
        return address_space_write(&address_space_memory, addr,
                                   MEMTXATTRS_UNSPECIFIED, bytes, size);
    }
    return address_space_read(&address_space_memory, addr,
                              MEMTXATTRS_UNSPECIFIED, bytes, size);
}

static MemTxResult ap01_dma_fail(AP01DMA *s, unsigned index)
{
    s->channel[index].config &= ~1u;
    s->raw_error |= 1u << index;
    ap01_dma_irq(s);
    return MEMTX_ERROR;
}

static MemTxResult ap01_dma_run(AP01DMA *s, unsigned index)
{
    AP01DMAChannel *c = &s->channel[index];
    uint32_t seen[1024], total = 0;
    unsigned seen_count = 0, blocks = 0;
    if (!(s->config & 1) || !(c->config & 1)) return MEMTX_OK;
    if (c->config & 0x10000) return ap01_dma_fail(s, index); /* lock unsupported */
    /* SDK DMA_H bit18: stop accepting transfers while keeping E/registers.
     * Synchronous model has no in-flight beat (A remains0). Clearing H resumes
     * the saved channel; setting H never fabricates TC/ERR or drops a waiter. */
    if (c->config & 0x40000) return MEMTX_OK;
    unsigned flow = (c->config >> 11) & 7;
    unsigned source_req = (c->config >> 1) & 31;
    unsigned dest_req = (c->config >> 6) & 31;
    bool spi = flow == 1 && dest_req == 11;
    bool uart = flow == 2 && (source_req == 0 || source_req == 2);
    if (flow != 0 && !spi && !uart) return ap01_dma_fail(s, index);
    for (;;) {
        if (blocks++ == 1024) return ap01_dma_fail(s, index);
        unsigned sw = (c->control >> 18) & 3;
        unsigned dw = (c->control >> 21) & 3;
        unsigned count = c->control & 0xfff;
        bool si = c->control & (1u << 26), di = c->control & (1u << 27);
        if (sw == 3 || dw == 3 || !count || (c->control & 0x03824000)) {
            return ap01_dma_fail(s, index); /* unsupported fix/add/min modes */
        }
        sw = 1u << sw;
        dw = 1u << dw;
        unsigned bytes = count * sw;
        if (bytes % dw || bytes > 0x400000 - total ||
            (c->src & (sw - 1)) || (c->dst & (dw - 1))) {
            return ap01_dma_fail(s, index);
        }
        total += bytes;
        if (uart) {
            unsigned id = source_req / 2;
            if (!s->uart[id] || !(s->uart[id]->rx_config & 1) ||
                !(s->uart[id]->dma_config & 2) || si ||
                c->src != 0x2000a08c + id * 0x100 ||
                !ap01_dma_range(c->dst, di ? bytes : dw, true)) {
                return ap01_dma_fail(s, index);
            }
            /* No incoming bytes: retain enabled channel, no memory read/IRQ. */
            return MEMTX_OK;
        }
        if (!ap01_dma_range(c->src, si ? bytes : sw, false) ||
            (!spi && !ap01_dma_range(c->dst, di ? bytes : dw, true))) {
            return ap01_dma_fail(s, index);
        }
        unsigned frame = spi && s->spi ? ((s->spi->config >> 2) & 3) + 1 : 1;
        if (spi && (!s->spi || c->dst != 0x2000a288 || di ||
                    !(s->spi->fifo_config & 1) || frame == 3 || dw != frame)) {
            return ap01_dma_fail(s, index);
        }
        uint8_t buffer[8];
        unsigned used = 0;
        uint32_t src = c->src, dst = c->dst;
        for (unsigned remaining = bytes; remaining; remaining -= sw) {
            if (ap01_dma_memory(src, buffer + used, sw, false) != MEMTX_OK) {
                return ap01_dma_fail(s, index);
            }
            used += sw;
            if (si) src += sw;
            while (used >= dw) {
                if (spi) {
                    uint32_t value = 0;
                    for (unsigned k = 0; k < frame; k++) {
                        value |= (uint32_t)buffer[k] << (8 * k);
                    }
                    if (ap01_spi_dma_write(s->spi, value, frame) != MEMTX_OK) {
                        return ap01_dma_fail(s, index);
                    }
                } else if (ap01_dma_memory(dst, buffer, dw, true) != MEMTX_OK) {
                    return ap01_dma_fail(s, index);
                }
                if (di) dst += dw;
                used -= dw;
                memmove(buffer, buffer + dw, used);
            }
        }
        c->src = src;
        c->dst = dst;
        if (c->control & (1u << 31)) s->raw_tc |= 1u << index;
        c->control &= ~0xfffu;
        if (!c->lli) {
            c->config &= ~1u;
            ap01_dma_irq(s);
            return MEMTX_OK;
        }
        if ((c->lli & 3) || seen_count == G_N_ELEMENTS(seen)) {
            return ap01_dma_fail(s, index);
        }
        for (unsigned i = 0; i < seen_count; i++) {
            if (seen[i] == c->lli) return ap01_dma_fail(s, index);
        }
        seen[seen_count++] = c->lli;
        uint8_t descriptor[16];
        if (ap01_dma_memory(c->lli, descriptor, 16, false) != MEMTX_OK) {
            return ap01_dma_fail(s, index);
        }
        c->src = ldl_le_p(descriptor);
        c->dst = ldl_le_p(descriptor + 4);
        c->lli = ldl_le_p(descriptor + 8);
        c->control = ldl_le_p(descriptor + 12);
    }
}

static MemTxResult ap01_dma_access(AP01DMA *s, hwaddr offset, uint64_t *value,
                                  unsigned size, bool write)
{
    uint32_t v = *value;
    if (size != 4 || (offset & 3)) return MEMTX_ERROR;
    if (offset < 0x100) {
        if (write) {
            switch (offset) {
            case 0x08: s->raw_tc &= ~(v & 255); break;
            case 0x10: s->raw_error &= ~(v & 255); break;
            case 0x30:
                if (v & ~1u) return MEMTX_ERROR;
                s->config = v;
                for (unsigned i = 0; i < 8; i++) {
                    if (ap01_dma_run(s, i) != MEMTX_OK) return MEMTX_ERROR;
                }
                break;
            case 0x34: s->sync = v; break; /* request synchronization latch */
            default: return MEMTX_ERROR;
            }
        } else {
            uint32_t enabled = 0;
            for (unsigned i = 0; i < 8; i++) enabled |= (s->channel[i].config & 1) << i;
            switch (offset) {
            case 0x00: *value = ap01_dma_masked(s, false) | ap01_dma_masked(s, true); break;
            case 0x04: *value = ap01_dma_masked(s, false); break;
            case 0x08: case 0x10: *value = 0; break;
            case 0x0c: *value = ap01_dma_masked(s, true); break;
            case 0x14: *value = s->raw_tc; break;
            case 0x18: *value = s->raw_error; break;
            case 0x1c: *value = enabled; break;
            case 0x30: *value = s->config; break;
            case 0x34: *value = s->sync; break;
            default: return MEMTX_ERROR;
            }
        }
    } else if (offset < 0x900) {
        unsigned index = (offset - 0x100) / 0x100;
        AP01DMAChannel *c = &s->channel[index];
        uint32_t *reg;
        switch (offset & 255) {
        case 0x00: reg = &c->src; break;
        case 0x04: reg = &c->dst; break;
        case 0x08: reg = &c->lli; break;
        case 0x0c: reg = &c->control; break;
        case 0x10: reg = &c->config; break;
        default: return MEMTX_ERROR;
        }
        if (write) {
            if ((offset & 255) == 0x10 && (v & ~0x5ffffu)) return MEMTX_ERROR;
            if (reg != &c->config && (c->config & 1)) return MEMTX_ERROR;
            *reg = v;
            if (reg == &c->config && ap01_dma_run(s, index) != MEMTX_OK) return MEMTX_ERROR;
        } else {
            *value = *reg;
        }
    } else {
        return MEMTX_ERROR;
    }
    ap01_dma_irq(s);
    return MEMTX_OK;
}
#endif
