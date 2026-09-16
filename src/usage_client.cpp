#include "usage_client.h"
#include <HTTPClient.h>

static bool httpGet(const String &url, const EndpointRec &rec, String &out, String &err,
                    uint32_t timeoutMs) {
    HTTPClient http;
    http.setConnectTimeout(timeoutMs);
    http.setTimeout(timeoutMs);
    if (!http.begin(url)) {
        err = "begin failed";
        return false;
    }
    http.addHeader("Authorization", "Bearer " + rec.token);
    int code = http.GET();
    if (code == 200) {
        out = http.getString();
        http.end();
        return true;
    }
    err = "http " + String(code);
    http.end();
    return false;
}

bool usageHttpGet(const EndpointRec &rec, String &out, String &err, uint32_t timeoutMs) {
    String url = "http://" + rec.host + ":" + String(rec.port) + "/usage";
    return httpGet(url, rec, out, err, timeoutMs);
}

bool usageTemplateGet(const EndpointRec &rec, const String &id, const String &hash,
                      String &out, String &err, uint32_t timeoutMs) {
    String url = "http://" + rec.host + ":" + String(rec.port) +
                 "/template?id=" + id + "&hash=" + hash;
    return httpGet(url, rec, out, err, timeoutMs);
}
