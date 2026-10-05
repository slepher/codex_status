use serde_json::{json, Value};

#[cfg(target_os = "macos")]
unsafe fn native_json(pointer: *mut std::ffi::c_char, fallback: Value) -> Value {
    if pointer.is_null() { return fallback; }
    let result = unsafe { serde_json::from_slice(std::ffi::CStr::from_ptr(pointer).to_bytes()) }.unwrap_or(fallback);
    unsafe { libc::free(pointer.cast()); }
    result
}
#[cfg(target_os = "macos")]
pub fn read(request_permission: bool) -> Value {
    unsafe extern "C" { fn codex_host_wifi_read(request: i32) -> *mut std::ffi::c_char; }
    unsafe { native_json(codex_host_wifi_read(i32::from(request_permission)), json!({"state":"unavailable", "networks":[]})) }
}

/// 0: passive check, 1: explicit location request, 2: explicit Bluetooth request.
#[cfg(target_os = "macos")]
pub fn permissions(request: i32) -> Value {
    unsafe extern "C" { fn codex_permissions_read(request: i32) -> *mut std::ffi::c_char; }
    unsafe { native_json(codex_permissions_read(request), json!({"platform":"macos","location":"unavailable","bluetooth":"unavailable"})) }
}
#[cfg(not(target_os = "macos"))]
pub fn permissions(_request: i32) -> Value { json!({"platform":"other"}) }

#[cfg(any(target_os = "macos", test))]
fn bluetooth_permission(authorization: i32) -> Result<(), String> {
    match authorization {
        3 => Ok(()),
        0 => Err("bluetooth_permission_required".into()),
        1 => Err("bluetooth_permission_restricted".into()),
        _ => Err("bluetooth_permission_denied".into()),
    }
}
/// Check without constructing a CoreBluetooth central (which can show a prompt).
pub fn require_bluetooth() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" { fn codex_bluetooth_authorization() -> i32; }
        bluetooth_permission(unsafe { codex_bluetooth_authorization() })
    }
    #[cfg(not(target_os = "macos"))]
    { Ok(()) }
}
/// Simulator transport uses loopback HTTP, never the host Bluetooth adapter.
pub fn require_device_bluetooth() -> Result<(), String> {
    if std::env::var_os("CODEX_STATUS_SIM_BLE_ENDPOINTS").is_some() { return Ok(()); }
    require_bluetooth()
}
#[cfg(target_os = "macos")]
pub fn open_settings(section: i32) -> Result<(), String> {
    unsafe extern "C" { fn codex_privacy_open(section: i32) -> i32; }
    if unsafe { codex_privacy_open(section) } != 0 { Ok(()) } else { Err("privacy_settings_unavailable".into()) }
}
#[cfg(not(target_os = "macos"))]
pub fn open_settings(_section: i32) -> Result<(), String> { Err("privacy_settings_unsupported".into()) }

#[cfg(test)]
mod permission_tests {
    #[test]
    fn only_an_actual_grant_allows_bluetooth() {
        assert_eq!(super::bluetooth_permission(0).unwrap_err(), "bluetooth_permission_required");
        assert_eq!(super::bluetooth_permission(1).unwrap_err(), "bluetooth_permission_restricted");
        assert_eq!(super::bluetooth_permission(2).unwrap_err(), "bluetooth_permission_denied");
        assert!(super::bluetooth_permission(3).is_ok());
        assert!(super::bluetooth_permission(99).is_err());
    }
}

#[cfg(windows)]
pub fn read(_request_permission: bool) -> Value {
    use windows_sys::Win32::NetworkManagement::WiFi::*;
    unsafe {
        let mut handle = std::ptr::null_mut();
        let mut version = 0;
        if WlanOpenHandle(2, std::ptr::null(), &mut version, &mut handle) != 0 {
            return json!({"state":"unavailable", "networks":[]});
        }
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        let mut networks = vec![];
        let mut denied = false;
        if WlanEnumInterfaces(handle, std::ptr::null(), &mut list) == 0 && !list.is_null() {
            for interface in std::slice::from_raw_parts((*list).InterfaceInfo.as_ptr(), (*list).dwNumberOfItems as usize) {
                if interface.isState != wlan_interface_state_connected { continue; }
                let mut size = 0;
                let mut data = std::ptr::null_mut();
                let code = WlanQueryInterface(handle, &interface.InterfaceGuid, wlan_intf_opcode_current_connection,
                    std::ptr::null(), &mut size, &mut data, std::ptr::null_mut());
                denied |= code == 5;
                if code == 0 && !data.is_null() && size as usize >= std::mem::size_of::<WLAN_CONNECTION_ATTRIBUTES>() {
                    let connection = &*data.cast::<WLAN_CONNECTION_ATTRIBUTES>();
                    let ssid = &connection.wlanAssociationAttributes.dot11Ssid;
                    if let Some(bytes) = ssid.ucSSID.get(..ssid.uSSIDLength as usize) {
                        if let Ok(name) = std::str::from_utf8(bytes) { networks.push(json!({"ssid":name})); }
                    }
                }
                if !data.is_null() { WlanFreeMemory(data); }
            }
            WlanFreeMemory(list.cast());
        }
        WlanCloseHandle(handle, std::ptr::null());
        json!({"state":if networks.len() > 1 {"ambiguous"} else if networks.len() == 1 {"connected"}
            else if denied {"permission_denied"} else {"no_wifi"}, "networks":networks})
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn read(_request_permission: bool) -> Value { json!({"state":"unsupported", "networks":[]}) }

pub fn current_ssid(value: &Value) -> Option<&str> {
    (value["state"] == "connected").then(|| value["networks"][0]["ssid"].as_str()).flatten()
}

// Freeze the actual environment across save→activate. Saved configuration is
// never evidence of which network the host currently uses.
pub fn require_same_network(expected: &Value, actual: &Value) -> Result<(), String> {
    let expected = current_ssid(expected).filter(|s| !s.is_empty()).ok_or("host_network_unknown")?;
    let actual = current_ssid(actual).filter(|s| !s.is_empty()).ok_or("host_network_unknown")?;
    if expected != actual { return Err("host_network_changed".into()); }
    Ok(())
}

#[cfg(test)]
mod network_contact_tests {
    use super::*;
    #[test]
    fn current_network_must_be_known_and_unchanged() {
        let a = json!({"state":"connected","networks":[{"ssid":"A"}]});
        let b = json!({"state":"connected","networks":[{"ssid":"B"}]});
        assert!(require_same_network(&b, &b).is_ok());
        assert_eq!(require_same_network(&a, &b).unwrap_err(), "host_network_changed");
        for state in ["permission_denied","ambiguous","unavailable","no_wifi"] {
            assert_eq!(require_same_network(&b, &json!({"state":state,"networks":[{"ssid":"B"}]})).unwrap_err(), "host_network_unknown");
        }
    }
}
