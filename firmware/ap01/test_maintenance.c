#include "maintenance.h"
#ifdef AM_RV32_SELFTEST
/* Freestanding test runtime only. No vendor functions or addresses are used. */
volatile uint32_t am_test_failure;
static void test_fail(uint32_t line) { am_test_failure = line; __builtin_trap(); }
#define assert(expression) do { if (!(expression)) test_fail(__LINE__); } while (0)
#define printf(...) ((void)0)
static void *memset(void *target, int value, unsigned long n)
{
    volatile uint8_t *b = target;
    while (n--) *b++ = (uint8_t)value;
    return target;
}
static void *memcpy(void *target, const void *source, unsigned long n)
{
    volatile uint8_t *a = target;
    const uint8_t *b = source;
    while (n--) *a++ = *b++;
    return target;
}
static int memcmp(const void *left, const void *right, unsigned long n)
{
    const uint8_t *a = left, *b = right;
    while (n--) { if (*a != *b) return (int)*a - *b; ++a; ++b; }
    return 0;
}
static const char *strstr(const char *text, const char *part)
{
    for (; *text; ++text) {
        unsigned long n = 0;
        while (part[n] && text[n] && part[n] == text[n]) ++n;
        if (!part[n]) return text;
    }
    return 0;
}
#else
#include <assert.h>
#include <stdio.h>
#include <string.h>
#endif

typedef struct {
    uint8_t staged[16], journal[AP01_MAINT_RECORD_BYTES];
    uint32_t journal_bytes, saves, save_fail_at, writes, stops, installs, queries;
    int invalid_auth, prepare_fail, write_fail, verify_fail, native_busy, stop_fail;
    int install_result, query_result, exclusion;
    uint64_t now, stop_elapsed;
} fixture;

static const uint8_t mac[6] = {1, 2, 3, 4, 5, 6};
static const uint8_t token[16] = {7}; /* Synthetic credential, never a device token. */
static const uint8_t payload[] = "123456789";

static int auth(void *context, const uint8_t *principal, uint32_t n, const uint8_t *secret)
{
    fixture *f = context;
    uint32_t i;
    if (f->invalid_auth || n != 64 || memcmp(secret, token, 16)) return -1;
    for (i = 0; i < n; ++i) if (principal[i] != 'a') return -1;
    return 0;
}
static uint64_t now_ms(void *context) { return ((fixture *)context)->now; }
static int save_record(void *context, const uint8_t *b, uint32_t n)
{
    fixture *f = context;
    if (++f->saves == f->save_fail_at) return -1;
    assert(n <= sizeof(f->journal));
    memcpy(f->journal, b, n);
    f->journal_bytes = n;
    return 0;
}
static int prepare(void *context, const am_request *r)
{
    fixture *f = context;
    assert(r->total_bytes == 9);
    memset(f->staged, 0, sizeof(f->staged));
    return f->prepare_fail ? -1 : 0;
}
static int write_chunk(void *context, uint32_t offset, const uint8_t *b, uint32_t n)
{
    fixture *f = context;
    ++f->writes;
    if (f->write_fail) return -1;
    assert(offset + n <= sizeof(f->staged));
    memcpy(f->staged + offset, b, n);
    return 0;
}
static int verify(void *context, const am_request *r)
{
    fixture *f = context;
    assert(r->total_bytes == 9);
    return f->verify_fail || memcmp(f->staged, payload, 9) ? -1 : 0;
}
static int take(void *context, const am_request *r)
{
    fixture *f = context;
    assert(r->principal_bytes == 64 && r->total_bytes == 9);
    if (f->native_busy || f->exclusion) return -1;
    f->exclusion = 1;
    return 0;
}
static void release(void *context)
{
    fixture *f = context;
    assert(f->exclusion);
    f->exclusion = 0;
}
static int stop(void *context)
{
    fixture *f = context;
    assert(f->exclusion);
    ++f->stops;
    f->now += f->stop_elapsed;
    return f->stop_fail ? -1 : 0;
}
static int install(void *context, const am_request *r)
{
    fixture *f = context;
    assert(f->exclusion && f->stops && r->total_bytes == 9);
    ++f->installs;
    return f->install_result;
}
static int query(void *context, const am_request *r)
{
    fixture *f = context;
    assert(f->exclusion && r->id[0] == 1);
    ++f->queries;
    return f->query_result;
}
static void setup(fixture *f, am_state *s, am_request *r, am_ports *p)
{
    memset(f, 0, sizeof(*f));
    memset(r, 0, sizeof(*r));
    am_init(s, mac);
    memcpy(r->mac, mac, 6);
    r->id[0] = 1;
    memset(r->principal, 'a', 64);
    r->principal_bytes = 64;
    r->kind = 1;
    r->total_bytes = 9;
    r->expected_crc = 0xcbf43926u;
    *p = (am_ports){f, auth, now_ms, save_record, prepare, write_chunk, verify,
                    take, release, stop, install, query};
}
static void ready(fixture *f, am_state *s, am_request *r, am_ports *p)
{
    setup(f, s, r, p);
    assert(am_begin(s, p, r, token, 0, 1000, 9) == AM_OK);
    assert(am_receive(s, p, r, token, 0, payload, 4, 0) == AM_OK);
    assert(am_receive(s, p, r, token, 4, payload + 4, 5, 0) == AM_OK);
    assert(s->phase == AM_READY && !f->stops && !f->installs);
}

int main(void)
{
    fixture f;
    am_state s, restored, snapshot;
    am_request r, changed;
    am_ports p;
    uint8_t record[AP01_MAINT_RECORD_BYTES], wrong_token[16] = {0};
    uint32_t bytes, i;

    ready(&f, &s, &r, &p);
    assert(am_commit(&s, &p, &r, token, 1) == AM_OK);
    assert(s.phase == AM_SUCCEEDED && f.installs == 1 && !f.exclusion);
    assert(am_commit(&s, &p, &r, token, 2) == AM_OK && f.installs == 1);
    assert(strstr(am_message(&s), "confirmed"));

    /* Reject a full-principal mismatch, not a truncated prefix comparison. */
    snapshot = s;
    changed = r; changed.principal[63] = 'b';
    assert(am_query(&s, &p, &changed, token) == AM_AUTH);
    changed = r; changed.mac[5] ^= 1;
    assert(am_query(&s, &p, &changed, token) == AM_IDENTITY);
    assert(am_query(&s, &p, &r, wrong_token) == AM_AUTH);
    assert(!memcmp(&snapshot, &s, sizeof(s)));
    changed = r; changed.expected_crc ^= 1;
    assert(am_begin(&s, &p, &changed, token, 3, 1000, 9) == AM_CONFLICT);
    changed = r; changed.principal_bytes = 65;
    assert(am_begin(&s, &p, &changed, token, 3, 1000, 9) == AM_BOUNDS);

    /* Revisions prevent replaying an old accepted request after a later one. */
    changed = r; changed.id[0] = 2; changed.revision = 1;
    assert(am_begin(&s, &p, &changed, token, 3, 1000, 9) == AM_OK);
    assert(am_begin(&s, &p, &r, token, 4, 1000, 9) == AM_BUSY);
    assert(am_receive(&s, &p, &changed, token, 0, payload, 9, 4) == AM_OK);
    assert(am_commit(&s, &p, &changed, token, 4) == AM_OK);
    assert(am_begin(&s, &p, &r, token, 4, 1000, 9) == AM_CONFLICT);

    setup(&f, &s, &r, &p);
    p.take = 0;
    assert(am_begin(&s, &p, &r, token, 0, 1000, 9) == AM_PORT_UNAVAILABLE && !f.saves);
    setup(&f, &s, &r, &p);
    assert(am_begin(&s, &p, &r, token, 0, 1000, 8) == AM_BOUNDS);
    assert(am_begin(&s, &p, &r, token, UINT64_MAX, 1, 9) == AM_BOUNDS);
    assert(am_begin(&s, &p, &r, token, 0, 1000, 9) == AM_OK);
    assert(am_begin(&s, &p, &r, token, 999, 99999, 9) == AM_OK);
    assert(s.deadline_ms == 1000 && f.saves == 1);
    assert(am_receive(&s, &p, &r, token, 1, payload, 3, 0) == AM_BOUNDS && !f.writes);
    assert(am_receive(&s, &p, &r, token, 0, payload, UINT32_MAX, 0) == AM_BOUNDS);
    assert(am_receive(&s, &p, &r, token, 0, payload, 4, 0) == AM_OK);
    assert(am_receive(&s, &p, &r, token, 0, payload, 4, 0) == AM_BOUNDS && f.writes == 1);
    assert(am_receive(&s, &p, &r, token, 4, payload + 4, 5, 1000) == AM_TIMEOUT);
    assert(!f.stops && !f.installs);

    ready(&f, &s, &r, &p);
    f.native_busy = 1;
    assert(am_commit(&s, &p, &r, token, 0) == AM_BUSY && !f.stops && !f.installs);
    f.native_busy = 0; f.stop_fail = 1;
    assert(am_commit(&s, &p, &r, token, 0) == AM_STOP && !f.installs && !f.exclusion);
    ready(&f, &s, &r, &p);
    f.stop_elapsed = 1000;
    assert(am_commit(&s, &p, &r, token, 0) == AM_TIMEOUT && !f.installs && !f.exclusion);

    /* Unknown commit keeps native exclusion; retry only queries the same job. */
    ready(&f, &s, &r, &p);
    f.install_result = 1;
    assert(am_commit(&s, &p, &r, token, 0) == AM_RESULT_UNKNOWN && f.exclusion);
    assert(am_commit(&s, &p, &r, token, 1) == AM_RESULT_UNKNOWN && f.installs == 1);
    f.query_result = 1;
    assert(am_query(&s, &p, &r, token) == AM_RESULT_UNKNOWN && f.installs == 1);
    bytes = am_encode(&s, record);
    assert(bytes == 136 && am_restore(&restored, record, bytes, mac) == AM_OK);
    assert(restored.phase == AM_UNKNOWN && !restored.locked);
    f.exclusion = 0; /* Simulated reboot; real adapter must restore native fencing. */
    f.query_result = 0;
    assert(am_query(&restored, &p, &r, token) == AM_OK && !f.exclusion && f.installs == 1);

    /* A failed checkpoint after commit must never turn into a second install. */
    ready(&f, &s, &r, &p);
    f.save_fail_at = f.saves + 3;
    assert(am_commit(&s, &p, &r, token, 0) == AM_STORAGE);
    assert(s.phase == AM_UNKNOWN && f.exclusion && f.installs == 1);
    assert(am_commit(&s, &p, &r, token, 1) == AM_RESULT_UNKNOWN && f.installs == 1);
    assert(am_query(&s, &p, &r, token) == AM_OK && !f.exclusion);

    for (i = 0; i < 5; ++i) {
        setup(&f, &s, &r, &p);
        if (i == 0) f.save_fail_at = 1;
        if (i == 1) f.prepare_fail = 1;
        if (i == 2) f.write_fail = 1;
        if (i == 3) f.verify_fail = 1;
        if (i == 4) r.expected_crc ^= 1;
        if (am_begin(&s, &p, &r, token, 0, 1000, 9) == AM_OK)
            assert(am_receive(&s, &p, &r, token, 0, payload, 9, 0) != AM_OK);
        assert(!f.installs && !f.stops);
    }

    /* Every truncation and every flipped byte is rejected without changing RAM. */
    ready(&f, &s, &r, &p);
    bytes = am_encode(&s, record);
    snapshot = s;
    for (i = 0; i < bytes; ++i) {
        assert(am_restore(&s, record, i, mac) == AM_STORAGE);
        record[i] ^= 0x80;
        assert(am_restore(&s, record, bytes, mac) == AM_STORAGE);
        record[i] ^= 0x80;
        assert(!memcmp(&snapshot, &s, sizeof(s)));
    }
    assert(am_restore(&restored, record, bytes, mac) == AM_OK && restored.phase == AM_RECOVERY);
    changed = r; changed.id[0] = 2; changed.revision = 1;
    assert(am_begin(&restored, &p, &changed, token, 0, 1000, 9) == AM_OK);
    assert(restored.request.revision == 2 && restored.phase == AM_RECEIVING);
    for (i = 0; i < 5; ++i) {
        restored = s;
        if (i == 0) restored.phase = (am_phase)999;
        if (i == 1) restored.received = 10;
        if (i == 2) restored.request.kind = 3;
        if (i == 3) restored.request.revision = 0;
        if (i == 4) restored.request.principal_bytes = 0;
        bytes = am_encode(&restored, record); /* Valid CRC, invalid semantic fields. */
        assert(am_restore(&restored, record, bytes, mac) == AM_STORAGE);
    }
    for (i = 1; i <= 64; ++i) {
        s.request.principal_bytes = (uint8_t)i;
        bytes = am_encode(&s, record);
        assert(bytes == 72 + i && am_restore(&restored, record, bytes, mac) == AM_OK);
    }
    am_init(&s, mac);
    bytes = am_encode(&s, record);
    assert(bytes == 72 && am_restore(&restored, record, bytes, mac) == AM_OK);
    printf("PASS maintenance: auth/identity/replay/stream/deadline/exclusion/unknown/recovery/corruption\n");
    printf("state_bytes=%zu request_bytes=%zu record_empty=72 record_principal32=104 record_max=136\n",
           sizeof(am_state), sizeof(am_request));
    return 0;
}
