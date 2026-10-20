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
const BLE_RECOVERY_EXIT: i32 = 75;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct BleRecovery {
    pub state: String,
    pub error: String,
    pub stage: String,
    pub action: String,
    pub attempt: u32,
    pub failed_at: u64,
    pub retry_at: u64,
    pub recovered_at: Option<u64>,
}

impl BleRecovery {
    fn next(previous: Option<&Self>, now: u64, automatic: bool) -> Self {
        // Only a sustained healthy interval resets the backoff.
        let attempt = previous.filter(|p| p.recovered_at.is_none_or(|at| now.saturating_sub(at) < 300))
            .map_or(1, |p| p.attempt.saturating_add(1));
        let delay = match attempt { 1 => 5, 2 => 30, 3 => 120, _ => 300 };
        Self { state: if automatic { "recovering" } else { "restart_required" }.into(),
            error: "ble_native_quarantined_restart_required".into(), stage: "native_shutdown".into(),
            action: if automatic { "wait_for_automatic_recovery" } else { "restart_managed_bridge" }.into(),
            attempt, failed_at: now, retry_at: now.saturating_add(delay), recovered_at: None }
    }
}

fn recovery_path() -> PathBuf { bridge_core::paths::data_root().join("ble-recovery.json") }
pub(crate) fn ble_recovery() -> Option<BleRecovery> {
    serde_json::from_slice(&fs::read(recovery_path()).ok()?).ok()
}
fn save_recovery(recovery: &BleRecovery) -> anyhow::Result<()> {
    bridge_core::platform::store::write_json(&recovery_path(), recovery)
}

/// Called even when paused or permissions are unavailable. The old native
/// process exits before the supervisor is allowed to construct its replacement.
pub(crate) fn recover_ble(pending: &mut Option<BleRecovery>, failure: Option<&str>) {
    let Some(failure)=failure else{return;};
    let automatic = cfg!(windows) || std::env::args().any(|arg| arg == "--managed-child");
    if pending.is_none() {
        let mut recovery = BleRecovery::next(ble_recovery().as_ref(), now_secs(), automatic);
        recovery.error=failure.into();
        if failure!="ble_native_quarantined_restart_required" {recovery.stage="service_task".into();}
        if let Err(error) = save_recovery(&recovery) {
            tracing::error!(%error, "Cannot persist BLE recovery status");
        }
        tracing::warn!(attempt=recovery.attempt, retry_at=recovery.retry_at, automatic,
            error=failure, "Bridge service failed; recovery scheduled");
        *pending = Some(recovery);
    }
    if automatic && pending.as_ref().is_some_and(|p| now_secs() >= p.retry_at) {
        append_log("BLE recovery: ending isolated child before replacement");
        std::process::exit(BLE_RECOVERY_EXIT);
    }
}

/// Supervision never waits on the BLE worker or its delivery permits.
async fn monitor_services<F: std::future::Future<Output=()>+Send+'static>(
    services:F, native_failed:impl Fn()->bool,
    mut report:impl FnMut(Option<&'static str>), period:std::time::Duration,
) {
    let mut services=tokio::spawn(services);
    let mut task_failure=None;
    let mut interval=tokio::time::interval(period);
    loop {
        tokio::select! {
            result=&mut services, if task_failure.is_none() => {
                let reason=if result.is_err_and(|e|e.is_panic()) {"service_task_panicked"}else{"service_task_exited"};
                // Panic payloads may contain request data; record only the fixed category.
                tracing::error!(error=reason,"Bridge service task ended unexpectedly");
                task_failure=Some(reason);
            }
            _=interval.tick()=>{}
        }
        report(task_failure.or_else(||native_failed().then_some("ble_native_quarantined_restart_required")));
    }
}

pub(crate) async fn supervise_services(services:impl std::future::Future<Output=()>+Send+'static) {
    let mut pending=None;
    monitor_services(services,bridge_ble::native_quarantined,
        |failure|recover_ble(&mut pending,failure),std::time::Duration::from_secs(1)).await;
}

pub(crate) fn ble_recovered() {
    if let Some(mut recovery) = ble_recovery().filter(|r| r.recovered_at.is_none()) {
        recovery.state = "recovered".into();
        recovery.recovered_at = Some(now_secs());
        if let Err(error) = save_recovery(&recovery) {
            tracing::error!(%error, "Cannot persist successful BLE recovery");
        }
    }
}

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
    recent_stamps_at(&log_path(), now)
}

fn recent_stamps_at(path: &std::path::Path, now: u64) -> Vec<u64> {
    fs::read_to_string(path)
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

fn append_log(line: &str) { append_log_at(&log_path(), line); }
fn append_log_at(path: &std::path::Path, line: &str) {
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
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
        if code == BLE_RECOVERY_EXIT as u32 {
            append_log("BLE recovery: isolated parent exited; relaunching");
            std::process::exit(if relaunch() { 0 } else { 1 });
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

/// launchd owns this supervisor; it owns exactly one child from the same app.
#[cfg(target_os = "macos")]
pub fn supervise() -> ! {
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe().expect("supervised app path");
    let mut command = Command::new(&exe);
    command.arg("--managed-child").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    supervise_command(&mut command, &log_path());
    std::process::exit(0);
}

#[cfg(target_os = "macos")]
fn supervise_command(command: &mut std::process::Command, path: &std::path::Path) {
    loop {
        let status = command.spawn().and_then(|mut child| child.wait());
        if status.as_ref().is_ok_and(|status| status.success()) {
            append_log_at(path, "managed child exited normally");
            return;
        }
        if status.as_ref().is_ok_and(|status| status.code() == Some(BLE_RECOVERY_EXIT)) {
            append_log_at(path, "BLE recovery: isolated child exited; relaunching");
            continue;
        }
        let now = now_secs();
        let count = recent_stamps_at(path, now).len() + 1;
        append_log_at(path, &format!("abnormal {now} managed_exit recent={count}"));
        if count >= MAX_ABNORMAL {
            append_log_at(path, "managed restart circuit open");
            // Successful supervisor exit leaves launchd idle until explicit start.
            return;
        }
        std::thread::sleep(std::time::Duration::from_secs(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn review_supervisor_observes_quarantine_while_worker_is_stuck_and_task_panics() {
        use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
        let native=Arc::new(AtomicBool::new(false));
        let flag=native.clone();let (tx,mut rx)=tokio::sync::mpsc::unbounded_channel();
        let monitor=tokio::spawn(monitor_services(std::future::pending(),move||flag.load(Ordering::SeqCst),
            move|failure|{if let Some(failure)=failure{let _=tx.send(failure);}},std::time::Duration::from_millis(10)));
        native.store(true,Ordering::SeqCst);
        assert_eq!(tokio::time::timeout(std::time::Duration::from_secs(1),rx.recv()).await.unwrap(),Some("ble_native_quarantined_restart_required"));
        monitor.abort();
        for panic in [false,true] {
            let (tx,mut rx)=tokio::sync::mpsc::unbounded_channel();
            let monitor=tokio::spawn(monitor_services(async move{assert!(!panic,"injected service panic");},||false,
                move|failure|{if let Some(failure)=failure{let _=tx.send(failure);}},std::time::Duration::from_millis(10)));
            assert_eq!(tokio::time::timeout(std::time::Duration::from_secs(1),rx.recv()).await.unwrap(),Some(if panic{"service_task_panicked"}else{"service_task_exited"}));
            monitor.abort();
        }
    }

    #[test]
    fn ble_recovery_backoff_survives_restarts_and_does_not_clear_on_startup() {
        let mut recovery = BleRecovery::next(None, 100, true);
        assert_eq!(recovery.retry_at, 105);
        for (at, delay) in [(110,30), (150,120), (280,300), (600,300)] {
            let persisted = serde_json::from_slice::<BleRecovery>(&serde_json::to_vec(&recovery).unwrap()).unwrap();
            assert_eq!(persisted.state, "recovering");
            recovery = BleRecovery::next(Some(&persisted), at, true);
            assert_eq!(recovery.retry_at - at, delay);
        }
        recovery.recovered_at = Some(601);
        assert_eq!(BleRecovery::next(Some(&recovery), 650, true).attempt, 6);
        assert_eq!(BleRecovery::next(Some(&recovery), 901, true).attempt, 1);
        assert_eq!(BleRecovery::next(None,100,false).state,"restart_required");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn managed_ble_recovery_waits_for_old_child_and_bypasses_crash_circuit() {
        use std::process::Command;
        let stem = std::env::temp_dir().join(format!("bridge-recovery-{}-{}",std::process::id(),now_secs()));
        let marker = stem.with_extension("children");
        let log = stem.with_extension("log");
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(r#"
            if test -f "$1"; then
                old=$(tail -1 "$1")
                if kill -0 "$old" 2>/dev/null; then exit 9; fi
            fi
            echo $$ >> "$1"
            if test "$(wc -l < "$1")" -lt 5; then exit 75; fi
            exit 0
        "#).arg("recovery-test").arg(&marker);
        supervise_command(&mut command, &log);
        assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(),5);
        let text=fs::read_to_string(&log).unwrap();
        assert_eq!(text.matches("isolated child exited").count(),4);
        assert!(!text.contains("abnormal"));
        assert!(text.contains("exited normally"));
        fs::remove_file(marker).unwrap(); fs::remove_file(log).unwrap();
    }
}
