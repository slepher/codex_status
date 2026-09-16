#pragma once
#include <Arduino.h>

typedef void (*TplChangedHandler)();

void tplXferBegin(const String &fwVersion, TplChangedHandler onChanged);
void tplXferHandleCtrl(const String &json);
void tplXferHandleChunk(const uint8_t *data, size_t len);
void tplXferReset();
bool tplValidateForStorage(const String &tmplJson, const String &expectedHash,
                           const String &fwVersion, String &err);
