//! Hidden watchdog process: restarts the tray app after abnormal exits.
//!
//! Lifecycle: `bridge-app` spawns `<same exe> --watchdog <pid>` at startup.
//! The watchdog waits for the parent to exit; a clean tray Quit (exit code 0)
//! ends both processes, while a panic/crash (non-zero) relaunches the app.
//! Three abnormal exits inside five minutes stop the loop and are logged to
//! `<data>/logs/watchdog.log`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const GIVE_UP_WINDOW_SECS: u64 = 300;
const MAX_ABNORMAL: usize = 3;

fn log_path() -> PathBuf {
    let dir = bridge_core::paths::data_root().join("logs");
    let _ = fs::create_dir_all(&dir);
    dir.join("watchdog.log")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_recent_stamps() -> Vec<u64> {
    let now = now_secs();
    fs::read_to_string(log_path())
        .map(|text| {
            text.lines()
                .filter_map(|line| line.strip_prefix("abnormal "))
                .filter_map(|rest| rest.split_whitespace().next())
                .filter_map(|v| v.parse::<u64>().ok())
                .filter(|t| now.saturating_sub(*t) < GIVE_UP_WINDOW_SECS)
                .collect()
        })
        .unwrap_or_default()
}

fn append_log(line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "{line}");
    }
}

/// Restart the app with the same executable, detached and hidden.
fn relaunch() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, DETACHED_PROCESS};
        std::process::Command::new(exe)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .is_ok()
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(exe).spawn().is_ok()
    }
}

/// Spawn the hidden watchdog unless this process already is one.
pub fn spawn(parent_pid: u32) {
    if std::env::args().any(|a| a == "--watchdog") {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, DETACHED_PROCESS};
        let spawned = std::process::Command::new(exe)
            .arg("--watchdog")
            .arg(parent_pid.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn();
        if let Ok(child) = spawned {
            tracing::info!("watchdog started (pid {})", child.id());
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (exe, parent_pid);
    }
}

/// Watchdog entry point; never returns.
#[cfg(windows)]
pub fn run(parent_pid: u32) -> ! {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    const SYNCHRONIZE: u32 = 0x0010_0000;
    unsafe {
        let handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
            0,
            parent_pid,
        );
        if handle.is_null() {
            // Parent already gone; nothing to supervise.
            std::process::exit(0);
        }
        WaitForSingleObject(handle, u32::MAX);
        let mut code: u32 = 0;
        GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        if code == 0 {
            std::process::exit(0);
        }
        let recent = read_recent_stamps();
        let now = now_secs();
        let count = recent.len() + 1;
        append_log(&format!("abnormal {now} exit_code={code} recent={count}"));
        if count >= MAX_ABNORMAL {
            append_log(&format!(
                "giving up: {count} abnormal exits within {GIVE_UP_WINDOW_SECS}s"
            ));
            std::process::exit(1);
        }
        if relaunch() {
            append_log(&format!("relaunched at {now}"));
            std::process::exit(0);
        }
        append_log("relaunch failed");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
pub fn run(_parent_pid: u32) -> ! {
    std::process::exit(0);
}
