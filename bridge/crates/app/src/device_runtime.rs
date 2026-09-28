//! Per-MAC device runtime records (docs/generic-display-platform-design-v2.md §3).
//!
//! The bridge used to keep one global device — `device_mac`/`device_ip`/
//! `device_name` plus a handful of single-device caches — so a second device
//! could only be reached by replacing the first. Multi-device routing keeps one
//! record per authenticated Wi-Fi MAC instead.
//!
//! Two rules follow from that and are enforced here:
//!
//! 1. A record is only ever changed by an announcement that names **its** MAC.
//!    There is no "switch the global device" path: an announcement from another
//!    device cannot rewrite this one's address, name or caches.
//! 2. `primary` is the MAC an operation *without* an explicit target resolves
//!    to. It stays a single-device convenience: while two or more devices are
//!    registered, an operation that names no MAC has no answer and must be
//!    refused by the caller rather than answered with the first entry
//!    (`design.md` §2: "多设备时缺少 MAC 返回请选择设备，不能取列表首项").

use std::collections::BTreeMap;

use crate::{normalize_mac, CachedDevice, CachedOwner, CachedPmStats, Discovery};

/// Runtime state of one device, keyed by its normalized Wi-Fi MAC.
#[derive(Clone, Default)]
pub struct DeviceRuntime {
    /// Normalized Wi-Fi MAC (12 uppercase hex); the identity key.
    pub mac: String,
    /// Last known address; an attribute, never an identity.
    pub ip: String,
    /// Editable display label; not a key, and never used to address a device.
    pub name: String,
    /// How and when `ip` was last learned (udp/arp/ble/http/config/switch).
    pub discover: Option<Discovery>,
    /// Set when `ip` changed: the push loop sends immediately.
    pub ip_dirty: bool,
    /// Cached `/status.json` read for this device.
    pub status: Option<CachedDevice>,
    /// Cached `/pmstats` read for this device.
    pub pmstats: Option<CachedPmStats>,
    /// Last owner seen in this device's `/status.json`.
    pub owner: Option<CachedOwner>,
    /// Timestamp of the last successful `/claim` on this device (renew throttle).
    /// The user released this device: no auto-claim and no pushes until resumed.
    pub yielded: bool,
    /// Consecutive failed `/status.json` reads (ARP trigger threshold).
    pub fail_streak: u32,
    /// Per-device note shown on the device page (offline, occupied, push error).
    /// It replaced the single global note once more than one device existed.
    pub note: Option<String>,
}

impl DeviceRuntime {
    pub fn new(mac: impl Into<String>) -> Self {
        Self {
            mac: mac.into(),
            ..Default::default()
        }
    }


    /// Remember a learned address. Returns true when the address changed.
    pub fn set_ip(&mut self, ip: &str, via: &str, now: i64) -> bool {
        let ip = ip.trim();
        if ip.is_empty() {
            return false;
        }
        let changed = self.ip != ip;
        self.ip = ip.to_string();
        self.discover = Some(Discovery {
            via: via.to_string(),
            at: now,
        });
        if changed {
            self.ip_dirty = true;
        }
        changed
    }

    /// Record a failed `/status.json` read. Returns the new streak length.
    pub fn note_failure(&mut self) -> u32 {
        self.fail_streak = self.fail_streak.saturating_add(1);
        self.fail_streak
    }

    /// A successful contact clears the failure streak.
    pub fn note_contact(&mut self) {
        self.fail_streak = 0;
    }
}

/// Every device the bridge knows at runtime, keyed by normalized MAC.
#[derive(Default)]
pub struct DeviceRegistry {
    devices: BTreeMap<String, DeviceRuntime>,
    /// Explicitly selected MAC, if any (see the module docs).
    primary: Option<String>,
}

impl DeviceRegistry {
    pub fn len(&self) -> usize {
        self.devices.len()
    }

    /// Registered MACs in stable (sorted) order — never discovery order.
    pub fn macs(&self) -> Vec<String> {
        self.devices.keys().cloned().collect()
    }

    pub fn get(&self, mac: &str) -> Option<&DeviceRuntime> {
        let mac = normalize_mac(mac);
        self.devices.get(&mac)
    }

    pub fn get_mut(&mut self, mac: &str) -> Option<&mut DeviceRuntime> {
        let mac = normalize_mac(mac);
        self.devices.get_mut(&mac)
    }

    /// Insert or update the record for exactly this MAC. An invalid or empty
    /// MAC is ignored (returns `None`) — it is never treated as "the" device.
    ///
    /// `name` only fills an empty label: an authenticated announcement must not
    /// overwrite a name the user chose.
    ///
    /// This never sets the selection: a device appearing is not the user
    /// choosing it (`primary_mac` still answers for a lone device).
    pub fn observe(
        &mut self,
        mac: &str,
        ip: Option<&str>,
        name: Option<&str>,
        via: &str,
        now: i64,
    ) -> Option<&mut DeviceRuntime> {
        let mac = normalize_mac(mac);
        if mac.len() != 12 {
            return None;
        }
        {
            let entry = self
                .devices
                .entry(mac.clone())
                .or_insert_with(|| DeviceRuntime::new(mac.clone()));
            if let Some(ip) = ip {
                entry.set_ip(ip, via, now);
            } else if entry.discover.is_none() {
                entry.discover = Some(Discovery {
                    via: via.to_string(),
                    at: now,
                });
            }
            if let Some(name) = name {
                if entry.name.trim().is_empty() && !name.trim().is_empty() {
                    entry.name = name.trim().to_string();
                }
            }
        }
        self.devices.get_mut(&mac)
    }


    /// Explicit user selection. Returns false for an unknown MAC.
    pub fn select(&mut self, mac: &str) -> bool {
        let mac = normalize_mac(mac);
        if !self.devices.contains_key(&mac) {
            return false;
        }
        self.primary = Some(mac);
        true
    }

    /// MAC an operation without an explicit target resolves to.
    ///
    /// Deliberately `None` while several devices are registered and none has
    /// been selected: picking one for the caller is how a Note4 decision ends up
    /// at the 1.54 address.
    pub fn primary_mac(&self) -> Option<&str> {
        if let Some(mac) = self.primary.as_deref() {
            if self.devices.contains_key(mac) {
                return Some(mac);
            }
        }
        if self.devices.len() == 1 {
            self.devices.keys().next().map(String::as_str)
        } else {
            None
        }
    }

    /// Selected MAC as owned string, for building JSON without holding a borrow.
    pub fn primary_mac_owned(&self) -> Option<String> {
        self.primary_mac().map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC_A: &str = "70041DD7A340";
    const MAC_B: &str = "7C4FADB93408";

    #[test]
    fn a_second_device_never_touches_the_first_record() {
        let mut registry = DeviceRegistry::default();
        registry.observe(MAC_A, Some("192.168.3.163"), Some("书桌屏"), "udp", 100);
        let a_ip = registry.get(MAC_A).unwrap().ip.clone();
        let a_name = registry.get(MAC_A).unwrap().name.clone();
        // Learning the address is itself a change; only what happens *after*
        // this point tells us whether the other device disturbed the record.
        registry.get_mut(MAC_A).unwrap().ip_dirty = false;

        registry.observe(MAC_B, Some("192.168.3.177"), Some("Note4"), "udp", 200);

        assert_eq!(registry.len(), 2);
        let a = registry.get(MAC_A).unwrap();
        assert_eq!(a.ip, a_ip);
        assert_eq!(a.name, a_name);
        assert_eq!(a.discover.as_ref().unwrap().at, 100);
        assert!(!a.ip_dirty, "another device's announce must not dirty this one");
        assert_eq!(registry.get(MAC_B).unwrap().ip, "192.168.3.177");
    }

    #[test]
    fn an_address_change_only_touches_its_own_record() {
        let mut registry = DeviceRegistry::default();
        registry.observe(MAC_A, Some("192.168.3.163"), None, "udp", 100);
        registry.observe(MAC_B, Some("192.168.3.177"), None, "udp", 100);
        registry.get_mut(MAC_A).unwrap().ip_dirty = false;
        registry.get_mut(MAC_B).unwrap().ip_dirty = false;

        registry.observe(MAC_A, Some("192.168.3.199"), None, "arp", 300);

        assert_eq!(registry.get(MAC_A).unwrap().ip, "192.168.3.199");
        assert!(registry.get(MAC_A).unwrap().ip_dirty);
        assert!(!registry.get(MAC_B).unwrap().ip_dirty);
        assert_eq!(registry.get(MAC_B).unwrap().ip, "192.168.3.177");
    }

    #[test]
    fn an_announcement_never_overwrites_a_chosen_name() {
        let mut registry = DeviceRegistry::default();
        registry.observe(MAC_A, Some("192.168.3.163"), Some("书桌屏"), "ble", 100);
        registry.observe(MAC_A, Some("192.168.3.163"), Some("CodexStatus-D7A340"), "udp", 200);
        assert_eq!(registry.get(MAC_A).unwrap().name, "书桌屏");

        registry.observe(MAC_B, None, Some("Note4"), "ble", 100);
        assert_eq!(registry.get(MAC_B).unwrap().name, "Note4");
    }

    #[test]
    fn an_empty_name_stays_empty_for_the_panel_to_default() {
        let mut registry = DeviceRegistry::default();
        registry.observe(MAC_A, Some("192.168.3.163"), None, "udp", 100);
        assert_eq!(registry.get(MAC_A).unwrap().name, "");
    }

    #[test]
    fn the_implicit_target_exists_only_for_a_single_device() {
        let mut registry = DeviceRegistry::default();
        assert_eq!(registry.primary_mac(), None);

        registry.observe(MAC_A, Some("192.168.3.163"), None, "udp", 100);
        assert_eq!(registry.primary_mac(), Some(MAC_A));

        // Two devices: no implicit target until the user names one.
        registry.observe(MAC_B, Some("192.168.3.177"), None, "udp", 100);
        assert_eq!(registry.primary_mac(), None);

        assert!(registry.select(MAC_B));
        assert_eq!(registry.primary_mac(), Some(MAC_B));
        assert_eq!(registry.primary_mac_owned().as_deref(), Some(MAC_B));

        // Selecting an unknown device changes nothing.
        assert!(!registry.select("AABBCCDDEEFF"));
        assert_eq!(registry.primary_mac(), Some(MAC_B));
    }

    #[test]
    fn invalid_macs_are_ignored_and_never_create_a_record() {
        let mut registry = DeviceRegistry::default();
        assert!(registry.observe("", None, None, "udp", 100).is_none());
        assert!(registry.observe("nope", None, None, "udp", 100).is_none());
        assert!(registry.observe("70:04:1D:D7:A3", None, None, "udp", 100).is_none());
        assert_eq!(registry.len(), 0);

        // A full MAC with separators and lowercase normalizes into the key.
        assert!(registry
            .observe("70:04:1d:d7:a3:40", Some("192.168.3.163"), None, "ble", 100)
            .is_some());
        assert_eq!(registry.len(), 1);
        assert!(registry.get(MAC_A).is_some());
    }

    #[test]
    fn failure_streaks_are_per_device() {
        let mut registry = DeviceRegistry::default();
        registry.observe(MAC_A, Some("192.168.3.163"), None, "udp", 100);
        registry.observe(MAC_B, Some("192.168.3.177"), None, "udp", 100);

        {
            let a = registry.get_mut(MAC_A).unwrap();
            assert_eq!(a.note_failure(), 1);
            assert_eq!(a.note_failure(), 2);
            a.note_contact();
        }
        assert_eq!(registry.get(MAC_A).unwrap().fail_streak, 0);
        assert_eq!(registry.get(MAC_B).unwrap().fail_streak, 0);
    }

    #[test]
    fn a_device_note_never_leaks_to_the_other_device() {
        let mut registry = DeviceRegistry::default();
        registry.observe(MAC_A, Some("192.168.3.163"), None, "udp", 100);
        registry.observe(MAC_B, Some("192.168.3.177"), None, "udp", 100);

        registry.get_mut(MAC_A).unwrap().note = Some("被别的桥占用".to_string());

        assert_eq!(
            registry.get(MAC_A).unwrap().note.as_deref(),
            Some("被别的桥占用")
        );
        assert!(registry.get(MAC_B).unwrap().note.is_none());
    }
}
