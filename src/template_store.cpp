#include "template_store.h"
#include "dev_log.h"
#include <LittleFS.h>
#include <ArduinoJson.h>
#include <vector>

static std::vector<TplMeta> sItems;
static String sActive;
static String sIdle;
static uint32_t sCtr = 0;

static const char *IDX_PATH = "/tpl/index.json";

static String tplPath(const String &id) { return String("/tpl/") + id + ".json"; }

static bool idValid(const String &id) {
    if (id.length() == 0 || id.length() > 16) return false;
    for (size_t i = 0; i < id.length(); i++) {
        char c = id[i];
        if (!isalnum((unsigned char)c) && c != '_' && c != '-') return false;
    }
    return true;
}

static void persist() {
    JsonDocument doc;
    doc["active"] = sActive;
    doc["idle"] = sIdle;
    JsonArray arr = doc["items"].to<JsonArray>();
    for (const auto &m : sItems) {
        JsonObject o = arr.add<JsonObject>();
        o["id"] = m.id;
        o["version"] = m.version;
        o["hash"] = m.hash;
        o["size"] = m.size;
        o["used"] = m.usedAt;
    }
    String out;
    serializeJson(doc, out);
    File f = LittleFS.open("/tpl/index.json.tmp", FILE_WRITE);
    if (!f) return;
    f.print(out);
    f.close();
    LittleFS.remove(IDX_PATH);
    LittleFS.rename("/tpl/index.json.tmp", IDX_PATH);
}

void tplStoreBegin() {
    if (!LittleFS.begin(true, "/littlefs", 10, "storage")) {
        DevLog.println("[tpl] LittleFS mount failed");
        return;
    }
    if (!LittleFS.exists("/tpl")) LittleFS.mkdir("/tpl");
    sItems.clear();
    sActive = "";
    sIdle = "";
    File f = LittleFS.open(IDX_PATH, FILE_READ);
    if (f) {
        JsonDocument doc;
        if (!deserializeJson(doc, f.readString())) {
            sActive = doc["active"] | "";
            sIdle   = doc["idle"] | "";
            JsonArray arr = doc["items"].as<JsonArray>();
            for (JsonObject o : arr) {
                TplMeta m;
                m.id      = String((const char *)(o["id"] | ""));
                m.version = o["version"] | 0;
                m.hash    = String((const char *)(o["hash"] | ""));
                m.size    = o["size"] | 0;
                m.usedAt  = o["used"] | 0;
                if (idValid(m.id)) sItems.push_back(m);
            }
        }
        f.close();
    }
    DevLog.printf("[tpl] store: %d template(s), active=%s\n",
                  (int)sItems.size(), sActive.c_str());
}

int tplStoreCount() { return (int)sItems.size(); }

bool tplStoreGet(int i, TplMeta &m) {
    if (i < 0 || i >= (int)sItems.size()) return false;
    m = sItems[i];
    return true;
}

bool tplStoreFind(const String &id, TplMeta &m) {
    for (const auto &it : sItems) {
        if (it.id == id) { m = it; return true; }
    }
    return false;
}

bool tplStoreSave(const String &id, uint32_t version, const String &hash,
                  const uint8_t *data, size_t len) {
    if (!idValid(id) || !data || len == 0 || len > 32768) return false;
    String tmp = tplPath(id) + ".tmp";
    String dst = tplPath(id);
    File f = LittleFS.open(tmp, FILE_WRITE);
    if (!f) return false;
    size_t wrote = f.write(data, len);
    f.close();
    if (wrote != len) {
        LittleFS.remove(tmp);
        return false;
    }
    LittleFS.remove(dst);
    if (!LittleFS.rename(tmp, dst)) {
        LittleFS.remove(tmp);
        return false;
    }
    if (sActive.length() == 0) sActive = id;
    bool found = false;
    for (auto &m : sItems) {
        if (m.id == id) {
            m.version = version;
            m.hash    = hash;
            m.size    = len;
            m.usedAt  = ++sCtr;
            found = true;
            break;
        }
    }
    if (!found) {
        TplMeta m;
        m.id = id; m.version = version; m.hash = hash; m.size = len; m.usedAt = ++sCtr;
        sItems.push_back(m);
    }
    while ((int)sItems.size() > TPL_STORE_MAX) {
        int victim = -1;
        uint32_t oldest = 0xFFFFFFFF;
        for (int i = 0; i < (int)sItems.size(); i++) {
            if (sItems[i].id == sActive) continue;
            if (sIdle.length() && sItems[i].id == sIdle) continue;
            if (sItems[i].usedAt < oldest) { oldest = sItems[i].usedAt; victim = i; }
        }
        if (victim < 0) break;
        DevLog.printf("[tpl] evict %s\n", sItems[victim].id.c_str());
        LittleFS.remove(tplPath(sItems[victim].id));
        sItems.erase(sItems.begin() + victim);
    }
    persist();
    DevLog.printf("[tpl] saved %s v%u hash=%s len=%u\n",
                  id.c_str(), (unsigned)version, hash.c_str(), (unsigned)len);
    return true;
}

bool tplStoreLoad(const String &id, String &out) {
    if (!idValid(id)) return false;
    File f = LittleFS.open(tplPath(id), FILE_READ);
    if (!f) return false;
    out = f.readString();
    f.close();
    return out.length() > 0;
}

bool tplStoreRemove(const String &id) {
    if (!idValid(id)) return false;
    LittleFS.remove(tplPath(id));
    for (size_t i = 0; i < sItems.size(); i++) {
        if (sItems[i].id == id) { sItems.erase(sItems.begin() + i); break; }
    }
    if (sActive == id) sActive = sItems.empty() ? "" : sItems[0].id;
    if (sIdle == id) sIdle = "";
    persist();
    return true;
}

String tplStoreActive() { return sActive; }

String tplStoreIdle() { return sIdle; }

void tplStoreSetIdle(const String &id) {
    if (sIdle == id) return;
    sIdle = id;
    persist();
}

bool tplStoreSetActive(const String &id) {
    TplMeta m;
    if (!tplStoreFind(id, m)) return false;
    sActive = id;
    for (auto &it : sItems) {
        if (it.id == id) it.usedAt = ++sCtr;
    }
    persist();
    return true;
}

void tplStoreTouch(const String &id) {
    for (auto &it : sItems) {
        if (it.id == id) { it.usedAt = ++sCtr; persist(); return; }
    }
}

void tplStoreClear() {
    for (const auto &m : sItems) LittleFS.remove(tplPath(m.id));
    LittleFS.remove(IDX_PATH);
    sItems.clear();
    sActive = "";
    sIdle = "";
    DevLog.println("[tpl] store cleared");
}
