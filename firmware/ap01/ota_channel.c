#include "ota_channel.h"
#if __STDC_HOSTED__
#include <stdio.h>
#include <string.h>
#else
int snprintf(char *, size_t, const char *, ...);
size_t strlen(const char *);
int strcmp(const char *, const char *);
char *strchr(const char *, int);
char *strstr(const char *, const char *);
int memcmp(const void *, const void *, size_t);
#endif

static int hex(char c)
{
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}
static int equal(const char *a, const char *b)
{
    while (*a && *b) {
        char x = *a++, y = *b++;
        if (x >= 'A' && x <= 'Z') x += 'a' - 'A';
        if (y >= 'A' && y <= 'Z') y += 'a' - 'A';
        if (x != y) return 0;
    }
    return *a == *b;
}
static uint32_t remaining(const ota_channel_io *io, uint64_t deadline)
{
    uint64_t now = io->now_ms(io->ctx);
    if (now >= deadline) return 0;
    uint64_t left = deadline - now;
    return left > UINT32_MAX ? UINT32_MAX : (uint32_t)left;
}
static uint32_t crc_update(uint32_t crc, const unsigned char *data, size_t n)
{
    while (n--) {
        crc ^= *data++;
        for (unsigned i = 0; i < 8; ++i)
            crc = (crc >> 1) ^ (0xedb88320u & (0u - (crc & 1u)));
    }
    return crc;
}
static const char *stage_name(enum ota_stage stage)
{
    static const char *const names[] = {"header","begin","body","crc","commit","send","done"};
    return names[stage];
}
static const char *error_name(enum ota_error error)
{
    static const char *const names[] = {"ok","bad_config","timeout","eof","io",
        "bad_header","unauthorized","too_large","crc_mismatch","busy","writer","result_unknown"};
    return names[error];
}
static enum ota_error reply(const ota_channel_io *io, uint64_t deadline,
                            const ota_channel_result *r, int health)
{
    char body[160], response[320];
    int status = r->error == OTA_OK ? 200 : r->error == OTA_UNAUTHORIZED ? 401 :
                 r->error == OTA_TOO_LARGE ? 413 : r->error == OTA_BUSY ? 409 :
                 r->error == OTA_BAD_HEADER || r->error == OTA_BAD_CRC ? 400 : 500;
    if (health && r->error == OTA_OK)
        snprintf(body, sizeof(body), "{\"version\":\"%s\"}\n", OTA_CHANNEL_VERSION);
    else
        snprintf(body, sizeof(body), "{\"stage\":\"%s\",\"error\":\"%s\",\"bytes\":%u}\n",
                 stage_name(r->stage), error_name(r->error), r->bytes);
    int count = snprintf(response, sizeof(response),
        "HTTP/1.1 %d %s\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: %u\r\n\r\n%s",
        status, status == 200 ? "OK" : "Error", (unsigned)strlen(body), body);
    if (count < 0 || (size_t)count >= sizeof(response)) return OTA_IO;
    size_t sent = 0;
    while (sent < (size_t)count) {
        uint32_t timeout = remaining(io, deadline);
        if (!timeout) return OTA_TIMEOUT;
        int n = io->write(io->ctx, response + sent, (size_t)count - sent, timeout);
        if (!remaining(io, deadline)) return OTA_TIMEOUT;
        if (n <= 0 || (size_t)n > (size_t)count - sent) return OTA_IO;
        sent += (size_t)n;
    }
    return OTA_OK;
}

/* No dynamic allocation: 1025-byte header +1024-byte body scratch. Header reads
 * deliberately stop at CRLFCRLF, so no body byte is lost to a header prefetch. */
ota_channel_result ota_channel_handle(const ota_channel_io *io, const char *key,
                                      uint64_t deadline)
{
    ota_channel_result r = { OTA_STAGE_HEADER, OTA_OK, OTA_OK, 0, 0, 0 };
    char header[1025];
    unsigned char body[1024];
    size_t used = 0;
    int begun = 0, health = 0;
    uint32_t length = 0, expected_crc = 0;
    if (!io || !key || !io->read || !io->write || !io->now_ms ||
        !io->begin || !io->chunk || !io->commit || !io->abort) {
        r.error = OTA_BAD_CONFIG; return r;
    }
    for (unsigned i = 0; i < 64; ++i) {
        if (!key[i] || hex(key[i]) < 0) { r.error = OTA_BAD_CONFIG; return r; }
    }
    if (key[64]) { r.error = OTA_BAD_CONFIG; return r; }
    while (used < 1024) {
        uint32_t timeout = remaining(io, deadline);
        if (!timeout) { r.error = OTA_TIMEOUT; goto done; }
        int n = io->read(io->ctx, header + used, 1, timeout);
        if (!remaining(io, deadline)) { r.error = OTA_TIMEOUT; goto done; }
        if (n != 1) { r.error = n == 0 ? OTA_EOF : OTA_IO; goto done; }
        unsigned char c = (unsigned char)header[used++];
        if ((c < 32 && c != '\r' && c != '\n') || c > 126) { r.error = OTA_BAD_HEADER; goto done; }
        if (used >= 4 && !memcmp(header + used - 4, "\r\n\r\n", 4)) break;
    }
    if (used < 4 || memcmp(header + used - 4, "\r\n\r\n", 4)) { r.error = OTA_BAD_HEADER; goto done; }
    header[used] = 0;
    char *line_end = strstr(header, "\r\n");
    *line_end = 0;
    health = !strcmp(header, "GET /health HTTP/1.1");
    if (!health && strcmp(header, "POST /firmware HTTP/1.1")) { r.error = OTA_BAD_HEADER; goto done; }
    unsigned have_length = 0, have_auth = 0, have_crc = 0;
    char *line = line_end + 2;
    while (*line) {
        if (!strcmp(line, "\r\n")) break;
        char *end = strstr(line, "\r\n");
        if (!end) { r.error = OTA_BAD_HEADER; goto done; }
        *end = 0;
        if (strchr(line, '\r') || strchr(line, '\n')) { r.error = OTA_BAD_HEADER; goto done; }
        char *colon = strchr(line, ':');
        if (!colon || colon == line) { r.error = OTA_BAD_HEADER; goto done; }
        for (char *p = line; p < colon; ++p)
            if (!((*p >= 'A' && *p <= 'Z') || (*p >= 'a' && *p <= 'z') || (*p >= '0' && *p <= '9') || *p == '-')) { r.error = OTA_BAD_HEADER; goto done; }
        *colon = 0;
        char *value = colon + 1;
        while (*value == ' ') ++value;
        char *tail = value + strlen(value);
        while (tail > value && tail[-1] == ' ') *--tail = 0;
        if (equal(line, "Content-Length")) {
            if (have_length++ || !*value) { r.error = OTA_BAD_HEADER; goto done; }
            for (char *p = value; *p; ++p) {
                if (*p < '0' || *p > '9') { r.error = OTA_BAD_HEADER; goto done; }
                if (length > OTA_CHANNEL_MAX_ROM / 10u ||
                    length * 10u + (unsigned)(*p - '0') > OTA_CHANNEL_MAX_ROM) { r.error = OTA_TOO_LARGE; goto done; }
                length = length * 10u + (unsigned)(*p - '0');
            }
        } else if (equal(line, "Authorization")) {
            if (have_auth++) { r.error = OTA_BAD_HEADER; goto done; }
            if (strlen(value) != 71 || memcmp(value, "Bearer ", 7)) { r.error = OTA_UNAUTHORIZED; goto done; }
            unsigned difference = 0;
            for (unsigned i = 0; i < 64; ++i) difference |= (unsigned char)value[i+7] ^ (unsigned char)key[i];
            if (difference) { r.error = OTA_UNAUTHORIZED; goto done; }
        } else if (equal(line, "X-ROM-CRC32")) {
            if (have_crc++ || strlen(value) != 8) { r.error = OTA_BAD_HEADER; goto done; }
            for (unsigned i = 0; i < 8; ++i) {
                int digit = hex(value[i]);
                if (digit < 0) { r.error = OTA_BAD_HEADER; goto done; }
                expected_crc = (expected_crc << 4) | (unsigned)digit;
            }
        } else if (equal(line, "Transfer-Encoding") ||
                   (equal(line, "Connection") && !equal(value, "close"))) {
            r.error = OTA_BAD_HEADER; goto done;
        }
        line = end + 2;
    }
    if (health) {
        if (length) { r.error = OTA_BAD_HEADER; goto done; }
        r.stage = OTA_STAGE_DONE; goto done;
    }
    if (!have_auth) { r.error = OTA_UNAUTHORIZED; goto done; }
    if (!have_length || !length || !have_crc) { r.error = OTA_BAD_HEADER; goto done; }
    r.stage = OTA_STAGE_BEGIN;
    uint32_t timeout = remaining(io, deadline);
    if (!timeout) { r.error = OTA_TIMEOUT; goto done; }
    begun = 1;
    r.store_status = io->begin(io->ctx, length, timeout);
    if (!remaining(io, deadline)) { r.error = OTA_TIMEOUT; goto done; }
    if (r.store_status) { r.error = r.store_status == OTA_STORE_BUSY ? OTA_BUSY : OTA_WRITER; goto done; }
    r.stage = OTA_STAGE_BODY;
    uint32_t crc = UINT32_MAX;
    while (r.bytes < length) {
        size_t wanted = length - r.bytes;
        if (wanted > sizeof(body)) wanted = sizeof(body);
        timeout = remaining(io, deadline);
        if (!timeout) { r.error = OTA_TIMEOUT; goto done; }
        int n = io->read(io->ctx, body, wanted, timeout);
        if (!remaining(io, deadline)) { r.error = OTA_TIMEOUT; goto done; }
        if (n <= 0 || (size_t)n > wanted) { r.error = n == 0 ? OTA_EOF : OTA_IO; goto done; }
        timeout = remaining(io, deadline);
        if (!timeout) { r.error = OTA_TIMEOUT; goto done; }
        /* Writer adapters may place an inactive-slot marker in this scratch. */
        uint32_t next_crc = crc_update(crc, body, (size_t)n);
        r.store_status = io->chunk(io->ctx, body, (size_t)n, timeout);
        if (!remaining(io, deadline)) { r.error = OTA_TIMEOUT; goto done; }
        if (r.store_status) { r.error = r.store_status == OTA_STORE_BUSY ? OTA_BUSY : OTA_WRITER; goto done; }
        crc = next_crc;
        r.bytes += (uint32_t)n;
    }
    r.stage = OTA_STAGE_CRC;
    if ((crc ^ UINT32_MAX) != expected_crc) { r.error = OTA_BAD_CRC; goto done; }
    r.stage = OTA_STAGE_COMMIT;
    timeout = remaining(io, deadline);
    if (!timeout) { r.error = OTA_TIMEOUT; goto done; }
    r.store_status = io->commit(io->ctx, timeout);
    if (r.store_status == OTA_STORE_OK) { r.committed = 1; begun = 0; }
    if (r.store_status) { r.error = r.store_status == OTA_STORE_UNKNOWN ? OTA_COMMIT_UNKNOWN : r.store_status == OTA_STORE_BUSY ? OTA_BUSY : OTA_WRITER; goto done; }
    if (!remaining(io, deadline)) { r.error = OTA_TIMEOUT; goto done; }
    r.stage = OTA_STAGE_DONE;
done:
    if (begun) io->abort(io->ctx);
    r.response_error = reply(io, deadline, &r, health);
    if (r.error == OTA_OK && r.response_error != OTA_OK) { r.stage = OTA_STAGE_SEND; r.error = r.response_error; }
    return r;
}
