#include "app_abi.h"

#include <stdint.h>
#include <stddef.h>

static void ap01_zero(void *destination, uint32_t length)
{
    uint8_t *out = (uint8_t *)destination;
    uint32_t index;

    for (index = 0; index < length; ++index) {
        out[index] = 0;
    }
}

static void ap01_copy(void *destination, const void *source, uint32_t length)
{
    uint8_t *out = (uint8_t *)destination;
    const uint8_t *in = (const uint8_t *)source;
    uint32_t index;

    for (index = 0; index < length; ++index) {
        out[index] = in[index];
    }
}

static uint32_t ap01_read_u32(const uint8_t *bytes, uint32_t offset)
{
    return ((uint32_t)bytes[offset]) |
           ((uint32_t)bytes[offset + 1u] << 8) |
           ((uint32_t)bytes[offset + 2u] << 16) |
           ((uint32_t)bytes[offset + 3u] << 24);
}

static uint32_t ap01_crc32(const uint8_t *bytes, uint32_t length,
                           uint32_t zero_begin, uint32_t zero_end)
{
    uint32_t crc = UINT32_C(0xffffffff);
    uint32_t index;

    for (index = 0; index < length; ++index) {
        uint32_t bit;
        uint8_t value = bytes[index];

        if (index >= zero_begin && index < zero_end) {
            value = 0;
        }
        crc ^= (uint32_t)value;
        for (bit = 0; bit < 8u; ++bit) {
            uint32_t mask = (uint32_t)-(int32_t)(crc & 1u);
            crc = (crc >> 1) ^ (UINT32_C(0xedb88320) & mask);
        }
    }
    return ~crc;
}

static int ap01_add_u32(uint32_t left, uint32_t right, uint32_t *out)
{
    if (left > UINT32_MAX - right) {
        return 0;
    }
    *out = left + right;
    return 1;
}

static int ap01_bytes_equal(const uint8_t *left, const uint8_t *right, uint32_t length)
{
    uint32_t index;

    for (index = 0; index < length; ++index) {
        if (left[index] != right[index]) {
            return 0;
        }
    }
    return 1;
}

static const uint8_t ap01_expected_0031_sha[AP01_EXPECTED_0031_SHA_BYTES] = {
    0x8a, 0x72, 0x1f, 0xc8, 0xef, 0x25, 0x45, 0x8d,
    0x41, 0x5b, 0x24, 0x60, 0xe4, 0xa2, 0x51, 0xe0,
    0x50, 0x3a, 0x82, 0xf7, 0x74, 0x3f, 0xdf, 0xf8,
    0x5b, 0x12, 0x61, 0x21, 0x90, 0xe5, 0xc1, 0xcb
};

ap01_error_phase ap01_error_get_phase(ap01_error error)
{
    switch (error) {
    case AP01_OK:
        return AP01_PHASE_NONE;
    case AP01_ERR_MAGIC:
    case AP01_ERR_FORMAT:
    case AP01_ERR_HEADER_LENGTH:
    case AP01_ERR_TOTAL_LENGTH:
    case AP01_ERR_RESERVED:
        return AP01_PHASE_HEADER;
    case AP01_ERR_TRUNCATED:
    case AP01_ERR_OFFSET:
    case AP01_ERR_TEXT:
    case AP01_ERR_ENTRY:
    case AP01_ERR_ALIGNMENT:
    case AP01_ERR_DATA_END:
    case AP01_ERR_ADDRESS:
        return AP01_PHASE_BOUNDS;
    case AP01_ERR_HEADER_CRC:
    case AP01_ERR_PAYLOAD_CRC:
    case AP01_ERR_BASE_SHA:
        return AP01_PHASE_INTEGRITY;
    case AP01_ERR_ABI_VERSION:
    case AP01_ERR_FLAGS:
        return AP01_PHASE_ABI;
    case AP01_ERR_STATE_BUDGET:
        return AP01_PHASE_BUDGET;
    case AP01_ERR_SERVICES:
    case AP01_ERR_ALLOC:
    case AP01_ERR_INIT:
    case AP01_ERR_INIT_STOP:
    case AP01_ERR_STEP:
    case AP01_ERR_STOP:
    case AP01_ERR_NOT_RUNNING:
    case AP01_ERR_REENTRANT:
    case AP01_ERR_RUNTIME_STATE:
        return AP01_PHASE_RUNTIME;
    case AP01_ERR_NULL:
    default:
        return AP01_PHASE_NONE;
    }
}

ap01_error ap01_decode_validate(const uint8_t *bytes, uint32_t length,
                                uint32_t expected_package_va, uint32_t state_budget,
                                ap01_view *out_view)
{
    uint32_t total_len;
    uint32_t text_offset;
    uint32_t text_len;
    uint32_t text_end;
    uint32_t data_offset;
    uint32_t data_len;
    uint32_t data_end;
    uint32_t bss_len;
    uint32_t state_size;
    uint32_t init_offset;
    uint32_t step_offset;
    uint32_t stop_offset;
    uint32_t index;

    if (bytes == NULL || out_view == NULL) {
        return AP01_ERR_NULL;
    }
    if (length < AP01_PACKAGE_HEADER_BYTES) {
        return AP01_ERR_TRUNCATED;
    }
    if (ap01_read_u32(bytes, 0) != AP01_PACKAGE_MAGIC) {
        return AP01_ERR_MAGIC;
    }
    if (ap01_read_u32(bytes, 4) != AP01_PACKAGE_FORMAT) {
        return AP01_ERR_FORMAT;
    }
    if (ap01_read_u32(bytes, 8) != AP01_PACKAGE_HEADER_BYTES) {
        return AP01_ERR_HEADER_LENGTH;
    }

    total_len = ap01_read_u32(bytes, 12);
    if (total_len != length || total_len < AP01_PACKAGE_HEADER_BYTES) {
        return AP01_ERR_TOTAL_LENGTH;
    }
    if (ap01_read_u32(bytes, 16) != AP01_APP_ABI_VERSION) {
        return AP01_ERR_ABI_VERSION;
    }
    if (ap01_read_u32(bytes, 20) != AP01_PACKAGE_FLAGS_XIP) {
        return AP01_ERR_FLAGS;
    }
    if (ap01_read_u32(bytes, 104) != 0 || ap01_read_u32(bytes, 108) != 0) {
        return AP01_ERR_RESERVED;
    }
    if (ap01_read_u32(bytes, 24) != expected_package_va) {
        return AP01_ERR_ADDRESS;
    }
    if (total_len > UINT32_MAX - expected_package_va) {
        return AP01_ERR_ADDRESS;
    }

    text_offset = ap01_read_u32(bytes, 28);
    text_len = ap01_read_u32(bytes, 32);
    init_offset = ap01_read_u32(bytes, 36);
    step_offset = ap01_read_u32(bytes, 40);
    stop_offset = ap01_read_u32(bytes, 44);
    data_offset = ap01_read_u32(bytes, 48);
    data_len = ap01_read_u32(bytes, 52);
    bss_len = ap01_read_u32(bytes, 56);

    if (text_offset != AP01_PACKAGE_HEADER_BYTES || text_len == 0) {
        return AP01_ERR_TEXT;
    }
    if (!ap01_add_u32(text_offset, text_len, &text_end) ||
        text_end > total_len || data_offset < text_end || data_offset > total_len) {
        return AP01_ERR_OFFSET;
    }
    if ((data_offset & 3u) != 0) {
        return AP01_ERR_ALIGNMENT;
    }
    if (!ap01_add_u32(data_offset, data_len, &data_end) || data_end != total_len) {
        return AP01_ERR_DATA_END;
    }
    if (init_offset >= text_len || step_offset >= text_len || stop_offset >= text_len ||
        (init_offset & 1u) != 0 || (step_offset & 1u) != 0 || (stop_offset & 1u) != 0) {
        return AP01_ERR_ENTRY;
    }
    if (ap01_read_u32(bytes, 60) != AP01_PACKAGE_DATA_ALIGNMENT) {
        return AP01_ERR_ALIGNMENT;
    }
    if (!ap01_add_u32(data_len, bss_len, &state_size) || state_size > state_budget) {
        return AP01_ERR_STATE_BUDGET;
    }
    for (index = text_end; index < data_offset; ++index) {
        if (bytes[index] != 0) {
            return AP01_ERR_OFFSET;
        }
    }
    if (ap01_crc32(bytes + AP01_PACKAGE_HEADER_BYTES,
                   total_len - AP01_PACKAGE_HEADER_BYTES, 0, 0) != ap01_read_u32(bytes, 64)) {
        return AP01_ERR_PAYLOAD_CRC;
    }
    if (ap01_crc32(bytes, AP01_PACKAGE_HEADER_BYTES, 68, 72) != ap01_read_u32(bytes, 68)) {
        return AP01_ERR_HEADER_CRC;
    }
    if (!ap01_bytes_equal(bytes + 72, ap01_expected_0031_sha,
                          AP01_EXPECTED_0031_SHA_BYTES)) {
        return AP01_ERR_BASE_SHA;
    }

    out_view->bytes = bytes;
    out_view->length = total_len;
    out_view->package_link_va = expected_package_va;
    out_view->text = bytes + text_offset;
    out_view->text_offset = text_offset;
    out_view->text_len = text_len;
    out_view->data_offset = data_offset;
    out_view->data_init = bytes + data_offset;
    out_view->data_len = data_len;
    out_view->bss_len = bss_len;
    out_view->state_len = state_size;
    out_view->init_offset = init_offset;
    out_view->step_offset = step_offset;
    out_view->stop_offset = stop_offset;
    return AP01_OK;
}

static uint32_t ap01_api_min_bytes(void)
{
    return (uint32_t)(offsetof(ap01_api, context) + sizeof(((ap01_api *)0)->context));
}

static uint32_t ap01_services_min_bytes(void)
{
    return (uint32_t)(offsetof(ap01_services, api) + sizeof(((ap01_services *)0)->api));
}

static int ap01_runtime_is_empty(const ap01_runtime *runtime)
{
    return runtime->state == NULL && runtime->allocation == NULL &&
           runtime->started == 0 && runtime->stop_pending == 0 &&
           runtime->callback_active == 0;
}

static void ap01_runtime_release_state(ap01_runtime *runtime)
{
    if (runtime->allocation != NULL) {
        runtime->services.release(runtime->allocation);
    }
    ap01_zero(runtime, (uint32_t)sizeof(*runtime));
}

static ap01_error ap01_call_stop(ap01_runtime *runtime)
{
    int callback_error;

    if (runtime->callback_active != 0) {
        return AP01_ERR_REENTRANT;
    }
    runtime->callback_active = 1;
    callback_error = runtime->entries.stop(runtime->services.api,
                                           runtime->state, runtime->state_size);
    runtime->callback_active = 0;
    runtime->last_callback_error = (uint32_t)callback_error;
    if (callback_error != 0) {
        runtime->stop_pending = 1;
        runtime->started = 0;
        return AP01_ERR_STOP;
    }
    ap01_runtime_release_state(runtime);
    return AP01_OK;
}

ap01_error ap01_runtime_start(const ap01_view *view,
                              const ap01_entries *entries,
                              const ap01_services *services,
                              ap01_runtime *runtime)
{
    uint32_t index;
    uint32_t state_size;
    uint32_t allocation_size;
    uintptr_t raw_address;
    uintptr_t aligned_address;
    int callback_error;

    if (view == NULL || entries == NULL || services == NULL || runtime == NULL) {
        return AP01_ERR_NULL;
    }
    if (!ap01_runtime_is_empty(runtime)) {
        return AP01_ERR_RUNTIME_STATE;
    }
    if (entries->init == NULL || entries->step == NULL || entries->stop == NULL ||
        services->abi_version != AP01_APP_ABI_VERSION ||
        services->struct_bytes < ap01_services_min_bytes() ||
        services->allocate == NULL || services->release == NULL || services->api == NULL ||
        services->api->abi_version != AP01_APP_ABI_VERSION ||
        services->api->struct_bytes < ap01_api_min_bytes() ||
        services->api->report == NULL) {
        return AP01_ERR_SERVICES;
    }
    state_size = view->state_len;
    if (view->data_len != 0 && view->data_init == NULL) {
        return AP01_ERR_RUNTIME_STATE;
    }
    if (view->data_len > UINT32_MAX - view->bss_len ||
        state_size != view->data_len + view->bss_len || state_size > services->state_budget) {
        return AP01_ERR_STATE_BUDGET;
    }
    ap01_zero(runtime, (uint32_t)sizeof(*runtime));
    runtime->entries = *entries;
    runtime->services = *services;
    runtime->state_size = state_size;

    if (state_size != 0) {
        if (state_size > UINT32_MAX - 15u) {
            ap01_zero(runtime, (uint32_t)sizeof(*runtime));
            return AP01_ERR_STATE_BUDGET;
        }
        allocation_size = state_size + 15u;
        runtime->allocation = services->allocate(allocation_size);
        if (runtime->allocation == NULL) {
            ap01_zero(runtime, (uint32_t)sizeof(*runtime));
            return AP01_ERR_ALLOC;
        }
        raw_address = (uintptr_t)runtime->allocation;
        if (raw_address > UINTPTR_MAX - (uintptr_t)15u) {
            services->release(runtime->allocation);
            ap01_zero(runtime, (uint32_t)sizeof(*runtime));
            return AP01_ERR_ALLOC;
        }
        aligned_address = (raw_address + (uintptr_t)15u) & ~(uintptr_t)15u;
        runtime->state = (void *)aligned_address;
        runtime->allocation_size = allocation_size;
        if (view->data_len != 0) {
            ap01_copy(runtime->state, view->data_init, view->data_len);
        }
        for (index = view->data_len; index < state_size; ++index) {
            ((uint8_t *)runtime->state)[index] = 0;
        }
    }

    runtime->callback_active = 1;
    callback_error = entries->init(services->api, runtime->state, state_size);
    runtime->callback_active = 0;
    runtime->last_callback_error = (uint32_t)callback_error;
    if (callback_error != 0) {
        runtime->stop_pending = 1;
        if (ap01_call_stop(runtime) != AP01_OK) {
            return AP01_ERR_INIT_STOP;
        }
        return AP01_ERR_INIT;
    }
    runtime->started = 1;
    return AP01_OK;
}

ap01_error ap01_runtime_step(ap01_runtime *runtime, uint32_t event)
{
    int callback_error;

    if (runtime == NULL) {
        return AP01_ERR_NULL;
    }
    if (runtime->callback_active != 0) {
        return AP01_ERR_REENTRANT;
    }
    if (runtime->started == 0 || runtime->stop_pending != 0) {
        return AP01_ERR_NOT_RUNNING;
    }
    runtime->callback_active = 1;
    callback_error = runtime->entries.step(runtime->services.api, runtime->state,
                                           runtime->state_size, event);
    runtime->callback_active = 0;
    runtime->last_callback_error = (uint32_t)callback_error;
    if (callback_error != 0) {
        return AP01_ERR_STEP;
    }
    return AP01_OK;
}

ap01_error ap01_runtime_stop(ap01_runtime *runtime)
{
    if (runtime == NULL) {
        return AP01_ERR_NULL;
    }
    if (runtime->callback_active != 0) {
        return AP01_ERR_REENTRANT;
    }
    if (runtime->started == 0 && runtime->stop_pending == 0) {
        return AP01_ERR_NOT_RUNNING;
    }
    return ap01_call_stop(runtime);
}
