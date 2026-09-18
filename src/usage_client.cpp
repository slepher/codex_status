#include "usage_client.h"
#include <HTTPClient.h>

static bool httpGet(const String &url, const EndpointRec &rec, String &out, String &err,
                    uint32_t timeoutMs) {
    out = "";
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

bool usageHttpGet(const EndpointRec &rec, String &out, String &err, uint32_t timeoutMs) {
    String url = "http://" + rec.host + ":" + String(rec.port) + "/usage";
    return httpGet(url, rec, out, err, timeoutMs);
}
