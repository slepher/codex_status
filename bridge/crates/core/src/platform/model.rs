//! v2 platform model (`docs/generic-display-platform-design-v2.md` §3).
//!
//! Minimal records, no per-entity services: Device, TemplateKey, Profile (1–8
//! ordered ids), CompiledTemplate (in `compile`), Bundle, PublishJob, PowerPlan.
//! `template_id + render_target` keeps only the latest content; per-device
//! Profile entries are plain ids with no revision or enabled subset.

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::compile::{CompiledTemplate, COMPILER_ABI};

pub const MAX_PROFILE_TEMPLATES: usize = 8;
pub const MAX_TEMPLATE_BYTES: usize = 32768;
pub const MAX_BUNDLE_BYTES: usize = 512 * 1024;
pub const MAX_SNAPSHOT_BYTES: usize = 8192;

pub const RENDER_TARGET_154G: &str = "epd-ssd1681-200x200-1bpp";
pub const RENDER_TARGET_NOTE4: &str = "epd-ssd2683-400x300-1bpp";
pub const RENDER_TARGET_GRAY4: &str = "epd-200x200-2bpp-gray4";
pub const FIRMWARE_TARGET_154G: &str = "codex-status-154g";
pub const FIRMWARE_TARGET_NOTE4: &str = "zectrix-note4-400x300";
pub const FIRMWARE_TARGET_GRAY4: &str = "codex-status-154g-gray4";

/// Device capability contract (v2 §5). Both sides validate the target; the
/// bridge pre-check never replaces the device-side rejection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    pub firmware_target: String,
    pub render_target: String,
    pub width: u32,
    pub height: u32,
    pub pixel_format: String,
    pub colors: String,
    pub compiler_abi: u32,
    pub max_templates: u32,
    pub max_bundle_bytes: u64,
    #[serde(default)]
    pub asset_publish_protocol: u32,
    #[serde(default)]
    pub max_object_bytes: u64,
    #[serde(default)]
    pub max_manifest_bytes: u64,
    #[serde(default)]
    pub install_peak_bytes: u64,
    #[serde(default)]
    pub free_bytes: u64,
    #[serde(default)]
    pub filesystem_overhead_bytes: u64,
    pub max_fields: u32,
    pub max_snapshot_bytes: u32,
    /// `ram`, `rtc`, `flash`.
    pub retention: Vec<String>,
    pub ble: bool,
    pub wifi: bool,
    pub partial: bool,
    pub max_light_s: u32,
    /// False marks an implemented but not hardware-verified target.
    pub hardware_verified: bool,
}

impl Default for DeviceCapabilities {
    fn default() -> Self {
        Self::ssd1681_154g()
    }
}

impl DeviceCapabilities {
    pub fn ssd1681_154g() -> Self {
        Self {
            firmware_target: FIRMWARE_TARGET_154G.into(),
            render_target: RENDER_TARGET_154G.into(),
            width: 200,
            height: 200,
            pixel_format: "1bpp".into(),
            colors: "bw".into(),
            compiler_abi: COMPILER_ABI,
            max_templates: MAX_PROFILE_TEMPLATES as u32,
            max_bundle_bytes: 262_144,
            asset_publish_protocol: 0,
            max_object_bytes: 0,
            max_manifest_bytes: 0,
            install_peak_bytes: 0,
            free_bytes: 0,
            filesystem_overhead_bytes: 0,
            max_fields: 64,
            max_snapshot_bytes: MAX_SNAPSHOT_BYTES as u32,
            retention: vec!["ram".into(), "rtc".into(), "flash".into()],
            ble: true,
            wifi: true,
            partial: true,
            max_light_s: 600,
            hardware_verified: true,
        }
    }

    /// Software-defined gray target: independent ROM/render path; the panel is
    /// not hardware-verified (`blocked_by_hardware_arrival`).
    pub fn gray4_software() -> Self {
        Self {
            firmware_target: FIRMWARE_TARGET_GRAY4.into(),
            render_target: RENDER_TARGET_GRAY4.into(),
            width: 200,
            height: 200,
            pixel_format: "2bpp".into(),
            colors: "gray4".into(),
            compiler_abi: COMPILER_ABI,
            max_templates: MAX_PROFILE_TEMPLATES as u32,
            max_bundle_bytes: 262_144,
            asset_publish_protocol: 0,
            max_object_bytes: 0,
            max_manifest_bytes: 0,
            install_peak_bytes: 0,
            free_bytes: 0,
            filesystem_overhead_bytes: 0,
            max_fields: 64,
            max_snapshot_bytes: MAX_SNAPSHOT_BYTES as u32,
            retention: vec!["ram".into()],
            ble: true,
            wifi: true,
            partial: false,
            max_light_s: 600,
            hardware_verified: false,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.firmware_target.is_empty() || self.render_target.is_empty() {
            bail!("capabilities: empty target id");
        }
        if self.width == 0 || self.height == 0 {
            bail!("capabilities: zero canvas");
        }
        let expected = match self.render_target.as_str() {
            RENDER_TARGET_154G => (200, 200, "1bpp", "bw"),
            RENDER_TARGET_NOTE4 => (400, 300, "1bpp", "bw"),
            RENDER_TARGET_GRAY4 => (200, 200, "2bpp", "gray4"),
            other => bail!("capabilities: unsupported render_target {other}"),
        };
        if (self.width, self.height, self.pixel_format.as_str(), self.colors.as_str()) != expected {
            bail!("capabilities: target {} disagrees with canvas/pixel format", self.render_target);
        }
        if self.render_target == RENDER_TARGET_NOTE4 && self.firmware_target != FIRMWARE_TARGET_NOTE4 {
            bail!("capabilities: Note4 firmware target mismatch");
        }
        if self.asset_publish_protocol > 0 && (self.max_object_bytes == 0 || self.max_manifest_bytes == 0 || self.install_peak_bytes == 0 || self.free_bytes == 0) {
            bail!("capabilities: incremental publish limits missing");
        }
        if !(1..=COMPILER_ABI).contains(&self.compiler_abi) {
            bail!(
                "capabilities: compiler_abi {} unsupported (bridge supports 1..={COMPILER_ABI})",
                self.compiler_abi
            );
        }
        if self.max_templates == 0 || self.max_templates > MAX_PROFILE_TEMPLATES as u32 {
            bail!("capabilities: max_templates out of range");
        }
        Ok(())
    }

    pub fn supports_render_target(&self, render_target: &str) -> bool {
        self.render_target == render_target
    }
}

/// Template library key: same id + render target keeps only the latest content.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TemplateKey {
    pub template_id: String,
    pub render_target: String,
}

impl TemplateKey {
    pub fn new(template_id: impl Into<String>, render_target: impl Into<String>) -> Self {
        Self {
            template_id: template_id.into(),
            render_target: render_target.into(),
        }
    }
}

/// A saved template: canonical source plus the compiled artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub key: TemplateKey,
    pub source: Value,
    pub source_crc: String,
    pub compiled: CompiledTemplate,
    pub saved_at: u64,
}

/// Binding of a template requirement to a DataSource field (v2 §3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub template_id: String,
    pub field: String,
    pub source_id: String,
    pub source_field: String,
}

/// Per-device Profile: 1–8 ordered template ids; order is the full key cycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub device_mac: String,
    pub template_ids: Vec<String>,
    /// Exact display target. Older drafts are migrated only with known device capabilities.
    #[serde(default)]
    pub render_target: Option<String>,
    /// Explicit asset version for each template-visible font name.
    #[serde(default)]
    pub font_ids: BTreeMap<String, String>,
    #[serde(default)]
    pub initial_active_id: Option<String>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
    #[serde(default)]
    pub sync_enabled: bool,
    /// Full-sync threshold shown in the device sync settings (seconds).
    #[serde(default = "default_full_sync_s")]
    pub full_sync_s: u64,
    #[serde(default)]
    pub updated_at: u64,
}

/// Reusable v2 Profile draft shared by every device with the same render target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FamilyProfile {
    pub render_target: String,
    pub id: String,
    pub name: String,
    pub template_ids: Vec<String>,
    #[serde(default)]
    pub enabled_template_ids: Option<Vec<String>>,
    pub initial_active_id: Option<String>,
    #[serde(default)]
    pub font_ids: BTreeMap<String, String>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
    #[serde(default)]
    pub sync_enabled: bool,
    #[serde(default = "default_full_sync_s")]
    pub full_sync_s: u64,
    #[serde(default)]
    pub updated_at: u64,
}

impl FamilyProfile {
    /// Enabled ids in profile order. Missing subsets from older drafts mean all enabled.
    pub fn enabled_ids(&self) -> Vec<String> {
        match &self.enabled_template_ids {
            Some(enabled) => self
                .template_ids
                .iter()
                .filter(|id| enabled.contains(id))
                .cloned()
                .collect(),
            None => self.template_ids.clone(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if ![
            RENDER_TARGET_154G,
            RENDER_TARGET_NOTE4,
            RENDER_TARGET_GRAY4,
        ]
        .contains(&self.render_target.as_str())
        {
            bail!("unsupported render_target {}", self.render_target);
        }
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            bail!("invalid family profile id: use ASCII letters, digits, _ or -");
        }
        if self.template_ids.len() > MAX_PROFILE_TEMPLATES {
            bail!("at most {MAX_PROFILE_TEMPLATES} templates are allowed");
        }
        if let Some(enabled) = &self.enabled_template_ids {
            let mut seen = std::collections::BTreeSet::new();
            for id in enabled {
                if !self.template_ids.contains(id) {
                    bail!("enabled template {id} is not in the profile order");
                }
                if !seen.insert(id) {
                    bail!("duplicate enabled template: {id}");
                }
            }
        }
        Profile {
            device_mac: String::new(),
            template_ids: self.template_ids.clone(),
            render_target: Some(self.render_target.clone()),
            font_ids: self.font_ids.clone(),
            initial_active_id: self.initial_active_id.clone(),
            bindings: self.bindings.clone(),
            sync_enabled: self.sync_enabled,
            full_sync_s: self.full_sync_s,
            updated_at: self.updated_at,
        }
        .validate()
    }
}

#[cfg(test)]
mod family_profile_tests {
    use super::*;

    fn draft(id: &str) -> FamilyProfile {
        FamilyProfile {
            render_target: RENDER_TARGET_154G.into(),
            id: id.into(),
            name: "默认".into(),
            template_ids: Vec::new(),
            enabled_template_ids: None,
            initial_active_id: None,
            font_ids: BTreeMap::new(),
            bindings: Vec::new(),
            sync_enabled: false,
            full_sync_s: default_full_sync_s(),
            updated_at: 0,
        }
    }

    #[test]
    fn family_profile_id_uses_ascii_letters_digits_underscore_and_hyphen() {
        assert!(draft("Default_2-x").validate().is_ok());
        assert!(draft("").validate().is_err());
        assert!(draft("含中文").validate().is_err());
        assert!(draft("with space").validate().is_err());
        assert!(draft("bad/id").validate().is_err());
    }
}

#[cfg(test)]
mod capability_abi_tests {
    use super::*;

    #[test]
    fn capabilities_accept_abi1_and_current_abi_but_reject_unknown_versions() {
        let mut capabilities = DeviceCapabilities::ssd1681_154g();
        capabilities.compiler_abi = 1;
        assert!(capabilities.validate().is_ok());
        capabilities.compiler_abi = COMPILER_ABI;
        assert!(capabilities.validate().is_ok());
        capabilities.compiler_abi = COMPILER_ABI + 1;
        assert!(capabilities.validate().is_err());
    }
}

pub fn default_full_sync_s() -> u64 {
    3600
}

impl Profile {
    pub fn draft(device_mac: &str) -> Self {
        Self {
            device_mac: device_mac.into(),
            template_ids: Vec::new(),
            render_target: None,
            font_ids: BTreeMap::new(),
            initial_active_id: None,
            bindings: Vec::new(),
            sync_enabled: false,
            full_sync_s: default_full_sync_s(),
            updated_at: crate::now_secs(),
        }
    }

    /// Empty draft is allowed to exist; publishing it is not.
    pub fn validate(&self) -> Result<()> {
        if self.template_ids.len() > MAX_PROFILE_TEMPLATES {
            bail!(
                "profile has {} templates; at most {MAX_PROFILE_TEMPLATES} are allowed",
                self.template_ids.len()
            );
        }
        let mut seen: Vec<String> = Vec::new();
        for id in &self.template_ids {
            if id.is_empty() {
                bail!("profile contains an empty template id");
            }
            if seen.contains(id) {
                bail!("duplicate template in profile: {id}");
            }
            seen.push(id.clone());
        }
        if let Some(active) = &self.initial_active_id {
            if !self.template_ids.contains(active) {
                bail!("initial_active_id {active} is not in the profile order");
            }
        }
        Ok(())
    }

    /// Publish requires a non-empty, fully bound profile.
    pub fn validate_publishable(&self, caps: &DeviceCapabilities) -> Result<()> {
        self.validate()?;
        if self.render_target.as_deref() != Some(caps.render_target.as_str()) {
            bail!("profile render_target {:?} does not match device {}; reselect the target explicitly", self.render_target, caps.render_target);
        }
        if self.template_ids.is_empty() {
            bail!("profile is empty; nothing to publish");
        }
        if self.template_ids.len() > caps.max_templates as usize {
            bail!(
                "profile has {} templates; device target {} allows {}",
                self.template_ids.len(),
                caps.firmware_target,
                caps.max_templates
            );
        }
        Ok(())
    }

    pub fn binding_for(&self, template_id: &str, field: &str) -> Option<&Binding> {
        self.bindings
            .iter()
            .find(|b| b.template_id == template_id && b.field == field)
    }

    /// Full key cycle order (all installed templates participate).
    pub fn cycle(&self, current: &str) -> String {
        if self.template_ids.is_empty() {
            return current.to_string();
        }
        let idx = self
            .template_ids
            .iter()
            .position(|t| t == current)
            .unwrap_or(0);
        self.template_ids[(idx + 1) % self.template_ids.len()].clone()
    }
}

fn device_mac_is_valid(mac: &str) -> bool {
    let compact: String = mac.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    compact.len() == 12
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishState {
    Waiting,
    Sending,
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}

impl PublishState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            PublishState::Succeeded
                | PublishState::Failed
                | PublishState::Cancelled
                | PublishState::Unknown
        )
    }
}

/// One explicit publish freezes a complete Bundle; later edits cannot change it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishJob {
    pub job_id: String,
    pub device_mac: String,
    pub frozen_bundle: Bundle,
    pub state: PublishState,
    pub last_error: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleProfile {
    pub template_ids: Vec<String>,
    pub initial_active_id: String,
    pub bindings: Vec<Binding>,
    pub full_sync_s: u64,
}

/// Self-contained complete Bundle (v2 §3/§9): profile order, initial active,
/// all template sources and compiled artifacts, bindings, target contract,
/// embedded resources, total length/CRC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub job_id: String,
    pub device_mac: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bridge_id: String,
    pub firmware_target: String,
    pub render_target: String,
    pub compiler_abi: u32,
    pub profile: BundleProfile,
    pub templates: Vec<Template>,
    #[serde(default)]
    pub resources: Vec<BundleResource>,
    /// Canonical payload length and CRC (the CRC field itself excluded).
    pub total_len: u32,
    pub crc: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleResource {
    pub id: String,
    /// ASCII/base64 payload; embedded so the Bundle has no external references.
    pub data: String,
}

/// Canonical payload without the integrity fields.
fn bundle_payload(b: &Bundle) -> Value {
    let mut v = serde_json::to_value(b).unwrap_or(Value::Null);
    if let Some(obj) = v.as_object_mut() {
        obj.remove("total_len");
        obj.remove("crc");
    }
    v
}

impl Bundle {
    /// Serialize canonically and fill length + CRC over the canonical payload.
    pub fn seal(mut self) -> Result<Self> {
        let payload = bundle_payload(&self);
        let bytes = crate::template::canonical_bytes(&payload);
        if bytes.len() > MAX_BUNDLE_BYTES {
            bail!(
                "bundle is {} bytes; limit is {MAX_BUNDLE_BYTES}",
                bytes.len()
            );
        }
        self.total_len = bytes.len() as u32;
        self.crc = format!("{:08x}", crc32fast::hash(&bytes));
        Ok(self)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        Ok(crate::template::canonical_bytes(&bundle_payload(self)))
    }

    pub fn verify(&self) -> Result<()> {
        let bytes = self.canonical_bytes()?;
        if bytes.len() != self.total_len as usize {
            bail!(
                "bundle length mismatch: header {} payload {}",
                self.total_len,
                bytes.len()
            );
        }
        let crc = format!("{:08x}", crc32fast::hash(&bytes));
        if crc != self.crc {
            bail!("bundle CRC mismatch: header {} payload {crc}", self.crc);
        }
        if self.compiler_abi != COMPILER_ABI {
            bail!("bundle compiler_abi {} unsupported", self.compiler_abi);
        }
        if self.templates.len() != self.profile.template_ids.len() {
            bail!("bundle template count does not match profile order");
        }
        for id in &self.profile.template_ids {
            if !self
                .templates
                .iter()
                .any(|t| &t.key.template_id == id && t.key.render_target == self.render_target)
            {
                bail!(
                    "bundle missing template variant {id} for {}",
                    self.render_target
                );
            }
        }
        if !self
            .profile
            .template_ids
            .contains(&self.profile.initial_active_id)
        {
            bail!("bundle initial active is not in the profile order");
        }
        Ok(())
    }

    /// Bounded BEGIN/CHUNK/COMMIT framing used by LAN and BLE transports.
    pub fn encode_chunks(&self, payload: usize) -> Result<Vec<Vec<u8>>> {
        let header = serde_json::json!({
            "op": "bundle_begin",
            "job_id": self.job_id,
            "len": self.total_len,
            "crc": self.crc,
            "render_target": self.render_target,
            "firmware_target": self.firmware_target,
            "template_count": self.profile.template_ids.len(),
        });
        let header = crate::template::canonical_bytes(&header);
        if header.len() > payload {
            bail!("bundle BEGIN header exceeds chunk payload");
        }
        let mut out = vec![header];
        out.extend(crate::template::encode_chunks(
            &self.canonical_bytes()?,
            payload,
        ));
        out.push(
            serde_json::json!({
                "op": "bundle_commit",
                "job_id": self.job_id,
                "crc": self.crc,
            })
            .to_string()
            .into_bytes(),
        );
        Ok(out)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanMode {
    Sleep,
    Light,
}

/// Formal business power plan; the Bridge is the only producer (v2 §4/§7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PowerPlan {
    pub plan_id: u64,
    pub mode: PlanMode,
    pub light_duration_s: u32,
    pub rendezvous_period_s: u32,
    #[serde(default)]
    pub reason: String,
}

impl PowerPlan {
    pub fn sleep(plan_id: u64, rendezvous_period_s: u32, reason: &str) -> Self {
        Self {
            plan_id,
            mode: PlanMode::Sleep,
            light_duration_s: 0,
            rendezvous_period_s,
            reason: reason.into(),
        }
    }

    pub fn light(
        plan_id: u64,
        light_duration_s: u32,
        rendezvous_period_s: u32,
        reason: &str,
    ) -> Self {
        Self {
            plan_id,
            mode: PlanMode::Light,
            light_duration_s,
            rendezvous_period_s,
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub device_mac: String,
    pub name: String,
    pub ip: Option<String>,
    pub discovered_via: String,
    pub last_seen_at: u64,
}

impl DeviceIdentity {
    pub fn new(mac: &str, name: &str) -> Result<Self> {
        if !device_mac_is_valid(mac) {
            bail!("invalid device MAC: {mac}");
        }
        Ok(Self {
            device_mac: mac.to_uppercase(),
            name: if name.is_empty() {
                "CodexStatus".into()
            } else {
                name.into()
            },
            ip: None,
            discovered_via: "unknown".into(),
            last_seen_at: 0,
        })
    }

    pub fn normalized_mac(mac: &str) -> Option<String> {
        let compact: String = mac
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .collect::<String>()
            .to_uppercase();
        (compact.len() == 12).then_some(compact)
    }
}

/// Bridge-side persisted state for one device (owner is device-authoritative and
/// only mirrored here for display; writes still need the token/owner protocol).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub identity: DeviceIdentity,
    pub capabilities: DeviceCapabilities,
    #[serde(default)]
    pub profile: Option<Profile>,
    /// Last observed device state (authoritative snapshot digest from Status).
    #[serde(default)]
    pub observed: ObservedState,
    /// Last MAC-checked, endpoint-token authenticated /v2/status response.
    #[serde(default)]
    pub last_authenticated: Option<AuthenticatedStatus>,
    #[serde(default)]
    pub last_authenticated_contact_at: Option<u64>,
    #[serde(default)]
    pub last_authenticated_transport: Option<String>,
    /// Most recent authenticated status attempt; never replaces the snapshot.
    #[serde(default)]
    pub last_status_attempt: Option<StatusAttempt>,
    #[serde(default)]
    pub sync_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedStatus {
    pub observed_at: u64,
    #[serde(default)]
    pub transport: String,
    #[serde(default)]
    pub schema_version: u32,
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusAttempt {
    pub at: u64,
    pub outcome: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ObservedState {
    pub active_context_id: Option<String>,
    pub active_template_id: Option<String>,
    pub committed_job_id: Option<String>,
    pub data_seq: Option<u64>,
    pub applied_seq: Option<u64>,
    pub last_acked_at: Option<u64>,
    pub power: Option<ObservedPower>,
    pub display_state: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservedPower {
    pub mode: PlanMode,
    #[serde(default)]
    pub plan_id: u64,
    #[serde(default)]
    pub remaining_s: u32,
    #[serde(default)]
    pub provisional: bool,
    #[serde(default)]
    pub rendezvous_in_s: u32,
    #[serde(default)]
    pub battery_percent: i32,
}

/// Field-level delivery semantics (v2 §6): only push and pull exist; the device
/// neither receives nor stores this classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldTrigger {
    Push,
    Pull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Number,
    Text,
    Bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingPolicy {
    /// Omit the field; template conditions decide what to show.
    Hide,
    /// Deliver numeric zero.
    Zero,
    /// Keep the last known value but mark it stale.
    Stale,
}

/// A single data field requirement compiled into a template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldRequirement {
    /// Bounded index used by Data messages.
    pub index: u32,
    pub field: String,
    pub kind: FieldKind,
    pub missing: MissingPolicy,
    /// True when the value is supplied by the device, not a DataSource.
    pub local: bool,
}

/// Normalized field value carried inside a snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotField {
    pub value: Value,
    pub quality: Quality,
    pub observed_at: u64,
    pub valid_until: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Good,
    Stale,
    Missing,
}

/// Latest value set for one DataSource (v2 §3). A failed fetch keeps the last
/// good values and reports the error separately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceSnapshot {
    pub source_id: String,
    pub fields: BTreeMap<String, SnapshotField>,
    pub observed_at: u64,
    pub valid_until: u64,
    pub last_success_at: u64,
    pub quality: Quality,
    pub error: Option<String>,
}

impl SourceSnapshot {
    pub fn missing(source_id: &str, error: Option<String>) -> Self {
        let now = crate::now_secs();
        Self {
            source_id: source_id.into(),
            fields: BTreeMap::new(),
            observed_at: now,
            valid_until: 0,
            last_success_at: 0,
            quality: Quality::Missing,
            error,
        }
    }

    pub fn empty_good(source_id: &str) -> Self {
        let now = crate::now_secs();
        Self {
            source_id: source_id.into(),
            fields: BTreeMap::new(),
            observed_at: now,
            valid_until: now + 3600,
            last_success_at: now,
            quality: Quality::Good,
            error: None,
        }
    }
}

/// Runtime data snapshot for the active context: complete field set with
/// quality/validity for the active template only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataSnapshot {
    pub active_context_id: String,
    pub data_seq: u64,
    pub fields: BTreeMap<String, SnapshotField>,
}
