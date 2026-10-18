#pragma once
#include "bundle_store.h"
#include "template_renderer.h"
#include <ArduinoJson.h>
#include <memory>
struct BsActivationReceipt {
    char owner[65] = {}, request[65] = {}, templateId[BS_ID_LEN] = {};
    char expected[BS_CTX_LEN] = {}, context[BS_CTX_LEN] = {};
};
#if defined(CODEX_AP01_NATIVE)
using BundleFilesystem = LittleFSClass;
#else
using BundleFilesystem = decltype(LittleFS);
#endif

// Single owned repository; serialized mutation, no writable XIP singleton.
class BundleStore {
    BundleFilesystem &fs;
    TemplateRenderer &renderer;
    bool mounted = false, configured = false, fontsLoaded = false;
    BsProfile profile;
    uint32_t sequence = 0, payloadOffset = 0, payloadLength = 0, payloadCrc = 0;
    uint32_t cacheOffset = 0, contentLength = 0, contentCrc = 0;
    String lastError;
    BsReceipt receipt;
    BsReceipt incomingReceipt;
    std::unique_ptr<BsActivationReceipt> activation;
    bool (*replaceGuard)(void *) = nullptr;
    void *replaceContext = nullptr;
    // The on-wire format permits at most eight font assets and template records.
    uint8_t *fontBytes[8] = {};
    uint8_t fontCount = 0;
    void clearFonts();
    size_t fileSize(const char *path);
    bool removeIfPresent(const char *path);
    bool readMeta(const char *path, JsonDocument &doc);
    bool writeMeta();
    bool loadMeta(JsonDocument &d);
    bool migrateLegacy();
    bool decodeFont(JsonObject entry, uint8_t *&bytes, size_t &len, String &err);
    bool loadFonts(String &err);
public:
    void beforeReplace(bool (*guard)(void *), void *context) { replaceGuard = guard; replaceContext = context; }
    File openRead(const char *path) { return fs.open(path, "r"); }
    BundleStore(BundleFilesystem &filesystem, TemplateRenderer &engine) : fs(filesystem), renderer(engine) {}
    BundleStore(const BundleStore &) = delete;
    BundleStore &operator=(const BundleStore &) = delete;
    ~BundleStore() { clearFonts(); }
    const char * bsReceivePath();
    uint32_t bsDecodedLimit();
    uint32_t bsCompressionProtocol();
    bool bsBegin();
    size_t bsFreeBytes();
    size_t bsMaxBundleBytes();
    bool bsStartReceive(uint32_t length, String &err,
                    const char *owner = "", const char *request = "");
    bool bsInstall(File &bundleFile, uint32_t bundleLength, uint32_t bundleCrc,
               const char *expectedFirmware, const char *expectedRender,
               const char *newContextId, String &err, void (*beforeReplace)() = nullptr);
    bool bsInstall(const String &bundleJson, const char *expectedFirmware,
               const char *expectedRender, const char *newContextId, String &err);
    bool bsLoadCompiled(uint8_t index, CtTemplate &out, String &err);
    bool bsSetActive(uint8_t index, const char *newContextId, String &err,
                     const BsActivationReceipt *transaction = nullptr);
    const BsActivationReceipt *bsActivationReceipt() const { return activation.get(); }
    bool bsConfigured();
    bool bsReceipt(BsReceipt &out);
    bool bsProfile(BsProfile &out);
    bool bsActiveJobPayload(const char *jobId, uint32_t &payloadCrc);
    uint32_t bsCommitSeq();
    const char * bsLastError();
    bool bsHasSource(uint8_t index);
    bool bsRecoveryDigest(String &out);
};

#if !defined(CODEX_AP01_NATIVE)
BundleStore &bsDefaultStore();
#endif
