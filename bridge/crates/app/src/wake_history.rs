//! Durable Bridge-side copy of the firmware wake history.
//!
//! The device owns the sequence. This module only keeps an append-only JSONL
//! copy and a small cursor checkpoint, so a power loss can at worst cause a
//! duplicate response (which is filtered by seq) rather than lose a record.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const SYNC_PERIOD_SECS: i64 = 15 * 60;
// Two records keeps the request conservative under the firmware's 512-byte
// GATT response cap; the device may still return fewer records after trimming.
pub const PAGE_LIMIT: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageProgress {
    pub more: bool,
    pub received: usize,
    pub cursor: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct CursorState {
    cursor: u64,
    gap_floor: u64,
    last_complete_at: Option<i64>,
    complete: bool,
    unsupported: bool,
}

/// One MAC/generation JSONL stream plus its recoverable checkpoint.
pub struct Store {
    path: PathBuf,
    state_path: PathBuf,
    generation: u64,
    state: CursorState,
    latest: HashMap<u64, Value>,
}

impl Store {
    pub fn open(mac: &str, generation: u64) -> io::Result<Self> {
        Self::open_at(
            &bridge_core::paths::data_root().join("platform"),
            mac,
            generation,
        )
    }

    fn open_at(root: &Path, mac: &str, generation: u64) -> io::Result<Self> {
        if !valid_mac_component(mac) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "wake history MAC must be a normalized hexadecimal MAC",
            ));
        }
        fs::create_dir_all(root)?;
        let stem = format!("wake-history-{mac}-{generation}");
        let path = root.join(format!("{stem}.jsonl"));
        let state_path = root.join(format!("{stem}.cursor.json"));
        let mut latest = HashMap::new();
        let mut max_seq = 0;
        let mut max_gap_floor = 0;
        if let Ok(file) = File::open(&path) {
            for line in BufReader::new(file).lines() {
                let Ok(line) = line else { continue };
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if value.get("kind").and_then(Value::as_str) == Some("gap") {
                    if value.get("wake_generation").and_then(Value::as_u64) == Some(generation) {
                        max_gap_floor = max_gap_floor.max(
                            value
                                .get("earliest_seq")
                                .and_then(Value::as_u64)
                                .unwrap_or(0)
                                .saturating_sub(1),
                        );
                    }
                    continue;
                }
                if value.get("format").and_then(Value::as_u64) != Some(2) {
                    continue;
                }
                if value.get("wake_generation").and_then(Value::as_u64) != Some(generation) {
                    continue;
                }
                let Some(seq) = value.get("seq").and_then(Value::as_u64) else {
                    continue;
                };
                if is_complete(&value) {
                    max_seq = max_seq.max(seq);
                }
                latest.insert(seq, value);
            }
        }
        let mut state = fs::read_to_string(&state_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<CursorState>(&raw).ok())
            .unwrap_or_default();
        // A crash after the JSONL fsync and before the cursor checkpoint must
        // recover the durable records rather than request them forever.
        state.cursor = state.cursor.max(max_seq);
        state.gap_floor = state.gap_floor.max(max_gap_floor);
        Ok(Self {
            path,
            state_path,
            generation,
            state,
            latest,
        })
    }

    pub fn cursor(&self) -> u64 {
        self.state.cursor
    }

    pub fn due(&self, now: i64) -> bool {
        if self.state.unsupported {
            return false;
        }
        if !self.state.complete {
            return true;
        }
        self.state
            .last_complete_at
            .map(|at| now.saturating_sub(at) >= SYNC_PERIOD_SECS)
            .unwrap_or(true)
    }

    /// Append one page. Every changed snapshot reaches stable storage before
    /// its cursor is checkpointed. Identical seq snapshots are ignored.
    pub fn append_page(&mut self, page: &Value, now: i64) -> io::Result<PageProgress> {
        if page.get("wake_generation").and_then(Value::as_u64) != Some(self.generation) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "history page generation mismatch",
            ));
        }
        let earliest = page
            .get("earliest_seq")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if let (Some(earliest), Some(latest)) = (
            page.get("earliest_seq").and_then(Value::as_u64),
            page.get("latest_seq").and_then(Value::as_u64),
        ) {
            if latest < earliest {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "history sequence bounds are reversed",
                ));
            }
        }
        let known_floor = self.state.cursor.max(self.state.gap_floor);
        if earliest > known_floor.saturating_add(1) {
            self.append_line(&json!({
                "format": 2,
                "kind": "gap",
                "wake_generation": self.generation,
                "earliest_seq": earliest,
                "previous_cursor": self.state.cursor,
                "latest_seq": page.get("latest_seq").and_then(Value::as_u64),
            }))?;
            self.state.gap_floor = earliest.saturating_sub(1);
            self.save_state()?;
        }

        let records = page
            .get("records")
            .and_then(Value::as_array)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "history records missing"))?;
        let mut received = 0;
        let mut previous_seq = None;
        for record in records {
            if record.get("wake_generation").and_then(Value::as_u64) != Some(self.generation) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "history record generation mismatch",
                ));
            }
            if record.get("format").and_then(Value::as_u64) != Some(2) {
                continue;
            }
            let Some(seq) = record.get("seq").and_then(Value::as_u64) else {
                continue;
            };
            if previous_seq.is_some_and(|previous| seq <= previous) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "history records are not strictly increasing",
                ));
            }
            previous_seq = Some(seq);
            if record.get("complete").and_then(Value::as_bool) == Some(false)
                && seq <= self.state.cursor
                && !self.latest.contains_key(&seq)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "new active history record is behind cursor",
                ));
            }
            let changed = self
                .latest
                .get(&seq)
                .map(|previous| previous != record)
                .unwrap_or(true);
            if !changed {
                continue;
            }
            self.append_line(record)?;
            if is_complete(record) {
                self.state.cursor = self.state.cursor.max(seq);
            }
            self.save_state()?;
            self.latest.insert(seq, record.clone());
            received += 1;
        }

        let more = page.get("more").and_then(Value::as_bool).unwrap_or(false);
        self.state.complete = !more;
        if !more {
            self.state.last_complete_at = Some(now);
        }
        self.save_state()?;
        Ok(PageProgress {
            more,
            received,
            cursor: self.state.cursor,
        })
    }

    pub fn mark_unsupported(&mut self) -> io::Result<()> {
        self.state.unsupported = true;
        self.state.complete = true;
        self.save_state()
    }

    fn append_line(&self, value: &Value) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, value)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        file.write_all(b"\n")?;
        file.flush()?;
        file.sync_data()
    }

    fn save_state(&self) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(&self.state)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&self.state_path)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.flush()?;
        file.sync_data()
    }
}

fn valid_mac_component(value: &str) -> bool {
    value.len() == 12 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_complete(record: &Value) -> bool {
    record
        .get("complete")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

    fn temp_root() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let serial = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("codex-status-history-{id}-{serial}"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn page(generation: u64, records: &[u64], earliest: u64, more: bool) -> Value {
        json!({
            "wake_generation": generation,
            "records": records.iter().map(|seq| json!({"format": 2, "wake_generation": generation, "seq": seq})).collect::<Vec<_>>(),
            "earliest_seq": earliest,
            "latest_seq": records.last().copied().unwrap_or(0),
            "more": more,
        })
    }

    #[test]
    fn durable_cursor_recovery_deduplicates_and_observes_cooldown() {
        let root = temp_root();
        let mut store = Store::open_at(&root, "AABBCCDDEEFF", 4).unwrap();
        let first = store.append_page(&page(4, &[1, 2], 1, false), 100).unwrap();
        assert_eq!(first.received, 2);
        assert!(store.state.complete);
        assert!(!store.due(101));
        assert!(store.due(100 + SYNC_PERIOD_SECS));

        let mut reopened = Store::open_at(&root, "AABBCCDDEEFF", 4).unwrap();
        assert_eq!(reopened.cursor(), 2);
        assert_eq!(
            reopened
                .append_page(&page(4, &[2, 3], 1, false), 1000)
                .unwrap()
                .received,
            1
        );
        let text = fs::read_to_string(root.join("wake-history-AABBCCDDEEFF-4.jsonl")).unwrap();
        assert_eq!(text.lines().count(), 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ring_overwrite_is_recorded_as_a_gap_and_cursor_resumes_at_earliest() {
        let root = temp_root();
        let mut store = Store::open_at(&root, "AABBCCDDEEFF", 9).unwrap();
        let progress = store.append_page(&page(9, &[8, 9], 8, false), 1).unwrap();
        assert_eq!(progress.cursor, 9);
        let mut reopened = Store::open_at(&root, "AABBCCDDEEFF", 9).unwrap();
        reopened.append_page(&page(9, &[12], 12, false), 2).unwrap();
        let text = fs::read_to_string(root.join("wake-history-AABBCCDDEEFF-9.jsonl")).unwrap();
        assert!(text.contains("\"kind\":\"gap\""));
        assert!(text.contains("\"previous_cursor\":9"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incomplete_round_is_due_even_inside_the_period() {
        let root = temp_root();
        let mut store = Store::open_at(&root, "AABBCCDDEEFF", 3).unwrap();
        store.append_page(&page(3, &[1], 1, true), 100).unwrap();
        assert!(!store.state.complete);
        assert!(store.due(101));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_snapshots_are_revised_without_advancing_cursor_or_duplicating_content() {
        let root = temp_root();
        let active = |complete: bool, awake_ms: u64| {
            json!({
                "wake_generation": 6,
                "records": [{
                    "format": 2,
                    "wake_generation": 6,
                    "seq": 1,
                    "complete": complete,
                    "awake_ms": awake_ms,
                }],
                "earliest_seq": 1,
                "latest_seq": 1,
                "more": false,
            })
        };
        let mut store = Store::open_at(&root, "AABBCCDDEEFF", 6).unwrap();
        assert_eq!(
            store.append_page(&active(false, 100), 100).unwrap().cursor,
            0
        );
        assert_eq!(
            store
                .append_page(&active(false, 100), 101)
                .unwrap()
                .received,
            0
        );
        assert_eq!(
            store
                .append_page(&active(false, 120), 102)
                .unwrap()
                .received,
            1
        );
        assert_eq!(store.cursor(), 0);

        let mut reopened = Store::open_at(&root, "AABBCCDDEEFF", 6).unwrap();
        assert_eq!(reopened.cursor(), 0);
        assert_eq!(
            reopened
                .append_page(&active(true, 130), 103)
                .unwrap()
                .cursor,
            1
        );
        assert!(!reopened.due(104));
        assert_eq!(
            reopened
                .append_page(&active(true, 130), 104)
                .unwrap()
                .received,
            0
        );
        let text = fs::read_to_string(root.join("wake-history-AABBCCDDEEFF-6.jsonl")).unwrap();
        assert_eq!(text.lines().count(), 3);
        fs::remove_dir_all(root).unwrap();
    }
}
