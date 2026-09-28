#pragma once
#include <Arduino.h>

typedef void (*UsageJsonHandler)(const String &json);
typedef void (*EndpointJsonHandler)(const String &json);
typedef void (*TemplateCtrlHandler)(const String &json);
typedef void (*TemplateDataHandler)(const uint8_t *data, size_t len);
typedef void (*TemplateResetHandler)();
typedef void (*AuthJsonHandler)(const String &json);
typedef void (*V2CtrlHandler)(const String &json);

void bleBegin(const String &deviceName, const String &fwVersion);
void bleDeinit();
bool bleInitialized();
bool bleIsConnected();
bool blePeerIsBonded();
bool blePeerIsEncrypted();
String blePeerAddress();
bool blePairingWindowOpen();
void bleOpenPairingWindow(uint32_t ms);
void bleAdvertiseStart();
void bleAdvertiseStop();
void bleSetHandlers(UsageJsonHandler onUsage, EndpointJsonHandler onEndpoint);
void bleSetTemplateHandlers(TemplateCtrlHandler onCtrl, TemplateDataHandler onData,
                            TemplateResetHandler onReset = nullptr);
// v2 rendezvous control split (design §5.1): a template-control JSON carrying
// `"rv":2` is routed here instead of the legacy template handler. Without a
// registered handler the device answers a bounded NACK, so an accidental v2
// frame can never be parsed as an old template op.
void bleSetV2Handler(V2CtrlHandler onV2);
void bleSetAuthHandler(AuthJsonHandler onAuth);
void bleSetInfoExtra(const String &json);
void bleNotifyStatus(const String &json);
void bleNotifyStatusQuiet(const String &json);
void blePoll();
void bleClearBonds();
// Diagnostic scan (Plan C task-6 §1.1): the device as the independent BLE
// receiver for the Windows publisher spike, and the SCAN half of the
// bridge_first candidate. Scans `seconds`, keeping records whose manufacturer
// payload starts with `companyFilter` (0 = all devices), and returns a JSON
// summary with per-advertisement arrival times, RSSI, connectability and raw
// payload. Blocking; call only while the device is awake.
String bleScanJson(uint32_t seconds, uint16_t companyFilter, uint8_t maxRecords);
// Note4 RF experiment only: bounded, unauthenticated A/B/C recovery exchange.
// Called solely from the token-gated diagnostic endpoint; never installs a plan.
String bleRecoveryTrialJson(char variant, uint32_t runId);

#define BLE_SVC_UUID   "e7f1a000-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_INFO   "e7f1a001-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_ENDPT  "e7f1a002-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_USAGE  "e7f1a003-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_STATUS "e7f1a004-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_TPLCTL "e7f1a005-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_TPLDAT "e7f1a006-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_AUTH   "e7f1a007-4b2a-4c9e-9a11-3c0d5e9a0000"
