pub mod codex;
pub mod device;
pub mod envelope;
pub mod http;
pub mod paths;
pub mod profile;
pub mod runtime;
pub mod template;

/// Deterministic short id used for `bridge.hostId`.
pub fn short_id(input: &str) -> String {
    let full = format!("{:08x}", crc32fast::hash(input.as_bytes()));
    full[..4].to_string()
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
