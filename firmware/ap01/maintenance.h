#ifndef AP01_MAINTENANCE_H
#define AP01_MAINTENANCE_H

#include <stdint.h>

#define AP01_MAINT_RECORD_BYTES 136u
#define AP01_MAINT_PRINCIPAL_MAX 64u
#define AP01_MAINT_CHUNK_MAX 1024u

typedef enum {
    AM_IDLE, AM_RECEIVING, AM_READY, AM_STOPPING, AM_INSTALLING,
    AM_UNKNOWN, AM_SUCCEEDED, AM_FAILED, AM_RECOVERY
} am_phase;
typedef enum {
    AM_OK, AM_AUTH, AM_IDENTITY, AM_CONFLICT, AM_BUSY, AM_BOUNDS,
    AM_TIMEOUT, AM_STORAGE, AM_INTEGRITY, AM_STOP, AM_INSTALL, AM_RESULT_UNKNOWN,
    AM_PORT_UNAVAILABLE
} am_error;

typedef struct {
    uint8_t mac[6], id[16], principal[AP01_MAINT_PRINCIPAL_MAX];
    uint8_t principal_bytes, kind; /* 1=Bundle, 2=App; never full ROM. */
    uint32_t revision, total_bytes, expected_crc;
} am_request;

typedef struct {
    am_request request;
    uint32_t received, crc;
    uint64_t deadline_ms;
    am_phase phase;
    am_error error;
    uint8_t locked;
} am_state;

/* All callbacks run only in the maintenance executor, with bounded timeouts.
 * auth maps the entire principal + credential to an existing durable binding
 * and checks the current owner/session policy without a business-task RPC.
 * save atomically commits + reads back the explicit AMT1 record below.
 * take/release MUST exclude the vendor OTA too; a private mutex is insufficient.
 * take rechecks this transaction's owner before allowing installation.
 * verify checks the actual staged package, including type/ABI and readback.
 * stop confirms the business code no longer executes before install may run.
 * install/query return 0=confirmed success, 1=unknown, -1=confirmed failure.
 * Unknown keeps exclusion held until query resolves the ORIGINAL transaction.
 * No callback may invoke a business-task queue or wait for the UI to respond. */
typedef struct {
    void *context;
    int (*auth)(void *, const uint8_t *, uint32_t, const uint8_t *);
    uint64_t (*now_ms)(void *);
    int (*save)(void *, const uint8_t *, uint32_t);
    int (*prepare)(void *, const am_request *);
    int (*write)(void *, uint32_t, const uint8_t *, uint32_t);
    int (*verify)(void *, const am_request *);
    int (*take)(void *, const am_request *);
    void (*release)(void *);
    int (*stop)(void *);
    int (*install)(void *, const am_request *);
    int (*query)(void *, const am_request *);
} am_ports;

void am_init(am_state *, const uint8_t mac[6]);
am_error am_begin(am_state *, const am_ports *, const am_request *,
                  const uint8_t token[16], uint64_t now_ms,
                  uint32_t budget_ms, uint32_t staging_free_bytes);
am_error am_receive(am_state *, const am_ports *, const am_request *,
                    const uint8_t token[16], uint32_t offset,
                    const uint8_t *, uint32_t bytes, uint64_t now_ms);
am_error am_commit(am_state *, const am_ports *, const am_request *,
                   const uint8_t token[16], uint64_t now_ms);
am_error am_query(am_state *, const am_ports *, const am_request *,
                  const uint8_t token[16]);
uint32_t am_encode(const am_state *, uint8_t record[AP01_MAINT_RECORD_BYTES]);
am_error am_restore(am_state *, const uint8_t *, uint32_t, const uint8_t mac[6]);
const char *am_message(const am_state *);

#endif
