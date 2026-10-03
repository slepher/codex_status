//! Optional macOS login launch. On-demand supervision is a separate entry.

use std::io;
use std::path::PathBuf;

fn path() -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| io::Error::other("user home missing"))?;
    Ok(PathBuf::from(home).join("Library/LaunchAgents/dev.codexstatus.bridge.login.plist"))
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn registered() -> Option<String> {
    std::fs::read_to_string(path().ok()?).ok()
}

pub fn enabled() -> bool {
    registered().is_some()
}

pub fn matches_current_exe() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    registered().is_some_and(|text| {
        text.contains(&format!("<string>{}</string>", xml(&exe.to_string_lossy())))
    })
}

pub fn set(enable: bool) -> io::Result<()> {
    if !cfg!(target_os = "macos") {
        return Err(io::Error::other("login launch unsupported"));
    }
    let path = path()?;
    if !enable {
        return match std::fs::remove_file(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        };
    }
    let exe = std::env::current_exe()?;
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>dev.codexstatus.bridge.login</string>
<key>ProgramArguments</key><array><string>{}</string></array>
<key>RunAtLoad</key><true/>
<key>ProcessType</key><string>Interactive</string>
</dict></plist>
"#,
        xml(&exe.to_string_lossy())
    );
    std::fs::create_dir_all(path.parent().unwrap())?;
    let temp = path.with_extension("plist.tmp");
    std::fs::write(&temp, body)?;
    std::fs::rename(temp, path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn launch_argument_escapes_xml_without_shell_expansion() {
        assert_eq!(super::xml("/a&b/<c>\"'"), "/a&amp;b/&lt;c&gt;&quot;&apos;");
    }
}
