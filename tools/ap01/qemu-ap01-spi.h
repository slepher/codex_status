/* SPDX-License-Identifier: GPL-2.0-or-later
 * Restricted BL616 SPI0 + JD9853 functional panel, not a timing model.
 * SDK 41bf338a drivers/lhal/include/hardware/spi_reg.h defines the fields;
 * bflb_spi.c defines BYTE_INV=MSB-byte-first and BIT_INV=LSB-bit-first.
 * Panel commands/opaque vendor payloads come from original0041 table
 * 0xa01c15b8..0xa01c1738. No SIM image or invented application graphics.
 */
#ifndef QEMU_AP01_SPI_H
#define QEMU_AP01_SPI_H

#include "ui/console.h"
#include "qemu/log.h"
#include "exec/memattrs.h"

typedef enum AP01SPILine {
    AP01_SPI_CS, AP01_SPI_DC, AP01_SPI_RESET,
} AP01SPILine;
typedef bool (*AP01SPILineRead)(void *opaque, AP01SPILine line);

typedef struct AP01SPI {
    QemuConsole *console;
    AP01SPILineRead line_read;
    void *line_opaque;
    bool landscape_clockwise;
    uint32_t config, int_config, period0, period1, ignore, timeout;
    uint32_t fifo_config, fifo_threshold;
    uint32_t pixels[240 * 320];
    uint8_t command, parameters[32], parameter_count, parameter_expected;
    uint8_t madctl, pixel_format, pixel_high;
    bool pixel_half, memory_write, sleep_out, display_on, te_enabled;
    bool reset_asserted, dirty;
    bool end_pending;
    uint8_t rx_fifo[32];
    unsigned rx_head, rx_count;
    bool rx_overflow, rx_underflow;
    uint16_t x0, x1, y0, y1, x, y;
    uint64_t tx_bytes, pixel_writes, command_count;
} AP01SPI;

static void ap01_spi_panel_reset(AP01SPI *s)
{
    memset(s->pixels, 0, sizeof(s->pixels));
    s->command = s->parameter_count = s->parameter_expected = 0;
    s->madctl = s->pixel_format = s->pixel_high = 0;
    s->pixel_half = s->memory_write = false;
    s->sleep_out = s->display_on = s->te_enabled = false;
    s->x0 = s->y0 = s->x = s->y = 0;
    s->x1 = 239;
    s->y1 = 319;
    s->dirty = true;
}

static void ap01_spi_reset(AP01SPI *s)
{
    /* Explicit functional initial latches; register header does not provide
     * silicon reset values. Empty TX FIFO is drained synchronously below. */
    s->config = s->int_config = s->period0 = s->period1 = 0;
    s->ignore = s->timeout = s->fifo_config = s->fifo_threshold = 0;
    s->reset_asserted = false;
    s->end_pending = false;
    memset(s->rx_fifo, 0, sizeof(s->rx_fifo));
    s->rx_head = s->rx_count = 0;
    s->rx_overflow = s->rx_underflow = false;
    s->tx_bytes = s->pixel_writes = s->command_count = 0;
    ap01_spi_panel_reset(s);
}

static void ap01_spi_gfx_update(void *opaque)
{
    AP01SPI *s = opaque;
    DisplaySurface *surface = qemu_console_surface(s->console);
    int width = s->landscape_clockwise ? 320 : 240;
    int height = s->landscape_clockwise ? 240 : 320;
    pixman_image_t *source;
    uint32_t *output;
    if (!s->dirty) {
        return;
    }
    output = g_new0(uint32_t, width * height);
    if (s->display_on && s->sleep_out && !s->reset_asserted) {
        for (unsigned y = 0; y < 320; y++) {
            for (unsigned x = 0; x < 240; x++) {
                unsigned target = s->landscape_clockwise ?
                    x * 320 + (319 - y) : y * 240 + x;
                output[target] = s->pixels[y * 240 + x];
            }
        }
    }
    source = pixman_image_create_bits(PIXMAN_x8r8g8b8, width, height,
                                     output, width * 4);
    pixman_image_composite32(PIXMAN_OP_SRC, source, NULL, surface->image,
                            0, 0, 0, 0, 0, 0, width, height);
    pixman_image_unref(source);
    g_free(output);
    s->dirty = false;
    dpy_gfx_update(s->console, 0, 0, width, height);
}

static void ap01_spi_invalidate(void *opaque)
{
    ((AP01SPI *)opaque)->dirty = true;
}

static const GraphicHwOps ap01_spi_graphic_ops = {
    .invalidate = ap01_spi_invalidate,
    .gfx_update = ap01_spi_gfx_update,
};

static void ap01_spi_init(AP01SPI *s, AP01SPILineRead read_line, void *opaque,
                          bool landscape_clockwise)
{
    memset(s, 0, sizeof(*s));
    s->line_read = read_line;
    s->line_opaque = opaque;
    /* Optional output-only rotation is an explicit model assumption. Native
     * GRAM is always 240x320; MADCTL independently transforms real writes. */
    s->landscape_clockwise = landscape_clockwise;
    s->console = graphic_console_init(NULL, 0, &ap01_spi_graphic_ops, s);
    qemu_console_resize(s->console, landscape_clockwise ? 320 : 240,
                         landscape_clockwise ? 240 : 320);
    ap01_spi_reset(s);
}

static void ap01_spi_lines_changed(AP01SPI *s)
{
    bool asserted = s->line_read &&
                    !s->line_read(s->line_opaque, AP01_SPI_RESET);
    if (asserted && !s->reset_asserted) { ap01_spi_panel_reset(s); }
    if (asserted != s->reset_asserted) { s->dirty = true; }
    s->reset_asserted = asserted;
}

static bool ap01_spi_need_te(AP01SPI *s)
{
    ap01_spi_lines_changed(s);
    return s->te_enabled && s->sleep_out && s->display_on && !s->reset_asserted;
}

static int ap01_spi_command_length(uint8_t command)
{
    switch (command) {
    case 0x01: case 0x10: case 0x11: case 0x20:
    case 0x28: case 0x29: case 0x2c: case 0x34: return 0;
    case 0x35: case 0x36: case 0x3a: case 0xde: case 0xb2:
    case 0xe6: case 0xbe: return 1;
    case 0xdf: case 0xc0: case 0xc2: case 0xd7: return 2;
    /* Vendor DE banks reuse command numbers with differing lengths. Treat
     * those opaque payloads as bounded data, never pixels or ready status. */
    case 0xb7: case 0xbb: case 0xc1: case 0xc3: case 0xc4:
    case 0xc8: case 0xd0: return 32;
    case 0x2a: case 0x2b: return 4;
    default: return -1;
    }
}

static MemTxResult ap01_spi_panel_byte(AP01SPI *s, bool data, uint8_t byte)
{
    if (!data) {
        int length = ap01_spi_command_length(byte);
        bool incomplete = s->parameter_count < s->parameter_expected &&
            (s->command == 0x2a || s->command == 0x2b || s->command == 0x35 ||
             s->command == 0x36 || s->command == 0x3a);
        if (length < 0 || s->pixel_half || incomplete) {
            return MEMTX_ERROR;
        }
        s->command = byte;
        s->parameter_count = 0;
        s->parameter_expected = length;
        s->memory_write = byte == 0x2c;
        s->command_count++;
        switch (byte) {
        case 0x01: ap01_spi_panel_reset(s); break;
        case 0x10: s->sleep_out = false; s->dirty = true; break;
        case 0x11: s->sleep_out = true; s->dirty = true; break;
        case 0x28: s->display_on = false; s->dirty = true; break;
        case 0x29: s->display_on = true; s->dirty = true; break;
        case 0x34: s->te_enabled = false; break;
        case 0x2c:
            if (s->pixel_format != 5 && s->pixel_format != 0x55) {
                return MEMTX_ERROR;
            }
            s->x = s->x0; s->y = s->y0;
            break;
        }
        return MEMTX_OK;
    }
    if (s->memory_write) {
        uint16_t rgb;
        unsigned x, y, red, green, blue;
        if (!s->pixel_half) {
            s->pixel_high = byte;
            s->pixel_half = true;
            return MEMTX_OK;
        }
        rgb = (s->pixel_high << 8) | byte;
        s->pixel_half = false;
        x = (s->madctl & 0x20) ? s->y : s->x;
        y = (s->madctl & 0x20) ? s->x : s->y;
        if (x >= 240 || y >= 320) {
            return MEMTX_ERROR;
        }
        if (s->madctl & 0x40) { x = 239 - x; }
        if (s->madctl & 0x80) { y = 319 - y; }
        red = ((rgb >> 11) & 31) * 255 / 31;
        green = ((rgb >> 5) & 63) * 255 / 63;
        blue = (rgb & 31) * 255 / 31;
        if (s->madctl & 8) { unsigned swap = red; red = blue; blue = swap; }
        s->pixels[y * 240 + x] = (red << 16) | (green << 8) | blue;
        s->pixel_writes++;
        s->dirty = true;
        if (++s->x > s->x1) {
            s->x = s->x0;
            if (++s->y > s->y1) { s->y = s->y0; }
        }
        return MEMTX_OK;
    }
    if (s->parameter_count >= s->parameter_expected ||
        s->parameter_count >= sizeof(s->parameters)) {
        return MEMTX_ERROR;
    }
    s->parameters[s->parameter_count++] = byte;
    if (s->parameter_count == s->parameter_expected) {
        switch (s->command) {
        case 0x2a: case 0x2b: {
            unsigned first = (s->parameters[0] << 8) | s->parameters[1];
            unsigned last = (s->parameters[2] << 8) | s->parameters[3];
            unsigned limit = s->command == 0x2a ?
                ((s->madctl & 0x20) ? 320 : 240) : ((s->madctl & 0x20) ? 240 : 320);
            if (first > last || last >= limit) { return MEMTX_ERROR; }
            if (s->command == 0x2a) { s->x0 = first; s->x1 = last; }
            else { s->y0 = first; s->y1 = last; }
            break;
        }
        case 0x36: s->madctl = byte; break;
        case 0x3a:
            if (byte != 5 && byte != 0x55) { return MEMTX_ERROR; }
            s->pixel_format = byte;
            break;
        case 0x35:
            if (byte != 0) { return MEMTX_ERROR; }
            s->te_enabled = true;
            break;
        }
    }
    return MEMTX_OK;
}

static MemTxResult ap01_spi_dma_write(AP01SPI *s, uint32_t value, unsigned width)
{
    unsigned frame = ((s->config >> 2) & 3) + 1;
    bool dc;
    if (!s->line_read || !(s->config & 1) || (s->config & 2) || frame == 3 ||
        (width != 1 && width != 2 && width != 4) || width < frame) {
        return MEMTX_ERROR;
    }
    ap01_spi_lines_changed(s);
    if (s->reset_asserted) { return MEMTX_ERROR; }
    if (s->line_read(s->line_opaque, AP01_SPI_CS)) { return MEMTX_ERROR; }
    dc = s->line_read(s->line_opaque, AP01_SPI_DC);
    for (unsigned i = 0; i < frame; i++) {
        unsigned shift = (s->config & 0x80) ? (frame - 1 - i) * 8 : i * 8;
        uint8_t byte = value >> shift;
        if (s->config & 0x40) {
            /* Bit reversal changes actual panel data, not parser semantics. */
            byte = ((byte & 0x55) << 1) | ((byte >> 1) & 0x55);
            byte = ((byte & 0x33) << 2) | ((byte >> 2) & 0x33);
            byte = (byte << 4) | (byte >> 4);
        }
        if (ap01_spi_panel_byte(s, dc, byte) != MEMTX_OK) { return MEMTX_ERROR; }
        s->tx_bytes++;
        /* Original 0041 send helper62fc223a waits for RX_CNT[13:8]>=
         * frame bytes then reads RDATA. SPI clocks sample MISO during real
         * writes even though this LCD's command path uses only MOSI.
         * Explicit synthetic unconnected MISO pull-high: no claimed panel
         * ID/register response. Queue only samples from actual TX clocks. */
        if (s->rx_count < G_N_ELEMENTS(s->rx_fifo)) {
            s->rx_fifo[(s->rx_head + s->rx_count) % G_N_ELEMENTS(s->rx_fifo)] = 0xff;
            s->rx_count++;
        } else {
            s->rx_overflow = true;
        }
    }
    s->end_pending = true;
    return MEMTX_OK;
}

static MemTxResult ap01_spi_access(AP01SPI *s, hwaddr offset, uint64_t *value,
                                  unsigned size, bool write)
{
    uint32_t *reg = NULL, mask = 0;
    if (offset == 0x88 && write) {
        return ap01_spi_dma_write(s, *value, size);
    }
    if (offset == 0x8c && !write) {
        unsigned frame = ((s->config >> 2) & 3) + 1;
        if ((size != 1 && size != 2 && size != 4) || size < frame || frame == 3) {
            return MEMTX_ERROR;
        }
        if (s->rx_count < frame) {
            s->rx_underflow = true;
            return MEMTX_ERROR;
        }
        *value = 0;
        for (unsigned i = 0; i < frame; i++) {
            unsigned shift = (s->config & 0x80) ? (frame - 1 - i) * 8 : i * 8;
            *value |= (uint64_t)s->rx_fifo[s->rx_head] << shift;
            s->rx_head = (s->rx_head + 1) % G_N_ELEMENTS(s->rx_fifo);
            s->rx_count--;
        }
        return MEMTX_OK;
    }
    if (size != 4 || (offset & 3)) { return MEMTX_ERROR; }
    switch (offset) {
    case 0x00:
        if (write && (*value & (2 | 0x400 | 0x100))) { return MEMTX_ERROR; }
        reg = &s->config; mask = 0xffff; break;
    case 0x04:
        /* No SPI IRQ generation. Reject enabling unmapped IRQ sources. */
        if (write && (*value & 0x3f000000)) { return MEMTX_ERROR; }
        if (!write) {
            *value = s->int_config | 2 | (s->end_pending ? 1 : 0) |
                     (s->rx_count > ((s->fifo_threshold >> 24) & 31) ? 4 : 0);
            return MEMTX_OK;
        }
        if (*value & (1 << 16)) { s->end_pending = false; }
        s->int_config = *value & 0x3f00;
        return MEMTX_OK;
    case 0x08:
        if (write) { return MEMTX_ERROR; }
        *value = 0; return MEMTX_OK; /* Synchronous functional FIFO drain. */
    case 0x10: reg = &s->period0; mask = UINT32_MAX; break;
    case 0x14: reg = &s->period1; mask = 0xff; break;
    case 0x18: reg = &s->ignore; mask = 0x001f001f; break;
    case 0x1c: reg = &s->timeout; mask = 0xfff; break;
    case 0x80:
        if (write && (*value & 2)) { return MEMTX_ERROR; } /* RX DMA unsupported. */
        if (!write) {
            *value = s->fifo_config | (s->rx_overflow ? 0x40 : 0) |
                     (s->rx_underflow ? 0x80 : 0);
            return MEMTX_OK;
        }
        if (*value & 8) {
            s->rx_head = s->rx_count = 0;
            s->rx_overflow = s->rx_underflow = false;
        }
        reg = &s->fifo_config; mask = 1; break;
    case 0x84:
        if (!write) { *value = s->fifo_threshold | (s->rx_count << 8) | 32; return MEMTX_OK; }
        reg = &s->fifo_threshold; mask = 0x1f1f0000; break;
    default: return MEMTX_ERROR; /* Non-LCD buses and panel read commands. */
    }
    if (write) { *reg = (*reg & ~mask) | ((uint32_t)*value & mask); }
    else { *value = *reg; }
    return MEMTX_OK;
}
#endif
