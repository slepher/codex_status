pub mod codex;
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
