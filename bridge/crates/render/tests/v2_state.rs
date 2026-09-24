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
    fn codex_v2_plan_decide(
        p: *mut c_void,
        message: *const std::os::raw::c_char,
        now_ms: u64,
        provisional: c_int,
        out: *mut std::os::raw::c_char,
        cap: c_int,
    ) -> c_int;
    fn codex_v2_bundle_begin_new() -> *mut c_void;
    fn codex_v2_bundle_begin_free(p: *mut c_void);
    fn codex_v2_bundle_begin_committed(
        p: *mut c_void,
        owner: *const std::os::raw::c_char,
        request: *const std::os::raw::c_char,
        crc: u32,
        length: u32,
        context: *const std::os::raw::c_char,
    );
    fn codex_v2_bundle_begin_seed_rx(
        p: *mut c_void,
        owner: *const std::os::raw::c_char,
        request: *const std::os::raw::c_char,
        nonce: *const std::os::raw::c_char,
        length: u32,
        crc: u32,
        offset: u32,
        deadline: u64,
    ) -> c_int;
    fn codex_v2_bundle_begin_decide(
        p: *mut c_void,
        message: *const std::os::raw::c_char,
        nonce: *const std::os::raw::c_char,
        now_ms: u64,
        out: *mut std::os::raw::c_char,
        cap: c_int,
    ) -> c_int;
    fn codex_v2_bundle_begin_commit_candidate(p: *mut c_void) -> c_int;
    fn codex_v2_bundle_chunk_start(
        p: *mut c_void,
        request: *const std::os::raw::c_char,
        nonce: *const std::os::raw::c_char,
        offset: *const std::os::raw::c_char,
        now_ms: u64,
        out: *mut std::os::raw::c_char,
        cap: c_int,
    ) -> c_int;
    fn codex_v2_bundle_chunk_write(
        p: *mut c_void,
        offset: u32,
        replay: c_int,
        processed: u32,
        size: u32,
        out: *mut std::os::raw::c_char,
        cap: c_int,
    ) -> c_int;
    fn codex_v2_bundle_chunk_end(
        p: *mut c_void,
        offset: u32,
        replay: c_int,
        processed: u32,
        now_ms: u64,
        out: *mut std::os::raw::c_char,
        cap: c_int,
    ) -> c_int;
    fn codex_v2_bundle_commit_reset_store() -> c_int;
    fn codex_v2_bundle_commit_install_active(
        bundle: *const std::os::raw::c_char,
        context: *const std::os::raw::c_char,
    ) -> c_int;
    fn codex_v2_bundle_commit_seed_rx(
        p: *mut c_void,
        owner: *const std::os::raw::c_char,
        request: *const std::os::raw::c_char,
        nonce: *const std::os::raw::c_char,
        length: u32,
        crc: u32,
        offset: u32,
        deadline: u64,
    ) -> c_int;
    fn codex_v2_bundle_commit_write_file(
        path: *const std::os::raw::c_char,
        body: *const u8,
        length: c_int,
    ) -> c_int;
    fn codex_v2_bundle_commit_decide(
        p: *mut c_void,
        message: *const std::os::raw::c_char,
        nonce: *const std::os::raw::c_char,
        now_ms: u64,
        path: *const std::os::raw::c_char,
        fallback: *const std::os::raw::c_char,
        out: *mut std::os::raw::c_char,
        cap: c_int,
        body_out: *mut std::os::raw::c_char,
        body_cap: c_int,
    ) -> c_int;
    fn codex_v2_activate_new(configured: c_int, profile: *const std::os::raw::c_char) -> *mut c_void;
    fn codex_v2_activate_free(p: *mut c_void);
    fn codex_v2_activate_seed_fingerprint(
        p: *mut c_void,
        request: *const std::os::raw::c_char,
        owner: *const std::os::raw::c_char,
        template_id: *const std::os::raw::c_char,
        expected: *const std::os::raw::c_char,
        context: *const std::os::raw::c_char,
    );
    fn codex_v2_activate_decide(
        p: *mut c_void,
        message: *const std::os::raw::c_char,
        out: *mut std::os::raw::c_char,
        cap: c_int,
    ) -> c_int;
    fn codex_v2_claim_decide(
        message: *const std::os::raw::c_char,
        have_owner: c_int,
        current: *const std::os::raw::c_char,
        out: *mut std::os::raw::c_char,
        cap: c_int,
    ) -> c_int;
    fn codex_v2_command_parse(
        message: *const std::os::raw::c_char,
        out: *mut std::os::raw::c_char,
        cap: c_int,
    ) -> c_int;
    fn codex_v2_command_check(
        message: *const std::os::raw::c_char,
        current_mac: *const std::os::raw::c_char,
        nonce: *const std::os::raw::c_char,
        out: *mut std::os::raw::c_char,
        cap: c_int,
    ) -> c_int;
    fn codex_v2_build_ack(
        op: *const std::os::raw::c_char,
        result: *const std::os::raw::c_char,
        display: *const std::os::raw::c_char,
        retention: *const std::os::raw::c_char,
        error: *const std::os::raw::c_char,
        seq: i64,
        plan_id: u64,
        context: *const std::os::raw::c_char,
        accepted_remaining_s: u32,
        fw_target: *const std::os::raw::c_char,
        out: *mut std::os::raw::c_char,
        cap: c_int,
    ) -> c_int;
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
fn shared_plan_decision_classifies_ack_and_preserves_plan_state() {
    let decide = |p: *mut c_void, message: serde_json::Value, now_ms, provisional| unsafe {
        let text = std::ffi::CString::new(message.to_string()).unwrap();
        let mut out = vec![0i8; 2048];
        let rc = codex_v2_plan_decide(
            p,
            text.as_ptr(),
            now_ms,
            provisional,
            out.as_mut_ptr(),
            out.len() as c_int,
        );
        assert!(rc > 0);
        serde_json::from_str::<serde_json::Value>(
            std::ffi::CStr::from_ptr(out.as_ptr()).to_str().unwrap(),
        )
        .unwrap()
    };
    unsafe {
        let p = codex_v2_plan_new();
        let shape = decide(p, serde_json::json!({"plan_id": 1, "mode": "light"}), 0, 0);
        assert_eq!(shape["accepted"], false);
        assert_eq!(shape["result"], "rejected");
        assert_eq!(shape["display"], "unchanged");
        assert_eq!(shape["error"], "plan_shape");
        assert_eq!(shape["plan_id"], 0);
        assert_eq!(shape["include_context"], false);
        assert_eq!(shape["state_accepted"], false);
        assert_eq!(shape["high_id"], 0);

        let provisional_cap = decide(
            p,
            serde_json::json!({"plan_id": 1, "mode": "light", "light_duration_s": 600}),
            0,
            1,
        );
        assert_eq!(provisional_cap["accepted"], true);
        assert_eq!(provisional_cap["result"], "applied");
        assert_eq!(provisional_cap["display"], "unchanged");
        assert!(provisional_cap.get("error").is_none());
        assert_eq!(provisional_cap["plan_id"], 1);
        assert_eq!(provisional_cap["granted_s"], 300);
        assert_eq!(provisional_cap["include_context"], true);
        assert_eq!(provisional_cap["deadline_ms"], 300_000);

        let replay = decide(
            p,
            serde_json::json!({"plan_id": 1, "mode": "light", "light_duration_s": 600}),
            60_000,
            1,
        );
        assert_eq!(replay["result"], "applied");
        assert_eq!(replay["display"], "unchanged");
        assert_eq!(replay["deadline_ms"], 300_000);
        assert_eq!(replay["remaining_s"], 240);

        let conflict = decide(
            p,
            serde_json::json!({"plan_id": 1, "mode": "light", "light_duration_s": 599}),
            60_000,
            1,
        );
        assert_eq!(conflict["accepted"], false);
        assert_eq!(conflict["result"], "rejected");
        assert_eq!(conflict["display"], "unchanged");
        assert_eq!(conflict["error"], "plan_conflict");
        assert_eq!(conflict["plan_id"], 1);
        assert_eq!(conflict["include_context"], true);
        assert_eq!(conflict["deadline_ms"], 300_000);
        assert_eq!(conflict["remaining_s"], 240);

        let stale = decide(
            p,
            serde_json::json!({"plan_id": 0, "mode": "sleep"}),
            60_000,
            1,
        );
        assert_eq!(stale["result"], "rejected");
        assert_eq!(stale["display"], "unchanged");
        assert_eq!(stale["error"], "stale_plan");
        assert_eq!(stale["plan_id"], 0);
        assert_eq!(stale["include_context"], true);
        assert_eq!(stale["high_id"], 1);
        assert_eq!(stale["deadline_ms"], 300_000);

        let minimum = decide(
            p,
            serde_json::json!({"plan_id": 2, "mode": "light", "light_duration_s": 1}),
            60_000,
            1,
        );
        assert_eq!(minimum["accepted"], true);
        assert_eq!(minimum["result"], "applied");
        assert_eq!(minimum["display"], "unchanged");
        assert_eq!(minimum["granted_s"], 30);
        assert_eq!(minimum["include_context"], true);
        assert_eq!(minimum["deadline_ms"], 90_000);
        assert_eq!(minimum["remaining_s"], 30);

        let sleep = decide(p, serde_json::json!({"plan_id": 3, "mode": "sleep"}), 61_000, 0);
        assert_eq!(sleep["accepted"], true);
        assert_eq!(sleep["result"], "applied");
        assert_eq!(sleep["display"], "unchanged");
        assert!(sleep.get("error").is_none());
        assert_eq!(sleep["plan_id"], 3);
        assert_eq!(sleep["granted_s"], 0);
        assert_eq!(sleep["include_context"], true);
        assert_eq!(sleep["light_active"], false);
        assert_eq!(sleep["high_id"], 3);
        codex_v2_plan_free(p);

        let normal = codex_v2_plan_new();
        let normal_cap = decide(
            normal,
            serde_json::json!({"plan_id": 8, "mode": "light", "light_duration_s": 900}),
            0,
            0,
        );
        assert_eq!(normal_cap["accepted"], true);
        assert_eq!(normal_cap["granted_s"], 600);
        codex_v2_plan_free(normal);
    }
}

#[test]
fn shared_activate_decision_classifies_replay_and_profile_switches() {
    let profile = serde_json::json!({
        "context": "ctx-current",
        "ids": ["t0", "t1", "t2", "t3", "t4", "t5", "t6", "t7"]
    });
    let profile_text = std::ffi::CString::new(profile.to_string()).unwrap();
    let harness = unsafe { codex_v2_activate_new(1, profile_text.as_ptr()) };
    assert!(!harness.is_null());
    let decide = |message: serde_json::Value| unsafe {
        let text = std::ffi::CString::new(message.to_string()).unwrap();
        let mut out = vec![0i8; 2048];
        let rc = codex_v2_activate_decide(harness, text.as_ptr(), out.as_mut_ptr(), out.len() as c_int);
        assert!(rc > 0);
        serde_json::from_str::<serde_json::Value>(
            std::ffi::CStr::from_ptr(out.as_ptr()).to_str().unwrap(),
        )
        .unwrap()
    };
    let command = |request: &str, template: &str, expected: &str| {
        serde_json::json!({
            "request_id": request,
            "bridge_id": "owner",
            "template_id": template,
            "expected_active_context_id": expected,
        })
    };

    let switch = decide(command("req-new", "t7", "ctx-current"));
    assert_eq!(switch["action"], "switch");
    assert_eq!(switch["index"], 7);
    assert_eq!(switch["saved_request"], "");

    unsafe {
        let request = std::ffi::CString::new("req-done").unwrap();
        let owner = std::ffi::CString::new("owner").unwrap();
        let template = std::ffi::CString::new("t3").unwrap();
        let expected = std::ffi::CString::new("ctx-before").unwrap();
        let context = std::ffi::CString::new("ctx-after").unwrap();
        codex_v2_activate_seed_fingerprint(
            harness, request.as_ptr(), owner.as_ptr(), template.as_ptr(),
            expected.as_ptr(), context.as_ptr(),
        );
    }
    let replay = decide(command("req-done", "t3", "ctx-before"));
    assert_eq!(replay["action"], "replay");
    assert_eq!(replay["result"], "applied");
    assert_eq!(replay["display"], "unchanged");
    assert!(replay.get("error").is_none());
    assert_eq!(replay["context"], "ctx-after");
    assert_eq!(replay["saved_request"], "req-done");
    assert_eq!(replay["saved_template"], "t3");

    let conflict = decide(command("req-done", "t4", "ctx-before"));
    assert_eq!(conflict["action"], "reject");
    assert_eq!(conflict["result"], "rejected");
    assert_eq!(conflict["display"], "unchanged");
    assert_eq!(conflict["error"], "request_conflict");
    assert_eq!(conflict["context"], "ctx-after");
    assert_eq!(conflict["saved_template"], "t3");

    let stale_context = decide(command("req-old", "t0", "ctx-old"));
    assert_eq!(stale_context["action"], "reject");
    assert_eq!(stale_context["result"], "rejected");
    assert_eq!(stale_context["error"], "context");
    assert_eq!(stale_context["context"], "ctx-current");
    assert_eq!(stale_context["saved_request"], "req-done");

    let unknown = decide(command("req-unknown", "missing", "ctx-current"));
    assert_eq!(unknown["action"], "reject");
    assert_eq!(unknown["error"], "unknown_template");
    assert_eq!(unknown["saved_request"], "req-done");
    unsafe { codex_v2_activate_free(harness) };

    let unconfigured = unsafe { codex_v2_activate_new(0, profile_text.as_ptr()) };
    assert!(!unconfigured.is_null());
    let text = std::ffi::CString::new(command("req", "t0", "ctx-current").to_string()).unwrap();
    let mut out = vec![0i8; 2048];
    assert!(unsafe {
        codex_v2_activate_decide(unconfigured, text.as_ptr(), out.as_mut_ptr(), out.len() as c_int)
    } > 0);
    let decision = unsafe {
        serde_json::from_str::<serde_json::Value>(
            std::ffi::CStr::from_ptr(out.as_ptr()).to_str().unwrap(),
        )
        .unwrap()
    };
    assert_eq!(decision["result"], "rejected");
    assert_eq!(decision["display"], "unchanged");
    assert_eq!(decision["error"], "context");
    assert_eq!(decision["saved_request"], "");
    unsafe { codex_v2_activate_free(unconfigured) };
}

#[test]
fn shared_command_envelope_checks_session_and_builds_ack_shape() {
    fn output_from_parse(message: &str) -> serde_json::Value {
        let message = std::ffi::CString::new(message).unwrap();
        let mut out = vec![0i8; 2048];
        let rc = unsafe { codex_v2_command_parse(message.as_ptr(), out.as_mut_ptr(), out.len() as c_int) };
        assert!(rc > 0);
        unsafe {
            serde_json::from_str::<serde_json::Value>(
                std::ffi::CStr::from_ptr(out.as_ptr()).to_str().unwrap(),
            )
            .unwrap()
        }
    }
    fn check(message: &serde_json::Value) -> serde_json::Value {
        let message = std::ffi::CString::new(message.to_string()).unwrap();
        let mac = std::ffi::CString::new("70:04:1D:AA:BB:CC").unwrap();
        let nonce = std::ffi::CString::new("session-nonce").unwrap();
        let mut out = vec![0i8; 2048];
        let rc = unsafe {
            codex_v2_command_check(
                message.as_ptr(), mac.as_ptr(), nonce.as_ptr(),
                out.as_mut_ptr(), out.len() as c_int,
            )
        };
        assert!(rc > 0);
        unsafe {
            serde_json::from_str::<serde_json::Value>(
                std::ffi::CStr::from_ptr(out.as_ptr()).to_str().unwrap(),
            )
            .unwrap()
        }
    }
    fn ack(error: *const std::os::raw::c_char, seq: i64, plan_id: u64,
           context: *const std::os::raw::c_char, remaining: u32) -> serde_json::Value {
        let op = std::ffi::CString::new("plan").unwrap();
        let result = std::ffi::CString::new("applied").unwrap();
        let display = std::ffi::CString::new("unchanged").unwrap();
        let retention = std::ffi::CString::new("ram").unwrap();
        let target = std::ffi::CString::new("codex-status-test").unwrap();
        let mut out = vec![0i8; 2048];
        let rc = unsafe {
            codex_v2_build_ack(
                op.as_ptr(), result.as_ptr(), display.as_ptr(), retention.as_ptr(),
                error, seq, plan_id, context, remaining, target.as_ptr(),
                out.as_mut_ptr(), out.len() as c_int,
            )
        };
        assert!(rc > 0);
        unsafe {
            serde_json::from_str::<serde_json::Value>(
                std::ffi::CStr::from_ptr(out.as_ptr()).to_str().unwrap(),
            )
            .unwrap()
        }
    }

    let malformed = output_from_parse("{");
    assert_eq!(malformed["parsed"], false);
    assert_eq!(malformed["error"], "json");

    let mut message = serde_json::json!({
        "bridge_id": "owner-to-check-outside",
        "protocol": 2,
        "device_mac": "70:04:1D:AA:BB:CC",
        "request_id": "r",
        "session_nonce": "session-nonce",
    });
    let parsed = output_from_parse(&message.to_string());
    assert_eq!(parsed["parsed"], true);
    assert_eq!(parsed["bridge_id"], "owner-to-check-outside");

    let accepted = check(&message);
    assert_eq!(accepted["accepted"], true);
    assert!(accepted.get("error").is_none());
    assert_eq!(accepted["bridge_id"], "owner-to-check-outside");
    message["protocol"] = 1.into();
    assert_eq!(check(&message)["error"], "session");
    message["protocol"] = 2.into();
    message["device_mac"] = "70:04:1D:AA:BB:CD".into();
    assert_eq!(check(&message)["error"], "session");
    message["device_mac"] = "70:04:1D:AA:BB:CC".into();
    message["request_id"] = "".into();
    assert_eq!(check(&message)["error"], "session");
    message["request_id"] = "x".repeat(65).into();
    assert_eq!(check(&message)["error"], "session");
    message["request_id"] = "x".repeat(64).into();
    assert_eq!(check(&message)["accepted"], true);
    message["session_nonce"] = "other".into();
    assert_eq!(check(&message)["error"], "session");

    let minimal = ack(std::ptr::null(), -1, 0, std::ptr::null(), u32::MAX);
    assert_eq!(minimal, serde_json::json!({
        "op": "plan", "result": "applied", "display_state": "unchanged",
        "retention": "ram", "fw_target": "codex-status-test"
    }));
    let error = std::ffi::CString::new("plan_limit").unwrap();
    let context = std::ffi::CString::new("ctx-1").unwrap();
    let full = ack(error.as_ptr(), 5, 7, context.as_ptr(), 0);
    assert_eq!(full["error"], "plan_limit");
    assert_eq!(full["data_seq"], 5);
    assert_eq!(full["plan_id"], 7);
    assert_eq!(full["active_context_id"], "ctx-1");
    assert_eq!(full["accepted_remaining_s"], 0);
    assert!(full.get("ack").is_none());
    assert!(full.get("request_id").is_none());
}

#[test]
fn shared_claim_decisions_prepare_text_and_classify_owner_actions() {
    let current = serde_json::json!({
        "id": "held", "name": "Held", "host": "old-host", "port": 7,
        "since": 11, "last_seen": 22, "lease": 300
    });
    let call = |input: serde_json::Value, have_owner: bool| unsafe {
        let message = std::ffi::CString::new(input.to_string()).unwrap();
        let owner = std::ffi::CString::new(current.to_string()).unwrap();
        let mut out = vec![0i8; 2048];
        let rc = codex_v2_claim_decide(
            message.as_ptr(), have_owner as c_int, owner.as_ptr(),
            out.as_mut_ptr(), out.len() as c_int,
        );
        assert!(rc > 0);
        serde_json::from_str::<serde_json::Value>(
            std::ffi::CStr::from_ptr(out.as_ptr()).to_str().unwrap(),
        )
        .unwrap()
    };
    let args = |id: &str, force: bool, release: bool, port: &str,
                lease: &str, has_lease: bool| {
        serde_json::json!({
            "id": id, "name": "é".repeat(16) + "z", "host": "host\nname",
            "port": port, "lease": lease, "has_lease": has_lease,
            "force": force, "release": release
        })
    };
    let check_current = |decision: &serde_json::Value| {
        assert_eq!(decision["current"]["id"], "held");
        assert_eq!(decision["current"]["name"], "Held");
        assert_eq!(decision["current"]["host"], "old-host");
        assert_eq!(decision["current"]["since"], 11);
        assert_eq!(decision["current"]["last_seen"], 22);
        assert_eq!(decision["current"]["lease"], 300);
    };

    let oversized_id = format!("{}zQ", "é".repeat(31));
    let prepared = call(args(&oversized_id, false, false, "65535", "9999", true), false);
    assert_eq!(prepared["valid_id"], true);
    assert_eq!(prepared["action"], "claim");
    assert_eq!(prepared["request_id"], format!("{}z", "é".repeat(31)));
    assert_eq!(prepared["request_name"], "é".repeat(16));
    assert_eq!(prepared["request_host"], "host?name");
    assert_eq!(prepared["request_port"], 65535);
    assert_eq!(prepared["request_lease"], 3600);
    assert_eq!(prepared["keep_since"], false);
    assert_eq!(prepared["new_claim"], true);
    check_current(&prepared);

    let sanitized = call(args("  é🙂x \n ", false, false, "0", "1", true), false);
    assert_eq!(sanitized["request_id"], "é🙂x ?");
    assert_eq!(sanitized["request_port"], 0);
    assert_eq!(sanitized["request_lease"], 60);
    let default_lease = call(args("new", false, false, "65536", "ignored", false), false);
    assert_eq!(default_lease["request_port"], 0);
    assert_eq!(default_lease["request_lease"], 300);

    let empty = call(args("   ", false, false, "80", "300", true), true);
    assert_eq!(empty["valid_id"], false);
    assert_eq!(empty["action"], "args");
    check_current(&empty);

    let renew = call(args("held", false, false, "80", "120", true), true);
    assert_eq!(renew["action"], "claim");
    assert_eq!(renew["keep_since"], true);
    assert_eq!(renew["new_claim"], false);
    check_current(&renew);

    let occupied = call(args("other", false, false, "80", "300", true), true);
    assert_eq!(occupied["action"], "occupied");
    assert_eq!(occupied["keep_since"], false);
    check_current(&occupied);
    let force_claim = call(args("other", true, false, "80", "300", true), true);
    assert_eq!(force_claim["action"], "claim");
    assert_eq!(force_claim["new_claim"], true);
    check_current(&force_claim);

    let release_empty = call(args("other", false, true, "80", "300", true), false);
    assert_eq!(release_empty["action"], "release_empty");
    let release_occupied = call(args("other", false, true, "80", "300", true), true);
    assert_eq!(release_occupied["action"], "occupied");
    check_current(&release_occupied);
    let release_same = call(args("held", false, true, "80", "300", true), true);
    assert_eq!(release_same["action"], "release");
    check_current(&release_same);
    let release_force = call(args("other", true, true, "80", "300", true), true);
    assert_eq!(release_force["action"], "release");
    check_current(&release_force);
}

#[test]
fn shared_bundle_begin_decision_preserves_rx_for_replay_and_rejection() {
    let nonce = std::ffi::CString::new("session").unwrap();
    let message = |owner: &str, request: &str, length: u32, crc: &str| {
        serde_json::json!({
            "bridge_id": owner,
            "request_id": request,
            "length": length,
            "content_crc": crc,
        })
    };
    let decide = |p: *mut c_void, body: serde_json::Value, now_ms| unsafe {
        let text = std::ffi::CString::new(body.to_string()).unwrap();
        let mut out = vec![0i8; 2048];
        let rc = codex_v2_bundle_begin_decide(
            p,
            text.as_ptr(),
            nonce.as_ptr(),
            now_ms,
            out.as_mut_ptr(),
            out.len() as c_int,
        );
        assert!(rc > 0);
        serde_json::from_str::<serde_json::Value>(
            std::ffi::CStr::from_ptr(out.as_ptr()).to_str().unwrap(),
        )
        .unwrap()
    };
    unsafe {
        let p = codex_v2_bundle_begin_new();
        let original = message("owner", "request-a", 100, "12345678");
        let start = decide(p, original.clone(), 1_000);
        assert_eq!(start["action"], "start");
        assert_eq!(start["next_offset"], 0);
        assert_eq!(start["current"]["deadline"], 0);
        assert_eq!(start["candidate"]["owner"], "owner");
        assert_eq!(start["candidate"]["length"], 100);
        assert_eq!(start["candidate"]["crc"], 0x12345678u32);
        assert_eq!(start["candidate"]["deadline"], 121_000);
        assert_eq!(codex_v2_bundle_begin_commit_candidate(p), 1);

        assert_eq!(codex_v2_bundle_begin_seed_rx(
            p,
            std::ffi::CString::new("owner").unwrap().as_ptr(),
            std::ffi::CString::new("request-a").unwrap().as_ptr(),
            nonce.as_ptr(), 100, 0x12345678, 40, 121_000
        ), 1);
        let resume = decide(p, original.clone(), 2_000);
        assert_eq!(resume["action"], "resume");
        assert_eq!(resume["next_offset"], 40);
        assert_eq!(resume["current"]["offset"], 40);
        assert_eq!(resume["current"]["deadline"], 121_000, "resume must not renew");

        let before = resume["current"].clone();
        let busy = decide(p, message("owner", "request-b", 100, "87654321"), 2_000);
        assert_eq!(busy["action"], "reject");
        assert_eq!(busy["error"], "busy");
        assert_eq!(busy["current"], before);
        let conflict = decide(p, message("owner", "request-a", 100, "87654321"), 2_000);
        assert_eq!(conflict["action"], "reject");
        assert_eq!(conflict["error"], "request_conflict");
        assert_eq!(conflict["current"], before);
        let bad_crc = decide(p, message("owner", "request-b", 100, "bad"), 2_000);
        assert_eq!(bad_crc["action"], "reject");
        assert_eq!(bad_crc["error"], "crc");
        assert_eq!(bad_crc["current"], before);
        let bad_size = decide(p, message("owner", "request-b", 262_145, "12345678"), 121_000);
        assert_eq!(bad_size["action"], "reject");
        assert_eq!(bad_size["error"], "size");
        assert_eq!(bad_size["current"], before);

        let expired = decide(p, message("owner", "request-c", 50, "aaaaaaaa"), 121_000);
        assert_eq!(expired["action"], "start");
        assert_eq!(expired["candidate"]["request"], "request-c");
        assert_eq!(expired["current"], before);

        let committed_owner = std::ffi::CString::new("done-owner").unwrap();
        let committed_request = std::ffi::CString::new("done-request").unwrap();
        let committed_context = std::ffi::CString::new("committed-context").unwrap();
        codex_v2_bundle_begin_committed(
            p,
            committed_owner.as_ptr(),
            committed_request.as_ptr(),
            0xabcdef01,
            80,
            committed_context.as_ptr(),
        );
        let before_replay = expired["current"].clone();
        let replay = decide(p, message("done-owner", "done-request", 80, "abcdef01"), 121_000);
        assert_eq!(replay["action"], "replay");
        assert_eq!(replay["replay_context"], "committed-context");
        assert_eq!(replay["current"], before_replay);
        let replay_conflict = decide(
            p,
            message("done-owner", "done-request", 81, "abcdef01"),
            121_000,
        );
        assert_eq!(replay_conflict["action"], "reject");
        assert_eq!(replay_conflict["error"], "request_conflict");
        assert_eq!(replay_conflict["current"], before_replay);
        codex_v2_bundle_begin_free(p);
    }
}

#[test]
fn shared_bundle_chunk_decisions_check_identity_offsets_and_end_state() {
    unsafe {
        let p = codex_v2_bundle_begin_new();
        let owner = std::ffi::CString::new("owner").unwrap();
        let request = std::ffi::CString::new("request").unwrap();
        let nonce = std::ffi::CString::new("session").unwrap();
        assert_eq!(codex_v2_bundle_begin_seed_rx(
            p, owner.as_ptr(), request.as_ptr(), nonce.as_ptr(),
            100, 0x12345678, 50, 10_000
        ), 1);

        let start = |request: &std::ffi::CString, nonce: &std::ffi::CString,
                     offset: &std::ffi::CString, now_ms| {
            let mut out = vec![0i8; 1024];
            assert!(codex_v2_bundle_chunk_start(
                p, request.as_ptr(), nonce.as_ptr(), offset.as_ptr(), now_ms,
                out.as_mut_ptr(), out.len() as c_int
            ) > 0);
            serde_json::from_str::<serde_json::Value>(
                std::ffi::CStr::from_ptr(out.as_ptr()).to_str().unwrap()
            ).unwrap()
        };
        let replay = start(&request, &nonce, &std::ffi::CString::new("20").unwrap(), 100);
        assert_eq!(replay["allowed"], true);
        assert_eq!(replay["replay"], true);
        assert_eq!(replay["offset"], 20);
        let append = start(&request, &nonce, &std::ffi::CString::new("50").unwrap(), 100);
        assert_eq!(append["allowed"], true);
        assert_eq!(append["replay"], false);
        assert_eq!(append["offset"], 50);

        let before = replay["current"].clone();
        for (request_id, session, offset, now_ms, error) in [
            ("wrong", "session", "20", 100, "session"),
            ("request", "wrong", "20", 100, "session"),
            ("request", "session", "", 100, "session"),
            ("request", "session", "2x", 100, "session"),
            ("request", "session", "51", 100, "offset_or_size"),
            ("request", "session", "20", 10_000, "session"),
        ] {
            let got = start(
                &std::ffi::CString::new(request_id).unwrap(),
                &std::ffi::CString::new(session).unwrap(),
                &std::ffi::CString::new(offset).unwrap(),
                now_ms,
            );
            assert_eq!(got["allowed"], false);
            assert_eq!(got["error"], error);
            assert_eq!(got["current"], before);
        }

        let write = |offset, replay, processed, size| {
            let mut out = vec![0i8; 1024];
            assert!(codex_v2_bundle_chunk_write(
                p, offset, replay, processed, size, out.as_mut_ptr(), out.len() as c_int
            ) > 0);
            serde_json::from_str::<serde_json::Value>(
                std::ffi::CStr::from_ptr(out.as_ptr()).to_str().unwrap()
            ).unwrap()
        };
        let write_ok = write(80, 0, 10, 10);
        assert_eq!(write_ok["allowed"], true);
        let write_too_far = write(90, 0, 5, 6);
        assert_eq!(write_too_far["allowed"], false);
        assert_eq!(write_too_far["error"], "offset_or_size");
        let write_u64_sum = write(0, 0, u32::MAX, 2);
        assert_eq!(write_u64_sum["allowed"], false);
        let replay_ok = write(20, 1, 20, 10);
        assert_eq!(replay_ok["allowed"], true);
        let replay_too_far = write(20, 1, 20, 11);
        assert_eq!(replay_too_far["allowed"], false);

        let end = |offset, replay, processed, now_ms| {
            let mut out = vec![0i8; 1024];
            assert!(codex_v2_bundle_chunk_end(
                p, offset, replay, processed, now_ms, out.as_mut_ptr(), out.len() as c_int
            ) > 0);
            serde_json::from_str::<serde_json::Value>(
                std::ffi::CStr::from_ptr(out.as_ptr()).to_str().unwrap()
            ).unwrap()
        };
        let end_ok = end(50, 0, 10, 100);
        assert_eq!(end_ok["allowed"], true);
        assert_eq!(end_ok["next_offset"], 60);
        assert_eq!(end_ok["current"], before, "decision must not mutate current RX");
        let replay_end = end(20, 1, 10, 10_000);
        assert_eq!(replay_end["allowed"], true);
        assert_eq!(replay_end["next_offset"], 50);
        for (offset, replay, processed, now_ms) in [
            (50, 0, 0, 100),
            (50, 0, 16_385, 100),
            (49, 0, 1, 100),
            (50, 0, 1, 10_000),
        ] {
            let got = end(offset, replay, processed, now_ms);
            assert_eq!(got["allowed"], false);
            assert_eq!(got["error"], "offset_or_size");
            assert_eq!(got["current"], before);
        }
        codex_v2_bundle_begin_free(p);
    }
}

#[test]
fn shared_bundle_commit_decision_validates_payload_before_side_effects() {
    fn make_bundle(job_id: &str) -> String {
        let source: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../tools/test-bridge/templates/quad.json"
        ))
        .unwrap();
        let compiled = bridge_core::compile::compile(
            &source,
            "epd-ssd1681-200x200-1bpp",
        )
        .unwrap();
        let bundle = serde_json::json!({
            "job_id": job_id,
            "bridge_id": "owner",
            "firmware_target": "codex-status-154g",
            "render_target": "epd-ssd1681-200x200-1bpp",
            "compiler_abi": 2,
            "profile": {"template_ids": ["quad"], "initial_active_id": "quad"},
            "templates": [{
                "key": {"template_id": "quad", "render_target": "epd-ssd1681-200x200-1bpp"},
                "source": source,
                "compiled": compiled
            }],
            "resources": [],
            "bindings": []
        });
        String::from_utf8(bridge_core::template::canonical_bytes(&bundle)).unwrap()
    }
    fn crc(body: &str) -> u32 {
        unsafe { codex_v2_crc(body.as_ptr(), body.len() as c_int) }
    }

    let path = std::ffi::CString::new("/bundle/rx.bin").unwrap();
    let fallback = std::ffi::CString::new("owner").unwrap();
    let command = |owner: &str, request: &str, length: u32, crc: &str| {
        serde_json::json!({
            "bridge_id": owner,
            "request_id": request,
            "length": length,
            "content_crc": crc,
        })
    };
    let seed = |p: *mut c_void, owner: &str, request: &str, rx_nonce: &str,
                length: u32, crc: u32, offset: u32| unsafe {
        codex_v2_bundle_commit_seed_rx(
            p,
            std::ffi::CString::new(owner).unwrap().as_ptr(),
            std::ffi::CString::new(request).unwrap().as_ptr(),
            std::ffi::CString::new(rx_nonce).unwrap().as_ptr(),
            length,
            crc,
            offset,
            10_000,
        )
    };
    let write_file = |file_path: &std::ffi::CString, body: &[u8]| unsafe {
        codex_v2_bundle_commit_write_file(
            file_path.as_ptr(),
            body.as_ptr(),
            body.len() as c_int,
        )
    };
    let decide = |p: *mut c_void, message: serde_json::Value, nonce: &str,
                  file_path: &std::ffi::CString, fallback: &std::ffi::CString,
                  now_ms: u64| unsafe {
        let text = std::ffi::CString::new(message.to_string()).unwrap();
        let nonce = std::ffi::CString::new(nonce).unwrap();
        let mut out = vec![0i8; 4096];
        let mut body_out = vec![0i8; 300_000];
        let rc = codex_v2_bundle_commit_decide(
            p,
            text.as_ptr(),
            nonce.as_ptr(),
            now_ms,
            file_path.as_ptr(),
            fallback.as_ptr(),
            out.as_mut_ptr(),
            out.len() as c_int,
            body_out.as_mut_ptr(),
            body_out.len() as c_int,
        );
        assert!(rc > 0);
        let result = serde_json::from_str::<serde_json::Value>(
            std::ffi::CStr::from_ptr(out.as_ptr()).to_str().unwrap(),
        )
        .unwrap();
        let body = std::ffi::CStr::from_ptr(body_out.as_ptr())
            .to_string_lossy()
            .into_owned();
        (result, body)
    };

    unsafe {
        assert_eq!(codex_v2_bundle_commit_reset_store(), 1);
        let p = codex_v2_bundle_begin_new();

        // A submitted owner/request pair replays before session and file checks.
        let replay_body = "replay payload";
        let replay_crc = crc(replay_body);
        let committed_owner = std::ffi::CString::new("done-owner").unwrap();
        let committed_request = std::ffi::CString::new("done-request").unwrap();
        let committed_context = std::ffi::CString::new("done-context").unwrap();
        codex_v2_bundle_begin_committed(
            p,
            committed_owner.as_ptr(),
            committed_request.as_ptr(),
            replay_crc,
            replay_body.len() as u32,
            committed_context.as_ptr(),
        );
        assert_eq!(seed(p, "owner", "live-request", "session", 40, 0x11111111, 40), 1);
        let replay = decide(
            p,
            command("done-owner", "done-request", replay_body.len() as u32,
                    &format!("{replay_crc:08x}")),
            "different-session",
            &std::ffi::CString::new("/bundle/missing.bin").unwrap(),
            &fallback,
            50_000,
        ).0;
        assert_eq!(replay["action"], "replay");
        assert_eq!(replay["replay_context"], "done-context");
        assert_eq!(replay["current"]["request"], "live-request");
        let replay_conflict = decide(
            p,
            command("done-owner", "done-request", 99, &format!("{replay_crc:08x}")),
            "session",
            &path,
            &fallback,
            1,
        ).0;
        assert_eq!(replay_conflict["action"], "reject");
        assert_eq!(replay_conflict["error"], "request_conflict");
        assert_eq!(replay_conflict["current"]["request"], "live-request");

        // All ordinary validation errors leave the current RX metadata intact.
        codex_v2_bundle_begin_committed(
            p,
            std::ffi::CString::new("").unwrap().as_ptr(),
            std::ffi::CString::new("").unwrap().as_ptr(),
            0, 0, std::ffi::CString::new("").unwrap().as_ptr(),
        );
        let simple_body = r#"{"bridge_id":"owner","job_id":"other"}"#;
        let simple_crc = crc(simple_body);
        let simple_bytes = simple_body.as_bytes();
        let rx_request = "request";
        assert_eq!(seed(p, "owner", rx_request, "wrong-session", simple_bytes.len() as u32, simple_crc, simple_bytes.len() as u32), 1);
        let bad_crc = decide(
            p,
            command("owner", rx_request, simple_bytes.len() as u32, "nope"),
            "session", &path, &fallback, 1,
        ).0;
        assert_eq!(bad_crc["error"], "crc");
        assert_eq!(bad_crc["current"]["offset"], simple_bytes.len());

        assert_eq!(seed(p, "owner", rx_request, "wrong-session", simple_bytes.len() as u32, simple_crc, simple_bytes.len() as u32), 1);
        let session_error = decide(
            p,
            command("owner", rx_request, simple_bytes.len() as u32, &format!("{simple_crc:08x}")),
            "session", &path, &fallback, 1,
        ).0;
        assert_eq!(session_error["error"], "session_or_length");
        assert_eq!(session_error["current"]["nonce"], "wrong-session");

        assert_eq!(seed(p, "owner", rx_request, "session", simple_bytes.len() as u32, simple_crc, simple_bytes.len() as u32 - 1), 1);
        let incomplete = decide(
            p,
            command("owner", rx_request, simple_bytes.len() as u32, &format!("{simple_crc:08x}")),
            "session", &path, &fallback, 1,
        ).0;
        assert_eq!(incomplete["error"], "session_or_length");
        assert_eq!(incomplete["current"]["offset"], simple_bytes.len() - 1);

        assert_eq!(seed(p, "owner", rx_request, "session", simple_bytes.len() as u32, simple_crc, simple_bytes.len() as u32), 1);
        let missing = decide(
            p,
            command("owner", rx_request, simple_bytes.len() as u32, &format!("{simple_crc:08x}")),
            "session", &std::ffi::CString::new("/bundle/missing.bin").unwrap(), &fallback, 1,
        ).0;
        assert_eq!(missing["error"], "length");

        assert_eq!(seed(p, "owner", rx_request, "session", simple_bytes.len() as u32, simple_crc, simple_bytes.len() as u32), 1);
        assert_eq!(write_file(&path, &simple_bytes[..simple_bytes.len() - 1]), 1);
        let wrong_file_length = decide(
            p,
            command("owner", rx_request, simple_bytes.len() as u32, &format!("{simple_crc:08x}")),
            "session", &path, &fallback, 1,
        ).0;
        assert_eq!(wrong_file_length["error"], "length");

        let mut corrupt = simple_bytes.to_vec();
        corrupt[0] ^= 1;
        assert_eq!(seed(p, "owner", rx_request, "session", simple_bytes.len() as u32, simple_crc, simple_bytes.len() as u32), 1);
        assert_eq!(write_file(&path, &corrupt), 1);
        let bad_body_crc = decide(
            p,
            command("owner", rx_request, simple_bytes.len() as u32, &format!("{simple_crc:08x}")),
            "session", &path, &fallback, 1,
        ).0;
        assert_eq!(bad_body_crc["error"], "crc");

        let wrong_owner_body = r#"{"bridge_id":"someone-else","job_id":"other"}"#;
        let wrong_owner_crc = crc(wrong_owner_body);
        assert_eq!(seed(p, "owner", rx_request, "session", wrong_owner_body.len() as u32, wrong_owner_crc, wrong_owner_body.len() as u32), 1);
        assert_eq!(write_file(&path, wrong_owner_body.as_bytes()), 1);
        let wrong_owner = decide(
            p,
            command("owner", rx_request, wrong_owner_body.len() as u32, &format!("{wrong_owner_crc:08x}")),
            "session", &path, &fallback, 1,
        ).0;
        assert_eq!(wrong_owner["error"], "owner");

        let malformed = "{";
        let malformed_crc = crc(malformed);
        assert_eq!(seed(p, "owner", rx_request, "session", malformed.len() as u32, malformed_crc, malformed.len() as u32), 1);
        assert_eq!(write_file(&path, malformed.as_bytes()), 1);
        let json_error = decide(
            p,
            command("owner", rx_request, malformed.len() as u32, &format!("{malformed_crc:08x}")),
            "session", &path, &fallback, 1,
        ).0;
        assert_eq!(json_error["error"], "json");

        let new_bundle = make_bundle("new-job");
        let new_bytes = new_bundle.as_bytes();
        let new_crc = crc(&new_bundle);
        assert_eq!(seed(p, "owner", rx_request, "session", new_bytes.len() as u32, new_crc, new_bytes.len() as u32), 1);
        assert_eq!(write_file(&path, new_bytes), 1);
        let install = decide(
            p,
            command("owner", rx_request, new_bytes.len() as u32, &format!("{new_crc:08x}")),
            "session", &path, &fallback, 1,
        );
        assert_eq!(install.0["action"], "install");
        assert_eq!(install.0["current"]["offset"], new_bytes.len());
        assert_eq!(install.1, new_bundle);

        // Existing valid fixture exercises both active-job branches.
        let active_bundle = make_bundle("active-job");
        let active_c = std::ffi::CString::new(active_bundle.as_str()).unwrap();
        let active_context = std::ffi::CString::new("active-context").unwrap();
        assert_eq!(codex_v2_bundle_commit_install_active(active_c.as_ptr(), active_context.as_ptr()), 1);
        let active_crc = crc(&active_bundle);
        assert_eq!(seed(p, "owner", rx_request, "session", active_bundle.len() as u32, active_crc, active_bundle.len() as u32), 1);
        assert_eq!(write_file(&path, active_bundle.as_bytes()), 1);
        let already_active = decide(
            p,
            command("owner", rx_request, active_bundle.len() as u32, &format!("{active_crc:08x}")),
            "session", &path, &fallback, 1,
        ).0;
        assert_eq!(already_active["action"], "already_active");
        assert_eq!(already_active["current"]["offset"], active_bundle.len());

        let changed_active = format!(" \n{active_bundle}");
        let changed_crc = crc(&changed_active);
        assert_eq!(seed(p, "owner", rx_request, "session", changed_active.len() as u32, changed_crc, changed_active.len() as u32), 1);
        assert_eq!(write_file(&path, changed_active.as_bytes()), 1);
        let active_conflict = decide(
            p,
            command("owner", rx_request, changed_active.len() as u32, &format!("{changed_crc:08x}")),
            "session", &path, &fallback, 1,
        ).0;
        assert_eq!(active_conflict["action"], "reject");
        assert_eq!(active_conflict["error"], "request_conflict");
        assert_eq!(active_conflict["current"]["offset"], changed_active.len());
        codex_v2_bundle_begin_free(p);
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
