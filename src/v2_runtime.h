// v2 runtime: apply bounded Data messages against the active CompiledTemplate
// and build the usage document the compiled renderer consumes. No template JSON
// is parsed here; field indices are validated against the compiled requirement
// table.
#pragma once

#include <Arduino.h>
#include <ArduinoJson.h>
#include "template_engine.h"
#include "v2_state.h"

// CRC over the Data `fields` array, identical on the Bridge and the device:
// for each entry in index order: "<i>:<k>:<json value or ~>:<quality>;".
// A missing value is encoded as `~` so absence participates in the fingerprint.
uint32_t v2DataFieldsCrc(JsonArrayConst fields);

// Apply one complete Data message. `currentContext` is the active context id;
// on V2_DATA_APPLIED `usageOut` is the rebuilt usage document. `ct` must be the
// active compiled template.
V2DataAck v2ApplyData(const CtTemplate &ct, const String &messageJson,
                      const char *currentContext, String &usageOut, String &errorOut);

// Build a synthetic usage document from already-applied field values (used at
// boot to re-render the last data without a fresh message).
bool v2UsageFromFields(const CtTemplate &ct, const String &storedFieldsJson,
                       String &usageOut, String &errorOut);

// Compute and store the bounded device wire CRC for a generated message.
uint32_t v2CrcOf(const String &text);
