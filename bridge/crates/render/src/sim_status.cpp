#include "status_pages.h"
#include "network_display_state.h"
#include <cstring>
#include <new>
extern "C" {
void *sim_status_create() { return new (std::nothrow) StatusPages; }
void sim_status_delete(void *p) { delete static_cast<StatusPages *>(p); }
int sim_status_page(void *p, const char *body, const char *token, const char *id,
                    size_t offset, uint64_t now, char *out, size_t capacity) {
    auto *pages = static_cast<StatusPages *>(p); String reply;
    if (body) { String snapshot(body); reply = pages->begin(snapshot, token, id, now); }
    else reply = pages->read(token, id, offset, now);
    if (reply.length() >= capacity) return -1;
    memcpy(out, reply.c_str(), reply.length()); out[reply.length()] = 0; return reply.length();
}
int sim_network_display_test() {
    NetworkDisplayState glyph; glyph.observe(false); if (!glyph.due(0)) return 1;
    glyph.rendered(true,0); if (glyph.due(0)) return 2;
    glyph.observe(true); if (!glyph.due(0)) return 3;
    glyph.rendered(false,0); if (glyph.due(4999) || !glyph.due(5000)) return 4;
    glyph.rendered(false,5000); glyph.rendered(false,10000); if (glyph.due(20000)) return 5;
    glyph.observe(false); if (!glyph.due(20000)) return 6;
    glyph.rendered(true,20000); glyph.observe(false); if (glyph.due(20001)) return 7;
    return 0;
}
}
