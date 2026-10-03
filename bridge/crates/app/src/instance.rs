//! Process-local identity for independently launched tray bridges.

use std::env;

use crate::icon::Shape;

fn valid_name(value: &str) -> bool {
    value != "default"
        && value.len() <= 27
        && value.starts_with(|c: char| c.is_ascii_lowercase())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[derive(Clone)]
pub struct Instance {
    pub name: Option<String>,
    pub shape: Shape,
}

impl Instance {
    pub fn from_env() -> Result<Self, String> {
        let name = env::var("CODEX_STATUS_INSTANCE")
            .ok()
            .filter(|s| !s.is_empty());
        if let Some(value) = &name {
            if !valid_name(value) {
                return Err("CODEX_STATUS_INSTANCE must be 1-27 lowercase letters, digits or hyphens, starting with a letter".into());
            }
        }
        let shape =
            Shape::parse(&env::var("CODEX_STATUS_ICON_SHAPE").unwrap_or_else(|_| "square".into()))
                .ok_or("CODEX_STATUS_ICON_SHAPE must be square, circle or diamond")?;
        if let Some(value) = &name {
            #[cfg(target_os = "macos")]
            if env::var_os("CODEX_STATUS_DATA").is_none() {
                env::set_var(
                    "CODEX_STATUS_DATA",
                    bridge_core::paths::macos_support_root()
                        .join("instances")
                        .join(value)
                        .join("data"),
                );
            }
            #[cfg(not(target_os = "macos"))]
            {
                let exe = env::current_exe().map_err(|error| error.to_string())?;
                let data = exe
                    .parent()
                    .ok_or("bridge executable has no parent directory")?
                    .join("instances")
                    .join(value)
                    .join("data");
                env::set_var("CODEX_STATUS_DATA", data);
            }
        }
        Ok(Self { name, shape })
    }

    pub fn mutex_name(&self) -> String {
        format!(
            "codex-status-bridge-{}",
            self.name.as_deref().unwrap_or("default")
        )
    }

    pub fn bridge_id(&self, host: &str) -> String {
        let base = bridge_core::short_id(host);
        match &self.name {
            Some(name) => format!("{base}-{name}"),
            None => base,
        }
    }
}

#[cfg(target_os = "macos")]
pub fn installation_id() -> std::io::Result<String> {
    use std::io::Write;
    let root = bridge_core::paths::macos_support_root();
    std::fs::create_dir_all(&root)?;
    let path = root.join("installation-id");
    match std::fs::read_to_string(&path) {
        Ok(id) => {
            uuid::Uuid::parse_str(id.trim()).map_err(std::io::Error::other)?;
            Ok(id.trim().into())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let id = uuid::Uuid::new_v4().to_string();
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            file.write_all(id.as_bytes())?;
            file.sync_all()?;
            Ok(id)
        }
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
pub struct MutexGuard {
    _file: std::fs::File,
}

#[cfg(unix)]
pub fn acquire(_instance: &Instance) -> Result<MutexGuard, String> {
    acquire_in(&bridge_core::paths::data_root())
}

#[cfg(unix)]
fn acquire_in(root: &std::path::Path) -> Result<MutexGuard, String> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(root.join("bridge.lock"))
        .map_err(|error| error.to_string())?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(format!(
            "bridge data directory already locked: {}",
            root.display()
        ));
    }
    Ok(MutexGuard { _file: file })
}

#[cfg(windows)]
pub fn acquire(instance: &Instance) -> Result<MutexGuard, String> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let name: Vec<u16> = instance
        .mutex_name()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(format!("CreateMutexW failed: {}", unsafe {
            GetLastError()
        }));
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(handle) };
        return Err(format!(
            "bridge instance already running: {}",
            instance.name.as_deref().unwrap_or("default")
        ));
    }
    Ok(MutexGuard(handle))
}

#[cfg(windows)]
pub struct MutexGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for MutexGuard {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn data_lock_is_exclusive_and_released_on_drop() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let held = acquire_in(first.path()).unwrap();
        assert!(acquire_in(first.path()).is_err());
        assert!(acquire_in(second.path()).is_ok());
        drop(held);
        assert!(acquire_in(first.path()).is_ok());
    }

    #[test]
    fn lock_is_scoped_to_instance() {
        assert_eq!(
            Instance {
                name: None,
                shape: Shape::Square
            }
            .mutex_name(),
            "codex-status-bridge-default"
        );
        assert_eq!(
            Instance {
                name: Some("flash".into()),
                shape: Shape::Circle
            }
            .mutex_name(),
            "codex-status-bridge-flash"
        );
    }

    #[test]
    fn name_stays_safe_for_paths_and_lock_names() {
        assert!(valid_name("note4-b"));
        assert!(!valid_name("default"));
        assert!(!valid_name("../note4"));
        assert!(!valid_name("Note4"));
        assert!(!valid_name(&"a".repeat(28)));
    }

    #[test]
    fn owner_ids_are_distinct_and_fit_device_claim_limit() {
        let default = Instance {
            name: None,
            shape: Shape::Square,
        };
        let named = Instance {
            name: Some("a".repeat(27)),
            shape: Shape::Circle,
        };
        assert_ne!(default.bridge_id("host"), named.bridge_id("host"));
        assert_eq!(named.bridge_id("host").len(), 32);
    }
}
