use super::{Arc, Mutex, Value, json, Header, HEADER_LEN, RequestFence};

// Connection-local GATT attributes, distinct from business HTTP responses.
#[derive(Default)]
pub(super) struct BleDevice {
    pub(super) generation: u64,
    pub(super) drop_close_callback: bool,
    pub(super) disconnect_on_discovery: bool,
    pub(super) endpoint_writes: u64,
    pub(super) connected: bool,
    pub(super) wake: (u64, u64),
    pub(super) info: Vec<u8>,
    pub(super) status: Vec<u8>,
    pub(super) pending: Option<(u64, Vec<u8>)>,
    pub(super) fault: Option<BleFault>,
    pub(super) read_failed: bool,
    pub(super) events: std::collections::VecDeque<Value>,
    pub(super) fence: RequestFence,
}
#[derive(Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BleFault {
    pub(super) kind: String,
    #[serde(default)] pub(super) op: Option<String>,
    #[serde(default)] pub(super) size: usize,
    #[serde(default)] pub(super) delay_ms: u64,
}
impl BleDevice {
    pub(super) fn store(&mut self, op: &str, mut bytes: Vec<u8>, now: u64) {
        if op == "endpoint" {self.endpoint_writes += 1;}
        let fault = if self.fault.as_ref().is_some_and(|f| f.op.as_deref().is_none_or(|target| target == op)) {
            self.fault.take()
        } else { None };
        let kind = fault.as_ref().map(|f| f.kind.as_str()).unwrap_or("none");
        let framed = Header::decode(&bytes, 8192).ok().map(|(header, payload)| (header, payload.to_vec()));
        if let Some(f) = &fault {
            if f.size > 0 {
                // Explicit SYNTHETIC same-length frame; not a captured ROM frame.
                if let Some((header, payload)) = &framed {
                    let mut header = *header;
                    if let Ok(mut value) = serde_json::from_slice::<Value>(payload) {
                        value["padding"] = json!("");
                        let base = serde_json::to_vec(&value).unwrap();
                        if f.size >= HEADER_LEN + base.len() {
                            value["padding"] = json!("x".repeat(f.size - HEADER_LEN - base.len()));
                            let payload = serde_json::to_vec(&value).unwrap();
                            header.payload_len = payload.len() as u16;
                            bytes = header.encode().unwrap().to_vec();
                            bytes.extend_from_slice(&payload);
                        } else { bytes.resize(f.size, b'x'); }
                    } else { bytes.resize(f.size, b'x'); }
                } else if bytes.last() == Some(&b'}') && f.size >= bytes.len() + 13 {
                    bytes.pop(); bytes.extend_from_slice(b",\"padding\":\"");
                    bytes.resize(f.size - 2, b'x'); bytes.extend_from_slice(b"\"}");
                } else { bytes.resize(f.size, b'x'); }
            }
        }
        if matches!(kind, "wrong_request" | "wrong_wake" | "wrong_mac" | "wrong_session" | "wrong_op") {
            let frame_header = Header::decode(&bytes, 8192).ok().map(|(h,p)| (h,p.to_vec()));
            let json_bytes = frame_header.as_ref().map_or(bytes.as_slice(), |(_,p)| p.as_slice());
            if let Ok(mut value) = serde_json::from_slice::<Value>(json_bytes) {
                match kind {
                    "wrong_op" => value["op"] = json!("other"),
                    "wrong_request" => { value["rid"] = json!("c3RhbGU="); value["request_id"] = json!("stale"); },
                    "wrong_wake" => value["w"][1] = json!(self.wake.1.saturating_add(1)),
                    "wrong_mac" => value["device_mac"] = json!("AA:BB:CC:DD:EE:FF"),
                    "wrong_session" => value["session_nonce"] = json!("f".repeat(32)),
                    _ => unreachable!(),
                }
                let payload = serde_json::to_vec(&value).unwrap();
                if let Some((mut header,_)) = frame_header {
                    header.payload_len = payload.len() as u16;
                    bytes = header.encode().unwrap().to_vec();
                    bytes.extend_from_slice(&payload);
                } else { bytes = payload; }
            }
        }
        let attempted = bytes.len();
        let result = match kind {
            "legacy_overflow" if attempted > 512 => { self.status.clear(); "library_cleared" }
            "guard_overflow" if attempted > 512 => "precheck_failed",
            "write_failed" | "drop_ack" => "write_failed",
            "disconnect" | "window_closed" => { self.disconnect("fault"); "disconnected" }
            "empty" => { self.status.clear(); "empty" }
            "malformed" => { self.status = b"invalid-json".to_vec(); "malformed" }
            "stale" | "overwrite" => "old_value",
            "read_failed" => { self.read_failed = true; "read_failed" }
            "delay" => {
                self.pending = Some((now.saturating_add(fault.as_ref().unwrap().delay_ms), bytes));
                "delayed"
            }
            _ if attempted > 512 => "precheck_failed",
            _ => { self.status = bytes; "written" } // notify_lost is still readable
        };
        self.events.push_back(json!({"op":op,"characteristic":"status","bytes":attempted,
            "max":512,"result":result,"generation":self.generation,"wake_generation":self.wake.0,"wake_seq":self.wake.1}));
        while self.events.len() > 64 { self.events.pop_front(); }
    }
}

impl BleDevice {
    pub(super) fn disconnect(&mut self, reason: &str) {
        self.connected = false;
        self.pending = None;
        self.status.clear();
        self.events.push_back(json!({"event":"disconnected","reason":reason,"generation":self.generation}));
        while self.events.len() > 64 { self.events.pop_front(); }
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"generation":self.generation,"connected":self.connected,
            "close_callback":!self.drop_close_callback})
    }
}

/// ROM accesses a BLE device through this proxy; control operations address the
/// same device independently of ROM business commands.
#[derive(Clone, Default)]
pub(super) struct BleProxy(Arc<Mutex<BleDevice>>);
impl BleProxy {
    pub(super) fn connect(&self, wake: (u64, u64), info: Vec<u8>, force: bool) -> (u64, bool) {
        let mut device = self.0.lock().unwrap();
        let changed = force || !device.connected || device.wake != wake;
        if changed {
            device.generation = device.generation.saturating_add(1);
            device.connected = true;
            device.wake = wake;
            device.status.clear();
            device.pending = None;
            device.read_failed = false;
            device.fence = RequestFence::default();
        }
        device.info = info;
        (device.generation, changed)
    }
    pub(super) fn reset(&self) {
        let mut device = self.0.lock().unwrap();
        let generation = device.generation.saturating_add(1);
        *device = BleDevice { generation, ..Default::default() };
    }
    pub(super) fn lock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, BleDevice>> { self.0.lock() }
    pub(super) fn disconnect(&self, reason: &str) { self.0.lock().unwrap().disconnect(reason); }
    pub(super) fn store(&self, op: &str, bytes: Vec<u8>, now: u64) { self.0.lock().unwrap().store(op, bytes, now); }
}
