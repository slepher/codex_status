#ifndef AP01_APP_ABI_H
#define AP01_APP_ABI_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#define AP01_PACKAGE_MAGIC UINT32_C(0x31585041)
#define AP01_PACKAGE_FORMAT UINT32_C(1)
#define AP01_PACKAGE_HEADER_BYTES UINT32_C(112)
#define AP01_APP_ABI_VERSION UINT32_C(1)
#define AP01_ABI_VERSION AP01_APP_ABI_VERSION
#define AP01_PACKAGE_FLAGS_XIP UINT32_C(1)
#define AP01_PACKAGE_DATA_ALIGNMENT UINT32_C(16)

#define AP01_EXPECTED_0031_SHA_BYTES UINT32_C(32)

typedef enum ap01_error_phase {
    AP01_PHASE_NONE = 0,
    AP01_PHASE_HEADER = 1,
    AP01_PHASE_BOUNDS = 2,
    AP01_PHASE_INTEGRITY = 3,
    AP01_PHASE_ABI = 4,
    AP01_PHASE_BUDGET = 5,
    AP01_PHASE_RUNTIME = 6
} ap01_error_phase;

typedef enum ap01_error {
    AP01_OK = 0,
    AP01_ERR_NULL = 1,
    AP01_ERR_TRUNCATED = 2,
    AP01_ERR_MAGIC = 3,
    AP01_ERR_FORMAT = 4,
    AP01_ERR_HEADER_LENGTH = 5,
    AP01_ERR_TOTAL_LENGTH = 6,
    AP01_ERR_ABI_VERSION = 7,
    AP01_ERR_FLAGS = 8,
    AP01_ERR_RESERVED = 9,
    AP01_ERR_OFFSET = 10,
    AP01_ERR_TEXT = 11,
    AP01_ERR_ENTRY = 12,
    AP01_ERR_ALIGNMENT = 13,
    AP01_ERR_DATA_END = 14,
    AP01_ERR_ADDRESS = 15,
    AP01_ERR_HEADER_CRC = 16,
    AP01_ERR_PAYLOAD_CRC = 17,
    AP01_ERR_BASE_SHA = 18,
    AP01_ERR_STATE_BUDGET = 19,
    AP01_ERR_SERVICES = 20,
    AP01_ERR_ALLOC = 21,
    AP01_ERR_INIT = 22,
    AP01_ERR_INIT_STOP = 23,
    AP01_ERR_STEP = 24,
    AP01_ERR_STOP = 25,
    AP01_ERR_NOT_RUNNING = 26,
    AP01_ERR_REENTRANT = 27,
    AP01_ERR_RUNTIME_STATE = 28
} ap01_error;

ap01_error_phase ap01_error_get_phase(ap01_error error);

typedef struct ap01_view {
    const uint8_t *bytes;
    uint32_t length;
    uint32_t package_link_va;
    const uint8_t *text;
    uint32_t text_offset;
    uint32_t text_len;
    uint32_t data_offset;
    const uint8_t *data_init;
    uint32_t data_len;
    uint32_t bss_len;
    uint32_t state_len;
    uint32_t init_offset;
    uint32_t step_offset;
    uint32_t stop_offset;
} ap01_view;

typedef struct ap01_api ap01_api;
typedef struct ap01_services ap01_services;
typedef struct ap01_runtime ap01_runtime;

typedef void (*ap01_report_fn)(void *context, uint32_t code, uint32_t value);

struct ap01_api {
    uint32_t abi_version;
    uint32_t struct_bytes;
    ap01_report_fn report;
    void *context;
};

typedef void *(*ap01_allocate_fn)(uint32_t size);
typedef void (*ap01_release_fn)(void *pointer);

struct ap01_services {
    uint32_t abi_version;
    uint32_t struct_bytes;
    ap01_allocate_fn allocate;
    ap01_release_fn release;
    uint32_t state_budget;
    ap01_api *api;
};

typedef int (*ap01_app_init_fn)(ap01_api *api, void *state, uint32_t state_size);
typedef int (*ap01_app_step_fn)(ap01_api *api, void *state, uint32_t state_size,
                                uint32_t event);
typedef int (*ap01_app_stop_fn)(ap01_api *api, void *state, uint32_t state_size);

/* API/service and entry pointers are trusted ROM bindings; package bytes never supply them. */
typedef struct ap01_entries {
    ap01_app_init_fn init;
    ap01_app_step_fn step;
    ap01_app_stop_fn stop;
} ap01_entries;

typedef ap01_entries ap01_app_entries;

/* These fields are caller-owned; the loader never allocates the runtime object. */
struct ap01_runtime {
    void *state;
    void *allocation;
    uint32_t state_size;
    uint32_t allocation_size;
    uint32_t started;
    uint32_t stop_pending;
    uint32_t callback_active;
    uint32_t last_callback_error;
    ap01_entries entries;
    ap01_services services;
};

ap01_error ap01_decode_validate(const uint8_t *bytes, uint32_t length,
                                uint32_t expected_package_va, uint32_t state_budget,
                                ap01_view *out_view);

ap01_error ap01_runtime_start(const ap01_view *view,
                              const ap01_entries *entries,
                              const ap01_services *services,
                              ap01_runtime *runtime);

ap01_error ap01_runtime_step(ap01_runtime *runtime, uint32_t event);
ap01_error ap01_runtime_stop(ap01_runtime *runtime);

#ifdef __cplusplus
}
#endif

#endif
