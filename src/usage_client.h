#pragma once
#include <Arduino.h>
#include "bridge_store.h"

bool usageHttpGet(const EndpointRec &rec, String &out, String &err, uint32_t timeoutMs = 3000);
