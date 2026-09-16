#include "template_xfer.h"
#include <ArduinoJson.h>
#include <esp32-hal-psram.h>
#include "template_store.h"
#include "template_engine.h"
#include "ble_bridge.h"

#define TPL_MAX_LEN 32768

static TplChangedHandler sChanged = nullptr;
static String   sFw;
static uint8_t *sBuf = nullptr;
static size_t   sLen = 0;
static size_t   sGot = 0;
static String   sId;
static String   sHash;
static uint32_t sVer = 0;
static uint32_t sCrc = 0;
static bool     sInProgress = false;

static uint32_t crc32buf(const uint8_t *d, size_t n) {
    uint32_t crc = 0xFFFFFFFF;
    for (size_t i = 0; i < n; i++) {
        crc ^= d[i];
        for (int k = 0; k < 8; k++) crc = (crc >> 1) ^ (0xEDB88320 & (-(int32_t)(crc & 1)));
    }
    return ~crc;
}

static bool idValid(const String &id) {
    if (id.length() == 0 || id.length() > 16) return false;
    for (size_t i = 0; i < id.length(); i++) {
        char c = id[i];
        if (!isalnum((unsigned char)c) && c != '_' && c != '-') return false;
    }
    return true;
}

static void abortRecv() {
    if (sBuf) free(sBuf);
    sBuf = nullptr;
    sLen = sGot = 0;
    sInProgress = false;
}

static bool versionGE(const String &fw, const String &minv) {
    if (minv.length() == 0) return true;
    int fmaj = 0, fmin = 0, mmaj = 0, mmin = 0;
    sscanf(fw.c_str(), "%d.%d", &fmaj, &fmin);
    sscanf(minv.c_str(), "%d.%d", &mmaj, &mmin);
    if (fmaj != mmaj) return fmaj > mmaj;
    return fmin >= mmin;
}

static void ack(const char *op, bool ok, const char *err = nullptr) {
    String j = String("{\"ack\":\"tpl\",\"op\":\"") + op + "\",\"ok\":" + (ok ? "true" : "false");
    if (err) j += String(",\"err\":\"") + err + "\"";
    j += "}";
    bleNotifyStatus(j);
}

static void handleBegin(JsonDocument &d) {
    abortRecv();
    String id   = String((const char *)(d["id"] | ""));
    String hash = String((const char *)(d["hash"] | ""));
    uint32_t ver = d["version"] | 0;
    uint32_t len = d["len"] | 0;
    uint32_t crc = d["crc"] | 0;
    if (!idValid(id) || len == 0 || len > TPL_MAX_LEN || hash.length() == 0) {
        ack("begin", false, "args");
        return;
    }
    sBuf = (uint8_t *)ps_malloc(len);
    if (!sBuf) sBuf = (uint8_t *)malloc(len);
    if (!sBuf) {
        ack("begin", false, "oom");
        return;
    }
    sId = id; sHash = hash; sVer = ver; sLen = len; sCrc = crc;
    sGot = 0;
    sInProgress = true;
    Serial.printf("[tpl] begin id=%s v%u len=%u crc=%08x\n",
                  id.c_str(), (unsigned)ver, (unsigned)len, (unsigned)crc);
    String j = String("{\"ack\":\"tpl\",\"op\":\"begin\",\"ok\":true,\"len\":") + String(len) + "}";
    bleNotifyStatus(j);
}

static void handleEnd() {
    if (!sInProgress) { ack("end", false, "nobegin"); return; }
    if (sGot != sLen) {
        ack("end", false, "short");
        abortRecv();
        return;
    }
    uint32_t crc = crc32buf(sBuf, sLen);
    if (sCrc != 0 && crc != sCrc) {
        Serial.printf("[tpl] crc mismatch %08x != %08x\n", (unsigned)crc, (unsigned)sCrc);
        ack("end", false, "crc");
        abortRecv();
        return;
    }
    String json;
    json.reserve(sLen);
    json.concat((const char *)sBuf, sLen);
    abortRecv();

    String err;
    if (!tplValidate(json, err)) {
        ack("end", false, err.c_str());
        return;
    }
    {
        JsonDocument td;
        if (!deserializeJson(td, json)) {
            const char *minfw = td["min_fw"] | "";
            if (!versionGE(sFw, String(minfw))) {
                ack("end", false, "minfw");
                return;
            }
        }
    }
    TplMeta existing;
    bool unchanged = tplStoreFind(sId, existing) && existing.hash == sHash;
    if (!unchanged) {
        if (!tplStoreSave(sId, sVer, sHash, (const uint8_t *)json.c_str(), json.length())) {
            ack("end", false, "save");
            return;
        }
    }
    tplStoreSetActive(sId);
    tplStoreTouch(sId);
    ack("end", true);
    Serial.printf("[tpl] end id=%s hash=%s%s\n", sId.c_str(), sHash.c_str(),
                  unchanged ? " (unchanged)" : "");
    if (sChanged) sChanged();
}

static void handleActivate(JsonDocument &d) {
    String id = String((const char *)(d["id"] | ""));
    if (!idValid(id)) { ack("activate", false, "args"); return; }
    TplMeta m;
    if (!tplStoreFind(id, m)) { ack("activate", false, "missing"); return; }
    tplStoreSetActive(id);
    tplStoreTouch(id);
    ack("activate", true);
    Serial.printf("[tpl] activate %s\n", id.c_str());
    if (sChanged) sChanged();
}

static void handleList() {
    String j = "{\"ack\":\"tpl\",\"op\":\"list\",\"ok\":true,\"items\":[";
    String active = tplStoreActive();
    for (int i = 0; i < tplStoreCount(); i++) {
        TplMeta m;
        if (!tplStoreGet(i, m)) continue;
        if (i) j += ",";
        j += String("{\"id\":\"") + m.id + "\",\"version\":" + String(m.version) +
             ",\"hash\":\"" + m.hash + "\",\"active\":" +
             ((m.id == active) ? "true" : "false") + "}";
    }
    j += "]}";
    bleNotifyStatus(j);
}

static void handleDelete(JsonDocument &d) {
    String id = String((const char *)(d["id"] | ""));
    if (!idValid(id)) { ack("delete", false, "args"); return; }
    tplStoreRemove(id);
    ack("delete", true);
    if (sChanged) sChanged();
}

void tplXferHandleCtrl(const String &json) {
    JsonDocument d;
    if (deserializeJson(d, json)) { ack("ctrl", false, "json"); return; }
    const char *op = d["op"] | "";
    if (!strcmp(op, "begin"))         handleBegin(d);
    else if (!strcmp(op, "end"))      handleEnd();
    else if (!strcmp(op, "activate")) handleActivate(d);
    else if (!strcmp(op, "list"))     handleList();
    else if (!strcmp(op, "delete"))   handleDelete(d);
    else ack("ctrl", false, "op");
}

void tplXferHandleChunk(const uint8_t *data, size_t len) {
    if (!sInProgress || !sBuf || len < 3) return;
    size_t off = (size_t)data[0] | ((size_t)data[1] << 8);
    const uint8_t *payload = data + 2;
    size_t plen = len - 2;
    if (off != sGot || off + plen > sLen) {
        Serial.printf("[tpl] bad chunk off=%u got=%u plen=%u len=%u\n",
                      (unsigned)off, (unsigned)sGot, (unsigned)plen, (unsigned)sLen);
        ack("data", false, "off");
        abortRecv();
        return;
    }
    memcpy(sBuf + off, payload, plen);
    sGot += plen;
}

void tplXferBegin(const String &fwVersion, TplChangedHandler onChanged) {
    sFw = fwVersion;
    sChanged = onChanged;
}
