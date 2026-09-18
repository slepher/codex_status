#pragma once
#include <Arduino.h>

#define TPL_STORE_MAX 4

struct TplMeta {
    String   id;
    uint32_t version = 0;
    String   hash;
    uint32_t size   = 0;
    uint32_t usedAt = 0;
};

void   tplStoreBegin();
int    tplStoreCount();
bool   tplStoreGet(int i, TplMeta &m);
bool   tplStoreFind(const String &id, TplMeta &m);
bool   tplStoreSave(const String &id, uint32_t version, const String &hash,
                    const uint8_t *data, size_t len);
bool   tplStoreLoad(const String &id, String &out);
bool   tplStoreRemove(const String &id);
String tplStoreActive();
bool   tplStoreSetActive(const String &id);
void   tplStoreTouch(const String &id);
void   tplStoreClear();
