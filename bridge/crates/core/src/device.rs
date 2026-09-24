//! Fetch and parse the device's LAN status page (`<li>Key: value</li>` list).

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

use anyhow::{anyhow, Result};

#[derive(Debug, Clone, Default)]
pub struct DeviceStatus {
    pub fields: Vec<(String, String)>,
    /// Parsed `/status.json` document when the structured path was used.
    pub raw: Option<serde_json::Value>,
}

impl DeviceStatus {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// GET `http://<ip><path>` and return the body.
fn http_get(ip: &str, path: &str, timeout: Duration) -> Result<String> {
    let addr = parse_device_addr(ip)?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: device\r\nConnection: close\r\nAccept: */*\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw)?;
    let body = raw.split("\r\n\r\n").nth(1).unwrap_or(&raw);
    Ok(body.to_string())
}

fn parse_device_addr(address: &str) -> Result<SocketAddr> {
    address
        .parse()
        .or_else(|_| address.parse::<IpAddr>().map(|ip| SocketAddr::new(ip, 80)))
        .map_err(|e| anyhow!("bad device address {address}: {e}"))
}

/// Prefer the structured `/status.json` (fw >= 0.8.0), fall back to parsing
/// the HTML status page on older firmware.
pub fn fetch(ip: &str, timeout: Duration) -> Result<DeviceStatus> {
    if let Ok(body) = http_get(ip, "/status.json", timeout) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(fields) = fields_from_json(&value) {
                return Ok(DeviceStatus {
                    fields,
                    raw: Some(value),
                });
            }
        }
    }
    let body = http_get(ip, "/", timeout)?;
    let mut fields = Vec::new();
    let mut rest = body.as_str();
    while let Some(start) = rest.find("<li>") {
        let after = &rest[start + 4..];
        let Some(end) = after.find("</li>") else { break };
        let text = after[..end].trim();
        if let Some((key, value)) = text.split_once(':') {
            fields.push((key.trim().to_string(), value.trim().to_string()));
        }
        rest = &after[end..];
    }
    if fields.is_empty() {
        return Err(anyhow!("no status fields found on device page"));
    }
    Ok(DeviceStatus { fields, raw: None })
}

/// Raw PM statistics from `GET /pmstats` (firmware >= 0.13.0): light-sleep
/// counters and the PM lock dump, consumed by the panel 功耗 tab and the MCP
/// `pm_stats` tool.
pub fn fetch_pmstats(ip: &str, timeout: Duration) -> Result<String> {
    let body = http_get(ip, "/pmstats", timeout)?;
    if !body.contains("Mode stats:") || !body.contains("Lock stats:") {
        return Err(anyhow!(
            "/pmstats unavailable on {ip} (needs firmware >= 0.13.0): {}",
            body.trim().chars().take(80).collect::<String>()
        ));
    }
    Ok(body)
}

fn fields_from_json(value: &serde_json::Value) -> Option<Vec<(String, String)>> {
    let text = |key: &str| value.get(key).and_then(|v| v.as_str()).map(str::to_string);
    let number = |key: &str| value.get(key).and_then(|v| v.as_i64());
    let fw = text("fw")?;
    let mac = text("mac").unwrap_or_default();
    let slot = text("slot").unwrap_or_default();
    let next_slot = text("next_slot").unwrap_or_default();
    let reset = text("reset").unwrap_or_default();
    let uptime = number("uptime_s").unwrap_or(0);
    let ssid = text("ssid").unwrap_or_default();
    let ip = text("ip").unwrap_or_default();
    let rssi = number("rssi").map(|v| format!("{v} dBm")).unwrap_or_default();
    let ble = value.get("ble").and_then(|v| v.as_bool()).unwrap_or(false);
    let state = text("state").unwrap_or_default();
    let plugged = value.get("plugged").and_then(|v| v.as_bool()).unwrap_or(false);
    let wifi_state = text("wifi_state").unwrap_or_default();
    let retry_stage = number("retry_stage").unwrap_or(0);
    let endpoints = number("endpoints").unwrap_or(0);
    let channel = text("channel").unwrap_or_default();
    let battery = number("battery").unwrap_or(-1);
    let battery_mv = number("battery_mv").unwrap_or(0);
    let heap = number("heap").unwrap_or(0);
    let epd_writes = number("epd_writes").unwrap_or(0);
    let epd_partial = value.get("epd_partial").and_then(|v| v.as_bool()).unwrap_or(false);
    let epd_streak = number("epd_streak").unwrap_or(0);
    let templates = value
        .get("templates")
        .and_then(|v| v.as_array())
        .map(|list| {
            let count = list.len();
            let active = list
                .iter()
                .find(|t| t.get("active").and_then(|v| v.as_bool()).unwrap_or(false))
                .map(|t| {
                    format!(
                        "{} hash {}",
                        t.get("id").and_then(|v| v.as_str()).unwrap_or("?"),
                        t.get("hash").and_then(|v| v.as_str()).unwrap_or("?")
                    )
                })
                .unwrap_or_else(|| "-".to_string());
            format!("{count} (active: {active})")
        })
        .unwrap_or_else(|| "-".to_string());
    let battery_text = if battery >= 0 {
        format!("{battery}% ({battery_mv} mV)")
    } else {
        "--".to_string()
    };
    // v0.14 deep/light mode fields (absent on older firmware).
    let mode = text("mode").unwrap_or_default();
    let next_contact = number("next_contact_s").unwrap_or(0);
    let next_in = number("next_contact_in_s").unwrap_or(0);
    let mode_text = if mode.is_empty() {
        "-".to_string()
    } else if next_contact > 0 {
        format!("{mode} (next contact {next_in}s / every {next_contact}s)")
    } else {
        mode
    };
    let deep_text = value
        .get("deep")
        .map(|deep| {
            let clock = deep.get("clock_ticks").and_then(|v| v.as_u64()).unwrap_or(0);
            let net = deep.get("net_windows").and_then(|v| v.as_u64()).unwrap_or(0);
            let fails = deep.get("net_fails").and_then(|v| v.as_u64()).unwrap_or(0);
            format!("clock {clock}, net {net} ({fails} fail)")
        })
        .unwrap_or_else(|| "-".to_string());
    Some(vec![
        ("Version".to_string(), fw),
        ("MAC".to_string(), mac),
        ("State".to_string(), format!("{state} (USB {plugged}, wifi {wifi_state}, retry {retry_stage})")),
        ("Mode".to_string(), mode_text),
        ("Deep".to_string(), deep_text),
        ("Running".to_string(), format!("{slot} (next OTA slot: {next_slot})")),
        ("Reset reason".to_string(), format!("{reset} (uptime {uptime}s)")),
        ("SSID".to_string(), ssid),
        ("IP".to_string(), ip),
        ("RSSI".to_string(), rssi),
        ("Battery".to_string(), battery_text),
        ("BLE connected".to_string(), if ble { "yes" } else { "no" }.to_string()),
        ("Endpoints stored".to_string(), endpoints.to_string()),
        ("Last channel".to_string(), channel),
        ("Templates".to_string(), templates),
        (
            "EPD writes".to_string(),
            format!(
                "{epd_writes} (partial {}, streak {epd_streak})",
                if epd_partial { "ready" } else { "off" }
            ),
        ),
        ("Free heap".to_string(), heap.to_string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn bare_ipv4_defaults_to_port_80() {
        assert_eq!(parse_device_addr("192.0.2.10").unwrap(), "192.0.2.10:80".parse().unwrap());
    }

    #[test]
    fn fetch_uses_explicit_port_and_parses_status_mac() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = stream.read(&mut request).unwrap();
            let body = r#"{"fw":"test","mac":"AA:BB:CC:DD:EE:FF"}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });

        let status = fetch(&address.to_string(), Duration::from_secs(2)).unwrap();
        assert_eq!(status.get("MAC"), Some("AA:BB:CC:DD:EE:FF"));
        assert_eq!(status.raw.unwrap()["fw"], "test");
        server.join().unwrap();
    }
}
