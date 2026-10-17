#ifndef AP01_OTA_CHANNEL_H
#define AP01_OTA_CHANNEL_H
#include <stddef.h>
#include <stdint.h>
#define OTA_CHANNEL_MAX_ROM (7u * 1024u * 1024u)
#define OTA_CHANNEL_VERSION "ap01-ota/1"
/* Standalone protocol component, NOT a bootable/flashing ROM. Caller owns ctx,
 * inactive-slot selection, exclusive transaction/busy control and durable commit.
 * Callbacks must obey timeout_ms; now_ms is monotonic. No callback may block
 * indefinitely. abort releases only an unfinished transaction, never rolls back
 * an already committed/unknown-outcome image. No automatic retry is performed. */
enum ota_store_status { OTA_STORE_OK, OTA_STORE_BUSY, OTA_STORE_ERROR, OTA_STORE_UNKNOWN };
enum ota_stage { OTA_STAGE_HEADER, OTA_STAGE_BEGIN, OTA_STAGE_BODY, OTA_STAGE_CRC,
                 OTA_STAGE_COMMIT, OTA_STAGE_SEND, OTA_STAGE_DONE };
enum ota_error { OTA_OK, OTA_BAD_CONFIG, OTA_TIMEOUT, OTA_EOF, OTA_IO,
                 OTA_BAD_HEADER, OTA_UNAUTHORIZED, OTA_TOO_LARGE, OTA_BAD_CRC,
                 OTA_BUSY, OTA_WRITER, OTA_COMMIT_UNKNOWN };
typedef struct {
    void *ctx;
    int (*read)(void *, void *, size_t, uint32_t timeout_ms);
    int (*write)(void *, const void *, size_t, uint32_t timeout_ms);
    uint64_t (*now_ms)(void *);
    int (*begin)(void *, uint32_t length, uint32_t timeout_ms);
    int (*chunk)(void *, const void *, size_t, uint32_t timeout_ms);
    int (*commit)(void *, uint32_t timeout_ms);
    void (*abort)(void *);
} ota_channel_io;
typedef struct {
    enum ota_stage stage;
    enum ota_error error, response_error;
    uint32_t bytes;
    int store_status;
    int committed;
} ota_channel_result;
/* One HTTP/1.1 request; caller closes connection on return. Absolute deadline
 * applies to headers, body, storage and reply, never renewed by progress.
 * key must be exactly64 hex chars plus NUL; it is never echoed/logged. */
ota_channel_result ota_channel_handle(const ota_channel_io *, const char *key,
                                      uint64_t absolute_deadline_ms);
#endif
