//! Device discovery fallbacks (docs/power-state.md §9, device-discovery task-2):
//! ARP lookup by MAC on the local /24 (Windows `SendARP` + neighbor table) plus
//! the shared identity helpers (MAC normalization, default display name).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Strip separators and uppercase: `70:04:1D:AA:BB:CC` -> `70041DAABBCC`.
pub fn normalize_mac(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_ascii_uppercase()
}

/// Default display name for a device MAC: last 3 bytes, device convention
/// (`70:04:1D:AA:BB:CC` -> `CodexStatus-AABBCC`).
pub fn default_device_name(mac: &str) -> String {
    let hex = normalize_mac(mac);
    let suffix = if hex.len() >= 6 {
        &hex[hex.len() - 6..]
    } else {
        hex.as_str()
    };
    format!("CodexStatus-{suffix}")
}

fn ip_octets(ip: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut parts = ip.split('.');
    for slot in out.iter_mut() {
        *slot = parts.next()?.trim().parse().ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(out)
}

/// Single ARP resolution via Windows `SendARP`. Returns the MAC of `ip` only
/// when the host answers on the local L2 segment.
#[cfg(windows)]
pub fn arp_lookup(ip: &str) -> Option<String> {
    use windows_sys::Win32::NetworkManagement::IpHelper::SendARP;
    let octets = ip_octets(ip)?;
    // SendARP takes the destination in network byte order, i.e. the address
    // bytes in memory order.
    let dest = u32::from_ne_bytes(octets);
    let mut mac = [0u8; 8];
    let mut len = mac.len() as u32;
    let rc = unsafe { SendARP(dest, 0, mac.as_mut_ptr() as *mut _, &mut len) };
    if rc != 0 || len < 6 {
        return None;
    }
    Some(format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    ))
}

#[cfg(not(windows))]
pub fn arp_lookup(_ip: &str) -> Option<String> {
    None
}

/// Read the Windows IPv4 neighbor table and map a normalized MAC back to its
/// current IP. `dwAddr` is stored in network byte order.
#[cfg(windows)]
fn neighbor_ip_for_mac(target: &str) -> Option<String> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{GetIpNetTable, MIB_IPNETTABLE};
    let mut size: u32 = 0;
    unsafe { GetIpNetTable(std::ptr::null_mut(), &mut size, 0) };
    if (size as usize) < std::mem::size_of::<MIB_IPNETTABLE>() {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    let rc = unsafe { GetIpNetTable(buf.as_mut_ptr() as *mut MIB_IPNETTABLE, &mut size, 0) };
    if rc != 0 {
        return None;
    }
    let table = buf.as_ptr() as *const MIB_IPNETTABLE;
    unsafe {
        let count = (*table).dwNumEntries as usize;
        let rows = std::slice::from_raw_parts((*table).table.as_ptr(), count);
        for row in rows {
            if row.dwPhysAddrLen < 6 {
                continue;
            }
            let mac = normalize_mac(&format!(
                "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                row.bPhysAddr[0],
                row.bPhysAddr[1],
                row.bPhysAddr[2],
                row.bPhysAddr[3],
                row.bPhysAddr[4],
                row.bPhysAddr[5]
            ));
            if mac == target {
                let ip = std::net::Ipv4Addr::from(row.dwAddr.to_le_bytes());
                return Some(ip.to_string());
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn neighbor_ip_for_mac(_target: &str) -> Option<String> {
    None
}

/// Parallel `SendARP` pass over the local /24; returns the first matching IP.
fn sendarp_scan(hosts: &[String], target: &str) -> Option<String> {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::scope(|scope| {
        let workers = 48usize;
        let step = hosts.len().div_ceil(workers).max(1);
        for chunk in hosts.chunks(step) {
            let tx = tx.clone();
            let stop = stop.clone();
            let target = target.to_string();
            scope.spawn(move || {
                for ip in chunk {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Some(mac) = arp_lookup(ip) {
                        if normalize_mac(&mac) == target {
                            let _ = tx.send(ip.clone());
                            stop.store(true, Ordering::Relaxed);
                            return;
                        }
                    }
                }
            });
        }
        drop(tx);
        rx.recv_timeout(Duration::from_secs(10)).ok()
    })
}

/// Scan the local /24 for `target_mac` and return its IP. Used when the
/// configured/learned IP is stale but the device is still on the same L2
/// segment.
///
/// Wi-Fi stations in light sleep can miss a single ARP request, so the scan
/// first pokes every host with a UDP datagram (forcing the OS to resolve all
/// addresses), then polls the neighbor table while the device's wake window
/// comes around; a direct `SendARP` pass is the final fallback.
pub fn arp_scan_for_mac(local_ip: &str, target_mac: &str, timeout: Duration) -> Option<String> {
    let target = normalize_mac(target_mac);
    if target.len() != 12 {
        return None;
    }
    // Fast path: already in the neighbor table (stale entries are fine here).
    if let Some(ip) = neighbor_ip_for_mac(&target) {
        return Some(ip);
    }
    let octets = ip_octets(local_ip)?;
    let base = format!("{}.{}.{}", octets[0], octets[1], octets[2]);
    let hosts: Vec<String> = (1..=254u16).map(|i| format!("{base}.{i}")).collect();

    if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
        for host in &hosts {
            let _ = sock.send_to(b"codex-status", (host.as_str(), 9));
        }
    }

    let deadline = Instant::now() + timeout;
    loop {
        for _ in 0..4 {
            if let Some(ip) = neighbor_ip_for_mac(&target) {
                return Some(ip);
            }
            if Instant::now() >= deadline {
                return sendarp_scan(&hosts, &target);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        if Instant::now() >= deadline {
            return sendarp_scan(&hosts, &target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_helpers_normalize_mac_format() {
        assert_eq!(normalize_mac("70:04:1d:aa:bb:cc"), "70041DAABBCC");
        assert_eq!(normalize_mac("70-04-1D-AA-BB-CC"), "70041DAABBCC");
        assert_eq!(default_device_name("70:04:1D:AA:BB:CC"), "CodexStatus-AABBCC");
    }

    #[test]
    fn arp_scan_requires_a_full_mac() {
        assert_eq!(arp_scan_for_mac("192.168.1.100", "AABBCC", Duration::ZERO), None);
    }
}
