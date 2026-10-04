"""Keep NimBLE's queued host-timer event alive until ble_hs_deinit()."""
from pathlib import Path

OLD = '''static void
ble_hs_timer_reset(uint32_t ticks)
{
    int rc;

    if (!ble_hs_is_enabled()) {
        ble_npl_callout_stop(&ble_hs_timer);
        ble_npl_callout_deinit(&ble_hs_timer);
    } else {
        rc = ble_npl_callout_reset(&ble_hs_timer, ticks);
        BLE_HS_DBG_ASSERT_EVAL(rc == 0);
    }
}'''
NEW = OLD.replace('        ble_npl_callout_deinit(&ble_hs_timer);',
                  '        /* Codex Status: queued events retain fn until host stop completes.\n'
                  '         * ble_hs_deinit() owns final timer destruction. */')


def patched_source(text):
    cleanup = text.split('\nvoid\nble_hs_deinit(void)\n', 1)
    if len(cleanup) != 2 or '    ble_npl_callout_deinit(&ble_hs_timer);' not in cleanup[1]:
        raise RuntimeError('NimBLE host final timer cleanup changed; review the fix')
    if text.count(NEW) == 1 and OLD not in text:
        return text
    if text.count(OLD) != 1 or NEW in text:
        raise RuntimeError('Unknown NimBLE host timer implementation; review the fix')
    return text.replace(OLD, NEW, 1)


def apply_file(path):
    text = path.read_text()
    patched = patched_source(text)
    if patched != text:
        path.write_text(patched)
    print('nimble_host_timer_fix: host timer cleanup deferred until host is stopped')


if 'Import' in globals():
    Import('env')  # noqa: F821 - PlatformIO post script, after dependency resolution
    apply_file(Path(env.subst('$PROJECT_LIBDEPS_DIR')) / env['PIOENV'] /  # noqa: F821
               'NimBLE-Arduino/src/nimble/nimble/host/src/ble_hs.c')
elif __name__ == '__main__':
    import sys
    apply_file(Path(sys.argv[1]))
