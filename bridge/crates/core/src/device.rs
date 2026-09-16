//! Fetch and parse the device's LAN status page (`<li>Key: value</li>` list).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::{anyhow, Result};

#[derive(Debug, Clone, Default)]
pub struct DeviceStatus {
    pub fields: Vec<(String, String)>,
}

impl DeviceStatus {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// GET `http://<ip>/` and parse the `<li>Key: value</li>` entries.
pub fn fetch(ip: &str, timeout: Duration) -> Result<DeviceStatus> {
    let addr = format!("{ip}:80")
        .parse()
        .map_err(|e| anyhow!("bad device ip {ip}: {e}"))?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(
        b"GET / HTTP/1.1\r\nHost: device\r\nConnection: close\r\nAccept: text/html\r\n\r\n",
    )?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw)?;
    let body = raw.split("\r\n\r\n").nth(1).unwrap_or(&raw);
    let mut fields = Vec::new();
    let mut rest = body;
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
    Ok(DeviceStatus { fields })
}
