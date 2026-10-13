#include "app_abi.h"

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#define TEXT_OFFSET 112u
#define TEXT_LENGTH 6u
#define DATA_OFFSET 120u
#define DATA_LENGTH 4u
#define BSS_LENGTH 8u
#define PACKAGE_LENGTH 124u
#define TEST_VA UINT32_C(0x10000000)

static const uint8_t expected_sha[32] = {
    0x8a, 0x72, 0x1f, 0xc8, 0xef, 0x25, 0x45, 0x8d,
    0x41, 0x5b, 0x24, 0x60, 0xe4, 0xa2, 0x51, 0xe0,
    0x50, 0x3a, 0x82, 0xf7, 0x74, 0x3f, 0xdf, 0xf8,
    0x5b, 0x12, 0x61, 0x21, 0x90, 0xe5, 0xc1, 0xcb
};

static uint32_t read_u32(const uint8_t *bytes, uint32_t offset)
{
    return ((uint32_t)bytes[offset]) |
           ((uint32_t)bytes[offset + 1u] << 8) |
           ((uint32_t)bytes[offset + 2u] << 16) |
           ((uint32_t)bytes[offset + 3u] << 24);
}

static void write_u32(uint8_t *bytes, uint32_t offset, uint32_t value)
{
    bytes[offset] = (uint8_t)value;
    bytes[offset + 1u] = (uint8_t)(value >> 8);
    bytes[offset + 2u] = (uint8_t)(value >> 16);
    bytes[offset + 3u] = (uint8_t)(value >> 24);
}

static uint32_t crc32(const uint8_t *bytes, uint32_t length,
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
        crc ^= value;
        for (bit = 0; bit < 8u; ++bit) {
            uint32_t mask = (uint32_t)-(int32_t)(crc & 1u);
            crc = (crc >> 1) ^ (UINT32_C(0xedb88320) & mask);
        }
    }
    return ~crc;
}

static void finalize_package(uint8_t *package)
{
    write_u32(package, 64, crc32(package + TEXT_OFFSET, PACKAGE_LENGTH - TEXT_OFFSET, 0, 0));
    write_u32(package, 68, 0);
    write_u32(package, 68, crc32(package, TEXT_OFFSET, 68, 72));
}

static void make_package(uint8_t *package)
{
    uint32_t index;

    for (index = 0; index < PACKAGE_LENGTH; ++index) {
        package[index] = 0;
    }
    write_u32(package, 0, AP01_PACKAGE_MAGIC);
    write_u32(package, 4, AP01_PACKAGE_FORMAT);
    write_u32(package, 8, AP01_PACKAGE_HEADER_BYTES);
    write_u32(package, 12, PACKAGE_LENGTH);
    write_u32(package, 16, AP01_APP_ABI_VERSION);
    write_u32(package, 20, AP01_PACKAGE_FLAGS_XIP);
    write_u32(package, 24, TEST_VA);
    write_u32(package, 28, TEXT_OFFSET);
    write_u32(package, 32, TEXT_LENGTH);
    write_u32(package, 36, 0);
    write_u32(package, 40, 2);
    write_u32(package, 44, 4);
    write_u32(package, 48, DATA_OFFSET);
    write_u32(package, 52, DATA_LENGTH);
    write_u32(package, 56, BSS_LENGTH);
    write_u32(package, 60, AP01_PACKAGE_DATA_ALIGNMENT);
    for (index = 0; index < 32u; ++index) {
        package[72u + index] = expected_sha[index];
    }
    package[TEXT_OFFSET] = 0x13;
    package[TEXT_OFFSET + 1u] = 0x00;
    package[TEXT_OFFSET + 2u] = 0x85;
    package[TEXT_OFFSET + 3u] = 0x00;
    package[TEXT_OFFSET + 4u] = 0x67;
    package[TEXT_OFFSET + 5u] = 0x80;
    package[DATA_OFFSET] = 1;
    package[DATA_OFFSET + 1u] = 2;
    package[DATA_OFFSET + 2u] = 3;
    package[DATA_OFFSET + 3u] = 4;
    finalize_package(package);
}

static void expect_decode(const uint8_t *package, uint32_t length, uint32_t va,
                          uint32_t budget, ap01_error expected)
{
    ap01_view view;
    ap01_error actual = ap01_decode_validate(package, length, va, budget, &view);
    if (actual != expected) {
        fprintf(stderr, "expected decode error %d, got %d (phase %d)\n", expected,
                actual, ap01_error_get_phase(actual));
        abort();
    }
}

static uint8_t allocation_storage[256 + 16];
static uint32_t allocation_calls;
static uint32_t release_calls;
static uint32_t allocation_failure;

static void *test_allocate(uint32_t size)
{
    allocation_calls += 1;
    if (allocation_failure != 0 || size > sizeof(allocation_storage) - 3u) {
        return NULL;
    }
    return allocation_storage + 3u;
}

static void test_release(void *pointer)
{
    assert(pointer != NULL);
    release_calls += 1;
}

typedef struct test_context {
    ap01_runtime *runtime;
    uint32_t init_calls;
    uint32_t step_calls;
    uint32_t stop_calls;
    uint32_t init_result;
    uint32_t step_result;
    uint32_t stop_failures;
    ap01_error reentrant_result;
    uint32_t report_calls;
    uint32_t data_ok;
} test_context;

static void test_report(void *opaque, uint32_t code, uint32_t value)
{
    test_context *context = (test_context *)opaque;
    (void)code;
    (void)value;
    context->report_calls += 1;
    if (context->runtime != NULL) {
        context->reentrant_result = ap01_runtime_step(context->runtime, 99);
    }
}

static int test_init(ap01_api *api, void *state, uint32_t state_size)
{
    test_context *context = (test_context *)api->context;
    uint8_t *bytes = (uint8_t *)state;
    uint32_t index;

    context->init_calls += 1;
    context->data_ok = state_size == 12u && ((uintptr_t)state & 15u) == 0;
    for (index = 0; index < 4u && context->data_ok != 0; ++index) {
        context->data_ok = bytes[index] == index + 1u;
    }
    for (index = 4; index < 12u && context->data_ok != 0; ++index) {
        context->data_ok = bytes[index] == 0;
    }
    api->report(api->context, 1, state_size);
    return (int)context->init_result;
}

static int test_step(ap01_api *api, void *state, uint32_t state_size, uint32_t event)
{
    test_context *context = (test_context *)api->context;
    (void)state;
    (void)state_size;
    (void)event;
    context->step_calls += 1;
    return (int)context->step_result;
}

static int test_stop(ap01_api *api, void *state, uint32_t state_size)
{
    test_context *context = (test_context *)api->context;
    (void)state;
    (void)state_size;
    context->stop_calls += 1;
    if (context->stop_failures != 0) {
        context->stop_failures -= 1;
        return -1;
    }
    return 0;
}

static void test_parser_boundaries(void)
{
    uint8_t package[PACKAGE_LENGTH + 1u];
    uint8_t original[PACKAGE_LENGTH];
    uint32_t index;

    make_package(package);
    allocation_calls = 0;
    for (index = 0; index < PACKAGE_LENGTH; ++index) {
        original[index] = package[index];
    }
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_OK);
    expect_decode(package, 111, TEST_VA, 12, AP01_ERR_TRUNCATED);
    package[PACKAGE_LENGTH] = 0;
    expect_decode(package, PACKAGE_LENGTH + 1u, TEST_VA, 12, AP01_ERR_TOTAL_LENGTH);
    package[0] ^= 1;
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_MAGIC);
    package[0] ^= 1;
    package[104] = 1;
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_RESERVED);
    package[104] = 0;
    package[16] = 2;
    finalize_package(package);
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_ABI_VERSION);
    package[16] = 1;
    package[4] = 2;
    finalize_package(package);
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_FORMAT);
    package[4] = 1;
    package[20] = 2;
    finalize_package(package);
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_FLAGS);
    package[20] = 1;
    package[60] = 8;
    finalize_package(package);
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_ALIGNMENT);
    package[60] = 16;
    write_u32(package, 68, read_u32(package, 68) ^ 1u);
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_HEADER_CRC);
    for (index = 0; index < PACKAGE_LENGTH; ++index) {
        package[index] = original[index];
    }
    package[28] = 113;
    finalize_package(package);
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_TEXT);
    for (index = 0; index < PACKAGE_LENGTH; ++index) {
        package[index] = original[index];
    }
    package[36] = 1;
    finalize_package(package);
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_ENTRY);
    for (index = 0; index < PACKAGE_LENGTH; ++index) {
        package[index] = original[index];
    }
    package[36] = 6;
    finalize_package(package);
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_ENTRY);
    for (index = 0; index < PACKAGE_LENGTH; ++index) {
        package[index] = original[index];
    }
    package[TEXT_OFFSET + TEXT_LENGTH] = 9;
    finalize_package(package);
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_OFFSET);
    for (index = 0; index < PACKAGE_LENGTH; ++index) {
        package[index] = original[index];
    }
    write_u32(package, 52, 3);
    finalize_package(package);
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_DATA_END);
    for (index = 0; index < PACKAGE_LENGTH; ++index) {
        package[index] = original[index];
    }
    package[DATA_OFFSET] ^= 1;
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_PAYLOAD_CRC);
    for (index = 0; index < PACKAGE_LENGTH; ++index) {
        package[index] = original[index];
    }
    package[72] ^= 1;
    finalize_package(package);
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 12, AP01_ERR_BASE_SHA);
    for (index = 0; index < PACKAGE_LENGTH; ++index) {
        package[index] = original[index];
    }
    write_u32(package, 24, UINT32_MAX - PACKAGE_LENGTH + 2u);
    finalize_package(package);
    expect_decode(package, PACKAGE_LENGTH, UINT32_MAX - PACKAGE_LENGTH + 2u, 12,
                  AP01_ERR_ADDRESS);
    for (index = 0; index < PACKAGE_LENGTH; ++index) {
        package[index] = original[index];
    }
    expect_decode(package, PACKAGE_LENGTH, TEST_VA + 1u, 12, AP01_ERR_ADDRESS);
    expect_decode(package, PACKAGE_LENGTH, TEST_VA, 11, AP01_ERR_STATE_BUDGET);
    assert(allocation_calls == 0);
}

static void test_runtime_lifecycle(void)
{
    uint8_t package[PACKAGE_LENGTH];
    ap01_view view;
    ap01_runtime runtime = {0};
    ap01_api api;
    ap01_services services;
    ap01_entries entries;
    test_context context = {0};

    make_package(package);
    assert(ap01_decode_validate(package, PACKAGE_LENGTH, TEST_VA, 12, &view) == AP01_OK);
    api.abi_version = AP01_APP_ABI_VERSION;
    api.struct_bytes = sizeof(api);
    api.report = test_report;
    api.context = &context;
    services.abi_version = AP01_APP_ABI_VERSION;
    services.struct_bytes = sizeof(services);
    services.allocate = test_allocate;
    services.release = test_release;
    services.state_budget = 12;
    services.api = &api;
    entries.init = test_init;
    entries.step = test_step;
    entries.stop = test_stop;
    context.runtime = &runtime;
    allocation_calls = 0;
    release_calls = 0;
    allocation_failure = 0;
    assert(ap01_runtime_start(&view, &entries, &services, &runtime) == AP01_OK);
    assert(allocation_calls == 1 && release_calls == 0);
    assert(context.init_calls == 1 && context.data_ok != 0);
    assert(context.report_calls == 1 && context.reentrant_result == AP01_ERR_REENTRANT);
    assert(ap01_runtime_step(&runtime, 7) == AP01_OK && context.step_calls == 1);
    context.step_result = 1;
    assert(ap01_runtime_step(&runtime, 8) == AP01_ERR_STEP);
    assert(ap01_runtime_stop(&runtime) == AP01_OK);
    assert(context.stop_calls == 1 && release_calls == 1);
    assert(ap01_runtime_step(&runtime, 9) == AP01_ERR_NOT_RUNNING);

    runtime = (ap01_runtime){0};
    context.init_result = 1;
    context.step_result = 0;
    context.stop_failures = 0;
    assert(ap01_runtime_start(&view, &entries, &services, &runtime) == AP01_ERR_INIT);
    assert(context.stop_calls == 2 && release_calls == 2);
    assert(ap01_runtime_stop(&runtime) == AP01_ERR_NOT_RUNNING);

    runtime = (ap01_runtime){0};
    context.init_result = 1;
    context.stop_failures = 1;
    assert(ap01_runtime_start(&view, &entries, &services, &runtime) == AP01_ERR_INIT_STOP);
    assert(release_calls == 2);
    assert(ap01_runtime_stop(&runtime) == AP01_OK);
    assert(context.stop_calls == 4 && release_calls == 3);

    runtime = (ap01_runtime){0};
    context.init_result = 0;
    allocation_failure = 1;
    assert(ap01_runtime_start(&view, &entries, &services, &runtime) == AP01_ERR_ALLOC);
    assert(context.init_calls == 3 && release_calls == 3);
    allocation_failure = 0;
}

static void test_zero_state(void)
{
    ap01_runtime runtime = {0};
    test_context context = {0};
    ap01_api api = {AP01_APP_ABI_VERSION, sizeof(api), test_report, &context};
    ap01_services services = {AP01_APP_ABI_VERSION, sizeof(services), test_allocate,
                              test_release, 0, &api};
    ap01_entries entries = {test_init, test_step, test_stop};
    ap01_view view = {0};
    uint32_t calls = allocation_calls;

    view.state_len = 0;
    view.data_len = 0;
    view.bss_len = 0;
    view.data_init = NULL;
    assert(ap01_runtime_start(&view, &entries, &services, &runtime) == AP01_OK);
    assert(allocation_calls == calls);
    assert(ap01_runtime_stop(&runtime) == AP01_OK);
}

int main(void)
{
    test_parser_boundaries();
    test_runtime_lifecycle();
    test_zero_state();
    puts("AP01 loader tests passed");
    return 0;
}
