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
namespace {
constexpr size_t maxUsage = 6144;
constexpr size_t header = 18; // magic/version, bundle CRC, source CRC, slot, HH:MM
constexpr char snapshotPath[] = "/usage.bin";
constexpr char pendingPath[] = "/usage.tmp";
void put32(uint8_t *p,uint32_t value) {
    for(unsigned i=0;i<4;++i)p[i]=uint8_t(value>>(8*i));
}
uint32_t get32(const uint8_t *p) {
    return uint32_t(p[0])|(uint32_t(p[1])<<8)|(uint32_t(p[2])<<16)|(uint32_t(p[3])<<24);
}
}
bool Ap01Business::saveSnapshot() {
    size_t n=usage.length();
    if(!n||n>maxUsage)return false;
    size_t total=header+n+4;
    std::unique_ptr<uint8_t[]> record(new(std::nothrow) uint8_t[total]);
    if(!record)return false;
    auto *p=record.get();
    memcpy(p,"USA1",4);put32(p+4,profile.bundleCrc);put32(p+8,compiled->sourceCrc);
    p[12]=profile.initial;
    memcpy(p+13,syncTime.length()==5?syncTime.c_str():"--:--",5);
    memcpy(p+header,usage.c_str(),n);
    put32(p+header+n,v2Crc32(p,header+n));
    File file=fs.open(pendingPath,"w");
    if(!file||file.write(p,total)!=total)return false;
    file.close();
    file=fs.open(pendingPath,"r");
    if(!file||file.size()!=total)return false;
    uint8_t block[128];
    for(size_t at=0;at<total;){
        size_t len=total-at;if(len>sizeof(block))len=sizeof(block);
        if(file.read(block,len)!=len||memcmp(block,p+at,len))return false;
        at+=len;
    }
    file.close();
    return fs.rename(pendingPath,snapshotPath);
}
bool Ap01Business::restoreSnapshot() {
    File file=fs.open(snapshotPath,"r");
    size_t total=file.size();
    if(!file||total<=header+4||total>header+maxUsage+4)return false;
    std::unique_ptr<uint8_t[]> record(new(std::nothrow) uint8_t[total]);
    auto *p=record.get();
    if(!p||file.read(p,total)!=total||memcmp(p,"USA1",4)||
       get32(p+4)!=profile.bundleCrc||get32(p+8)!=compiled->sourceCrc||
       p[12]!=profile.initial||get32(p+total-4)!=v2Crc32(p,total-4))return false;
    if(!usage.concat(reinterpret_cast<const char*>(p+header),total-header-4))return false;
    syncTime.concat(reinterpret_cast<const char*>(p+13),5);
    return true;
}
bool Ap01Business::loadActive(uint64_t now) {
    configured = store.bsProfile(profile);
    if (!configured) { compiled.reset(); return false; }
    if (!compiled) compiled.reset(new (std::nothrow) CtTemplate);
    String error;
    if (!compiled || !store.bsLoadCompiled(profile.initial, *compiled, error)) {
        configured = false; compiled.reset(); return false;
    }
    sequence.beginContext(now, 1); usage = ""; syncTime = "";
    snapshotSaved=restoreSnapshot();
    if(!generation && snapshotSaved){bootSnapshot=true;bootUsageCrc=v2CrcOf(usage);}
    return true;
}
bool Ap01Business::begin(uint64_t now, bool readOnly) {
    writesEnabled.store(false,std::memory_order_release);
    auto finish=[&](bool okay){writesEnabled.store(okay&&!readOnly,std::memory_order_release);return okay;};
    if (!store.bsBegin(readOnly)) return finish(!strcmp(store.bsLastError(), "unconfigured"));
    if (!store.bsProfile(profile)) return false;
    if (readOnly) return loadActive(now) && render();
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
    if (!store.bsSetActive(profile.initial, next, error)) return false;
    resumeContextCommitted=true;
    bool okay=loadActive(now) && render();
    if(okay)resumeContextCommitted=false;
    return finish(okay);
}
const char *Ap01Business::writeReason() const {
    if(writeResumeFailed)return "write_restore_failed";
    if(resumeAfter)return "write_restore_retry";
    if(writesAllowed())return "writable";
    return previouslyWritable?"platform_readonly":"awaiting_write_approval";
}
bool Ap01Business::syncWritePermission(uint64_t now,bool allow) {
    if(!allow){
        writesEnabled.store(false,std::memory_order_release);
        resumeAfter=0;resumeRetries=0;resumeContextCommitted=false;
        return !writeResumeFailed;
    }
    if(writeResumeFailed)return false;
    if(writesAllowed())return true;
    if(now<resumeAfter)return true;
    struct Resume {Ap01Business *self;uint64_t now;} resume{this,now};
    auto restore=[](void *p)->int32_t{
        auto &r=*(Resume *)p;
        auto &business=*r.self;
        if(!business.resumeContextCommitted)return business.begin(r.now,false)?0:-1;
        // The context was durably committed. Replay only reads and presentation.
        if(!business.loadActive(r.now)||!business.render())return -1;
        business.resumeContextCommitted=false;
        business.writesEnabled.store(true,std::memory_order_release);
        return 0;
    };
    int32_t result=-1;
    int32_t admitted=0;
    if(ui.writeScope)admitted=ui.writeScope(ui.context,restore,&resume,&result);
    else result=restore(&resume);
    if(admitted==1){resumeAfter=now+1000;return true;}
    if(admitted==0&&result==0){previouslyWritable=true;resumeRetries=0;resumeAfter=0;return true;}
    if(admitted==0&&resumeContextCommitted&&resumeRetries<3){
        resumeAfter=now+(1000u<<resumeRetries++);return true;
    }
    writeResumeFailed=true;return false;
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
bool Ap01Business::localActivate(uint8_t index, uint64_t now, bool *retry) {
    if(retry)*retry=false;
    if (!writesAllowed() || storageUnknown || !configured || index >= profile.count) return false;
    struct Call {Ap01Business *self;uint8_t index;uint64_t now;} call{this,index,now};
    auto activate=[](void *p)->int32_t{
        auto &c=*(Call *)p;auto &s=*c.self;
        char next[BS_CTX_LEN];s.newContext(next);String error;
        return s.store.bsSetActive(c.index,next,error)&&s.loadActive(c.now)&&s.render()?0:-1;
    };
    if(!ui.writeScope)return activate(&call)==0;
    int32_t result=-1;
    int32_t admitted=ui.writeScope(ui.context,activate,&call,&result);
    if(retry)*retry=admitted==1;
    return admitted==0&&result==0;
}
void Ap01Business::displayFeedback(JsonDocument &out, const char *state) {
    out["active_template_id"] = configured ? profile.ids[profile.initial] : "";
    const char *reason=ui.displayReason?ui.displayReason(ui.context):"waiting_for_draw";
    out["display_reason"] = !strcmp(state,"pending")||(!strcmp(state,"displayed")&&!strcmp(reason,"ui_heartbeat_stale"))
        ? reason
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
    out["write_permission"] = writeReason();
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
    out["boot_snapshot_loaded"]=bootSnapshot;
    if(bootSnapshot)out["boot_usage_crc"]=bootUsageCrc;
    out["snapshot_saved"]=snapshotSaved;
    if(usage.length())out["usage_crc"]=v2CrcOf(usage);
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
    if (!writesAllowed()) {
        reply = v2BuildAck("storage", "rejected", "unchanged", "lifecycle", writeReason(),
                          -1, 0, nullptr, UINT32_MAX, FW_TARGET_ID);
        return 503;
    }
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
            usage = std::move(decision.usage); syncTime = currentTime;
            snapshotSaved=saveSnapshot();
            display = render() ? "pending" : "failed";
        } else if(!snapshotSaved && usage.length())snapshotSaved=saveSnapshot();
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
