/* SPDX-License-Identifier: GPL-2.0-or-later
 * Explicit synthetic PEC UART2 service. This neither decrypts nor restores
 * the original PEC library. Contracts are bounded by observed 0041 callers:
 * init a002a448, endpoints a002613c/6146, RX a00196e0/971e,
 * TX a00198b8/9774. No RX provider, DMA, fabricated handles or generic success.
 */
#ifndef QEMU_AP01_PEC_H
#define QEMU_AP01_PEC_H

#include "chardev/char-fe.h"
#include "system/address-spaces.h"
#include "qemu/bswap.h"

typedef struct AP01PEC {
    CharBackend chr;
    bool initialized, ready;
    uint32_t synthetic_clock_hz;
    uint32_t rx_object, tx_object;
    uint8_t rx_config[20], tx_config[20];
    uint32_t args[4];
    unsigned args_written;
    uint32_t result;
    uint32_t tx_queue[32];
    unsigned tx_head, tx_count;
    uint64_t accepted_words, drained_bytes;
} AP01PEC;

static bool ap01_pec_ram_range(uint32_t address, size_t length)
{
    static const uint32_t bases[] = {
        0x20fc0000, 0x22fc0000, 0x60fc0000, 0x62fc0000,
        0x88000000, 0xa8000000, 0xd8000000,
    };
    uint64_t end = (uint64_t)address + length;
    if (!length || end > UINT64_C(0x100000000)) {
        return false;
    }
    for (size_t i = 0; i < G_N_ELEMENTS(bases); i++) {
        uint32_t size = i < 4 ? 0x78000 : 0x400000;
        if (address >= bases[i] && end <= (uint64_t)bases[i] + size) {
            return true;
        }
    }
    return false;
}

static bool ap01_pec_read_ram(uint32_t address, void *buffer, size_t length)
{
    return ap01_pec_ram_range(address, length) &&
        address_space_read(&address_space_memory, address,
                           MEMTXATTRS_UNSPECIFIED, buffer, length) == MEMTX_OK;
}

static void ap01_pec_reset(AP01PEC *s)
{
    /* Preserve the initialized frontend and its registration/ownership. */
    s->initialized = s->ready = false;
    s->synthetic_clock_hz = s->rx_object = s->tx_object = 0;
    memset(s->rx_config, 0, sizeof(s->rx_config));
    memset(s->tx_config, 0, sizeof(s->tx_config));
    memset(s->args, 0, sizeof(s->args));
    memset(s->tx_queue, 0, sizeof(s->tx_queue));
    s->args_written = s->result = s->tx_head = s->tx_count = 0;
    s->accepted_words = s->drained_bytes = 0;
}

static void ap01_pec_drain(AP01PEC *s)
{
    /* Nonblocking: only successfully delivered bytes leave the queue. A
     * missing/backpressured backend leaves a bounded observable backlog. */
    while (s->tx_count) {
        uint8_t byte = s->tx_queue[s->tx_head] & 0xff;
        if (qemu_chr_fe_write(&s->chr, &byte, 1) != 1) {
            break;
        }
        s->tx_head = (s->tx_head + 1) % G_N_ELEMENTS(s->tx_queue);
        s->tx_count--;
        s->drained_bytes++;
    }
}

static bool ap01_pec_config(uint32_t object, uint8_t config[20], bool rx)
{
    return !(object & 3) && ap01_pec_read_ram(object, config, 20) &&
        ldl_le_p(config) == 0 && ldl_le_p(config + 4) == 160000000 &&
        ldl_le_p(config + 8) == 115200 && config[12] == 0 &&
        config[13] < 35 && config[14] == 1 && config[15] == 0 &&
        ldl_le_p(config + 16) == (rx ? 20 : 0);
}

static MemTxResult ap01_pec_commit(AP01PEC *s, uint32_t slot)
{
    uint32_t result = 0;
    if (s->args_written != 15) {
        return MEMTX_ERROR;
    }
    switch (slot) {
    case 0x30:
        if (s->args[0] != 1 || s->args[1] || s->args[2]) {
            return MEMTX_ERROR;
        }
        /* This unique synthetic profile supplies the clock in the endpoint
         * configs; it is NOT a recovered physical slot30 implementation. */
        s->initialized = true;
        s->synthetic_clock_hz = 160000000;
        break;
    case 0x88:
        if (!s->initialized) { return MEMTX_ERROR; }
        s->ready = true;
        s->rx_object = s->tx_object = 0;
        memset(s->rx_config, 0, sizeof(s->rx_config));
        memset(s->tx_config, 0, sizeof(s->tx_config));
        memset(s->tx_queue, 0, sizeof(s->tx_queue));
        s->tx_head = s->tx_count = 0;
        break;
    case 0x40: case 0x44: {
        uint8_t config[20];
        bool rx = slot == 0x40;
        if (!s->ready || !ap01_pec_config(s->args[0], config, rx) ||
            (rx ? s->args[0] == s->tx_object : s->args[0] == s->rx_object)) {
            return MEMTX_ERROR;
        }
        if (rx) {
            s->rx_object = s->args[0];
            memcpy(s->rx_config, config, sizeof(config));
        } else {
            if (s->tx_count) { return MEMTX_ERROR; }
            s->tx_object = s->args[0];
            memcpy(s->tx_config, config, sizeof(config));
        }
        /* Never alter config word0 or guess the original handle format. */
        break;
    }
    case 0x48:
        if (!s->ready || !s->rx_object || s->args[0] != s->rx_object) {
            return MEMTX_ERROR;
        }
        result = 0; /* Actual no-input state; no RX provider is installed. */
        break;
    case 0x4c:
        /* Caller reads a complete output word. Never return with invented or
         * uninitialized RX data when no provider can produce that word. */
        return MEMTX_ERROR;
    case 0x50: {
        uint8_t bytes[32 * 4];
        uint32_t count = s->args[2];
        if (!s->ready || !s->tx_object || s->args[0] != s->tx_object ||
            !count || count > G_N_ELEMENTS(s->tx_queue) || (s->args[1] & 3) ||
            count > G_N_ELEMENTS(s->tx_queue) - s->tx_count ||
            !ap01_pec_read_ram(s->args[1], bytes, count * 4)) {
            return MEMTX_ERROR;
        }
        /* Validate the entire transaction before enqueueing any word. */
        for (uint32_t i = 0; i < count; i++) {
            unsigned tail = (s->tx_head + s->tx_count) % G_N_ELEMENTS(s->tx_queue);
            s->tx_queue[tail] = ldl_le_p(bytes + i * 4);
            s->tx_count++;
        }
        s->accepted_words += count;
        result = count;
        ap01_pec_drain(s);
        break;
    }
    case 0xe8:
        if (!s->ready || !s->tx_object || s->args[0] != s->tx_object) {
            return MEMTX_ERROR;
        }
        ap01_pec_drain(s);
        result = s->tx_count;
        break;
    default:
        return MEMTX_ERROR;
    }
    s->result = result;
    s->args_written = 0;
    return MEMTX_OK;
}

/* Mailbox at parent-selected F1000080: args0..3 at00/04/08/0c,
 * write10 commits the literal PEC table byte offset, read14 returns result.
 * A full fresh argument set is required even for zero-argument mock calls. */
static MemTxResult ap01_pec_access(AP01PEC *s, hwaddr offset, uint64_t *value,
                                  unsigned size, bool write)
{
    if (size != 4 || (offset & 3)) { return MEMTX_ERROR; }
    if (offset < 0x10) {
        unsigned index = offset / 4;
        if (write) {
            s->args[index] = *value;
            s->args_written |= 1U << index;
        } else {
            *value = s->args[index];
        }
        return MEMTX_OK;
    }
    if (offset == 0x10 && write) { return ap01_pec_commit(s, *value); }
    if (offset == 0x14 && !write) { *value = s->result; return MEMTX_OK; }
    return MEMTX_ERROR;
}
#endif
