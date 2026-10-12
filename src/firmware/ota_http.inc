// Included by transport.cpp and the host fault-injection test.
// WebServer processes one HTTP request at a time. canHandle starts request
// state before parsing the body; upload callbacks never reset that state.
struct OtaHttpResult {
    int status = 400;
    const char *error = "file_required";
    bool fileSeen = false;
    bool busy = false;
    bool ownsUpdate = false;
};
static OtaHttpResult otaHttp;

void otaUploadCleanup(const char *reason) {
    if (!state.otaInProgress && !otaHttp.ownsUpdate) return;
    DevLog.printf("[ota] abort (%s) err=%u\n", reason, (unsigned)Update.getError());
    Update.abort();
    state.otaInProgress = false;
    otaHttp.ownsUpdate = false;
    otaHttp.status = !strcmp(reason, "stalled") ? 408 : 500;
    otaHttp.error = reason;
    power::setOtaLock(false);
    power::setOtaWifiAwake(false);
    if (display::state.frame) display::renderCurrent();
}

void otaUploadTick() {
    if (state.otaInProgress && (uint32_t)(millis() - state.otaLastDataMs) >= 20000) {
        otaUploadCleanup("stalled");
    }
}

static void otaHttpUpload(HTTPUpload &up) {
    if (up.status == UPLOAD_FILE_START) {
        if (!requestAuthorized()) { otaHttp.status = 401; otaHttp.error = "unauthorized"; return; }
        if (otaHttp.busy) { otaHttp.status = 409; otaHttp.error = "busy"; return; }
        if (otaHttp.fileSeen) {
            otaUploadCleanup("multiple_files");
            otaHttp.status = 400; otaHttp.error = "multiple_files"; return;
        }
        otaHttp.fileSeen = true;
        if (state.server.hasArg("target") && state.server.arg("target") != FW_TARGET_ID) {
            otaHttp.status = 400; otaHttp.error = "target_mismatch"; return;
        }
        otaHttp.ownsUpdate = true;
        otaHttp.status = 100; otaHttp.error = "incomplete_upload";
        state.otaInProgress = true;
        state.otaLastDataMs = millis();
        power::setOtaLock(true);
        power::setOtaWifiAwake(true);
        display::screen({"OTA update", application::state.firmwareVersion});
        if (!Update.begin(UPDATE_SIZE_UNKNOWN)) otaUploadCleanup("begin_failed");
    } else if (up.status == UPLOAD_FILE_WRITE) {
        if (!otaHttp.ownsUpdate || otaHttp.status != 100) return;
        state.otaLastDataMs = millis();
        if (up.currentSize) dataLedActivity();
        if (Update.write(up.buf, up.currentSize) != up.currentSize) otaUploadCleanup("write_failed");
    } else if (up.status == UPLOAD_FILE_END) {
        if (!otaHttp.ownsUpdate || otaHttp.status != 100) return;
        if (!up.totalSize) { otaUploadCleanup("empty_file"); otaHttp.status = 400; return; }
        if (!Update.end(true)) { otaUploadCleanup("end_failed"); return; }
        otaHttp.ownsUpdate = false;
        state.otaInProgress = false;
        power::setOtaLock(false);
        power::setOtaWifiAwake(false);
        otaHttp.status = 200; otaHttp.error = nullptr;
    } else if (up.status == UPLOAD_FILE_ABORTED) {
        if (otaHttp.ownsUpdate) otaUploadCleanup("aborted");
    }
}

static void otaHttpComplete() {
    state.server.sendHeader("Connection", "close");
    // Authenticate even a request with no multipart file / no upload callback.
    if (!requestAuthorized()) {
        if (otaHttp.ownsUpdate) otaUploadCleanup("unauthorized");
        otaHttp.status = 401; otaHttp.error = "unauthorized";
    } else if (otaHttp.ownsUpdate) {
        otaUploadCleanup("incomplete_upload");
    } else if (otaHttp.busy) {
        otaHttp.status = 409; otaHttp.error = "busy";
    }
    if (otaHttp.status == 200 && otaHttp.fileSeen) {
        state.server.send(200, "text/plain", "UPDATE OK");
        display::screen({"OTA success", "Rebooting..."});
        // Independent native OTA: no session, owner, frame or PowerPlan.
        state.otaRebootPending = true;
        state.otaRebootAt = millis() + 1500;
    } else {
        char body[128];
        snprintf(body, sizeof(body), "{\"error\":\"%s\",\"stage\":\"upload\"}", otaHttp.error ? otaHttp.error : "file_required");
        state.server.send(otaHttp.status, "application/json", body);
    }
    otaHttp = OtaHttpResult{};
}

class OtaHttpHandler final : public RequestHandler {
public:
    bool canHandle(WebServer &, HTTPMethod method, const String &uri) override {
        if (method != HTTP_POST || uri != "/doUpdate") return false;
        otaHttp = OtaHttpResult{};
        otaHttp.busy = state.otaInProgress || state.otaRebootPending || Update.isRunning();
        return true;
    }
    bool canUpload(WebServer &, const String &uri) override { return uri == "/doUpdate"; }
    bool handle(WebServer &, HTTPMethod, const String &) override { otaHttpComplete(); return true; }
    void upload(WebServer &, const String &, HTTPUpload &up) override { otaHttpUpload(up); }
};
