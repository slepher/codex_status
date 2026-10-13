/* Native RV32 XIP ABI exerciser. Not the complete Bridge business App. */
#include "app_abi.h"

typedef struct {
    uint32_t base_version;
    uint32_t seed;
    uint32_t ready;
    uint32_t calls;
} diagnostic_state;

int ap01_diag_init(ap01_api *api, void *memory, uint32_t bytes)
{
    diagnostic_state *state = memory;
    if (!api || api->abi_version != 1 || !api->report || bytes != sizeof(*state)) return -22;
    if (state->base_version != 0x31 || state->seed != 7 || state->ready || state->calls) return -22;
    state->ready = 0x41505031;
    api->report(api->context, 1, state->base_version);
    return 0;
}

int ap01_diag_step(ap01_api *api, void *memory, uint32_t bytes, uint32_t event)
{
    diagnostic_state *state = memory;
    if (!api || bytes != sizeof(*state) || state->ready != 0x41505031 || event == UINT32_MAX) return -22;
    state->calls += event;
    api->report(api->context, 2, state->calls + state->seed);
    return 0;
}

int ap01_diag_stop(ap01_api *api, void *memory, uint32_t bytes)
{
    diagnostic_state *state = memory;
    if (!api || bytes != sizeof(*state)) return -22;
    api->report(api->context, 3, state->calls);
    state->ready = 0;
    return 0;
}
