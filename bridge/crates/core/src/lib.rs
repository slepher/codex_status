pub mod activity;
pub mod codex;
pub mod compile;
pub mod coordinator;
pub mod datasource;
pub mod device;
pub mod envelope;
pub mod http;
pub mod paths;
pub mod platform;
pub mod runtime;
pub mod template;
pub mod v2_client;

/// Deterministic short id used for `bridge.hostId`.
pub fn short_id(input: &str) -> String {
    let full = format!("{:08x}", crc32fast::hash(input.as_bytes()));
    full[..4].to_string()
}

/// Wall-clock seconds since the Unix epoch (UTC).
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Local UTC offset in minutes (east positive: CST = +480). Sent to the device
/// so it can follow the PC timezone (the firmware builds a POSIX TZ string).
pub fn local_offset_minutes() -> i64 {
    (chrono::Local::now().offset().local_minus_utc() as i64) / 60
}

/// Best-effort LAN IP advertised in the envelope (`bridge.host`) so the device
/// can self-heal its endpoint record when the bridge moves (docs/power-state §9).
pub fn lan_ip() -> String {
    if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}
