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
        .warnings(false)
        .flag_if_supported("/utf-8")
        .file(firmware.join("template_engine.cpp"))
        .file(firmware.join("v2_runtime.cpp"))
        .file(firmware.join("v2_data_command.cpp"))
        .file(firmware.join("bundle_store.cpp"))
        .file(firmware.join("refresh_policy.cpp"))
        .file(firmware.join("font_asset.cpp"))
        .file(firmware.join("font_store.cpp"))
        .file(firmware.join("GUI_Paint.cpp"))
        .file(&ffi);
    for name in ["font8", "font12", "font16", "font20", "font24"] {
        build.file(firmware.join(format!("{name}.cpp")));
    }
    build.compile("bridge_render");

    // Every header the engine pulls in has to be tracked: the generated font
    // tables are headers, so without this a re-crop would leave the host preview
    // rendering the previous glyphs while the firmware builds the new ones.
    for tracked in ["template_engine.cpp", "template_engine.h", "GUI_Paint.cpp",
                    "GUI_Paint.h", "refresh_policy.cpp", "refresh_policy.h",
                    "v2_state.h", "v2_runtime.h", "v2_runtime.cpp",
                    "v2_data_command.h", "v2_data_command.cpp",
                    "bundle_store.h", "bundle_store.cpp", "fonts.h",
                    "font_noto.h", "font_noto_nt16.h", "font_noto_nt30.h",
                    "font_noto_ntthin18.h", "font_noto_ntreg64.h",
                    "font_noto_ntreg96.h",
                    "font_asset.h", "font_asset.cpp",
                    "font_store.h", "font_store.cpp",
                    "platform_target.h"] {
        println!("cargo:rerun-if-changed={}", firmware.join(tracked).display());
    }
    println!("cargo:rerun-if-changed={}", ffi.display());
    println!("cargo:rerun-if-changed={}", manifest.join("shim").display());
}
