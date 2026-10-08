//! Fixed 48-byte Bridge/device frame header. Payload and carrier headers are separate.

pub const HEADER_LEN: usize = 48;
pub const MARKER: u8 = 0xC1;
pub const CONTENT_TYPE: &str = "application/vnd.codex-status.frame";
pub const MAX_OP: u16 = 27;
pub const MAX_HTTP_FRAME: usize = 4096;
pub const MAX_BLE_FRAME: usize = 8192;
pub const MAX_ATTRIBUTE: usize = 512;

pub fn operation(name: &str) -> Option<u16> {
    match name {
        "status" => Some(1), "data" => Some(2), "plan" => Some(3),
        "claim" => Some(4), "renew" => Some(5),
        "sync_begin" => Some(6), "sync_page" => Some(7),
        "sync_ack" => Some(8), "sync_complete" => Some(9),
        "bundle_begin" => Some(10), "bundle_chunk" => Some(11),
        "bundle_commit" => Some(12), "activate" => Some(13),
        "wifi_snapshot" => Some(14), "wifi_apply" => Some(15),
        "wifi_result" => Some(16), "wifi_result_ack" => Some(17),
        "wifi_command" => Some(18), "ota_image" => Some(19),
        "ready_report" => Some(20), "business_query" => Some(21),
        "network_start" => Some(22), "sync_config" => Some(23),
        "sync_open" => Some(24), "history" => Some(25),
        "wifi_owner" => Some(26), "wifi_info" => Some(27),
        other if other.starts_with("wifi_") => Some(18),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub kind: u8,
    pub op: u16,
    pub result: u16,
    pub payload_len: u16,
    pub session: [u8; 16],
    pub request_no: u64,
    pub context: [u8; 16],
}

impl Header {
    fn valid(&self) -> Result<(), &'static str> {
        if self.kind > 2 || self.op == 0 || self.op > MAX_OP || self.result > 3
            || (self.kind != 1 && self.result != 0) || self.request_no == 0
        {
            return Err("invalid frame header");
        }
        if self.context != [0; 16] && !self.context.iter().all(u8::is_ascii_hexdigit) {
            return Err("invalid context reference");
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<[u8; HEADER_LEN], &'static str> {
        self.valid()?;
        let mut out = [0; HEADER_LEN];
        out[0] = MARKER;
        out[1] = self.kind;
        out[2..4].copy_from_slice(&self.op.to_be_bytes());
        out[4..6].copy_from_slice(&self.result.to_be_bytes());
        out[6..8].copy_from_slice(&self.payload_len.to_be_bytes());
        out[8..24].copy_from_slice(&self.session);
        out[24..32].copy_from_slice(&self.request_no.to_be_bytes());
        out[32..48].copy_from_slice(&self.context);
        Ok(out)
    }

    pub fn decode(frame: &[u8], limit: usize) -> Result<(Self, &[u8]), &'static str> {
        if frame.len() < HEADER_LEN || frame.len() > limit || frame[0] != MARKER {
            return Err("invalid frame length or marker");
        }
        let payload_len = u16::from_be_bytes([frame[6], frame[7]]);
        if frame.len() != HEADER_LEN + usize::from(payload_len) {
            return Err("frame payload length mismatch");
        }
        let header = Self {
            kind: frame[1],
            op: u16::from_be_bytes([frame[2], frame[3]]),
            result: u16::from_be_bytes([frame[4], frame[5]]),
            payload_len,
            session: frame[8..24].try_into().unwrap(),
            request_no: u64::from_be_bytes(frame[24..32].try_into().unwrap()),
            context: frame[32..48].try_into().unwrap(),
        };
        header.valid()?;
        Ok((header, &frame[HEADER_LEN..]))
    }
}

/// Transient per-direction request fence. Durable business receipts remain in
/// their existing stores; this cache only prevents an old message from being
/// executed twice within the current authenticated session.
#[derive(Default)]
pub struct RequestFence {
    highest: u64,
    digest: [u8; 32],
    response: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Admission<'a> {
    New,
    Replay(&'a [u8]),
    Retired,
    Conflict,
}

impl RequestFence {
    pub fn inspect(&self, number: u64, frame: &[u8]) -> Admission<'_> {
        use sha2::{Digest, Sha256};
        if number == 0 { return Admission::Retired; }
        if number > self.highest { return Admission::New; }
        if number < self.highest || self.response.is_empty() { return Admission::Retired; }
        let digest: [u8; 32] = Sha256::digest(frame).into();
        if digest != self.digest { return Admission::Conflict; }
        Admission::Replay(&self.response)
    }

    pub fn retain(&mut self, number: u64, frame: &[u8], response: &[u8]) {
        use sha2::{Digest, Sha256};
        assert!(number > self.highest && number != 0);
        self.highest = number;
        self.digest = Sha256::digest(frame).into();
        self.response.clear();
        if response.len() <= MAX_ATTRIBUTE { self.response.extend_from_slice(response); }
    }

    pub fn highest(&self) -> u64 { self.highest }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_frame_and_boundaries() {
        let header = Header { kind: 0, op: 3, result: 0, payload_len: 2,
            session: *b"0123456789abcdef", request_no: 0x0102_0304_0506_0708,
            context: *b"0123456789abcdef" };
        let bytes = header.encode().unwrap();
        assert_eq!(&bytes[..8], &[0xC1, 0, 0, 3, 0, 0, 0, 2]);
        assert_eq!(&bytes[8..24], b"0123456789abcdef");
        assert_eq!(&bytes[24..32], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&bytes[32..48], b"0123456789abcdef");
        let mut frame = bytes.to_vec(); frame.extend_from_slice(b"{}");
        assert_eq!(frame, bridge_render::simulator_compact_frame_golden().unwrap());
        assert_eq!(Header::decode(&frame, 50).unwrap(), (header, b"{}".as_slice()));
        assert!(Header::decode(&frame, 49).is_err());
        frame[0] = 0; assert!(Header::decode(&frame, 50).is_err());
        frame[0] = MARKER; frame.push(0); assert!(Header::decode(&frame, 51).is_err());
        let mut header = header;
        header.op = MAX_OP;
        assert!(header.encode().is_ok());
        header.op += 1;
        assert!(header.encode().is_err());
        header.op = 3;
        header.request_no = 0;
        assert!(header.encode().is_err());
        header.request_no = 1;
        header.context[15] = b'g';
        assert!(header.encode().is_err());
    }

    #[test]
    fn request_fence_replays_only_the_retained_identical_result() {
        let mut fence = RequestFence::default();
        assert_eq!(fence.inspect(1, b"plan a"), Admission::New);
        fence.retain(1, b"plan a", b"accepted");
        assert_eq!(fence.inspect(1, b"plan a"), Admission::Replay(b"accepted"));
        assert_eq!(fence.inspect(1, b"plan b"), Admission::Conflict);
        assert_eq!(fence.inspect(2, b"plan b"), Admission::New);
        fence.retain(2, b"plan b", b"rejected");
        assert_eq!(fence.inspect(1, b"plan a"), Admission::Retired);
        let oversized = vec![0; MAX_ATTRIBUTE + 1];
        fence.retain(3, b"plan c", &oversized);
        assert_eq!(fence.inspect(3, b"plan c"), Admission::Retired);
    }
}
