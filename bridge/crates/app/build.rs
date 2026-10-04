fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        let object = out.join("host_wifi.o");
        let arch = if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64") {"arm64"} else {"x86_64"};
        assert!(std::process::Command::new("clang").args(["-arch", arch, "-c", "-fobjc-arc", "-fblocks", "-mmacosx-version-min=14.0", "src/host_wifi.m", "-o"])
            .arg(&object).status().unwrap().success());
        assert!(std::process::Command::new("ar").arg("rcs").arg(out.join("libhost_wifi.a")).arg(&object)
            .status().unwrap().success());
        println!("cargo:rustc-link-search=native={}", out.display());
        println!("cargo:rustc-link-lib=static=host_wifi");
        for framework in ["CoreWLAN", "CoreLocation", "CoreBluetooth", "AppKit", "Foundation"] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
        println!("cargo:rerun-if-changed=src/host_wifi.m");
    }
    tauri_build::build()
}
