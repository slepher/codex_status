#pragma once
#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>
#ifdef __cplusplus
extern "C" {
#endif

// The port must atomically replace and read back the explicit APB1 byte stream.
// Neither the core nor the on-disk representation contains native pointers.
typedef struct {
    void *user;
    void *(*allocate)(size_t);
    void (*release)(void *);
    int (*save_verified)(void *,const uint8_t *,size_t);
} Ap01BindingIo;

typedef struct {
    char bridge[65], name[97], code[5], token[33], request[65];
} Ap01Binding;

typedef struct {
    Ap01BindingIo io;
    Ap01Binding *bindings;
    unsigned count;
    uint64_t boot_id;
    uint32_t revision;
    Ap01Binding pending; // At most one in-flight physical-confirmation request.
    uint64_t deadline, displayed_event, pressed_event;
    unsigned state; // 0 empty; 1 pending_display; 2 awaiting; 3 cancelled; 4 expired; 5 rejected.
    char terminal_error[25];
    char owner[65];
    uint64_t owner_until;
    bool maintenance, storage_unknown;
} Ap01Bindings;

bool ap01BindingsInit(Ap01Bindings *,const Ap01BindingIo *,uint64_t boot_id,
                      const uint8_t *stored,size_t length);
void ap01BindingsFree(Ap01Bindings *);
const char *ap01PairBegin(Ap01Bindings *,const Ap01Binding *,uint64_t expected_boot,
                         uint32_t expected_revision,uint64_t now_ms);
const char *ap01PairResult(Ap01Bindings *,const char *request,const char *token,uint64_t now_ms);
const char *ap01PairCancel(Ap01Bindings *,const char *request,const char *token,uint64_t now_ms);
bool ap01BindingsResolve(Ap01Bindings *,const uint8_t *,size_t);
bool ap01PairDisplayed(Ap01Bindings *,const char *request,uint32_t revision,uint64_t event);
const char *ap01PairButton(Ap01Bindings *,bool pressed,uint64_t event,uint64_t now_ms);
const char *ap01BindingPrincipal(const Ap01Bindings *,const char *token);
const char *ap01BindingClaim(Ap01Bindings *,const char *bridge,const char *token,uint64_t now_ms);
uint32_t ap01BindingCrc(const uint8_t *,size_t);

#ifdef __cplusplus
}
#endif
