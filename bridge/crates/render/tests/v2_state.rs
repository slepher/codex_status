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
    fn codex_v2_seq_free(p: *mut c_void);
    fn codex_v2_seq_begin(p: *mut c_void, now_ms: u64, keep_next: u32);
    fn codex_v2_seq_observe(p: *mut c_void, seq: u64, crc: u32) -> c_int;
    fn codex_v2_seq_note(p: *mut c_void, seq: u64, crc: u32);
    fn codex_v2_seq_next(p: *mut c_void) -> u64;
    fn codex_v2_seq_applied(p: *mut c_void) -> u64;
    fn codex_v2_crc(data: *const u8, len: c_int) -> u32;
    fn codex_v2_fields_crc(fields_json: *const std::os::raw::c_char) -> u32;
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
        assert_eq!(codex_v2_plan_accept(p, 8, 1, 1, 0, 0, 600), ACCEPTED);
        assert_eq!(codex_v2_plan_remaining(p, 0), 30, "minimum light window");
        codex_v2_plan_free(p);
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
