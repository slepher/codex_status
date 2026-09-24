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
    fn codex_v2_accept_data_template(
        source: *const std::os::raw::c_char,
        message: *const std::os::raw::c_char,
    ) -> c_int;
    fn codex_v2_data_decide(
        p: *mut c_void,
        configured: c_int,
        source: *const std::os::raw::c_char,
        message: *const std::os::raw::c_char,
        context: *const std::os::raw::c_char,
        out: *mut std::os::raw::c_char,
        cap: c_int,
    ) -> c_int;
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
fn shared_data_decision_classifies_acks_and_preserves_rejected_state() {
    let template = r#"{"schema":1,"id":"t","canvas":{"w":200,"h":200},"elements":[
        {"type":"text","bind":"bridge.label","x":8,"y":8,"font":"f12"},
        {"type":"text","bind":"bridge.hostId","x":8,"y":24,"font":"f12"}]}"#;
    let source = std::ffi::CString::new(template).unwrap();
    let context = std::ffi::CString::new("ctx").unwrap();
    fn make_message(seq: u64, ctx: &str, value: &str, bad_crc: bool, swap: bool) -> serde_json::Value {
        let mut fields = vec![
            serde_json::json!({"i": 0, "k": "bridge.label", "v": value, "q": "good"}),
            serde_json::json!({"i": 1, "k": "bridge.hostId", "v": "host", "q": "good"}),
        ];
        if swap {
            fields.reverse();
        }
        let crc = format!("{:08x}", bridge_core::coordinator::data_fields_crc(&fields));
        serde_json::json!({
            "active_context_id": ctx,
            "seq": seq,
            "fields": fields,
            "crc": if bad_crc { "00000000".to_owned() } else { crc }
        })
    }
    let p = unsafe { codex_v2_seq_new() };
    let decide = |p, configured: bool, m: &serde_json::Value| {
        let text = std::ffi::CString::new(m.to_string()).unwrap();
        let mut out = vec![0i8; 2048];
        let rc = unsafe {
            codex_v2_data_decide(
                p,
                configured as c_int,
                source.as_ptr(),
                text.as_ptr(),
                context.as_ptr(),
                out.as_mut_ptr(),
                out.len() as c_int,
            )
        };
        assert!(rc > 0);
        let output = unsafe { std::ffi::CStr::from_ptr(out.as_ptr()) };
        serde_json::from_str::<serde_json::Value>(output.to_str().unwrap()).unwrap()
    };
    unsafe {
        let unconfigured = decide(p, false, &make_message(1, "ctx", "first", false, false));
        assert_eq!(unconfigured["result"], "rejected");
        assert_eq!(unconfigured["display"], "failed");
        assert_eq!(unconfigured["error"], "unconfigured");
        assert_eq!(unconfigured["seq"], -1);
        assert_eq!(unconfigured["include_context"], false);
        assert_eq!(codex_v2_seq_next(p), 1);
        assert_eq!(codex_v2_seq_applied(p), 0);

        let bad_source = std::ffi::CString::new("{}").unwrap();
        let bad_message = std::ffi::CString::new(
            make_message(1, "ctx", "first", false, false).to_string()
        ).unwrap();
        let mut bad_out = vec![0i8; 2048];
        assert!(codex_v2_data_decide(
            p, 1, bad_source.as_ptr(), bad_message.as_ptr(), context.as_ptr(),
            bad_out.as_mut_ptr(), bad_out.len() as c_int
        ) > 0);
        let bad_template_decision = serde_json::from_str::<serde_json::Value>(
            std::ffi::CStr::from_ptr(bad_out.as_ptr()).to_str().unwrap()
        ).unwrap();
        assert_eq!(bad_template_decision["result"], "rejected");
        assert_eq!(bad_template_decision["display"], "failed");
        assert_ne!(bad_template_decision["error"], "unconfigured");
        assert_eq!(codex_v2_seq_next(p), 1);

        let first = make_message(1, "ctx", "first", false, false);
        let applied = decide(p, true, &first);
        assert_eq!(applied["result"], "applied");
        assert_eq!(applied["first_applied"], true);
        assert_eq!(applied["seq"], 1);
        assert_eq!(applied["include_context"], true);
        assert!(applied.get("display").is_none());
        assert!(applied.get("error").is_none());
        assert_eq!(codex_v2_seq_applied(p), 1);

        let replay = decide(p, true, &first);
        assert_eq!(replay["result"], "applied");
        assert_eq!(replay["display"], "unchanged");
        assert!(replay.get("error").is_none());
        assert_eq!(replay["first_applied"], false);
        assert_eq!(codex_v2_seq_applied(p), 1);

        let conflict = decide(p, true, &make_message(1, "ctx", "different", false, false));
        assert_eq!(conflict["result"], "rejected");
        assert_eq!(conflict["display"], "unchanged");
        assert_eq!(conflict["error"], "seq_conflict");
        assert_eq!(codex_v2_seq_applied(p), 1);

        let later = decide(p, true, &make_message(3, "ctx", "different", false, false));
        assert_eq!(later["result"], "applied");
        assert_eq!(codex_v2_seq_applied(p), 3);
        let stale = decide(p, true, &make_message(2, "ctx", "different", false, false));
        assert_eq!(stale["result"], "rejected");
        assert_eq!(stale["display"], "unchanged");
        assert_eq!(stale["error"], "stale_seq");
        assert_eq!(codex_v2_seq_applied(p), 3);

        let wrong_context = decide(p, true, &make_message(4, "wrong", "different", false, false));
        assert_eq!(wrong_context["result"], "rejected");
        assert_eq!(wrong_context["display"], "pending");
        assert_eq!(wrong_context["error"], "context");
        assert_eq!(codex_v2_seq_applied(p), 3);

        let bad_crc = decide(p, true, &make_message(4, "ctx", "different", true, false));
        assert_eq!(bad_crc["result"], "rejected");
        assert_eq!(bad_crc["display"], "failed");
        assert_eq!(bad_crc["error"], "crc");
        assert_eq!(codex_v2_seq_applied(p), 3);
        let bad_order = decide(p, true, &make_message(4, "ctx", "different", false, true));
        assert_eq!(bad_order["result"], "rejected");
        assert_eq!(bad_order["display"], "failed");
        assert_eq!(bad_order["error"], "order");
        assert_eq!(codex_v2_seq_applied(p), 3);
        codex_v2_seq_free(p);
    }
}

#[test]
fn remote_data_snapshot_covers_all_non_local_requirements() {
    // Regression: the bridge sends exactly the remote requirements (device.*
    // binds are local and never transmitted); a complete snapshot must be
    // accepted for a template that also declares device.now.
    let template = r#"{"schema":1,"id":"t","canvas":{"w":200,"h":200},"elements":[
        {"type":"text","bind":"bridge.label","x":8,"y":8,"font":"f12"},
        {"type":"text","bind":"device.now","x":8,"y":40,"font":"f12"}]}"#;
    let fields = serde_json::json!([{"i":0,"k":"bridge.label","v":"first","q":"good"}]);
    let sign = |m: &mut serde_json::Value| {
        m["crc"] = format!(
            "{:08x}",
            bridge_core::coordinator::data_fields_crc(m["fields"].as_array().unwrap())
        )
        .into();
    };
    let accept = |m: &serde_json::Value| unsafe {
        let source = std::ffi::CString::new(template).unwrap();
        let text = std::ffi::CString::new(m.to_string()).unwrap();
        codex_v2_accept_data_template(source.as_ptr(), text.as_ptr())
    };
    let mut message = serde_json::json!({
        "active_context_id": "ctx", "seq": 1, "fields": fields
    });
    sign(&mut message);
    assert_eq!(accept(&message), DATA_APPLIED);
    // The device-local entry must not be transmitted by the bridge.
    let mut with_local = serde_json::json!({
        "active_context_id": "ctx", "seq": 2,
        "fields": [
            {"i":0,"k":"bridge.label","v":"first","q":"good"},
            {"i":1,"k":"device.now","v":"12:00","q":"good"}
        ]
    });
    sign(&mut with_local);
    assert_eq!(accept(&with_local), 5);
    // A remote field may not be dropped from the complete snapshot.
    let mut missing = serde_json::json!({
        "active_context_id": "ctx", "seq": 3, "fields": []
    });
    sign(&mut missing);
    assert_eq!(accept(&missing), 5);
    // Index/path tampering stays rejected.
    let mut swapped = serde_json::json!({
        "active_context_id": "ctx", "seq": 4,
        "fields": [{"i":1,"k":"device.now","v":"12:00","q":"good"}]
    });
    sign(&mut swapped);
    assert_eq!(accept(&swapped), 5);
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
