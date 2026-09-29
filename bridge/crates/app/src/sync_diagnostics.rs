//! One durable diagnostic batch per MAC. Pages are acknowledged only after the
//! part file and checkpoint are both flushed. A device receipt is reconciled
//! separately from the Bridge's own archive commitment.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

use super::DeviceLink;
use bridge_core::device_client;
use bridge_core::platform::model::DeviceIdentity;

#[derive(Clone, Serialize, Deserialize)]
struct Checkpoint {
    mac: String,
    bridge_id: String,
    client_serial: u64,
    batch_id: Option<String>,
    bytes: usize,
    sha256: String,
    durable_offset: usize,
    prefix_sha256: String,
    phase: String,
}

pub struct Store {
    root: PathBuf,
    checkpoint: Checkpoint,
}

/// Read-only local archive summary for the device page. Missing files are
/// reported as unknown, never as an empty diagnostic stream.
pub fn summary(data_root: &Path, mac: &str) -> Value {
    let root = data_root.join("platform/diagnostics").join(mac);
    let checkpoint: Option<Checkpoint> = fs::read(root.join("checkpoint.json")).ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let Some(checkpoint) = checkpoint else { return json!({"available":false}); };
    let gaps = checkpoint.batch_id.as_ref().and_then(|id| {
        fs::read(root.join(format!("{id}.json"))).ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|body| body["diag"]["gaps"].as_array().cloned())
    });
    let retired_unconfirmed = fs::read_dir(&root).ok().into_iter().flatten()
        .filter_map(|entry| entry.ok()).any(|entry| entry.file_name().to_string_lossy()
            .starts_with("retired-lost-"));
    let last_full_success = if checkpoint.phase == "complete" { checkpoint.batch_id.clone() } else { None };
    json!({"available":true, "phase":checkpoint.phase, "batch_id":checkpoint.batch_id,
        "gap_count":gaps.as_ref().map(Vec::len),
        "last_full_success":last_full_success,
        "retired_unconfirmed":retired_unconfirmed})
}

fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }

fn write_checkpoint(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    let temp = path.with_extension("json.tmp");
    let mut file = File::create(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(&temp, path)?;
    OpenOptions::new().write(true).open(path)?.sync_all()?;
    Ok(())
}

fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn retain_archives(root: &Path) -> Result<()> {
    let mut archives = Vec::new();
    let mut total = 0u64;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".json") else { continue; };
        let Some((serial, _)) = id.split_once('-') else { continue; };
        let Ok(serial) = serial.parse::<u64>() else { continue; };
        if !safe_id(id) || !entry.file_type()?.is_file() { continue; }
        let bytes = entry.metadata()?.len();
        total += bytes;
        archives.push((serial, id.to_owned(), entry.path(), bytes));
    }
    archives.sort_by_key(|(serial, _, _, _)| *serial);
    if archives.len() <= 128 && total <= 32 * 1024 * 1024 { return Ok(()); }
    let mut remove = Vec::new();
    while archives.len() - remove.len() > 128 || total > 32 * 1024 * 1024 {
        if archives.len() - remove.len() <= 1 { break; }
        let item = &archives[remove.len()];
        total -= item.3;
        remove.push(item.clone());
    }
    if remove.is_empty() { return Ok(()); }
    let floor_path = root.join("retention_floor.json");
    let previous: Value = fs::read(&floor_path).ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or(Value::Null);
    let last = remove.last().unwrap();
    let floor = json!({"pruned_count":previous["pruned_count"].as_u64().unwrap_or(0)
        + remove.len() as u64, "last_batch_id":last.1,
        "last_device_serial":last.0});
    let temp = root.join("retention_floor.json.tmp");
    let mut file = File::create(&temp)?;
    file.write_all(&serde_json::to_vec(&floor)?)?;
    file.sync_all()?;
    fs::rename(temp, floor_path)?;
    for (_, _, path, _) in remove { fs::remove_file(path)?; }
    Ok(())
}

fn validate_diag(body: &serde_json::Value) -> Result<()> {
    let diag = &body["diag"];
    let generation = diag["generation"].as_str().context("diagnostic generation missing")?;
    if generation.len() != 32 || !generation.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("invalid diagnostic generation");
    }
    let from = diag["from_seq"].as_str().context("from_seq missing")?.parse::<u64>()?;
    let through = diag["through_seq"].as_str().context("through_seq missing")?.parse::<u64>()?;
    let encoded = diag["records_b64"].as_str().context("diagnostic records missing")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    if bytes.len() > 4096 { bail!("diagnostic ring exceeds 4096 bytes"); }
    let gaps = diag["gaps"].as_array().context("diagnostic gaps missing")?;
    if gaps.len() > 8 { bail!("too many diagnostic gaps"); }
    let mut offset = 0usize;
    let mut previous = None::<u64>;
    while offset < bytes.len() {
        if bytes.len() - offset < 24 { bail!("truncated diagnostic record"); }
        let length = u16::from_le_bytes(bytes[offset..offset+2].try_into().unwrap()) as usize;
        if !(24..=120).contains(&length) || offset + length > bytes.len() {
            bail!("invalid diagnostic record length");
        }
        let record = &bytes[offset..offset+length];
        if record[3] & !3 != 0 || crc32fast::hash(&record[..length-4]) !=
            u32::from_le_bytes(record[length-4..].try_into().unwrap()) {
            bail!("diagnostic record CRC or flags invalid");
        }
        let seq = u64::from_le_bytes(record[4..12].try_into().unwrap());
        if seq < from || seq > through || previous.is_some_and(|prior| prior.checked_add(1) != Some(seq)) {
            bail!("diagnostic record sequence invalid");
        }
        if previous.is_none() && seq != from { bail!("diagnostic first sequence missing"); }
        previous = Some(seq);
        offset += length;
    }
    if from > through.saturating_add(1) { bail!("diagnostic range reversed"); }
    if (from <= through && previous != Some(through)) ||
       (from > through && previous.is_some()) { bail!("diagnostic range is not covered"); }
    for gap in gaps {
        let reason = gap["reason"].as_str().context("diagnostic gap reason missing")?;
        if !matches!(reason, "overwritten" | "corrupt" | "previous_generation_lost" | "migration_gap") {
            bail!("unknown diagnostic gap reason");
        }
        if reason == "overwritten" {
            let end = gap["through_seq"].as_str().context("overwritten gap endpoint missing")?.parse::<u64>()?;
            if end >= from { bail!("overwritten gap overlaps frozen records"); }
        }
    }
    Ok(())
}

fn diag_fingerprints(body: &Value) -> Result<Vec<(String, u64, String)>> {
    let generation = body["diag"]["generation"].as_str()
        .context("diagnostic generation missing")?.to_owned();
    let encoded = body["diag"]["records_b64"].as_str()
        .context("diagnostic records missing")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes.len() - offset < 24 { bail!("truncated diagnostic record"); }
        let length = u16::from_le_bytes(bytes[offset..offset+2].try_into().unwrap()) as usize;
        if !(24..=120).contains(&length) || offset + length > bytes.len() {
            bail!("invalid diagnostic record length");
        }
        let seq = u64::from_le_bytes(bytes[offset+4..offset+12].try_into().unwrap());
        result.push((generation.clone(), seq, digest(&bytes[offset..offset+length])));
        offset += length;
    }
    Ok(result)
}

fn check_existing_records(root: &Path, body: &Value) -> Result<()> {
    let fresh = diag_fingerprints(body)?;
    if fresh.is_empty() { return Ok(()); }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".json") else { continue; };
        if !safe_id(id) || !id.split_once('-').is_some_and(|(serial, _)| serial.parse::<u64>().is_ok()) ||
            !entry.file_type()?.is_file() { continue; }
        let old: Value = serde_json::from_slice(&fs::read(entry.path())?)?;
        validate_diag(&old).context("archived diagnostic batch corrupt")?;
        let prior = diag_fingerprints(&old)?;
        for (generation, seq, hash) in &fresh {
            if let Some((_, _, old_hash)) = prior.iter().find(|(g, s, _)| g == generation && s == seq) {
                if old_hash != hash { bail!("protocol_conflict: same diagnostic key has different bytes"); }
            }
        }
    }
    Ok(())
}

impl Store {
    pub fn open(data_root: &Path, mac: &str, bridge_id: &str) -> Result<Self> {
        if mac.len() != 12 || !mac.bytes().all(|b| b.is_ascii_hexdigit()) || bridge_id.is_empty() {
            bail!("invalid diagnostic identity");
        }
        let root = data_root.join("platform/diagnostics").join(mac);
        fs::create_dir_all(&root)?;
        let checkpoint_path = root.join("checkpoint.json");
        let mut checkpoint = if checkpoint_path.exists() {
            serde_json::from_slice::<Checkpoint>(&fs::read(&checkpoint_path)?)?
        } else {
            Checkpoint { mac: mac.into(), bridge_id: bridge_id.into(), client_serial: 0,
                batch_id: None, bytes: 0, sha256: String::new(), durable_offset: 0,
                prefix_sha256: digest(&[]), phase: "idle".into() }
        };
        if checkpoint.mac != mac { bail!("diagnostic MAC mismatch"); }
        if checkpoint.bridge_id != bridge_id {
            let retired = root.join(format!("retired-{}.json", &digest(checkpoint.bridge_id.as_bytes())[..16]));
            write_checkpoint(&retired, &checkpoint)?;
            checkpoint = Checkpoint { mac: mac.into(), bridge_id: bridge_id.into(), client_serial: 0,
                batch_id: None, bytes: 0, sha256: String::new(), durable_offset: 0,
                prefix_sha256: digest(&[]), phase: "idle".into() };
            write_checkpoint(&checkpoint_path, &checkpoint)?;
        }
        if let Some(id) = &checkpoint.batch_id {
            if !safe_id(id) { bail!("invalid diagnostic batch id"); }
            let part = root.join(format!("{id}.part"));
            let final_path = root.join(format!("{id}.json"));
            if checkpoint.phase == "awaiting_device_ack" || checkpoint.phase == "complete" {
                if !final_path.exists() || digest(&fs::read(&final_path)?) != checkpoint.sha256 {
                    bail!("archive_lost: committed diagnostic file missing or corrupt");
                }
            } else if checkpoint.phase == "receiving" {
                if !part.exists() {
                    checkpoint.durable_offset = 0;
                    checkpoint.prefix_sha256 = digest(&[]);
                    write_checkpoint(&checkpoint_path, &checkpoint)?;
                } else {
                    let file = OpenOptions::new().read(true).write(true).open(&part)?;
                    if file.metadata()?.len() < checkpoint.durable_offset as u64 {
                        bail!("diagnostic part shorter than checkpoint");
                    }
                    file.set_len(checkpoint.durable_offset as u64)?;
                    file.sync_all()?;
                    let prefix = fs::read(&part)?;
                    if digest(&prefix) != checkpoint.prefix_sha256 {
                        bail!("diagnostic prefix digest mismatch");
                    }
                }
            }
        }
        Ok(Self { root, checkpoint })
    }

    fn save(&self) -> Result<()> { write_checkpoint(&self.root.join("checkpoint.json"), &self.checkpoint) }

    pub fn serial(&mut self) -> Result<u64> {
        if self.checkpoint.phase == "idle" || self.checkpoint.phase == "complete" {
            self.checkpoint.client_serial = self.checkpoint.client_serial.checked_add(1)
                .context("diagnostic client serial overflow")?;
            self.checkpoint.batch_id = None;
            self.checkpoint.phase = "starting".into();
            self.save()?;
        }
        Ok(self.checkpoint.client_serial)
    }

    pub fn batch_id(&self) -> Option<&str> { self.checkpoint.batch_id.as_deref() }
    pub fn offset(&self) -> usize { self.checkpoint.durable_offset }
    pub fn awaiting_ack(&self) -> bool { self.checkpoint.phase == "awaiting_device_ack" }

    /// A device that lost its frozen batch cannot confirm an already archived file.
    /// Keep the archive and an explicit loss record before reserving a new serial.
    fn retire_lost_ack(&mut self, status: &Value) -> Result<bool> {
        if !self.awaiting_ack() { return Ok(false); }
        let id = self.batch_id().context("missing archived batch ID")?;
        let pending = &status["sync"]["pending_batch"];
        let completed = &status["sync"]["last_completed"];
        if pending["batch_id"] == id || completed["batch_id"] == id { return Ok(false); }
        if !pending.is_null() { bail!("diagnostic device has a different active batch"); }
        let archive = fs::read(self.root.join(format!("{id}.json")))?;
        if digest(&archive) != self.checkpoint.sha256 || archive.len() != self.checkpoint.bytes {
            bail!("archive_lost: unconfirmed diagnostic archive changed");
        }
        let body: Value = serde_json::from_slice(&archive)?;
        if body["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac)
                .as_deref() != Some(self.checkpoint.mac.as_str()) ||
            body["bridge_id"] != self.checkpoint.bridge_id || body["batch_id"] != id ||
            body["client_serial"] != self.checkpoint.client_serial.to_string() {
            bail!("unconfirmed diagnostic archive identity mismatch");
        }
        validate_diag(&body)?;
        let device_serial = completed["client_serial"].as_str()
            .and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        let retirement = self.root.join(format!("retired-lost-{id}.json"));
        if retirement.exists() { bail!("diagnostic retirement record already exists"); }
        write_checkpoint(&retirement, &json!({
            "reason":"device_batch_lost_without_ack", "checkpoint":self.checkpoint,
            "device_pending_batch":null, "device_last_completed":completed,
            "archive_sha256":digest(&archive),
        }))?;
        self.checkpoint.client_serial = self.checkpoint.client_serial.max(device_serial);
        self.checkpoint.batch_id = None;
        self.checkpoint.bytes = 0;
        self.checkpoint.sha256.clear();
        self.checkpoint.durable_offset = 0;
        self.checkpoint.prefix_sha256 = digest(&[]);
        self.checkpoint.phase = "idle".into();
        self.save()?;
        Ok(true)
    }

    pub fn begin(&mut self, id: &str, bytes: usize, sha256: &str) -> Result<()> {
        if !safe_id(id) || bytes == 0 || bytes > 16384 || sha256.len() != 64 ||
            !sha256.bytes().all(|b| b.is_ascii_hexdigit()) { bail!("invalid diagnostic manifest"); }
        if let Some(current) = self.checkpoint.batch_id.as_deref() {
            if current != id || self.checkpoint.bytes != bytes || self.checkpoint.sha256 != sha256 {
                bail!("diagnostic batch conflict");
            }
            return Ok(());
        }
        if self.checkpoint.phase != "starting" { bail!("diagnostic serial not reserved"); }
        if self.root.join(format!("{id}.json")).exists() {
            bail!("protocol_conflict: diagnostic batch ID already archived");
        }
        let part = self.root.join(format!("{id}.part"));
        let file = File::create(&part)?;
        file.sync_all()?;
        self.checkpoint.batch_id = Some(id.into());
        self.checkpoint.bytes = bytes;
        self.checkpoint.sha256 = sha256.into();
        self.checkpoint.durable_offset = 0;
        self.checkpoint.prefix_sha256 = digest(&[]);
        self.checkpoint.phase = "receiving".into();
        self.save()
    }

    pub fn append_page(&mut self, offset: usize, page: &[u8]) -> Result<(usize, String)> {
        if self.checkpoint.phase != "receiving" || page.len() > 1024 ||
            offset > self.checkpoint.bytes || offset + page.len() > self.checkpoint.bytes {
            bail!("diagnostic page range");
        }
        let id = self.checkpoint.batch_id.as_deref().context("no diagnostic batch")?;
        let part = self.root.join(format!("{id}.part"));
        let mut file = OpenOptions::new().read(true).write(true).open(&part)?;
        if offset < self.checkpoint.durable_offset {
            if offset + page.len() > self.checkpoint.durable_offset { bail!("overlapping diagnostic page"); }
            file.seek(SeekFrom::Start(offset as u64))?;
            let mut old = vec![0; page.len()];
            file.read_exact(&mut old)?;
            if old != page { bail!("protocol_conflict: duplicate page bytes differ"); }
        } else {
            if offset != self.checkpoint.durable_offset { bail!("diagnostic page skipped prefix"); }
            file.seek(SeekFrom::Start(offset as u64))?;
            file.write_all(page)?;
            file.sync_all()?;
            self.checkpoint.durable_offset += page.len();
            self.checkpoint.prefix_sha256 = digest(&fs::read(&part)?);
            self.save()?;
        }
        Ok((self.checkpoint.durable_offset, self.checkpoint.prefix_sha256.clone()))
    }

    pub fn finish(&mut self) -> Result<()> {
        if self.awaiting_ack() { return Ok(()); }
        if self.checkpoint.phase != "receiving" || self.checkpoint.durable_offset != self.checkpoint.bytes {
            bail!("diagnostic batch incomplete");
        }
        let id = self.checkpoint.batch_id.as_deref().context("no diagnostic batch")?;
        let part = self.root.join(format!("{id}.part"));
        let bytes = fs::read(&part)?;
        if digest(&bytes) != self.checkpoint.sha256 { bail!("diagnostic whole-file digest mismatch"); }
        let body: serde_json::Value = serde_json::from_slice(&bytes)?;
        if body["format"] != "device-sync-1" ||
            body["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac)
                .as_deref() != Some(self.checkpoint.mac.as_str()) ||
            body["bridge_id"] != self.checkpoint.bridge_id || body["batch_id"] != id {
            bail!("diagnostic frozen identity mismatch");
        }
        validate_diag(&body)?;
        check_existing_records(&self.root, &body)?;
        let final_path = self.root.join(format!("{id}.json"));
        fs::rename(&part, &final_path)?;
        OpenOptions::new().write(true).open(&final_path)?.sync_all()?;
        self.checkpoint.phase = "awaiting_device_ack".into();
        self.save()
    }

    pub fn mark_complete(&mut self) -> Result<()> {
        if !self.awaiting_ack() { bail!("diagnostic completion not pending"); }
        self.checkpoint.phase = "complete".into();
        self.save()?;
        retain_archives(&self.root)
    }

    pub fn bytes(&self) -> usize { self.checkpoint.bytes }
    pub fn hash(&self) -> &str { &self.checkpoint.sha256 }
}

/// Run a frozen transfer over the existing Bridge -> device HTTP direction.
/// A transport error leaves the checkpoint intact for the next Wi-Fi opening.
pub fn transfer(data_root: &Path, link: &DeviceLink) -> Result<Value> {
    let compact_mac = DeviceIdentity::normalized_mac(&link.mac)
        .context("sync device MAC invalid")?;
    let wire_mac = compact_mac.as_bytes().chunks(2)
        .map(|pair| std::str::from_utf8(pair).unwrap())
        .collect::<Vec<_>>().join(":");
    let timeout = Duration::from_secs(15);
    let mut status = None;
    for attempt in 0..3 {
        match device_client::status(&link.ip, &link.token, timeout) {
            Ok(value) => { status = Some(value); break; }
            Err(error) if error.to_string().contains("401") => return Err(error),
            Err(error) if attempt == 2 => return Err(error).context("sync Wi-Fi association unavailable"),
            Err(_) => std::thread::sleep(Duration::from_secs(if attempt == 0 { 1 } else { 3 })),
        }
    }
    let status = status.context("sync Wi-Fi status unavailable")?;
    if status["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac)
        .as_deref() != Some(compact_mac.as_str()) { bail!("sync status MAC mismatch"); }
    if status["sync_v1"] != 1 || status["sync"]["enabled"] != true {
        return Ok(json!({"result":"unsupported_or_disabled"}));
    }
    let nonce = status["session_nonce"].as_str().context("sync session nonce missing")?;
    let mut store = Store::open(data_root, &compact_mac, &link.bridge_id)?;
    store.retire_lost_ack(&status)?;
    let mut request = 0u32;
    let mut call = |op: &str, fields: Value| -> Result<Value> {
        request += 1;
        let id = format!("sync-{op}-{request}");
        device_client::sync(&link.ip, &link.token, &wire_mac, &link.bridge_id,
            nonce, op, &id, &fields, timeout)
    };
    if store.awaiting_ack() {
        let id = store.batch_id().context("missing completed batch id")?.to_owned();
        let reply = call("complete", json!({"batch_id":id,"bytes":store.bytes(),"sha256":store.hash()}))?;
        store.mark_complete()?;
        return Ok(json!({"result":"complete","receipt":reply["receipt"]}));
    }
    let pending = &status["sync"]["pending_batch"];
    let due = status["sync"]["due"] == true;
    let obligations = status["sync"]["reasons"].as_array().cloned().unwrap_or_default();
    if !due && pending.is_null() && obligations.is_empty() {
        return Ok(json!({"result":"idle"}));
    }
    let serial = store.serial()?;
    let reasons = if pending["reasons"].is_array() { pending["reasons"].clone() }
                  else {
                      let mut reasons = obligations;
                      if due { reasons.push(json!("periodic")); }
                      Value::Array(reasons)
                  };
    let manifest = call("begin", json!({"client_serial":serial.to_string(),"reasons":reasons}))?;
    if manifest["result"] == "already_complete" {
        bail!("archive_lost: device completed a batch without a durable local archive");
    }
    let id = manifest["batch_id"].as_str().context("sync batch id missing")?;
    let bytes = manifest["bytes"].as_u64().context("sync batch size missing")? as usize;
    let hash = manifest["sha256"].as_str().context("sync batch hash missing")?;
    store.begin(id, bytes, hash)?;
    let mut last_progress = Instant::now();
    let mut failures = 0u8;
    if store.offset() > 0 {
        let (_, prefix) = store.append_page(0, &[])?;
        call("ack", json!({"batch_id":id,"offset":store.offset(),"prefix_sha256":prefix}))?;
    }
    while store.offset() < bytes {
        if last_progress.elapsed() >= Duration::from_secs(90) { bail!("sync_incomplete: 90s without durable progress"); }
        let offset = store.offset();
        let page = match call("page", json!({"batch_id":id,"offset":offset,"limit":1024})) {
            Ok(page) => { failures = 0; page },
            Err(error) => {
                failures += 1;
                if failures >= 3 { return Err(error).context("sync_incomplete: three page failures"); }
                continue;
            }
        };
        if page["batch_id"] != id || page["offset"] != offset { bail!("sync page identity mismatch"); }
        let encoded = page["data_b64"].as_str().context("sync page bytes missing")?;
        let chunk = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        if page["next_offset"] != offset + chunk.len() ||
            page["chunk_sha256"] != digest(&chunk) || chunk.is_empty() {
            bail!("sync page digest or range mismatch");
        }
        let (durable, prefix) = store.append_page(offset, &chunk)?;
        last_progress = Instant::now();
        let mut ack_errors = 0;
        loop {
            match call("ack", json!({"batch_id":id,"offset":durable,"prefix_sha256":prefix})) {
                Ok(reply) if reply["acked_offset"] == durable => break,
                Ok(_) => bail!("sync ACK offset mismatch"),
                Err(error) => {
                    ack_errors += 1;
                    if ack_errors >= 3 { return Err(error).context("sync_incomplete: three ACK failures"); }
                }
            }
        }
    }
    store.finish()?;
    let reply = call("complete", json!({"batch_id":id,"bytes":bytes,"sha256":hash}))?;
    store.mark_complete()?;
    Ok(json!({"result":"complete","batch_id":id,"bytes":bytes,"receipt":reply["receipt"]}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;
    use std::process::{Command, Stdio};

    // Run explicitly with CODEX_STATUS_SYNC_TEST_SIM_EXE set to the isolated
    // device-sim binary. The production Bridge archive path drives a real
    // Note4 Fake ROM HTTP server; no in-memory page stub is involved.
    #[test]
    #[ignore = "requires CODEX_STATUS_SYNC_TEST_SIM_EXE"]
    fn sync_v1_bridge_archive_against_note4_fake_rom() {
        let exe = std::env::var("CODEX_STATUS_SYNC_TEST_SIM_EXE")
            .expect("set CODEX_STATUS_SYNC_TEST_SIM_EXE");
        let dir = tempfile::tempdir().unwrap();
        let mut child = Command::new(exe)
            .args(["--listen", "127.0.0.1:0", "--mac", "02:00:00:00:00:B2",
                   "--target", "zectrix-note4-400x300", "--data-dir"])
            .arg(dir.path().join("device"))
            .env("CODEX_STATUS_SIM_ENDPOINT_TOKEN", "sync-endpoint")
            .env("CODEX_STATUS_SIM_DEVICE_TOKEN", "sync-device")
            .env("CODEX_STATUS_SIM_CONTROL_TOKEN", "sync-control")
            .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let result = (|| -> Result<()> {
            let mut line = String::new();
            BufReader::new(child.stdout.as_mut().unwrap()).read_line(&mut line)?;
            let ready: Value = serde_json::from_str(&line)?;
            let address = ready["http"].as_str().context("fake ROM address")?
                .trim_start_matches("http://").to_owned();
            let http = |method: &str, path: &str, token: &str, body: &Value| -> Result<Value> {
                let bytes = if body.is_null() { Vec::new() } else { serde_json::to_vec(body)? };
                let mut socket = TcpStream::connect(&address)?;
                socket.set_read_timeout(Some(Duration::from_secs(5)))?;
                write!(socket, "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n", bytes.len())?;
                socket.write_all(&bytes)?;
                let mut response = Vec::new();
                socket.read_to_end(&mut response)?;
                let start = response.windows(4).position(|part| part == b"\r\n\r\n")
                    .context("fake ROM HTTP response")? + 4;
                Ok(serde_json::from_slice(&response[start..])?)
            };
            let claim = http("POST", "/claim?id=bridge-test&lease=120", "sync-device", &Value::Null)?;
            anyhow::ensure!(claim["owner"]["id"] == "bridge-test", "Fake ROM claim failed: {claim}");
            let status = http("GET", "/api/status", "sync-endpoint", &Value::Null)?;
            let config = json!({"op":"sync_config","request_id":"config-1",
                "token":"sync-endpoint","device_mac":"02:00:00:00:00:B2",
                "session_nonce":status["session_nonce"],"bridge_id":"bridge-test","enabled":true});
            let ack = http("POST", "/sim/ble/command", "sync-endpoint", &config)?;
            anyhow::ensure!(ack["result"] == "applied", "Fake ROM config failed: {ack}");
            let link = DeviceLink { mac: "0200000000B2".into(), ip: address,
                token: "sync-endpoint".into(), bridge_id: "bridge-test".into() };
            let transfer_result = transfer(&dir.path().join("bridge"), &link)?;
            anyhow::ensure!(transfer_result["result"] == "complete",
                "Bridge transfer incomplete: {transfer_result}");
            let archive = dir.path().join("bridge/platform/diagnostics/0200000000B2")
                .join(format!("{}.json", transfer_result["batch_id"].as_str()
                    .context("missing batch id")?));
            let body: Value = serde_json::from_slice(&fs::read(&archive)?)?;
            anyhow::ensure!(body["device_mac"].as_str()
                .and_then(DeviceIdentity::normalized_mac).as_deref() == Some(link.mac.as_str()) &&
                body["format"] == "device-sync-1",
                "Bridge archive identity mismatch");
            anyhow::ensure!(transfer(&dir.path().join("bridge"), &link)?["result"] == "idle",
                "completed batch did not become idle");
            Ok(())
        })();
        let _ = child.kill();
        let _ = child.wait();
        result.unwrap();
    }

    #[test]
    fn crashes_between_part_and_checkpoint_resume_exact_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path(), "0200000000A1", "bridge-a").unwrap();
        assert_eq!(store.serial().unwrap(), 1);
        let body = br#"{"format":"device-sync-1","device_mac":"0200000000A1","bridge_id":"bridge-a","batch_id":"1-abc","diag":{"generation":"00000000000000000000000000000001","from_seq":"1","through_seq":"0","records_b64":"","gaps":[]}}"#;
        store.begin("1-abc", body.len(), &digest(body)).unwrap();
        store.append_page(0, &body[..20]).unwrap();
        fs::write(store.root.join("1-abc.part"), body).unwrap(); // part ahead of checkpoint
        let mut resumed = Store::open(dir.path(), "0200000000A1", "bridge-a").unwrap();
        assert_eq!(resumed.offset(), 20);
        resumed.append_page(20, &body[20..]).unwrap();
        resumed.finish().unwrap();
        let pending = Store::open(dir.path(), "0200000000A1", "bridge-a").unwrap();
        assert!(pending.awaiting_ack());
        resumed.mark_complete().unwrap();
        assert_eq!(Store::open(dir.path(), "0200000000A1", "bridge-a").unwrap().serial().unwrap(), 2);
    }

    #[test]
    fn lost_device_batch_retires_archive_without_forging_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path(), "0200000000A1", "bridge-a").unwrap();
        assert_eq!(store.serial().unwrap(), 1);
        let body = br#"{"format":"device-sync-1","device_mac":"0200000000A1","bridge_id":"bridge-a","batch_id":"1-abc","client_serial":"1","diag":{"generation":"00000000000000000000000000000001","from_seq":"1","through_seq":"0","records_b64":"","gaps":[]}}"#;
        store.begin("1-abc", body.len(), &digest(body)).unwrap();
        store.append_page(0, body).unwrap();
        store.finish().unwrap();
        assert!(store.retire_lost_ack(&json!({"sync":{"pending_batch":{"batch_id":"2-other"}}})).is_err());
        assert!(store.awaiting_ack());
        assert!(!store.retire_lost_ack(&json!({"sync":{"pending_batch":null,
            "last_completed":{"batch_id":"1-abc"}}})).unwrap());
        assert!(store.retire_lost_ack(&json!({"sync":{"pending_batch":null,
            "last_completed":null}})).unwrap());
        assert_eq!(store.serial().unwrap(), 2);
        assert!(store.root.join("1-abc.json").exists());
        let record: Value = serde_json::from_slice(&fs::read(store.root.join("retired-lost-1-abc.json")).unwrap()).unwrap();
        assert_eq!(record["reason"], "device_batch_lost_without_ack");
        assert_eq!(record["checkpoint"]["phase"], "awaiting_device_ack");
        assert_eq!(record["archive_sha256"], digest(body));
    }

    #[test]
    fn same_generation_and_sequence_never_overwrite_different_record_bytes() {
        fn body(id: &str, uptime: u32) -> Vec<u8> {
            let mut record = [0u8; 24];
            record[..2].copy_from_slice(&24u16.to_le_bytes());
            record[2] = 2;
            record[4..12].copy_from_slice(&1u64.to_le_bytes());
            record[16..20].copy_from_slice(&uptime.to_le_bytes());
            let crc = crc32fast::hash(&record[..20]);
            record[20..].copy_from_slice(&crc.to_le_bytes());
            serde_json::to_vec(&json!({"format":"device-sync-1",
                "device_mac":"0200000000A1", "bridge_id":"bridge-a", "batch_id":id,
                "diag":{"generation":"00000000000000000000000000000001",
                    "from_seq":"1","through_seq":"1",
                    "records_b64":base64::engine::general_purpose::STANDARD.encode(record),
                    "gaps":[]}})).unwrap()
        }
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path(), "0200000000A1", "bridge-a").unwrap();
        for (id, uptime) in [("1-aaaa", 1), ("2-bbbb", 2)] {
            store.serial().unwrap();
            let bytes = body(id, uptime);
            store.begin(id, bytes.len(), &digest(&bytes)).unwrap();
            store.append_page(0, &bytes).unwrap();
            if uptime == 1 { store.finish().unwrap(); store.mark_complete().unwrap(); }
            else { assert!(store.finish().unwrap_err().to_string().contains("protocol_conflict")); }
        }
    }

    #[test]
    fn owner_change_preserves_old_partial_without_serving_it_to_new_owner() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = Store::open(dir.path(), "0200000000A1", "bridge-a").unwrap();
        first.serial().unwrap();
        first.begin("1-aaaa", 4, &digest(b"test")).unwrap();
        first.append_page(0, b"te").unwrap();
        drop(first);
        let mut second = Store::open(dir.path(), "0200000000A1", "bridge-b").unwrap();
        assert_eq!(second.serial().unwrap(), 1);
        assert!(second.batch_id().is_none());
        assert!(second.root.join("1-aaaa.part").exists());
        assert!(fs::read_dir(&second.root).unwrap().any(|entry|
            entry.unwrap().file_name().to_string_lossy().starts_with("retired-")));
    }
}
