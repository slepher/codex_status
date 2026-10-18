#pragma once
#include <lvgl/lvgl.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include "ap01_template_display.h"

// UI-thread-only LVGL 9 image sink. One packed active frame, including LVGL's
// sixteen-entry palette. No full RGB565 conversion framebuffer is allocated.
// Commit means UI ownership transfer, not a physical display/transport ACK.
typedef struct {
    lv_image_dsc_t image;
    lv_obj_t *object;
    uint8_t *storage;
} Ap01TemplateImage;

static inline uint8_t *ap01TemplateImageCandidate(void) {
    uint8_t *allocation = (uint8_t *)calloc(1, 64 + 38400);
    if (!allocation) return NULL;
    for (unsigned i = 0; i < 16; ++i) {
        uint16_t c = ap01Palette565(i);
        uint32_t r = (c >> 11) & 31, g = (c >> 5) & 63, b = c & 31;
        uint32_t color = 0xff000000u | (((r << 3) | (r >> 2)) << 16) |
                         (((g << 2) | (g >> 4)) << 8) | (b << 3) | (b >> 2);
        for (unsigned byte = 0; byte < 4; ++byte) allocation[i*4+byte] = (uint8_t)(color >> (8*byte));
    }
    return allocation + 64;
}
static inline void ap01TemplateImageDiscard(uint8_t *pixels) { if (pixels) free(pixels - 64); }
static inline bool ap01TemplateImageCommit(Ap01TemplateImage *image, lv_obj_t *parent,
                                          uint8_t *pixels) {
    // NuttX's indexed-image decoder allocates a 1280-byte ARGB scanline
    // through the image cache allocator. A zero cache budget rejects it.
    if (!image || !parent || !pixels || !lv_image_cache_is_enabled()) return false;
    if (!image->object) image->object = lv_image_create(parent);
    if (!image->object) return false;
    lv_image_cache_drop(&image->image);
    uint8_t *old = image->storage;
    image->storage = pixels - 64;
    image->image.header.magic = LV_IMAGE_HEADER_MAGIC;
    image->image.header.cf = LV_COLOR_FORMAT_I4;
    image->image.header.w = 320; image->image.header.h = 240;
    image->image.header.stride = 160;
    image->image.data_size = 64 + 38400;
    image->image.data = image->storage;
    lv_image_set_src(image->object, &image->image);
    lv_obj_set_pos(image->object, 0, 0);
    lv_obj_remove_flag(image->object, LV_OBJ_FLAG_CLICKABLE | LV_OBJ_FLAG_SCROLLABLE);
    free(old);
    return true;
}
static inline void ap01TemplateImageClose(Ap01TemplateImage *image) {
    if (!image) return;
    if (image->object) { lv_image_cache_drop(&image->image); lv_obj_delete(image->object); }
    free(image->storage);
    image->object = NULL; image->storage = NULL;
}
