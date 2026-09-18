#pragma once
#include <Arduino.h>

typedef void (*UsageJsonHandler)(const String &json);
typedef void (*EndpointJsonHandler)(const String &json);
typedef void (*TemplateCtrlHandler)(const String &json);
typedef void (*TemplateDataHandler)(const uint8_t *data, size_t len);
typedef void (*TemplateResetHandler)();
typedef void (*AuthJsonHandler)(const String &json);

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
void bleSetAuthHandler(AuthJsonHandler onAuth);
void bleSetInfoExtra(const String &json);
void bleNotifyStatus(const String &json);
void bleNotifyStatusQuiet(const String &json);
void blePoll();
void bleClearBonds();

#define BLE_SVC_UUID   "e7f1a000-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_INFO   "e7f1a001-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_ENDPT  "e7f1a002-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_USAGE  "e7f1a003-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_STATUS "e7f1a004-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_TPLCTL "e7f1a005-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_TPLDAT "e7f1a006-4b2a-4c9e-9a11-3c0d5e9a0000"
#define BLE_CHR_AUTH   "e7f1a007-4b2a-4c9e-9a11-3c0d5e9a0000"
