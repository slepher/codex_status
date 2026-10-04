#!/usr/bin/env python3
"""Replay a queued host timer across STOPPING using the installed NimBLE code."""
import argparse
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--expect-red', action='store_true')
parser.add_argument('--out', type=Path, default=ROOT / 'artifacts/bridge-macos/nimble-stop-fix')
args = parser.parse_args()
lib = ROOT / '.pio/libdeps/zectrix-note4-b/NimBLE-Arduino/src/nimble'
host = (lib / 'nimble/host/src/ble_hs.c').read_text()
stop = (lib / 'nimble/host/src/ble_hs_stop.c').read_text()
npl = (lib / 'porting/npl/freertos/src/npl_os_freertos.c').read_text()
header = (lib / 'porting/npl/freertos/include/nimble/nimble_npl_os.h').read_text()
# Replay the exact pre-fix function without changing the installed dependency.
if args.expect_red:
    import importlib.util
    spec = importlib.util.spec_from_file_location('nimble_fix_red', ROOT / 'tools/nimble_host_timer_fix.py')
    fix_red = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fix_red)
    host = host.replace(fix_red.NEW, fix_red.OLD)
# This build uses the non-ROM inline event runner, which is also visible in .59 ELF.

def function(source, name, last=False):
    matches = list(re.finditer(r'(?:static\s+)?(?:inline\s+)?(?:void|int|bool|ble_npl_error_t)\s+' + re.escape(name) + r'\s*\([^;{}]*\)\s*\{', source))
    assert matches, name
    match = matches[-1] if last else matches[0]
    start = match.end() - 1
    depth = 1
    end = start + 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[match.start():end]

prefix = r'''
#include <assert.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#define CONFIG_BT_NIMBLE_USE_ESP_TIMER 1
#define NIMBLE_BLE_CONNECT 0
#define BLE_MONITOR 0
#define MYNEWT_VAL(x) 0
#define ESP_LOGW(...) ((void)0)
#define BLE_HS_LOG(...) ((void)0)
#define BLE_HS_DBG_ASSERT_EVAL(x) assert(x)
#define BLE_HS_FOREVER INT32_MAX
#define BLE_HS_EALREADY 1
#define BLE_HS_EBUSY 2
#define BLE_HS_EUNKNOWN 3
#define BLE_HS_ENABLED_STATE_ON 1
#define BLE_HS_ENABLED_STATE_STOPPING 2
#define BLE_HS_ENABLED_STATE_OFF 3
#define BLE_HS_SYNC_STATE_GOOD 1
#define BLE_HS_SYNC_STATE_BAD 2
#define BLE_HS_SYNC_STATE_BRINGUP 3
typedef uint32_t ble_npl_time_t;
typedef int32_t ble_npl_stime_t;
struct ble_npl_event { bool queued; void (*fn)(struct ble_npl_event *); void *arg; };
struct ble_npl_callout { void *handle; void *evq; struct ble_npl_event ev; };
struct ble_hs_stop_listener { int unused; };
typedef void ble_hs_stop_fn(int, void *);
static struct ble_npl_callout ble_hs_timer;
static int ble_hs_enabled_state, ble_hs_sync_state = BLE_HS_SYNC_STATE_GOOD;
static int allocated, destroyed, resets, callbacks;
static int ble_hs_is_enabled(void) { return ble_hs_enabled_state == BLE_HS_ENABLED_STATE_ON; }
static int esp_timer_stop(void *handle) { assert(handle); return 0; }
static int esp_timer_delete(void *handle) { assert(handle); free(handle); destroyed++; return 0; }
static int ble_npl_callout_reset(struct ble_npl_callout *co, uint32_t ticks) {
    (void)ticks; assert(co->handle); resets++; return 0;
}
static int ble_npl_callout_is_active(struct ble_npl_callout *co) { (void)co; return 0; }
static ble_npl_time_t ble_npl_time_get(void) { return 1; }
static ble_npl_time_t ble_npl_callout_get_ticks(struct ble_npl_callout *co) { (void)co; return 0; }
static int ble_gap_timer(void) { callbacks++; return 4; }
static int ble_hs_reset(void) { assert(0); return 0; }
static void ble_hs_stop_register_listener(struct ble_hs_stop_listener *l, ble_hs_stop_fn *f, void *a) { (void)l; (void)f; (void)a; }
#define ble_npl_callout_stop npl_freertos_callout_stop
#define ble_npl_callout_deinit npl_freertos_callout_deinit
#define ble_hs_flow_deinit() ((void)0)
#define ble_npl_mutex_deinit(x) ((void)0)
#define ble_mqueue_deinit(x) ((void)0)
#define ble_hs_stop_deinit() ((void)0)
#define ble_gap_deinit() ((void)0)
#define ble_hs_hci_deinit() ((void)0)
#define ble_npl_event_deinit(x) ((void)0)
static void ble_hs_timer_sched(int32_t ticks);
'''
parts = [function(npl, 'npl_freertos_callout_stop'),
         function(npl, 'npl_freertos_callout_deinit'),
         function(host, 'ble_hs_timer_reset'), function(host, 'ble_hs_timer_sched'),
         function(host, 'ble_hs_timer_resched'), function(host, 'ble_hs_timer_exp'),
         function(stop, 'ble_hs_stop_begin'), function(header, 'ble_npl_event_run', last=True),
         function(host, 'ble_hs_deinit')]
suffix = r'''
int main(void) {
    for (int round = 0; round < 1000; round++) {
        for (int already_dequeued = 0; already_dequeued < 2; already_dequeued++) {
            int old_destroyed = destroyed, old_resets = resets, old_callbacks = callbacks;
            ble_hs_enabled_state = BLE_HS_ENABLED_STATE_ON;
            ble_hs_timer.handle = malloc(1); assert(ble_hs_timer.handle); allocated++;
            ble_hs_timer.ev.fn = ble_hs_timer_exp;
            ble_hs_timer.ev.queued = true;
            struct ble_npl_event *queue_slot = &ble_hs_timer.ev;
            struct ble_npl_event *ev = already_dequeued ? queue_slot : NULL;
            ble_hs_timer_reset(5); assert(resets == old_resets + 1);
            assert(ble_hs_stop_begin(NULL, NULL, NULL) == 0);
            assert(ble_hs_stop_begin(NULL, NULL, NULL) == BLE_HS_EBUSY);
            if (!already_dequeued) ev = queue_slot;
            ev->queued = false;
            // Same production indirect callback as the PC=0 dump; no NULL guard.
            ble_npl_event_run(ev);
            assert(callbacks == old_callbacks + 1);
            assert(resets == old_resets + 1); // STOPPING callback must not rearm.
            assert(destroyed == old_destroyed); // Owner survives until host is stopped.
            ble_hs_enabled_state = BLE_HS_ENABLED_STATE_OFF;
            assert(ble_hs_stop_begin(NULL, NULL, NULL) == BLE_HS_EALREADY);
            ble_hs_deinit(); // Actual production final cleanup still frees the timer.
            assert(destroyed == old_destroyed + 1);
            assert(!ble_hs_timer.handle && !ble_hs_timer.ev.fn);
            ble_hs_deinit(); assert(destroyed == old_destroyed + 1);
        }
    }
    assert(allocated == destroyed);
    puts("PASS: 2000 queued/dequeued STOPPING exchanges; repeat stop, no rearm, final cleanup/reinit");
}
'''
# The build patch is reproducible after dependency reinstall and fails closed on drift.
if not args.expect_red:
    import importlib.util
    spec = importlib.util.spec_from_file_location('nimble_fix', ROOT / 'tools/nimble_host_timer_fix.py')
    fix = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fix)
    original = host.replace(fix.NEW, fix.OLD)
    assert fix.patched_source(original) == host
    assert fix.patched_source(host) == host
    for drift in (original.replace('uint32_t ticks', 'uint16_t ticks'),
                  original.replace('\nvoid\nble_hs_deinit(void)\n', '\nvoid\nother_cleanup(void)\n')):
        try:
            fix.patched_source(drift)
        except RuntimeError:
            pass
        else:
            raise AssertionError('dependency drift was not rejected')
    print('PASS: patch reapplication, idempotence and unknown timer/cleanup rejection')
args.out.mkdir(parents=True, exist_ok=True)
source = args.out / 'host-stop-replay.c'
source.write_text(prefix + '\n'.join(parts) + suffix)
binary = args.out / 'host-stop-replay'
subprocess.run([os.environ.get('CC', 'clang'), '-std=c11', '-g', '-Wall', '-Wextra', '-Werror',
                '-Wno-unused-parameter', '-fsanitize=address,undefined', str(source), '-o', str(binary)], check=True)
result = subprocess.run([str(binary)], capture_output=True, text=True)
log = args.out / ('host-stop-red.log' if args.expect_red else 'host-stop-green.log')
log.write_text(result.stdout + result.stderr)
if args.expect_red:
    assert result.returncode and re.search(r'SEGV|DEADLYSIGNAL', result.stderr), result.stdout + result.stderr
    print('RED confirmed: installed production stop path invokes NULL queued timer callback')
    print('\n'.join(result.stderr.splitlines()[:9]))
else:
    print(result.stdout, end='')
    if result.returncode:
        print(result.stderr)
        raise SystemExit(result.returncode)
