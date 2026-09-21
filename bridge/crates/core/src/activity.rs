//! Bridge-side activity tracking for the v0.14 deep/light modes
//! (docs/power-state.md §13.3/§13.4).
//!
//! The bridge decides the device mode in the pull response. Activity is the
//! usage fingerprint (volatile fields removed) actually changing, plus manual
//! actions (template/claim/OTA) landing on `last_change_at`. Contact events
//! (pull / announce / HTTP status / claim / push) mean the device is awake.

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

/// Minimum dwell after the device is told to stay light.
pub const LIGHT_HOLD_S: i64 = 300;
/// Quiet period after which the bridge answers `deep`.
pub const QUIET_DEEP_S: i64 = 600;
/// Pull cadence while the bridge answers (docs §13.3: contact every minute;
/// the 1m × 3 → 5m × 3 → 15m stretch is the *device-side* Wi-Fi failure
/// backoff in `retryDelaySec`, not a bridge decision).
pub const ACTIVE_CONTACT_S: u64 = 60;
/// Pre-contact fallback cadence (the device default is also 60 s; replaced by
/// `ACTIVE_CONTACT_S` on the first pull response).
pub const QUIET_CONTACT_S: u64 = 900;
/// A push failure is "expected" when the device last announced deep and
/// stopped contacting us for at least this long.
const CONTACT_GRACE_S: i64 = 120;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// FNV-1a over the envelope with volatile fields removed (moved here from the
/// app so both the poller and the pull path share one fingerprint).
pub fn usage_fingerprint(usage: &Value) -> u64 {
    let mut value = usage.clone();
    if let Some(obj) = value.as_object_mut() {
        obj.remove("server_time");
        obj.remove("mode");
        obj.remove("next_contact_s");
        obj.remove("usage_rev");
        obj.remove("pending");
    }
    if let Some(buckets) = value.get_mut("buckets").and_then(|b| b.as_array_mut()) {
        for bucket in buckets.iter_mut() {
            if let Some(windows) = bucket.get_mut("windows").and_then(|w| w.as_array_mut()) {
                for window in windows.iter_mut() {
                    let used = window
                        .get("usedPercent")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    if used == 0 {
                        if let Some(obj) = window.as_object_mut() {
                            obj.remove("resetsAt");
                        }
                    }
                }
            }
        }
    }
    let text = value.to_string();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

#[derive(Default)]
struct Inner {
    usage_rev: u64,
    fingerprint: Option<u64>,
    last_change_at: Option<i64>,
    last_contact_at: Option<i64>,
    last_pull_at: Option<i64>,
    last_deep_at: Option<i64>,
    light_since: Option<i64>,
    /// Mode last reported to the device ("deep" until activity says otherwise).
    deep: bool,
    next_contact_s: u64,
    /// Last `usage_rev` reported by the device (diagnostics).
    device_usage_rev: u64,
    pending_templates: Vec<String>,
    pending_activate: Option<String>,
    pending_ota: bool,
    pull_generation: u64,
}

pub struct Activity {
    inner: Mutex<Inner>,
    pull_generation: AtomicU64,
    /// Debug override for the pull cadence (0 = auto: active 60 / quiet 900).
    debug_contact_s: AtomicU64,
    /// Debug override for the pull response/push mode: 0 auto, 1 force deep,
    /// 2 force light. Lets the test loop skip the 10 min quiet hysteresis.
    debug_mode: AtomicU8,
}

/// Bridge-side sleep intent (docs §13.4): pending work, an unexpired light
/// dwell, or recent usage activity keep the device light; otherwise it should
/// go deep. Shared by the pull response and the push envelope so the two
/// channels cannot disagree.
fn light_wanted(g: &Inner, now: i64, debug_mode: u8) -> bool {
    if debug_mode == 1 {
        return false;
    }
    if debug_mode == 2 {
        return true;
    }
    let quiet = g.last_change_at.map(|t| now - t).unwrap_or(i64::MAX);
    let light_dwell = g
        .light_since
        .map(|t| now - t < LIGHT_HOLD_S)
        .unwrap_or(false);
    let has_pending = !g.pending_templates.is_empty() || g.pending_ota;
    has_pending || (!g.deep && light_dwell) || quiet < QUIET_DEEP_S
}

impl Default for Activity {
    fn default() -> Self {
        Self::new()
    }
}

impl Activity {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                deep: true,
                next_contact_s: QUIET_CONTACT_S,
                ..Default::default()
            }),
            pull_generation: AtomicU64::new(0),
            debug_contact_s: AtomicU64::new(0),
            debug_mode: AtomicU8::new(0),
        }
    }

    /// Debug helper: force pull responses (and push envelopes) to `light` so a
    /// sleeping device comes back online and stays reachable.
    pub fn request_light(&self) {
        self.debug_mode.store(2, Ordering::SeqCst);
    }

    /// Debug helper: force pull responses/pushes to `deep` (persistent until
    /// cleared with `set_debug_mode(0)`), skipping the 10 min quiet window.
    pub fn request_deep(&self) {
        self.debug_mode.store(1, Ordering::SeqCst);
    }

    /// Debug helper: 0 = auto, 1 = deep, 2 = light.
    pub fn set_debug_mode(&self, mode: u8) {
        self.debug_mode.store(mode.min(2), Ordering::SeqCst);
    }

    pub fn debug_mode(&self) -> u8 {
        self.debug_mode.load(Ordering::SeqCst)
    }

    /// Debug helper: override the pull cadence in seconds (0 = auto).
    pub fn set_debug_contact_s(&self, secs: u64) {
        self.debug_contact_s.store(secs, Ordering::SeqCst);
    }

    pub fn debug_contact_s(&self) -> u64 {
        self.debug_contact_s.load(Ordering::SeqCst)
    }

    /// Record a new envelope; a real change bumps `usage_rev`/`last_change_at`.
    pub fn note_envelope(&self, usage: &Value) -> bool {
        let fp = usage_fingerprint(usage);
        let mut g = self.inner.lock().unwrap();
        if g.fingerprint == Some(fp) {
            return false;
        }
        g.fingerprint = Some(fp);
        if g.last_change_at.is_some() {
            g.usage_rev += 1;
        } else {
            g.usage_rev = 1;
        }
        g.last_change_at = Some(now_secs());
        true
    }

    /// Manual/explicit activity (template save/push, claim, user force sync).
    pub fn note_activity(&self, _source: &str) {
        let mut g = self.inner.lock().unwrap();
        g.last_change_at = Some(now_secs());
        g.usage_rev += 1;
    }

    /// The device is demonstrably online (pull/announce/HTTP/claim/push).
    pub fn note_contact(&self, _source: &str) {
        let mut g = self.inner.lock().unwrap();
        g.last_contact_at = Some(now_secs());
        g.last_deep_at = None;
    }

    /// `POST /deep`: the device announced it is going to sleep.
    pub fn note_deep(&self, next_contact_s: u64) {
        let mut g = self.inner.lock().unwrap();
        let now = now_secs();
        g.last_contact_at = Some(now);
        g.last_deep_at = Some(now);
        g.deep = true;
        if next_contact_s > 0 {
            g.next_contact_s = next_contact_s;
        }
    }

    /// True while a push is expected to fail (device sleeping).
    pub fn expects_deep(&self) -> bool {
        let g = self.inner.lock().unwrap();
        if g.last_deep_at.is_some() {
            return true;
        }
        let now = now_secs();
        let quiet = g.last_change_at.map(|t| now - t).unwrap_or(i64::MAX);
        let silent = g.last_contact_at.map(|t| now - t).unwrap_or(i64::MAX);
        quiet >= QUIET_DEEP_S && silent >= CONTACT_GRACE_S
    }

    /// Handle a device pull and compute the response decision.
    pub fn note_pull(&self, device_next_contact_s: u64, device_usage_rev: u64) -> Value {
        let mut g = self.inner.lock().unwrap();
        let now = now_secs();
        g.last_contact_at = Some(now);
        g.last_pull_at = Some(now);
        g.last_deep_at = None;
        g.device_usage_rev = device_usage_rev;
        let pending_templates = g.pending_templates.clone();

        let debug_mode = self.debug_mode.load(Ordering::SeqCst);
        let stay_light = light_wanted(&g, now, debug_mode);

        if stay_light {
            if g.deep {
                g.light_since = Some(now);
            }
            g.deep = false;
        } else {
            g.deep = true;
            g.light_since = None;
        }
        // The bridge is answering this pull, so it is reachable: contact every
        // minute regardless of mode. Longer intervals only come from the
        // device's own Wi-Fi failure backoff.
        g.next_contact_s = ACTIVE_CONTACT_S;
        // Debug cadence override (0 = auto) applies to both decisions.
        let debug = self.debug_contact_s.load(Ordering::SeqCst);
        if debug > 0 {
            g.next_contact_s = debug;
        }
        let _ = device_next_contact_s;
        let mode = if g.deep { "deep" } else { "light" };
        let out = json!({
            "mode": mode,
            "next_contact_s": g.next_contact_s,
            "usage_rev": g.usage_rev,
            "pending": {
                "ota": g.pending_ota,
                "templates": pending_templates,
            },
        });
        self.pull_generation.fetch_add(1, Ordering::SeqCst);
        out
    }

    /// Mode for the push envelope (docs §13.4). Uses the same quiet/dwell
    /// decision as the pull response: a light device gets `deep` after
    /// `QUIET_DEEP_S` even though the bridge's own renewals and 10 s status
    /// polls keep contacting it (contact is not activity). This is the
    /// bridge-controlled sleep path; the device's local idle timer is only a
    /// fallback.
    pub fn mode_str(&self) -> &'static str {
        let g = self.inner.lock().unwrap();
        let now = now_secs();
        let debug = self.debug_mode.load(Ordering::SeqCst);
        if light_wanted(&g, now, debug) {
            "light"
        } else {
            "deep"
        }
    }

    pub fn usage_rev(&self) -> u64 {
        self.inner.lock().unwrap().usage_rev
    }

    pub fn next_contact_s(&self) -> u64 {
        self.inner.lock().unwrap().next_contact_s
    }

    pub fn contact_generation(&self) -> u64 {
        self.pull_generation.load(Ordering::SeqCst)
    }

    /// Pending template push (queued while the device was unreachable/deep).
    pub fn queue_templates(&self, ids: Vec<String>, activate: Option<String>) {
        let mut g = self.inner.lock().unwrap();
        for id in ids {
            if !g.pending_templates.contains(&id) {
                g.pending_templates.push(id);
            }
        }
        if activate.is_some() {
            g.pending_activate = activate;
        }
    }

    pub fn pending_templates(&self) -> (Vec<String>, Option<String>) {
        let g = self.inner.lock().unwrap();
        (g.pending_templates.clone(), g.pending_activate.clone())
    }

    pub fn clear_pending_templates(&self) {
        let mut g = self.inner.lock().unwrap();
        g.pending_templates.clear();
        g.pending_activate = None;
    }

    pub fn queue_ota(&self) {
        self.inner.lock().unwrap().pending_ota = true;
    }

    pub fn pending_ota(&self) -> bool {
        self.inner.lock().unwrap().pending_ota
    }

    pub fn clear_pending_ota(&self) {
        self.inner.lock().unwrap().pending_ota = false;
    }

    /// Diagnostic snapshot for the panel/MCP.
    pub fn snapshot(&self) -> Value {
        let g = self.inner.lock().unwrap();
        let now = now_secs();
        json!({
            "mode": if g.deep { "deep" } else { "light" },
            "usage_rev": g.usage_rev,
            "last_change_s": g.last_change_at.map(|t| (now - t).max(0)),
            "last_contact_s": g.last_contact_at.map(|t| (now - t).max(0)),
            "last_pull_s": g.last_pull_at.map(|t| (now - t).max(0)),
            "last_deep_s": g.last_deep_at.map(|t| (now - t).max(0)),
            "next_contact_s": g.next_contact_s,
            "device_usage_rev": g.device_usage_rev,
            "pending_ota": g.pending_ota,
            "pending_templates": g.pending_templates,
            "pull_generation": g.pull_generation,
            "debug_contact_s": self.debug_contact_s.load(Ordering::SeqCst),
            "debug_mode": self.debug_mode.load(Ordering::SeqCst),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn fingerprint_ignores_volatile_fields() {
        let a = json!({"server_time": 1, "buckets": [{"windows": [{"usedPercent": 0, "resetsAt": 10}]}]});
        let b = json!({"server_time": 2, "buckets": [{"windows": [{"usedPercent": 0, "resetsAt": 20}]}]});
        assert_eq!(usage_fingerprint(&a), usage_fingerprint(&b));
        let c = json!({"server_time": 2, "buckets": [{"windows": [{"usedPercent": 5, "resetsAt": 20}]}]});
        assert_ne!(usage_fingerprint(&a), usage_fingerprint(&c));
    }

    #[test]
    fn rev_bumps_only_on_change() {
        let a = Activity::new();
        assert!(a.note_envelope(&json!({"server_time": 1, "buckets": []})));
        assert_eq!(a.usage_rev(), 1);
        assert!(!a.note_envelope(&json!({"server_time": 9, "buckets": []})));
        assert_eq!(a.usage_rev(), 1);
        assert!(a.note_envelope(&json!({"buckets": [{"id": "codex"}]})));
        assert_eq!(a.usage_rev(), 2);
    }

    #[test]
    fn pull_mode_hysteresis() {
        let a = Activity::new();
        let env = json!({"buckets": []});
        a.note_envelope(&env);
        // Fresh activity -> light.
        let out = a.note_pull(60, 1);
        assert_eq!(out["mode"], "light");
        assert_eq!(out["next_contact_s"], ACTIVE_CONTACT_S);
        // Quiet means the next pull after the light dwell answers deep.
        {
            let mut g = a.inner.lock().unwrap();
            g.last_change_at = Some(now_secs() - QUIET_DEEP_S - 1);
            g.light_since = Some(now_secs() - LIGHT_HOLD_S - 1);
        }
        let out = a.note_pull(60, 1);
        assert_eq!(out["mode"], "deep");
        // The bridge still answers deep, but the contact cadence stays 1 min:
        // 15 min is only the device-side Wi-Fi failure backoff.
        assert_eq!(out["next_contact_s"], ACTIVE_CONTACT_S);
        // Deep notification gates pushes.
        a.note_deep(60);
        assert!(a.expects_deep());
        a.note_contact("pull");
        assert!(!a.expects_deep());
    }

    #[test]
    fn push_mode_follows_quiet_not_contact() {
        let a = Activity::new();
        a.note_envelope(&json!({"buckets": []}));
        assert_eq!(a.mode_str(), "light", "fresh activity keeps light");
        {
            let mut g = a.inner.lock().unwrap();
            g.last_change_at = Some(now_secs() - QUIET_DEEP_S - 1);
            g.light_since = Some(now_secs() - LIGHT_HOLD_S - 1);
            g.deep = false;
        }
        // The bridge's own renewal/status contact must not extend light: the
        // push envelope is the bridge-controlled sleep command.
        a.note_contact("claim");
        assert_eq!(a.mode_str(), "deep");
        // Pending work forces light again.
        a.queue_templates(vec!["quad".into()], None);
        assert_eq!(a.mode_str(), "light");
    }

    #[test]
    fn pending_forces_light_and_is_reported() {
        let a = Activity::new();
        a.note_envelope(&json!({"buckets": []}));
        a.note_deep(900);
        a.queue_templates(vec!["quad".into()], Some("quad".into()));
        let out = a.note_pull(900, 3);
        assert_eq!(out["mode"], "light");
        assert_eq!(out["pending"]["templates"][0], "quad");
    }
}
