#pragma once

// Firmware integration only: state is owned here; never copy it as a snapshot.
#include "config.h"
#include "dependencies.h"

namespace firmware::transport {

struct V2BleMessage { String body; String peer; String identity; uint32_t generation; };

struct State {
    WebServer   server{80};
    bool        otaRebootPending = false;
    uint32_t    otaRebootAt = 0;
    String      authToken;
    bool     otaInProgress = false;
    uint32_t otaLastDataMs = 0;   // last UPLOAD_FILE_WRITE (stall watchdog)
    String v2HttpBody;

    // Bounded mailbox from NimBLE's callback task to the device loop task.
    QueueHandle_t v2BleQueue = nullptr;

    // ===========================================================================
    // v2 protocol endpoints (auth: endpoint token + owner; no implicit renewal).
    // ===========================================================================

    // `bridge_id` arrives in the JSON body for data/plan; bundle/activate calls also
    // carry it there (the query/form fallback exists for simple curl tests).
    bool v2ReplyOverBle = false;
    String v2RequestId;
    bool v2HttpFrameActive = false;
    String v2HttpRaw, v2HttpToken, v2HttpLastFrame;
    CompactFrameHeader v2HttpFrame;
};
extern State state;

bool endpointTokenAuthorized(String &mac);
void handleBleEndpoint(const String &json);
void loadOrIssueAuthToken();
bool requestAuthorized();
void handleBleAuth(const String &json);
void handleBleV2Ctrl(const String &json);
void otaUploadCleanup(const char *reason);
bool compactSessionFromHex(const String &text, uint8_t out[16]);
void v2Response(int status, const String &body);
void v2Ack(const char *op, const char *result, const char *display,
                  const char *retention, const char *error, int64_t seq, uint64_t planId,
                  const char *context, uint32_t acceptedRemainingS, bool fullSeq = false);
const String &v2Nonce();
bool v2Command(const String &body, JsonDocument &doc);
void v2HttpRawHandler();
void v2HttpRoute(uint16_t op, void (*handler)());
void serviceV2Ble();
void registerHttpRoutes();

} // namespace firmware::transport
