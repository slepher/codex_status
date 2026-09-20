#pragma once
#include <Arduino.h>
#include "bridge_store.h"

// GET <path> from the bridge endpoint (default /usage). `path` may carry the
// deep-pull query string (`?next_contact_s=&mode=&usage_rev=`).
bool usageHttpGet(const EndpointRec &rec, String &out, String &err,
                  uint32_t timeoutMs = 3000, const String &path = "/usage");

// POST a small JSON body (e.g. /deep notification). Return HTTP code in `code`.
bool usageHttpPost(const EndpointRec &rec, const String &path, const String &body,
                   int &code, String &out, String &err, uint32_t timeoutMs = 3000);
