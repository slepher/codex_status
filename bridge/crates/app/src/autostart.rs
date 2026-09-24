//! Windows logon autostart via `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
//! The registry is the source of truth; the tray menu check state is derived
//! from it at startup and after every toggle.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "CodexStatusBridge";

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn open(access: u32) -> Option<HKEY> {
    let mut hkey: HKEY = std::ptr::null_mut();
    let rc = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            0,
            access,
            &mut hkey,
        )
    };
    if rc == ERROR_SUCCESS {
        Some(hkey)
    } else {
        None
    }
}

/// Command line currently registered for the current user, if any.
pub fn registered() -> Option<String> {
    let hkey = open(KEY_READ)?;
    let name = wide(VALUE_NAME);
    let mut ty = 0u32;
    let mut len: u32 = 0;
    let mut rc = unsafe {
        RegQueryValueExW(
            hkey,
            name.as_ptr(),
            std::ptr::null(),
            &mut ty,
            std::ptr::null_mut(),
            &mut len,
        )
    };
    if rc == ERROR_SUCCESS && ty == REG_SZ && len > 0 && len <= 32 * 1024 {
        let mut buf = vec![0u8; len as usize];
        rc = unsafe {
            RegQueryValueExW(
                hkey,
                name.as_ptr(),
                std::ptr::null(),
                &mut ty,
                buf.as_mut_ptr(),
                &mut len,
            )
        };
        unsafe { RegCloseKey(hkey) };
        if rc == ERROR_SUCCESS {
            let units: Vec<u16> = buf
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
            return String::from_utf16(&units[..end]).ok();
        }
        return None;
    }
    unsafe { RegCloseKey(hkey) };
    None
}

pub fn enabled() -> bool {
    registered().is_some()
}

/// Whether the registered command points at the running executable.
pub fn matches_current_exe() -> bool {
    let Some(value) = registered() else {
        return false;
    };
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    value
        .trim()
        .trim_matches('"')
        .eq_ignore_ascii_case(&exe.to_string_lossy())
}

pub fn set(enable: bool) -> std::io::Result<()> {
    let hkey = open(KEY_READ | KEY_SET_VALUE).ok_or_else(|| std::io::Error::last_os_error())?;
    let name = wide(VALUE_NAME);
    let rc = if enable {
        let exe = std::env::current_exe()?;
        let command = format!("\"{}\"", exe.display());
        let data = wide(&command);
        unsafe {
            RegSetValueExW(
                hkey,
                name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                (data.len() * 2) as u32,
            )
        }
    } else {
        unsafe { RegDeleteValueW(hkey, name.as_ptr()) }
    };
    unsafe { RegCloseKey(hkey) };
    if rc == ERROR_SUCCESS || (!enable && rc == ERROR_FILE_NOT_FOUND) {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(rc as i32))
    }
}
