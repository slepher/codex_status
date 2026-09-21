//! Display-safety policy tests: semantic region derivation and the
//! conservative partial/full escalation on the shared C++ logic that the
//! firmware runs (design §8). State is global on the C++ side, so all
//! assertions live in one test.

use std::path::PathBuf;
use std::sync::Mutex;

static POLICY_LOCK: Mutex<()> = Mutex::new(());

fn quad_template() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tools/test-bridge/templates/quad.json");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn set_black(fb: &mut [u8], x: usize, y: usize) {
    let stride = bridge_render::ROW_BYTES;
    fb[y * stride + x / 8] &= !(0x80u8 >> (x % 8));
}

fn fill_black(fb: &mut [u8], x0: usize, y0: usize, w: usize, h: usize) {
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            set_black(fb, x, y);
        }
    }
}

#[test]
fn region_policy_escalates_conservatively() {
    let _guard = POLICY_LOCK.lock().unwrap();
    let template = quad_template();
    let n = bridge_render::rgn_build(&template).expect("quad regions");

    let dump = bridge_render::rgn_dump();
    println!("regions n={n}: {dump}");
    // quad v11 with pixel-overlap merging: the black tiles stay separate from
    // the neighboring icons/clock/right-column text, so icon changes can use
    // the partial path (BOOT click must not flash the whole panel).
    assert!(n >= 8, "expected several semantic regions, got {n}");
    assert!(dump.contains("\"cls\":\"solid\""), "no solid region: {dump}");
    assert!(dump.contains("\"cls\":\"usage\""), "no usage region: {dump}");
    assert!(dump.contains("\"cls\":\"clock\""), "no clock region: {dump}");
    assert!(dump.contains("\"hi\":1"), "no high-ink region: {dump}");

    let white = vec![0xFFu8; bridge_render::BUF_LEN];

    // Identical frame, trusted -> no refresh.
    let (action, detail) =
        bridge_render::rgn_decide(&white, &white, true, false, false).unwrap();
    assert_eq!(action, 0, "identical frame: {detail}");

    // Identical frame but clean requested -> full refresh (bypasses memcmp).
    let (action, detail) =
        bridge_render::rgn_decide(&white, &white, true, false, true).unwrap();
    assert_eq!(action, 2, "clean request: {detail}");
    assert!(detail.contains("\"reason\":\"clean\""), "{detail}");

    // Untrusted baseline -> full refresh even when identical.
    let (action, detail) =
        bridge_render::rgn_decide(&white, &white, false, false, false).unwrap();
    assert_eq!(action, 2, "untrusted: {detail}");
    assert!(detail.contains("\"reason\":\"trust\""), "{detail}");

    // Forced full -> full refresh.
    let (action, detail) =
        bridge_render::rgn_decide(&white, &white, true, true, false).unwrap();
    assert_eq!(action, 2, "forced: {detail}");
    assert!(detail.contains("\"reason\":\"force\""), "{detail}");

    // A single status-icon pixel (BOOT click toggles the BLE icon) -> partial.
    let mut icon = white.clone();
    set_black(&mut icon, 105, 12);
    let (action, detail) =
        bridge_render::rgn_decide(&white, &icon, true, false, false).unwrap();
    assert_eq!(action, 1, "icon change must stay partial: {detail}");

    // A clock pixel -> partial (clock region is separate from the tiles).
    let mut clock = white.clone();
    set_black(&mut clock, 170, 12);
    let (action, detail) =
        bridge_render::rgn_decide(&white, &clock, true, false, false).unwrap();
    assert_eq!(action, 1, "clock change: {detail}");
    assert!(detail.contains("\"reason\":\"ok\""), "{detail}");
    bridge_render::rgn_on_partial();

    // A single low-ink status pixel (bottom-left SYNC row) -> partial.
    let mut low = white.clone();
    set_black(&mut low, 5, 188);
    let (action, detail) =
        bridge_render::rgn_decide(&white, &low, true, false, false).unwrap();
    assert_eq!(action, 1, "low-ink change: {detail}");
    assert!(detail.contains("\"reason\":\"ok\""), "{detail}");
    bridge_render::rgn_on_partial();

    // A single pixel inside the large black tile -> conservative full refresh.
    let mut tile = white.clone();
    set_black(&mut tile, 50, 50);
    let (action, detail) =
        bridge_render::rgn_decide(&white, &tile, true, false, false).unwrap();
    assert_eq!(action, 2, "high-ink tile: {detail}");
    assert!(
        detail.contains("\"reason\":\"high_ink\"") || detail.contains("\"reason\":\"polarity\""),
        "{detail}"
    );

    // More than 12.5% of the panel changed -> full refresh by area.
    let mut half = white.clone();
    fill_black(&mut half, 0, 0, 200, 60); // 12000 px > 5000
    let (action, detail) =
        bridge_render::rgn_decide(&white, &half, true, false, false).unwrap();
    assert_eq!(action, 2, "area rule: {detail}");
    assert!(detail.contains("\"reason\":\"area\""), "{detail}");

    // Low-ink budget exhaustion escalates to full.
    let mut low2 = white.clone();
    set_black(&mut low2, 9, 188);
    for _ in 0..10 {
        let (action, detail) =
            bridge_render::rgn_decide(&white, &low2, true, false, false).unwrap();
        if action == 2 {
            assert!(detail.contains("\"reason\":\"budget\""), "unexpected escalation: {detail}");
            bridge_render::rgn_on_full();
            // Any successful full refresh resets budgets.
            let (action, _) =
                bridge_render::rgn_decide(&white, &low2, true, false, false).unwrap();
            assert_eq!(action, 1, "budget reset");
            return;
        }
        assert_eq!(action, 1, "partial expected: {detail}");
        bridge_render::rgn_on_partial();
    }
    panic!("low-ink budget never exhausted after 10 partials");
}
