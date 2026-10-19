#ifndef AP01_OTA_IMAGE_H
#define AP01_OTA_IMAGE_H

#include <stddef.h>
#include <stdint.h>

typedef struct ota_image {
    uint32_t length;
    uint32_t position;
    uint32_t recovery_crc;
    unsigned char header[256];
    unsigned char tail[40];
} ota_image;

int ota_image_begin(ota_image *, uint32_t length);
int ota_image_chunk(ota_image *, const void *, size_t bytes);
int ota_image_valid(const ota_image *);

#endif
