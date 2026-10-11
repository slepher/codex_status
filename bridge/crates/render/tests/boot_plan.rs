use bridge_render as _;
extern "C" {
    fn codex_plan_store_check() -> i32;
    fn codex_boot_capture_check() -> i32;
}
#[test]
fn production_plan_persistence_faults_and_reboots() {
    assert_eq!(unsafe { codex_plan_store_check() }, 0, "C++ assertion line");
}
#[test]
fn production_boot_capture_and_manual_icon_invalidation() {
    assert_eq!(unsafe { codex_boot_capture_check() }, 0, "C++ assertion line");
}
