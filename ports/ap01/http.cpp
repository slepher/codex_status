#include "http.h"
#include <new>

namespace {
bool same(const char *a, const char *b) {
    for (; *a && *b; ++a, ++b) {
        char c = *a >= 'A' && *a <= 'Z' ? *a + 32 : *a;
        if (c != *b) return false;
    }
    return !*a && !*b;
}
int digit(char c) {
    return c >= '0' && c <= '9' ? c-'0' : c >= 'a' && c <= 'f' ? c-'a'+10 :
        c >= 'A' && c <= 'F' ? c-'A'+10 : -1;
}
bool unescape(char *value) {
    char *out = value;
    for (char *p = value; *p; ++p) {
        unsigned c = (unsigned char)*p;
        if (c == '%') {
            if (!p[1] || !p[2] || digit(p[1]) < 0 || digit(p[2]) < 0) return false;
            c = digit(p[1])*16 + digit(p[2]); p += 2;
        } else if (c == '+') c = ' ';
        if (c <= 32 || c >= 127 || c == '/' || c == '\\' || c == '?' || c == '&' || c == '#') return false;
        *out++ = (char)c;
    }
    *out = 0; return true;
}
bool query(char *value, Ap01HttpRequest &request) {
    bool id = false, nonce = false, offset = false;
    while (value && *value) {
        char *next = strchr(value, '&'); if (next) *next++ = 0;
        char *equal = strchr(value, '='); if (!equal) return false; *equal++ = 0;
        if (!unescape(equal)) return false;
        if (!strcmp(value, "request_id") && !id && strlen(equal) <= 64) { request.requestId = equal; id = true; }
        else if (!strcmp(value, "session_nonce") && !nonce && strlen(equal) <= 32) { request.nonce = equal; nonce = true; }
        else if (!strcmp(value, "offset") && !offset && strlen(equal) <= 10) { request.offset = equal; offset = true; }
        else return false;
        value = next;
    }
    return true;
}
bool sendAll(const Ap01Stream &io, const uint8_t *bytes, size_t length, uint64_t deadline) {
    while (length) {
        int n = io.write(io.context, bytes, length, deadline);
        if (n <= 0 || (size_t)n > length) return false;
        bytes += n; length -= n;
    }
    return true;
}
bool reply(const Ap01Stream &io, int status, bool frame, const char *body, size_t bytes, uint64_t deadline) {
    char headers[256];
    int n = snprintf(headers, sizeof(headers), "HTTP/1.1 %d Response\r\nContent-Type: %s\r\nContent-Length: %u\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        status, frame ? "application/vnd.codex-status.frame" : "application/json", unsigned(bytes));
    return n > 0 && size_t(n) < sizeof(headers) && sendAll(io, (const uint8_t *)headers, n, deadline) &&
        sendAll(io, (const uint8_t *)body, bytes, deadline);
}
bool fail(const Ap01Stream &io, int status, const char *code, uint64_t deadline) {
    char body[192]; int n = snprintf(body, sizeof(body),
        "{\"result\":\"rejected\",\"error\":\"%s\",\"stage\":\"http_parse\",\"action\":\"correct_request\"}", code);
    return n > 0 && size_t(n) < sizeof(body) && reply(io, status, false, body, n, deadline);
}
}
bool ap01ServeHttp(Ap01Application &app, bool maintenance, const Ap01Stream &io,
                   uint64_t now, uint64_t deadline) {
    if (!io.read || !io.write || deadline <= now) return false;
    // The wire admits 2048 header bytes and at most one 4096-byte body.
    std::unique_ptr<uint8_t[]> storage(new (std::nothrow) uint8_t[6145]);
    if (!storage) return fail(io, 503, "oom", deadline);
    char *header = (char *)storage.get(); size_t used = 0, headerBytes = 0;
    while (!headerBytes) {
        if (used >= 2048) return fail(io, 431, "headers_limit", deadline);
        int n = io.read(io.context, storage.get()+used, 2048-used, deadline);
        if (n <= 0 || size_t(n) > 2048-used) return false;
        used += n; header[used] = 0;
        char *end = strstr(header, "\r\n\r\n"); if (end) headerBytes = end-header+4;
        if (memchr(header, 0, headerBytes ? headerBytes : used)) return fail(io, 400, "header_nul", deadline);
    }
    header[headerBytes-2] = 0;
    char *line = strstr(header, "\r\n"); if (!line) return fail(io, 400, "request_line", deadline);
    *line = 0; line += 2;
    char *path = strchr(header, ' '); if (!path) return fail(io, 400, "request_line", deadline); *path++ = 0;
    char *version = strchr(path, ' '); if (!version) return fail(io, 400, "request_line", deadline); *version++ = 0;
    if (strcmp(version, "HTTP/1.1") || path[0] != '/' || strlen(path) > 512 || strchr(path, '#') || strchr(path, '\\'))
        return fail(io, 400, "request_target", deadline);
    Ap01HttpRequest request{}; request.method = header; request.path = path;
    request.token = ""; request.deviceToken = ""; request.contentType = "";
    request.body = storage.get()+headerBytes;
    char *parameters = strchr(path, '?'); if (parameters) { *parameters++ = 0; if (!query(parameters, request)) return fail(io, 400, "query", deadline); }
    bool lengthSeen = false, authSeen = false, typeSeen = false, tokenSeen = false;
    const size_t bodyLimit = maintenance && !strcmp(request.path, "/api/firmware/chunk") ? 32768 : 4096;
    size_t length = 0;
    while (*line) {
        char *end = strstr(line, "\r\n"); if (!end) return fail(io, 400, "header_line", deadline);
        *end = 0; char *next = end+2;
        char *value = strchr(line, ':'); if (!value || value == line) return fail(io, 400, "header_name", deadline);
        *value++ = 0;
        for (const char *p = line; *p; ++p) if (!( (*p >= 'A' && *p <= 'Z') || (*p >= 'a' && *p <= 'z') ||
            (*p >= '0' && *p <= '9') || *p == '-')) return fail(io, 400, "header_name", deadline);
        while (*value == ' ' || *value == '\t') ++value;
        char *tail = value+strlen(value); while (tail > value && (tail[-1] == ' ' || tail[-1] == '\t')) *--tail = 0;
        for (const char *p = value; *p; ++p) if ((unsigned char)*p < 32 || (unsigned char)*p == 127) return fail(io, 400, "header_value", deadline);
        if (same(line, "transfer-encoding") || same(line, "expect")) return fail(io, 400, "unsupported_framing", deadline);
        if (same(line, "content-length")) {
            if (lengthSeen || !*value || strlen(value) > 7) return fail(io, 400, "content_length", deadline);
            lengthSeen = true;
            for (char *p = value; *p; ++p) { if (*p < '0' || *p > '9') return fail(io, 400, "content_length", deadline); length = length*10+*p-'0'; }
            if (length > bodyLimit) return fail(io, 413, "body_limit", deadline);
        } else if (same(line, "authorization")) {
            if (authSeen || strncmp(value, "Bearer ", 7)) return fail(io, 401, "authorization", deadline);
            const char *token = value+7;
            bool manual = maintenance && (!strncmp(request.path,"/api/wifi/",10) || !strncmp(request.path,"/api/diagnostic/",16) ||
                !strncmp(request.path,"/api/firmware/",14) || !strcmp(request.path,"/health") || !strcmp(request.path,"/api/maintenance/status"));
            if (manual) {
                // Let the common guard count invalid credentials, including missing ones.
                if (strlen(token) > 32) return fail(io, 401, "authorization", deadline);
            } else if (strlen(token) != 32) return fail(io, 401, "authorization", deadline);
            authSeen = true; request.token = token;
        } else if (same(line, "x-device-token")) {
            if (tokenSeen || strlen(value) != 32) return fail(io, 401, "device_token", deadline);
            tokenSeen = true; request.deviceToken = value;
        } else if (same(line, "content-type")) {
            if (typeSeen) return fail(io, 400, "content_type", deadline);
            typeSeen = true; request.contentType = value;
        }
        line = next;
    }
    if (!strcmp(request.method, "POST") && !lengthSeen) return fail(io, 411, "content_length_required", deadline);
    if (used > headerBytes+length) return fail(io, 400, "trailing_bytes", deadline);
    std::unique_ptr<uint8_t[]> large;
    uint8_t *body=storage.get()+headerBytes;size_t received=used-headerBytes;
    if(length>4096) {
        large.reset(new(std::nothrow) uint8_t[length]);if(!large)return fail(io,503,"oom",deadline);
        memcpy(large.get(),body,received);body=large.get();request.body=body;
    }
    while (received < length) {
        int n = io.read(io.context, body+received, length-received, deadline);
        if (n <= 0 || size_t(n) > length-received) return false;
        received += n;
    }
    request.length = length;
    // The body/header budget stays short. A complete bounded Bundle or ROM
    // verification needs its own finite execution/response budget.
    if(!maintenance&&!strcmp(request.path,"/api/bundle/commit"))deadline=now+30000;
    if(maintenance&&!strcmp(request.path,"/api/firmware/commit"))deadline=now+60000;
    if(maintenance&&(!strcmp(request.path,"/api/firmware/stable")||!strcmp(request.path,"/api/firmware/switch")))deadline=now+60000;
    Ap01HttpReply response = maintenance ? app.maintenanceHttp(request, now) : app.businessHttp(request, now);
    return reply(io, response.status, response.frame, response.bytes.c_str(), response.bytes.length(), deadline);
}
