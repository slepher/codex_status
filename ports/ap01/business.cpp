#include "business.h"
#include "local_json_allocator.h"
#include "v2_activate_command.h"
#include "v2_command_envelope.h"
#include "v2_data_command.h"
#include "v2_status_snapshot.h"
#include "platform_target.h"
#include <new>

Ap01Business::Ap01Business(BundleFilesystem &filesystem, const Ap01BusinessPort &port, uint64_t bootId)
    : ui(port), fs(filesystem), boot(bootId), services{store, durable},
      renderer(paint, 320, 240), store(filesystem, renderer) { store.beforeReplace(beforeReplace, this); }

bool Ap01Business::beforeReplace(void *context) {
    auto &self = *(Ap01Business *)context;
    if (self.ui.retire ? !self.ui.retire(self.ui.context) :
        !self.ui.present || !self.ui.present(self.ui.context, nullptr, 0)) return false;
    self.configured = false; self.compiled.reset(); self.usage = "";
    self.replacing = true;
    return true;
}

void Ap01Business::newContext(char (&out)[BS_CTX_LEN]) {
    ++contextCounter;
    const char *hex = "0123456789abcdef";
    for (unsigned i = 0; i < 16; ++i) {
        out[i] = hex[(boot >> (60 - 4 * i)) & 15];
        out[i + 16] = hex[(contextCounter >> (60 - 4 * i)) & 15];
    }
    out[32] = 0;
}
bool Ap01Business::loadActive(uint64_t now) {
    configured = store.bsProfile(profile);
    if (!configured) { compiled.reset(); return false; }
    if (!compiled) compiled.reset(new (std::nothrow) CtTemplate);
    String error;
    if (!compiled || !store.bsLoadCompiled(profile.initial, *compiled, error)) {
        configured = false; compiled.reset(); return false;
    }
    sequence.beginContext(now, 1); usage = "";
    return true;
}
bool Ap01Business::begin(uint64_t now) {
    if (!store.bsBegin()) return !strcmp(store.bsLastError(), "unconfigured");
    if (!store.bsProfile(profile)) return false;
    char next[BS_CTX_LEN]; newContext(next); String error;
    // Retain uniqueness even when a synthetic entropy fixture repeats after
    // reboot. The durable context already carries this boot's counter.
    if (strlen(profile.contextId) == 32 && !memcmp(next, profile.contextId, 16)) {
        uint64_t previous = 0; bool valid = true;
        for (unsigned i = 16; i < 32; ++i) {
            char c = profile.contextId[i];
            unsigned digit = c >= '0' && c <= '9' ? c-'0' : c >= 'a' && c <= 'f' ? c-'a'+10 : 16;
            if (digit == 16) { valid = false; break; }
            previous = (previous << 4) | digit;
        }
        if (valid) {
            if (previous == UINT64_MAX) return false;
            contextCounter = previous; newContext(next);
        }
    }
    return store.bsSetActive(profile.initial, next, error) && loadActive(now) && render();
}
bool Ap01Business::render() {
    auto failed=[this](){displayFailed(generation);return false;};
    if (!configured || !compiled) return failed();
    std::unique_ptr<uint8_t[]> frame(new (std::nothrow) uint8_t[38400]);
    if (!frame || generation == UINT32_MAX) return failed();
    renderer.Paint_NewImage(frame.get(), 320, 240, 0, 0);
    renderer.Paint_SetScale(16); renderer.Paint_Clear(0);
    TplEnv env; env.channel = "WIFI"; env.battery = -1; env.mode = "always_on";
    env.syncHHMM = syncTime.length() ? syncTime : "--:--";
    if (!renderer.tplDrawCt(*compiled, usage, env)) return failed();
    uint32_t crc = v2Crc32(frame.get(), 38400);
    if (!ui.present || !ui.present(ui.context, frame.get(), generation + 1)) return failed();
    frame.release(); ++generation; frameCrc = crc; return true;
}
void Ap01Business::clock(const char *hhmm) {
    if(!strcmp(currentTime.c_str(),hhmm))return;
    currentTime=hhmm;
    if(configured&&compiled)for(unsigned i=0;i<compiled->reqCount;++i) {
        const char *path=compiled->reqs[i].path;
        if(!strcmp(path,"device.now")||!strcmp(path,"device.date")){render();break;}
    }
}
bool Ap01Business::localActivate(uint8_t index, uint64_t now) {
    if (storageUnknown || !configured || index >= profile.count) return false;
    char next[BS_CTX_LEN]; newContext(next); String error;
    return store.bsSetActive(index, next, error) && loadActive(now) && render();
}
void Ap01Business::displayFeedback(JsonDocument &out, const char *state) {
    out["active_template_id"] = configured ? profile.ids[profile.initial] : "";
    out["display_reason"] = !strcmp(state,"pending")
        ? (ui.displayReason ? ui.displayReason(ui.context) : "waiting_for_draw")
        : !strcmp(state,"displayed") ? "current_template_drawn"
        : !strcmp(state,"failed") ? "render_or_ui_allocation" : "unchanged";
}
void Ap01Business::status(JsonDocument &out) {
    out["result"] = "ok"; out["configured"] = configured;
    out["firmware_target"] = FW_TARGET_ID; out["fw_target"] = FW_TARGET_ID;
    out["render_target"] = RENDER_TARGET_ID; out["compiler_abi"] = CT_ABI;
    out["width"] = 320; out["height"] = 240; out["pixel_format"] = "4bpp";
    out["colors"] = "ap01-palette-v2"; out["hardware_verified"] = false;
    out["wifi"] = true; out["ble"] = false; out["partial"] = false;
    out["power_mode"] = "always_on"; out["power_plan"] = false; out["max_light_s"] = 0;
    out["max_templates"] = 8; out["max_fields"] = 64; out["max_snapshot_bytes"] = 4048;
    out["storage_state"] = storageUnknown ? "unknown" : "ready";
    out["bundle_storage_protocol"] = 2; out["bundle_font_protocol"] = 3;
    out["bundle_compression"] = store.bsCompressionProtocol();
    out["max_bundle_bytes"] = store.bsMaxBundleBytes(); out["free_bytes"] = store.bsFreeBytes();
    out["max_bundle_decoded_bytes"] = store.bsDecodedLimit();
    out["retention"].add("ram"); out["retention"].add("flash");
    out["active_context_id"] = profile.contextId; out["context"] = profile.contextId;
    out["job_id"] = profile.jobId; out["commit_seq"] = store.bsCommitSeq();
    out["applied_seq"] = sequence.appliedSeq(); out["data_crc"] = sequence.appliedCrc();
    const char *storeError=store.bsLastError();
    bool recovery=!configured&&strcmp(storeError,"unconfigured");
    out["bundle_state"]=configured?"configured":recovery?"recovery":"unconfigured";
    if(recovery){out["bundle_error"]=*storeError?storeError:"restore_failed";out["bundle_action"]="publish_complete_bundle";}
    bool failed=generation==failedGeneration.load(std::memory_order_acquire);
    out["display_state"] = !configured ? (recovery?"failed":"builtin") : failed?"failed":
        generation && generation == displayedGeneration.load(std::memory_order_acquire) ? "displayed" : "pending";
    displayFeedback(out, out["display_state"] | "unchanged");
    if(configured&&failed){out["display_error"]="render_or_ui_allocation";out["display_action"]="refresh_or_publish_complete_bundle";}
    out["frame_generation"] = generation; out["frame_crc"] = frameCrc;
    for (uint8_t i = 0; configured && i < profile.count; ++i) out["template_ids"].add(profile.ids[i]);
    if (configured && compiled) {
        out["active_template_id"] = profile.ids[profile.initial];
        v2WriteDataContract(out["data_contract"].to<JsonObject>(), *compiled);
    } else out["data_contract"] = nullptr;
}
int Ap01Business::command(uint16_t op, JsonDocument &request, const char *nonce, uint64_t now,
                           const uint8_t *body, size_t bytes, const char *requestId,
                           const char *offset, String &reply) {
    const char *error = nullptr, *display = "unchanged", *result = "applied";
    int status = 200;
    const char *operation = op == 2 ? "data" : op == 13 ? "activate" : "bundle";
    uint64_t seq = 0; bool includeSeq = false;
    if (storageUnknown && op == unknownOperation && unknownOwner == (request["bridge_id"] | "") &&
        unknownRequest == (request["request_id"] | "") && store.bsBegin()) {
        bool resolved = false;
        if (op == 12) {
            BsReceipt receipt;
            resolved = store.bsReceipt(receipt) && !strcmp(receipt.owner, unknownOwner.c_str()) &&
                !strcmp(receipt.request, unknownRequest.c_str());
        } else if (op == 13) {
            auto receipt = store.bsActivationReceipt();
            resolved = receipt && !strcmp(receipt->owner, unknownOwner.c_str()) &&
                !strcmp(receipt->request, unknownRequest.c_str());
        }
        if (resolved) { storageUnknown = false; loadActive(now); render(); }
    }
    if (storageUnknown) {
        reply = v2BuildAck("storage", "unknown", "unchanged", "flash", "reread_required",
                          -1, 0, nullptr, UINT32_MAX, FW_TARGET_ID);
        return 503;
    }
    const char *ackContext = profile.contextId;
    V2BundleFingerprint fingerprint{nullptr, nullptr, 0, 0, nullptr};
    if (op == 10) {
        auto decision = v2DecideBundleBegin(request, rx, fingerprint, nonce, now, &services);
        if (decision.action == V2_BUNDLE_BEGIN_REJECT) error = decision.error;
        else if (decision.action == V2_BUNDLE_BEGIN_START) {
            String storageError;
            if (!store.bsStartReceive(decision.candidate.length, storageError,
                decision.candidate.owner, decision.candidate.request)) {
                reply = v2BuildAck(operation, "rejected", display, "flash", storageError.c_str(),
                    -1, 0, ackContext, UINT32_MAX, FW_TARGET_ID); return 409;
            }
            rx = decision.candidate;
        }
        if (!error && decision.action != V2_BUNDLE_BEGIN_REPLAY) {
            LocalJsonAllocator allocator; JsonDocument ack(&allocator);
            ack["result"] = "applied"; ack["next_offset"] = decision.nextOffset;
            reply = ""; serializeJson(ack, reply); return 200;
        }
        if (decision.action == V2_BUNDLE_BEGIN_REPLAY) ackContext = decision.replayContext;
    } else if (op == 11) {
        if (strcmp(rx.owner, request["bridge_id"] | "")) {
            reply = v2BuildAck(operation, "rejected", "unchanged", "flash", "rx_owner",
                              -1, 0, ackContext, UINT32_MAX, FW_TARGET_ID);
            return 409;
        }
        auto start = v2DecideBundleChunkStart(rx, requestId, nonce, offset, now);
        if (!start.allowed) error = start.error;
        else {
            auto write = v2DecideBundleChunkWrite(rx, start.offset, start.replay, 0, bytes);
            if (!write.allowed) error = write.error;
            else {
                File file = fs.open(store.bsReceivePath(), start.replay ? "r" : "a");
                bool ok = bool(file);
                if (start.replay) {
                    ok = ok && file.seek(start.offset);
                    for (size_t i = 0; ok && i < bytes; ++i) ok = file.read() == body[i];
                } else ok = ok && file.write(body, bytes) == bytes;
                file.close();
                if (!ok) { error = start.replay ? "chunk_conflict" : "write"; rx.deadline = 0; }
                else {
                    auto end = v2DecideBundleChunkEnd(rx, start.offset, start.replay, bytes, now);
                    if (!end.allowed) error = end.error;
                    else {
                        rx.offset = end.nextOffset;
                        LocalJsonAllocator allocator; JsonDocument ack(&allocator);
                        ack["result"] = "applied"; ack["next_offset"] = end.nextOffset;
                        reply = ""; serializeJson(ack, reply); return 200;
                    }
                }
            }
        }
    } else if (op == 12) {
        auto decision = v2DecideBundleCommit(request, rx, fingerprint, nonce, now,
            store.bsReceivePath(), request["bridge_id"] | "", &services);
        if (decision.action == V2_BUNDLE_COMMIT_REJECT) error = decision.error;
        else if (decision.action == V2_BUNDLE_COMMIT_REPLAY) ackContext = decision.replayContext;
        else if (decision.action == V2_BUNDLE_COMMIT_INSTALL) {
            char next[BS_CTX_LEN]; newContext(next); String storageError;
            File file = fs.open(store.bsReceivePath(), "r");
            replacing = false;
            if (!store.bsInstall(file, decision.length, decision.crc, FW_TARGET_ID,
                RENDER_TARGET_ID, next, storageError, nullptr,
                configured ? profile.ids[profile.initial] : nullptr)) {
                if (!replacing) {
                    if (storageError == "ui_busy") render();
                    reply = v2BuildAck(operation, "rejected", "unchanged", "flash", storageError.c_str(),
                        -1, 0, profile.contextId, UINT32_MAX, FW_TARGET_ID); return 409;
                }
                // A lost post-rename readback is not proof of rejection. Resolve
                // against the original durable receipt before reporting success.
                bool recovered = store.bsBegin();
                BsReceipt saved;
                bool committed = recovered && store.bsReceipt(saved) &&
                    !strcmp(saved.owner, request["bridge_id"] | "") &&
                    !strcmp(saved.request, request["request_id"] | "") &&
                    saved.length == decision.length && saved.crc == decision.crc;
                configured = recovered && loadActive(now);
                if (!configured) { compiled.reset(); if (ui.present) ui.present(ui.context, nullptr, 0); }
                else render();
                storageUnknown = !committed && storageError == "commit";
                if (storageUnknown) {
                    unknownOwner = request["bridge_id"] | ""; unknownRequest = request["request_id"] | "";
                    unknownOperation = op;
                }
                reply = v2BuildAck(operation, committed ? "applied" : storageUnknown ? "unknown" : "rejected",
                    configured ? "pending" : "failed", "flash", committed ? nullptr : storageUnknown ? "commit_unknown" : storageError.c_str(),
                    -1, 0, committed ? saved.context : nullptr, UINT32_MAX, FW_TARGET_ID);
                return committed ? 200 : storageUnknown ? 503 : 409;
            }
            if (!loadActive(now) || !render()) { error = "display_failed"; display = "failed"; }
            else display = "pending";
            ackContext = profile.contextId; rx = V2BundleRx{};
        }
    } else if (op == 13) {
        auto receipt = store.bsActivationReceipt();
        auto decision = v2DecideActivate(request, configured, profile, receipt ? receipt->request : "",
            receipt ? receipt->owner : "", receipt ? receipt->templateId : "",
            receipt ? receipt->expected : "", receipt ? receipt->context : "");
        if (decision.action == V2_ACTIVATE_REJECT) error = decision.error;
        else if (decision.action == V2_ACTIVATE_REPLAY) ackContext = decision.context;
        else {
            BsActivationReceipt transaction;
            if (!decision.owner.length() || decision.owner.length() >= sizeof(transaction.owner) ||
                !decision.request.length() || decision.request.length() >= sizeof(transaction.request) ||
                decision.templateId.length() >= sizeof(transaction.templateId) ||
                decision.expected.length() >= sizeof(transaction.expected)) error = "identity";
            else {
                strcpy(transaction.owner, decision.owner.c_str()); strcpy(transaction.request, decision.request.c_str());
                strcpy(transaction.templateId, decision.templateId.c_str()); strcpy(transaction.expected, decision.expected.c_str());
                newContext(transaction.context); String storageError;
                bool saved = store.bsSetActive(decision.index, transaction.context, storageError, &transaction);
                if (!saved) {
                    store.bsBegin();
                    auto readback = store.bsActivationReceipt();
                    saved = readback && !memcmp(readback, &transaction, sizeof(transaction));
                }
                if (!saved) {
                    storageUnknown = true;
                    unknownOwner = decision.owner; unknownRequest = decision.request; unknownOperation = op;
                    reply = v2BuildAck(operation, "unknown", "unchanged", "flash", "commit_unknown",
                        -1, 0, nullptr, UINT32_MAX, FW_TARGET_ID); return 503;
                }
                display = loadActive(now) && render() ? "pending" : "failed";
            }
        }
    } else if (op == 2) {
        String input; serializeJson(request, input);
        auto decision = v2DecideData(configured, compiled.get(), input, request["seq"] | uint64_t(0),
            profile.contextId, sequence);
        seq = request["seq"] | uint64_t(0); includeSeq = decision.includeContext;
        if (strcmp(decision.result, "applied")) {
            reply = v2BuildAck(operation, decision.result, decision.display, "ram", decision.error.c_str(),
                seq, 0, profile.contextId, UINT32_MAX, FW_TARGET_ID, true); return 409;
        }
        if (decision.firstApplied) {
            usage = decision.usage; syncTime = currentTime;
            display = render() ? "pending" : "failed";
        }
    } else error = op == 3 ? "power_plan_unsupported" : "unsupported";
    if (error) { result = "rejected"; status = 409; }
    reply = v2BuildAck(operation, result, display, op == 2 ? "ram" : "flash", error,
        includeSeq ? seq : -1, 0, ackContext, UINT32_MAX, FW_TARGET_ID, includeSeq);
    LocalJsonAllocator feedbackAllocator; JsonDocument feedback(&feedbackAllocator);
    if (!deserializeJson(feedback,reply)) {
        displayFeedback(feedback,display); reply=""; serializeJson(feedback,reply);
    }
    return status;
}
