#include "ota_image.h"

extern void *memcpy(void *, const void *, size_t);
extern void *memset(void *, int, size_t);
extern int memcmp(const void *, const void *, size_t);

/* 0041 immutable image; 0031's model field is at a different offset. */
#define OTA_IMAGE_MODEL_OFFSET 0x4f00u
#define OTA_IMAGE_MODEL_BYTES 17u
#define OTA_IMAGE_TAIL_BYTES 40u
#define OTA_IMAGE_MIN (OTA_IMAGE_MODEL_OFFSET + OTA_IMAGE_MODEL_BYTES + OTA_IMAGE_TAIL_BYTES)
#define OTA_IMAGE_MAX 0x700000u

static uint32_t le32(const unsigned char *p)
{
    return (uint32_t)p[0] | ((uint32_t)p[1] << 8) |
           ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

static uint32_t be32(const unsigned char *p)
{
    return ((uint32_t)p[0] << 24) | ((uint32_t)p[1] << 16) |
           ((uint32_t)p[2] << 8) | (uint32_t)p[3];
}

static void put_range(unsigned char *dst, uint32_t base, uint32_t length,
                      const unsigned char *src, uint32_t position,
                      uint32_t bytes)
{
    uint32_t end = position + bytes;
    uint32_t limit = base + length;
    uint32_t start;

    if (position >= limit || end <= base)
        return;
    start = position > base ? position : base;
    end = end < limit ? end : limit;
    memcpy(dst + start - base, src + start - position, end - start);
}

static uint32_t crc32_update(uint32_t value, const unsigned char *p, size_t n)
{
    while (n--) {
        unsigned bit;
        value ^= *p++;
        for (bit = 0; bit != 8; ++bit)
            value = (value >> 1) ^ (0xedb88320u & (0u - (value & 1u)));
    }
    return value;
}

int ota_image_begin(ota_image *image, uint32_t length)
{
    if (!image || length < OTA_IMAGE_MIN || length > OTA_IMAGE_MAX)
        return -1;
    memset(image, 0, sizeof(*image));
    image->length = length;
    return 0;
}

int ota_image_chunk(ota_image *image, const void *buffer, size_t bytes)
{
    const unsigned char *p = (const unsigned char *)buffer;
    uint32_t position;
    uint32_t count;
    uint32_t crc_end;

    if (!image || image->length < OTA_IMAGE_MIN || image->length > OTA_IMAGE_MAX ||
        image->position > image->length ||
        bytes > (size_t)(image->length - image->position) ||
        (bytes && !buffer))
        return -1;
    position = image->position;
    count = (uint32_t)bytes;
    put_range(image->header, 0, sizeof(image->header), p, position, count);
    put_range(image->tail, image->length - OTA_IMAGE_TAIL_BYTES,
              sizeof(image->tail), p, position, count);
    put_range(image->model, OTA_IMAGE_MODEL_OFFSET, sizeof(image->model),
              p, position, count);

    crc_end = image->length - 4u;
    if (position < crc_end) {
        uint32_t end = position + count;
        if (end > crc_end)
            end = crc_end;
        image->recovery_crc = crc32_update(image->recovery_crc,
                                           p, end - position);
    }
    image->position += count;
    return 0;
}

int ota_image_valid(const ota_image *image,
                    const unsigned char reference_header[256])
{
    static const unsigned char tag[] = "0x5245434f56455259544147";
    uint32_t code_length;
    uint32_t header_crc;

    if (!image || !reference_header || image->position != image->length)
        return 0;
    if (memcmp(image->header, "BFNP", 4) ||
        memcmp(image->header + 8, "FCFG", 4) ||
        memcmp(image->header + 0x64, "PCFG", 4))
        return 0;
    if (memcmp(image->header + 8, reference_header + 8, 0x70) ||
        le32(image->header + 0x78) != le32(reference_header + 0x78))
        return 0;
    if (le32(image->header + 0x7c) != 0x1000u)
        return 0;
    code_length = le32(image->header + 0x84);
    if (!code_length || code_length > image->length - 0x1000u)
        return 0;
    if (le32(image->header + 0xb0) != 0xa0000000u)
        return 0;
    header_crc = crc32_update(0xffffffffu, image->header, 252) ^ 0xffffffffu;
    if (header_crc != le32(image->header + 252))
        return 0;
    if (memcmp(image->model, "njcuk.enstor.ap01", OTA_IMAGE_MODEL_BYTES))
        return 0;
    if (memcmp(image->tail, tag, sizeof(tag)-1) ||
        be32(image->tail + 32) != image->length ||
        le32(image->tail + 36) != image->recovery_crc)
        return 0;
    return 1;
}
