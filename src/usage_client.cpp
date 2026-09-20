#include "usage_client.h"
#include <HTTPClient.h>

static void httpBegin(HTTPClient &http, const EndpointRec &rec, const String &path,
                      uint32_t timeoutMs) {
    http.setConnectTimeout(timeoutMs);
    http.setTimeout(timeoutMs);
    String base = "http://" + rec.host + ":" + String(rec.port);
    http.begin(base + (path.length() ? path : String("/usage")));
    http.addHeader("Authorization", "Bearer " + rec.token);
}

bool usageHttpGet(const EndpointRec &rec, String &out, String &err,
                  uint32_t timeoutMs, const String &path) {
    out = "";
    HTTPClient http;
    httpBegin(http, rec, path, timeoutMs);
    int code = http.GET();
    if (code == 200) {
        out = http.getString();
        http.end();
        if (out.length() == 0) {
            err = "empty response";
            return false;
        }
        return true;
    }
    err = "http " + String(code);
    http.end();
    return false;
}

bool usageHttpPost(const EndpointRec &rec, const String &path, const String &body,
                   int &code, String &out, String &err, uint32_t timeoutMs) {
    out = "";
    code = 0;
    HTTPClient http;
    httpBegin(http, rec, path, timeoutMs);
    http.addHeader("Content-Type", "application/json");
    code = http.POST((uint8_t *)body.c_str(), body.length());
    if (code > 0) {
        out = http.getString();
        http.end();
        return code >= 200 && code < 300;
    }
    err = "http " + String(code);
    http.end();
    return false;
}
