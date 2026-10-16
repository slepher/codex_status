#include "maintenance.h"

static void zero(void *p, uint32_t n)
{
    uint8_t *b = p;
    while (n--) *b++ = 0;
}

static void copy(uint8_t *a, const uint8_t *b, uint32_t n)
{
    while (n--) *a++ = *b++;
}

static int equal(const uint8_t *a, const uint8_t *b, uint32_t n)
{
    uint8_t difference = 0;
    while (n--) difference |= *a++ ^ *b++;
    return difference == 0;
}

static uint32_t crc_update(uint32_t crc, const uint8_t *b, uint32_t n)
{
    while (n--) {
        uint32_t bit;
        crc ^= *b++;
        for (bit = 0; bit < 8; ++bit)
            crc = (crc >> 1) ^ (0xedb88320u & (0u - (crc & 1u)));
    }
    return crc;
}

static void put32(uint8_t *b, uint32_t v)
{
    uint32_t i;
    for (i = 0; i < 4; ++i) b[i] = (uint8_t)(v >> (8u * i));
}

static uint32_t get32(const uint8_t *b)
{
    return (uint32_t)b[0] | ((uint32_t)b[1] << 8) |
        ((uint32_t)b[2] << 16) | ((uint32_t)b[3] << 24);
}

void am_init(am_state *s, const uint8_t mac[6])
{
    zero(s, (uint32_t)sizeof(*s));
    copy(s->request.mac, mac, 6);
}

uint32_t am_encode(const am_state *s, uint8_t b[AP01_MAINT_RECORD_BYTES])
{
    uint32_t bytes = 72u + s->request.principal_bytes;
    zero(b, AP01_MAINT_RECORD_BYTES);
    copy(b, (const uint8_t *)"AMT1", 4);
    b[4] = 1;
    b[6] = (uint8_t)bytes;
    put32(b + 8, (uint32_t)s->phase);
    put32(b + 12, (uint32_t)s->error);
    put32(b + 16, s->request.total_bytes);
    put32(b + 20, s->received);
    put32(b + 24, s->crc);
    put32(b + 28, s->request.expected_crc);
    put32(b + 32, (uint32_t)s->deadline_ms);
    put32(b + 36, (uint32_t)(s->deadline_ms >> 32));
    copy(b + 40, s->request.id, 16);
    copy(b + 56, s->request.mac, 6);
    b[62] = s->request.principal_bytes;
    b[63] = s->request.kind;
    copy(b + 64, s->request.principal, s->request.principal_bytes);
    put32(b + bytes - 8u, s->request.revision);
    put32(b + bytes - 4u, ~crc_update(0xffffffffu, b, bytes - 4u));
    return bytes;
}

am_error am_restore(am_state *s, const uint8_t *b, uint32_t n, const uint8_t mac[6])
{
    uint32_t phase, length;
    /* Validate every persisted bound before copy or allocation. Corruption must
     * not initialize an empty store and reopen authorization. */
    if (!b || n < 72u || n > AP01_MAINT_RECORD_BYTES ||
        !equal(b, (const uint8_t *)"AMT1", 4) ||
        b[4] != 1 || b[5] || b[6] != n || b[7] ||
        get32(b + n - 4u) != ~crc_update(0xffffffffu, b, n - 4u)) return AM_STORAGE;
    phase = get32(b + 8);
    length = b[62];
    if (phase > AM_RECOVERY || get32(b + 12) > AM_PORT_UNAVAILABLE ||
        length > AP01_MAINT_PRINCIPAL_MAX || n != 72u + length || !equal(b + 56, mac, 6) ||
        get32(b + 20) > get32(b + 16)) return AM_STORAGE;
    if (phase != AM_IDLE && (!length || !get32(b + 16) ||
        !get32(b + n - 8u) || (b[63] != 1 && b[63] != 2))) return AM_STORAGE;
    am_init(s, mac);
    s->phase = (am_phase)phase;
    s->error = (am_error)get32(b + 12);
    s->request.total_bytes = get32(b + 16);
    s->received = get32(b + 20);
    s->crc = get32(b + 24);
    s->request.expected_crc = get32(b + 28);
    s->deadline_ms = get32(b + 32) | ((uint64_t)get32(b + 36) << 32);
    copy(s->request.id, b + 40, 16);
    copy(s->request.principal, b + 64, length);
    s->request.principal_bytes = (uint8_t)length;
    s->request.kind = b[63];
    s->request.revision = get32(b + n - 8u);
    /* Monotonic deadlines never survive reboot. Do not resume partial writes.
     * A possible commit is queried rather than sent again. */
    if (s->phase == AM_INSTALLING || s->phase == AM_UNKNOWN) {
        s->phase = AM_UNKNOWN;
        s->error = AM_RESULT_UNKNOWN;
    } else if (s->phase == AM_RECEIVING || s->phase == AM_READY || s->phase == AM_STOPPING) {
        s->phase = AM_RECOVERY;
        s->error = AM_STORAGE;
    }
    return AM_OK;
}

static int complete_ports(const am_ports *p)
{
    return p && p->auth && p->now_ms && p->save && p->prepare && p->write && p->verify &&
        p->take && p->release && p->stop && p->install && p->query;
}

static am_error authorize(const am_state *s, const am_ports *p,
                           const am_request *r, const uint8_t *token)
{
    if (!r || !token || !complete_ports(p)) return AM_PORT_UNAVAILABLE;
    if (!r->principal_bytes || r->principal_bytes > AP01_MAINT_PRINCIPAL_MAX)
        return AM_BOUNDS;
    if (!equal(s->request.mac, r->mac, 6)) return AM_IDENTITY;
    return p->auth(p->context, r->principal, r->principal_bytes, token) == 0 ? AM_OK : AM_AUTH;
}

static int same(const am_state *s, const am_request *r)
{
    return equal(s->request.id, r->id, 16) &&
        s->request.principal_bytes == r->principal_bytes &&
        equal(s->request.principal, r->principal, r->principal_bytes) &&
        s->request.kind == r->kind && s->request.total_bytes == r->total_bytes &&
        s->request.expected_crc == r->expected_crc &&
        r->revision == s->request.revision - 1u;
}

static am_error save(am_state *s, const am_ports *p)
{
    uint8_t record[AP01_MAINT_RECORD_BYTES];
    uint32_t bytes = am_encode(s, record);
    if (p->save(p->context, record, bytes) == 0) return AM_OK;
    s->error = AM_STORAGE;
    if (s->phase == AM_INSTALLING || s->phase == AM_UNKNOWN || s->phase == AM_SUCCEEDED) {
        s->phase = AM_UNKNOWN;
    } else s->phase = AM_RECOVERY;
    return AM_STORAGE;
}

static am_error fail(am_state *s, const am_ports *p, am_error e)
{
    s->phase = AM_FAILED;
    s->error = e;
    return save(s, p) == AM_OK ? e : AM_STORAGE;
}

am_error am_begin(am_state *s, const am_ports *p, const am_request *r,
                  const uint8_t token[16], uint64_t now, uint32_t budget, uint32_t free_bytes)
{
    am_error e = authorize(s, p, r, token);
    if (e != AM_OK) return e;
    if (s->phase != AM_IDLE && equal(s->request.id, r->id, 16)) {
        if (!same(s, r)) return AM_CONFLICT;
        if ((s->phase == AM_RECEIVING || s->phase == AM_READY) && now >= s->deadline_ms)
            return fail(s, p, AM_TIMEOUT);
        return s->error;
    }
    if (s->locked || (s->phase != AM_IDLE && s->phase != AM_SUCCEEDED &&
                     s->phase != AM_FAILED && s->phase != AM_RECOVERY))
        return AM_BUSY;
    if (r->revision != s->request.revision || r->revision == UINT32_MAX) return AM_CONFLICT;
    if (!r->total_bytes || r->total_bytes > free_bytes || !budget ||
        now > UINT64_MAX - budget || (r->kind != 1 && r->kind != 2)) return AM_BOUNDS;
    am_init(s, r->mac);
    copy(s->request.id, r->id, 16);
    copy(s->request.principal, r->principal, r->principal_bytes);
    s->request.principal_bytes = r->principal_bytes;
    s->request.kind = r->kind;
    s->request.total_bytes = r->total_bytes;
    s->request.expected_crc = r->expected_crc;
    s->request.revision = r->revision + 1u;
    s->deadline_ms = now + budget;
    s->crc = 0xffffffffu;
    s->phase = AM_RECEIVING;
    if (save(s, p) != AM_OK) return AM_STORAGE;
    if (p->prepare(p->context, &s->request) != 0) return fail(s, p, AM_STORAGE);
    if (p->now_ms(p->context) >= s->deadline_ms) return fail(s, p, AM_TIMEOUT);
    return AM_OK;
}

am_error am_receive(am_state *s, const am_ports *p, const am_request *r,
                    const uint8_t token[16], uint32_t offset, const uint8_t *b,
                    uint32_t bytes, uint64_t now)
{
    am_error e = authorize(s, p, r, token);
    if (e != AM_OK) return e;
    if (!same(s, r)) return AM_CONFLICT;
    if (s->phase != AM_RECEIVING) return AM_BUSY;
    if (now >= s->deadline_ms) return fail(s, p, AM_TIMEOUT);
    if (!b || !bytes || bytes > AP01_MAINT_CHUNK_MAX || offset != s->received ||
        bytes > s->request.total_bytes - s->received) return AM_BOUNDS;
    if (p->write(p->context, offset, b, bytes) != 0) return fail(s, p, AM_STORAGE);
    s->crc = crc_update(s->crc, b, bytes);
    s->received += bytes;
    if (p->now_ms(p->context) >= s->deadline_ms) return fail(s, p, AM_TIMEOUT);
    if (s->received == s->request.total_bytes) {
        if (~s->crc != s->request.expected_crc) return fail(s, p, AM_INTEGRITY);
        if (p->verify(p->context, &s->request) != 0) return fail(s, p, AM_INTEGRITY);
        if (p->now_ms(p->context) >= s->deadline_ms) return fail(s, p, AM_TIMEOUT);
        s->phase = AM_READY;
    }
    return save(s, p);
}

static am_error result(am_state *s, const am_ports *p, int status)
{
    if (status == 0) { s->phase = AM_SUCCEEDED; s->error = AM_OK; }
    else if (status == -1) { s->phase = AM_FAILED; s->error = AM_INSTALL; }
    else { s->phase = AM_UNKNOWN; s->error = AM_RESULT_UNKNOWN; }
    if (save(s, p) != AM_OK) return AM_STORAGE;
    if (s->phase != AM_UNKNOWN && s->locked) {
        p->release(p->context);
        s->locked = 0;
    }
    return s->error;
}

am_error am_commit(am_state *s, const am_ports *p, const am_request *r,
                   const uint8_t token[16], uint64_t now)
{
    am_error e = authorize(s, p, r, token);
    if (e != AM_OK) return e;
    if (!same(s, r)) return AM_CONFLICT;
    if (s->phase == AM_UNKNOWN || s->phase == AM_INSTALLING) return AM_RESULT_UNKNOWN;
    if (s->phase == AM_SUCCEEDED || s->phase == AM_FAILED) return s->error;
    if (s->phase != AM_READY) return AM_BUSY;
    if (now >= s->deadline_ms) return fail(s, p, AM_TIMEOUT);
    if (p->take(p->context, &s->request) != 0) return AM_BUSY;
    s->locked = 1;
    s->phase = AM_STOPPING;
    if (save(s, p) != AM_OK) { p->release(p->context); s->locked = 0; return AM_STORAGE; }
    if (p->stop(p->context) != 0) {
        e = fail(s, p, AM_STOP);
        p->release(p->context);
        s->locked = 0;
        return e;
    }
    if (p->now_ms(p->context) >= s->deadline_ms) {
        e = fail(s, p, AM_TIMEOUT);
        p->release(p->context);
        s->locked = 0;
        return e;
    }
    s->phase = AM_INSTALLING;
    if (save(s, p) != AM_OK) {
        /* No install call yet; retain the saved STOPPING record for recovery. */
        s->phase = AM_RECOVERY;
        p->release(p->context);
        s->locked = 0;
        return AM_STORAGE;
    }
    if (p->now_ms(p->context) >= s->deadline_ms) {
        e = fail(s, p, AM_TIMEOUT);
        p->release(p->context);
        s->locked = 0;
        return e;
    }
    return result(s, p, p->install(p->context, &s->request));
}

am_error am_query(am_state *s, const am_ports *p, const am_request *r, const uint8_t token[16])
{
    am_error e = authorize(s, p, r, token);
    if (e != AM_OK) return e;
    if (!same(s, r)) return AM_CONFLICT;
    if ((s->phase == AM_RECEIVING || s->phase == AM_READY) && p->now_ms(p->context) >= s->deadline_ms)
        return fail(s, p, AM_TIMEOUT);
    if (s->phase != AM_UNKNOWN) return s->error;
    if (!s->locked) {
        if (p->take(p->context, &s->request) != 0) return AM_BUSY;
        s->locked = 1;
    }
    return result(s, p, p->query(p->context, &s->request));
}

const char *am_message(const am_state *s)
{
    switch (s->phase) {
    case AM_IDLE: return "AP01 maintenance: ready";
    case AM_RECEIVING: return "Receiving update; business may continue";
    case AM_READY: return "Package verified; ready to stop business";
    case AM_STOPPING: return "Stopping business; do not power off";
    case AM_INSTALLING: return "Installing; do not power off";
    case AM_UNKNOWN: return "Result unknown; query original update";
    case AM_SUCCEEDED: return "Update confirmed";
    case AM_FAILED: return "Update failed; authenticated retry required";
    default: return "Recovery required; authenticated repair available";
    }
}
