use std::path::{Path, PathBuf};

fn find_arduinojson(root: &Path) -> PathBuf {
    if let Ok(explicit) = std::env::var("CODEX_STATUS_ARDUINOJSON") {
        let path = PathBuf::from(explicit);
        if path.join("ArduinoJson.h").is_file() {
            return path;
        }
    }
    let libdeps = root.join(".pio/libdeps");
    if let Ok(entries) = std::fs::read_dir(&libdeps) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("ArduinoJson/src");
            if candidate.join("ArduinoJson.h").is_file() {
                return candidate;
            }
        }
    }
    panic!(
        "ArduinoJson.h not found; run `pio run` once or set CODEX_STATUS_ARDUINOJSON to the ArduinoJson src directory"
    );
}

fn main() {
    println!("cargo:rerun-if-changed=../../../src/wifi_storage_commit.inc");
    println!("cargo:rerun-if-changed=../../../tools/wifi-legacy-convert.inc");
    println!("cargo:rerun-if-changed=../../../src/battery_display.h");
    println!("cargo:rerun-if-changed=src/battery_display_test.cpp");
    println!("cargo:rerun-if-changed=../../../src/status_pages.h");
    println!("cargo:rerun-if-changed=../../../src/network_display_state.h");
    println!("cargo:rerun-if-changed=src/sim_status.cpp");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../../..");
    let firmware = root.join("src");
    let shim = manifest.join("shim");
    let ffi = manifest.join("src/ffi.cpp");

    let arduinojson = find_arduinojson(&root);

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .include(&shim)
        .include(&firmware)
        .include(&arduinojson)
        .define("ARDUINOJSON_ENABLE_ARDUINO_STRING", "1")
        .define("CODEX_RENDER_NOTE4_FONTS", "1")
        .define("CODEX_BUNDLE_HOST", "1")
        .warnings(false)
        .flag_if_supported("/utf-8")
        .file(firmware.join("template_engine.cpp"))
        .file(firmware.join("v2_runtime.cpp"))
        .file(firmware.join("rendezvous_protocol.cpp"))
        .file(firmware.join("v2_data_command.cpp"))
        .file(firmware.join("v2_plan_command.cpp"))
        .file(firmware.join("plan_store.cpp"))
        .file(manifest.join("src/plan_store_test.cpp"))
        .file(manifest.join("src/battery_display_test.cpp"))
        .file(firmware.join("v2_bundle_command.cpp"))
        .file(firmware.join("v2_activate_command.cpp"))
        .file(firmware.join("v2_claim_command.cpp"))
        .file(firmware.join("v2_command_envelope.cpp"))
        .file(firmware.join("ble_wire.cpp"))
        .file(firmware.join("v2_status_snapshot.cpp"))
        .file(firmware.join("v2_sync.cpp"))
        .file(firmware.join("v2_sync_store.cpp"))
        .file(firmware.join("offline_log.cpp"))
        .file(firmware.join("v2_sync_protocol.cpp"))
        .file(firmware.join("bundle_store.cpp"))
        .file(firmware.join("bundle_payload.cpp"))
        .file(firmware.join("refresh_policy.cpp"))
        .file(firmware.join("font_asset.cpp"))
        .file(firmware.join("font_store.cpp"))
        .file(firmware.join("GUI_Paint.cpp"))
        .file(manifest.join("src/sim_sync.cpp"))
        .file(manifest.join("src/sim_wifi.cpp"))
        .file(manifest.join("src/sim_status.cpp"))
        .file(&ffi);
    for name in ["font8", "font12", "font16", "font20", "font24"] {
        build.file(firmware.join(format!("{name}.cpp")));
    }
    build.compile("bridge_render");
    // The Wi-Fi HAL reads the same zlib NVS encoding as ESP ROM miniz.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        println!("cargo:rustc-link-lib=z");
    }

    // Every header the engine pulls in has to be tracked: the generated font
    // tables are headers, so without this a re-crop would leave the host preview
    // rendering the previous glyphs while the firmware builds the new ones.
    for tracked in ["template_engine.cpp", "template_engine.h", "GUI_Paint.cpp",
                    "GUI_Paint.h", "refresh_policy.cpp", "refresh_policy.h",
                    "v2_state.h", "v2_runtime.h", "v2_runtime.cpp",
                    "rendezvous_protocol.h", "rendezvous_protocol.cpp",
                    "v2_data_command.h", "v2_data_command.cpp",
                    "v2_plan_command.h", "v2_plan_command.cpp",
                    "plan_store.h", "plan_store.cpp", "boot_capture.h",
                    "v2_bundle_command.h", "v2_bundle_command.cpp",
                    "v2_activate_command.h", "v2_activate_command.cpp",
                    "v2_claim_command.h", "v2_claim_command.cpp",
                    "v2_command_envelope.h", "v2_command_envelope.cpp",
                    "ble_wire.h", "ble_wire.cpp", "wifi_scheme.h", "wifi_legacy.h", "wifi_storage.inc", "wifi_setup.h", "wifi_setup.cpp", "wifi_management.inc", "wake_trace_projection.h",
                    "v2_status_snapshot.h", "v2_status_snapshot.cpp",
                    "v2_sync.h", "v2_sync.cpp",
                    "v2_sync_store.h", "v2_sync_store.cpp",
                    "offline_log.h", "offline_log.cpp",
                    "v2_sync_protocol.h", "v2_sync_protocol.cpp",
                    "bundle_store.h", "bundle_store.cpp", "bundle_payload.h", "bundle_payload.cpp", "fonts.h",
                    "font_noto.h", "font_noto_nt16.h", "font_noto_nt30.h",
                    "font_noto_ntthin18.h", "font_noto_ntreg64.h",
                    "font_noto_ntreg96.h",
                    "font_asset.h", "font_asset.cpp",
                    "font_store.h", "font_store.cpp",
                    "platform_target.h"] {
        println!("cargo:rerun-if-changed={}", firmware.join(tracked).display());
    }
    println!("cargo:rerun-if-changed={}", ffi.display());
    println!("cargo:rerun-if-changed={}", manifest.join("src/plan_store_test.cpp").display());
    println!("cargo:rerun-if-changed={}", manifest.join("src/sim_sync.cpp").display());
    println!("cargo:rerun-if-changed={}", manifest.join("src/sim_wifi.cpp").display());
    println!("cargo:rerun-if-changed={}", root.join("tools/wifi-setup-test-hal.h").display());
    println!("cargo:rerun-if-changed={}", manifest.join("shim").display());
}
