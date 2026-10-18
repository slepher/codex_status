#pragma once
#include "ap01_binding.h"
#ifdef __cplusplus
extern "C" {
#endif
// Bounded JSON adapter. The caller supplies the device's learned Wi-Fi MAC.
// No HTTP endpoint here approves a binding: only the UI button API does.
int ap01PairingHttp(Ap01Bindings *,const char *mac,const char *method,const char *path,
                   const char *token,const char *body,uint64_t now_ms,char *out,size_t capacity);
int ap01PairingUi(Ap01Bindings *,uint64_t now_ms,char *out,size_t capacity);
#ifdef __cplusplus
}
#endif
