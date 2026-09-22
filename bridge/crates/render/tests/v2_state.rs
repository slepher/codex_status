//! Host tests for the device-side v2 state machines compiled from
//! `src/v2_state.h` (the same header the firmware uses): PowerPlan idempotency,
//! BOOT provisional 300 s, data_seq rules and integrity CRC.

use std::os::raw::{c_int, c_void};

// The C++ state machines live in the bridge-render static library.
use bridge_render as _;

extern "C" {
    fn codex_v2_plan_new() -> *mut c_void;
    fn codex_v2_plan_free(p: *mut c_void);
    fn codex_v2_plan_accept(
        p: *mut c_void,
        id: u64,
        mode: c_int,
        duration: u32,
        now_ms: u64,
        provisional: c_int,
        max_light: u32,
    ) -> c_int;
    fn codex_v2_plan_remaining(p: *mut c_void, now_ms: u64) -> u32;
    fn codex_v2_plan_high(p: *mut c_void) -> u64;
    fn codex_v2_plan_light_active(p: *mut c_void, now_ms: u64) -> c_int;
    fn codex_v2_boot_remaining(t_boot_ms: u64, now_ms: u64) -> u32;
    fn codex_v2_seq_new() -> *mut c_void;
    fn codex_v2_accept_data(p: *mut c_void, message: *const std::os::raw::c_char) -> c_int;
    fn codex_v2_seq_free(p: *mut c_void);
    fn codex_v2_seq_begin(p: *mut c_void, now_ms: u64, keep_next: u32);
    fn codex_v2_seq_observe(p: *mut c_void, seq: u64, crc: u32) -> c_int;
    fn codex_v2_seq_note(p: *mut c_void, seq: u64, crc: u32);
    fn codex_v2_seq_next(p: *mut c_void) -> u64;
    fn codex_v2_seq_applied(p: *mut c_void) -> u64;
    fn codex_v2_crc(data: *const u8, len: c_int) -> u32;
    fn codex_v2_fields_crc(fields_json: *const std::os::raw::c_char) -> u32;
    fn codex_v2_checkpoint_check() -> c_int;
    fn codex_v2_bundle_rx_check() -> c_int;
    fn codex_dirty_window(old: *const u8, new: *const u8, w: c_int, h: c_int, rect: *mut u16) -> c_int;
}

#[test]
fn wake_checkpoint_and_bundle_transfer_guards() {
    unsafe {
        assert_eq!(codex_v2_checkpoint_check(), 0);
        assert_eq!(codex_v2_bundle_rx_check(), 0);
    }
}

#[test]
fn dirty_windows_cover_changes_and_stay_inside_both_targets() {
    for (w, h) in [(200usize, 200usize), (400, 300)] {
        let old = vec![255u8; w * h / 8];
        for (x, y) in [(0, 0), (w - 1, h - 1), (8, 12), (w / 2, h / 2)] {
            let mut new = old.clone();
            new[y * w / 8 + x / 8] ^= 0x80 >> (x % 8);
            let mut rect = [0u16; 4];
            unsafe {
                assert_eq!(codex_dirty_window(old.as_ptr(), new.as_ptr(), w as i32, h as i32, rect.as_mut_ptr()), 1);
            }
            let [x0, y0, x1, y1] = rect.map(usize::from);
            assert!(x0 <= x && x1 >= x && y0 <= y && y1 >= y);
            assert_eq!(x0 % 8, 0);
            assert_eq!(x1 % 8, 7);
            assert!(x1 < w && y1 < h);
            assert!(x1 - x0 < 24 && y1 - y0 <= 2);
        }
        unsafe {
            let mut rect = [0u16; 4];
            assert_eq!(codex_dirty_window(old.as_ptr(), old.as_ptr(), w as i32, h as i32, rect.as_mut_ptr()), 0);
        }
    }
}

const ACCEPTED: i32 = 0;
const STALE: i32 = 1;
const CONFLICT: i32 = 2;

const DATA_APPLIED: i32 = 0;
const DATA_UNCHANGED: i32 = 1;
const DATA_CONFLICT: i32 = 2;
const DATA_STALE: i32 = 3;

#[test]
fn plan_ids_are_idempotent_and_never_restart_a_deadline() {
    unsafe {
        let p = codex_v2_plan_new();
        assert_eq!(codex_v2_plan_accept(p, 1, 1, 300, 0, 0, 600), ACCEPTED);
        assert_eq!(codex_v2_plan_remaining(p, 500), 300);
        // Replaying the same plan is idempotent and does not extend the deadline.
        assert_eq!(codex_v2_plan_accept(p, 1, 1, 300, 60_000, 0, 600), ACCEPTED);
        assert_eq!(codex_v2_plan_remaining(p, 61_000), 239);
        // Same id, different content is a conflict.
        assert_eq!(codex_v2_plan_accept(p, 1, 1, 600, 60_000, 0, 600), CONFLICT);
        // Older id is stale.
        assert_eq!(codex_v2_plan_accept(p, 0, 1, 300, 60_000, 0, 600), STALE);
        // New id extends.
        assert_eq!(codex_v2_plan_accept(p, 2, 1, 600, 60_000, 0, 600), ACCEPTED);
        assert_eq!(codex_v2_plan_remaining(p, 60_000), 600);
        assert_eq!(codex_v2_plan_high(p), 2);
        // Sleep plan ends light immediately.
        assert_eq!(codex_v2_plan_accept(p, 3, 0, 0, 61_000, 0, 600), ACCEPTED);
        assert_eq!(codex_v2_plan_light_active(p, 61_000), 0);
        codex_v2_plan_free(p);
    }
}

#[test]
fn device_safety_shortens_an_oversized_plan() {
    unsafe {
        let p = codex_v2_plan_new();
        assert_eq!(codex_v2_plan_accept(p, 7, 1, 3600, 0, 0, 600), ACCEPTED);
        assert_eq!(codex_v2_plan_remaining(p, 0), 600);
        assert_eq!(codex_v2_plan_accept(p, 7, 1, 3600, 60_000, 0, 600), ACCEPTED);
        assert_eq!(codex_v2_plan_remaining(p, 60_000), 540);
        assert_eq!(codex_v2_plan_accept(p, 8, 1, 1, 0, 0, 600), ACCEPTED);
        assert_eq!(codex_v2_plan_remaining(p, 0), 30, "minimum light window");
        codex_v2_plan_free(p);
    }
}

#[test]
fn transport_runtime_validates_crc_before_advancing_sequence() {
    let mut message = serde_json::json!({
        "active_context_id": "ctx", "seq": 1,
        "fields": [{"i":0,"k":"bridge.label","v":"first","q":"good"}]
    });
    let sign = |m: &mut serde_json::Value| {
        m["crc"] = format!("{:08x}", bridge_core::coordinator::data_fields_crc(
            m["fields"].as_array().unwrap())).into();
    };
    let accept = |p, m: &serde_json::Value| unsafe {
        let text = std::ffi::CString::new(m.to_string()).unwrap();
        codex_v2_accept_data(p, text.as_ptr())
    };
    unsafe {
        let p = codex_v2_seq_new();
        assert_eq!(accept(p, &message), 5, "missing CRC must not bypass validation");
        message["crc"] = "00000000".into();
        assert_eq!(accept(p, &message), 5, "zero is a CRC, not a validation bypass");
        message["crc"] = "zzzzzzzz".into();
        assert_eq!(accept(p, &message), 5);
        sign(&mut message);
        assert_eq!(accept(p, &message), DATA_APPLIED);
        assert_eq!(accept(p, &message), DATA_UNCHANGED);
        message["fields"][0]["v"] = "second".into();
        assert_eq!(accept(p, &message), 5, "tampered contents");
        sign(&mut message);
        assert_eq!(accept(p, &message), DATA_CONFLICT);
        message["seq"] = 2.into();
        message["active_context_id"] = "old".into();
        assert_eq!(accept(p, &message), 4);
        assert_eq!(codex_v2_seq_applied(p), 1, "rejection must not advance seq");
        message["active_context_id"] = "ctx".into();
        assert_eq!(accept(p, &message), DATA_APPLIED);
        codex_v2_seq_free(p);
    }
}

#[test]
fn boot_provisional_is_300s_and_counts_from_the_physical_wake() {
    unsafe {
        assert_eq!(codex_v2_boot_remaining(0, 0), 300);
        assert_eq!(codex_v2_boot_remaining(0, 3_000), 297);
        assert_eq!(codex_v2_boot_remaining(0, 300_000), 0);
        // A time jump cannot move it (monotonic elapsed only).
        assert_eq!(codex_v2_boot_remaining(1_000, 61_000), 240);
    }
}

#[test]
fn data_seq_replay_conflict_and_ordering() {
    unsafe {
        let p = codex_v2_seq_new();
        codex_v2_seq_begin(p, 0, 1);
        assert_eq!(codex_v2_seq_observe(p, 1, 0xAAAA), DATA_APPLIED);
        codex_v2_seq_note(p, 1, 0xAAAA);
        assert_eq!(codex_v2_seq_next(p), 2);
        // Same seq + same content is idempotent (no second display).
        assert_eq!(codex_v2_seq_observe(p, 1, 0xAAAA), DATA_UNCHANGED);
        // Same seq + different content conflicts.
        assert_eq!(codex_v2_seq_observe(p, 1, 0xBBBB), DATA_CONFLICT);
        // Older seq is rejected.
        assert_eq!(codex_v2_seq_observe(p, 0, 0xCCCC), DATA_STALE);
        // Gaps are allowed.
        assert_eq!(codex_v2_seq_observe(p, 5, 0xDDDD), DATA_APPLIED);
        codex_v2_seq_note(p, 5, 0xDDDD);
        assert_eq!(codex_v2_seq_applied(p), 5);
        // A new context keeps the per-device counter but resets the applied mark:
        // the first packet of the new context is applied (gaps stay allowed).
        codex_v2_seq_begin(p, 10_000, 6);
        assert_eq!(codex_v2_seq_next(p), 6);
        assert_eq!(codex_v2_seq_observe(p, 9, 0xEEEE), DATA_APPLIED);
        codex_v2_seq_note(p, 9, 0xEEEE);
        assert_eq!(codex_v2_seq_observe(p, 9, 0xEEEE), DATA_UNCHANGED);
        assert_eq!(codex_v2_seq_observe(p, 8, 0xFFFF), DATA_STALE);
        codex_v2_seq_free(p);
    }
}

#[test]
fn crc_matches_the_bridge_hash_algorithm() {
    unsafe {
        // CRC32/IEEE of "123456789" is the well-known 0xCBF43926.
        let data = b"123456789";
        assert_eq!(codex_v2_crc(data.as_ptr(), data.len() as c_int), 0xCBF43926);
    }
}

#[test]
fn data_fields_crc_is_byte_identical_to_the_bridge() {
    // Cross-implementation: the device C++ and the Rust bridge must produce the
    // same Data fingerprint for the same bounded field array, otherwise every
    // push would be rejected as a CRC mismatch.
    let fixtures = vec![
        serde_json::json!([{"i":0,"k":"a","v":1,"q":"good"}]),
        serde_json::json!([
            {"i":0,"k":"buckets[codex].weekly.remaining","v":69,"q":"good"},
            {"i":1,"k":"buckets[codex].weekly.resetsAt","v":1700500000i64,"q":"good"},
            {"i":2,"k":"server_time","v":null,"q":"missing"}
        ]),
        serde_json::json!([{"i":3,"k":"bridge.label","v":"tester","q":"good"}]),
    ];
    for fixture in fixtures {
        let text = std::ffi::CString::new(fixture.to_string()).unwrap();
        let device = unsafe { codex_v2_fields_crc(text.as_ptr()) };
        let fields = fixture.as_array().unwrap().clone();
        let bridge = bridge_core::coordinator::data_fields_crc(&fields);
        assert_eq!(device, bridge, "fixture {fixture}");
    }
}
