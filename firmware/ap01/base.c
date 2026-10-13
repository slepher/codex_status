#include "app_abi.h"

extern void *stock_malloc(uint32_t bytes);
extern void stock_free(void *pointer);
extern void stock_log(uint32_t level, const char *file, uint32_t line,
                      const char *function, const char *format, ...);
extern const uint8_t ap01_package_start[], ap01_package_end[];

/* GCC may lower structure assignment/initializers even with -fno-builtin. */
void *memcpy(void *destination, const void *source, size_t bytes)
{
    volatile uint8_t *out = destination;
    const uint8_t *in = source;
    size_t index;
    for (index = 0; index < bytes; ++index) out[index] = in[index];
    return destination;
}

void *memset(void *destination, int value, size_t bytes)
{
    volatile uint8_t *out = destination;
    size_t index;
    for (index = 0; index < bytes; ++index) out[index] = (uint8_t)value;
    return destination;
}

typedef struct report_state { uint32_t count; uint32_t valid; } report_state;

/* One bounded message per UI hook invocation, after the App stack is gone.
 * 0031 stock logging sink may be disabled; hardware retrieval is still unverified. */
void ap01_emit_result(uint32_t result)
{
    stock_log(3, "ap01/app-xip.c", 1, "ap01_xip_abi1",
              "APX1 diag0031 result=%u (0=init-step-stop-ok)", result);
}

static void discard_sync_state(ap01_runtime *runtime)
{
    /* This fixed synchronous App owns only its workspace, retains no callback,
     * and will never be retried after this caller returns. */
    if (runtime->allocation) stock_free(runtime->allocation);
}

void ap01_report(void *context, uint32_t code, uint32_t value)
{
    report_state *report = context;
    static const uint32_t values[] = {0x31, 8, 1};
    if (report->count >= 3 || code != report->count + 1 ||
        value != values[report->count]) {
        report->valid = 0;
    }
    ++report->count;
}

/* Synchronous diagnostic only: no callback or API pointer may escape this call. */
int ap01_boot_run(void)
{
    ap01_view view;
    ap01_runtime runtime = {0};
    report_state report = {0, 1};
    ap01_api api = {AP01_APP_ABI_VERSION, sizeof(ap01_api), ap01_report, &report};
    ap01_services services = {AP01_APP_ABI_VERSION, sizeof(ap01_services),
                             stock_malloc, stock_free, 16, &api};
    ap01_entries entries;
    ap01_error error = ap01_decode_validate(ap01_package_start,
        (uint32_t)(ap01_package_end - ap01_package_start), 0xa024c000, 16, &view);
    if (error != AP01_OK) return error;
    entries.init = (ap01_app_init_fn)(uintptr_t)(view.text + view.init_offset);
    entries.step = (ap01_app_step_fn)(uintptr_t)(view.text + view.step_offset);
    entries.stop = (ap01_app_stop_fn)(uintptr_t)(view.text + view.stop_offset);
    error = ap01_runtime_start(&view, &entries, &services, &runtime);
    if (error != AP01_OK) {
        discard_sync_state(&runtime);
        return error;
    }
    error = ap01_runtime_step(&runtime, 1);
    if (ap01_runtime_stop(&runtime) != AP01_OK) {
        discard_sync_state(&runtime);
        return AP01_ERR_STOP;
    }
    if (error != AP01_OK) return error;
    return report.valid && report.count == 3 ? AP01_OK : AP01_ERR_STEP;
}
