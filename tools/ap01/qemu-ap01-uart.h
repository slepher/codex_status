/* SPDX-License-Identifier: GPL-2.0-or-later
 * Bounded BL616 UART model. Register contracts: fixed Bouffalo SDK
 * 41bf338a57be53919d99f6c6715a96100428c97d, drivers/lhal/{include/
 * hardware/uart_reg.h,src/bflb_uart.c}. Synthetic cold reset, byte-oriented
 * host transport, synchronous nonblocking drain; no baud/wire timing claim.
 * RX/external charger communication, DMA, CTS, LIN, IR and autobaud unsupported.
 */
#ifndef AP01_QEMU_UART_H
#define AP01_QEMU_UART_H
#include "chardev/char-fe.h"
#include "hw/irq.h"
#include "exec/memattrs.h"

typedef struct AP01UART {
    CharBackend chr;
    qemu_irq irq;
    uint32_t tx_config, rx_config, bit_period, data_config;
    uint32_t rto, sw_mode, mask, enable, threshold, pending;
    uint32_t fifo_errors;
    uint32_t dma_config;
    uint8_t tx[32];
    unsigned head, count;
} AP01UART;

static uint32_t ap01_uart_status(AP01UART *u)
{
    unsigned free = 32 - u->count;
    unsigned threshold = (u->threshold >> 16) & 31;
    return u->pending | ((free > threshold) ? 4u : 0u) |
           ((u->fifo_errors & 0x30) ? 0x40u : 0u);
}

static void ap01_uart_irq(AP01UART *u)
{
    if (u->irq) {
        qemu_set_irq(u->irq, !!(ap01_uart_status(u) & u->enable & ~u->mask));
    }
}

static void ap01_uart_drain(AP01UART *u)
{
    if ((u->tx_config & 1) && qemu_chr_fe_backend_connected(&u->chr)) {
        while (u->count) {
            int sent = qemu_chr_fe_write(&u->chr, &u->tx[u->head], 1);
            if (sent != 1) {
                break;
            }
            u->head = (u->head + 1) % 32;
            u->count--;
            if (!u->count) {
                u->pending |= 1; /* TX END only after actual backend acceptance. */
            }
        }
    }
    ap01_uart_irq(u);
}

static void ap01_uart_reset(AP01UART *u)
{
    /* Preserve initialized backend and IRQ handles. Explicit synthetic reset. */
    u->tx_config = u->rx_config = u->bit_period = u->data_config = 0;
    u->rto = u->sw_mode = u->enable = u->threshold = u->pending = 0;
    u->mask = 0xfff;
    u->fifo_errors = u->head = u->count = 0;
    u->dma_config = 0;
    memset(u->tx, 0, sizeof(u->tx));
    ap01_uart_irq(u);
}

static MemTxResult ap01_uart_access(AP01UART *u, hwaddr off,
                                   uint64_t *value, unsigned size, bool write)
{
    uint32_t v = *value;
    bool fifo = off == 0x88 || off == 0x8c;
    if ((size != 4 && !(fifo && size == 1)) || (off & 3)) {
        return MEMTX_ERROR;
    }
    ap01_uart_drain(u);
    if (!write) {
        switch (off) {
        case 0x00: *value = u->tx_config; break;
        case 0x04: *value = u->rx_config; break;
        case 0x08: *value = u->bit_period; break;
        case 0x0c: *value = u->data_config; break;
        case 0x18: *value = u->rto; break;
        case 0x1c: *value = u->sw_mode; break;
        case 0x20: *value = ap01_uart_status(u); break;
        case 0x24: *value = u->mask; break;
        case 0x28: *value = 0; break; /* W1C strobes do not latch. */
        case 0x2c: *value = u->enable; break;
        case 0x30: *value = u->count && (u->tx_config & 1) ? 1 : 0; break;
        case 0x80: *value = u->fifo_errors | u->dma_config; break;
        case 0x84: *value = u->threshold | (32 - u->count); break;
        case 0x8c:
            /* No receive provider registered; never manufacture a received byte. */
            return MEMTX_ERROR;
        default: return MEMTX_ERROR;
        }
        return MEMTX_OK;
    }
    switch (off) {
    case 0x00:
        if (v & (2u | 8u | 0xc0u)) return MEMTX_ERROR;
        u->tx_config = v;
        break;
    case 0x04:
        if (v & (2u | 8u | 0xc0u)) return MEMTX_ERROR;
        u->rx_config = v;
        break;
    case 0x08: u->bit_period = v; break;
    case 0x0c:
        if (v & ~1u) return MEMTX_ERROR;
        u->data_config = v;
        break;
    case 0x18:
        if (v & ~0xffu) return MEMTX_ERROR;
        u->rto = v;
        break;
    case 0x1c:
        if (v & ~0xcu) return MEMTX_ERROR; /* TX software override unsupported. */
        u->sw_mode = v;
        break;
    case 0x24: u->mask = v & 0xfff; break;
    case 0x28:
        if (v & ~0xf33u) return MEMTX_ERROR;
        u->pending &= ~v;
        break;
    case 0x2c: u->enable = v & 0xfff; break;
    case 0x80:
        if (v & 1u) return MEMTX_ERROR; /* TX DMA endpoint unsupported. */
        /* RX gate may be configured, but empty RX never requests a DMA beat. */
        u->dma_config = v & 2;
        if (v & 4u) {
            u->head = u->count = 0;
            u->fifo_errors &= ~0x30u;
        }
        if (v & 8u) u->fifo_errors &= ~0xc0u;
        break;
    case 0x84:
        u->threshold = v & 0x1f1f0000;
        break;
    case 0x88:
        if (!(u->tx_config & 1) || u->count == 32) return MEMTX_ERROR;
        u->tx[(u->head + u->count) % 32] = v & 0xff;
        u->count++;
        u->pending &= ~1u;
        break;
    default: return MEMTX_ERROR;
    }
    ap01_uart_drain(u);
    return MEMTX_OK;
}
#endif
