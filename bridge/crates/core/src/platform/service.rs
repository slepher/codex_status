//! Shared application service (v2 §2/§11): the four UI pages and MCP call these
//! exact operations; there is no second business model and no direct file edit
//! path around them. Keeping the state in one mutex-serialized struct also gives
//! the per-device serial coordination the design requires.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;

use crate::coordinator::{AckOutcome, Coordinator, Delivery, DeliveryKind, DEFAULT_FULL_SYNC_S};
use crate::datasource::{DataSource, DataSourceKind};
use crate::platform::model::{
    Binding, Bundle, BundleFont, BundleProfile, BundleResource, DeviceCapabilities, DeviceIdentity,
    DeviceDataContract,
    AuthenticatedStatus, DeviceRecord, FamilyProfile, FieldRequirement, PlanMode, PowerPlan, Profile, PublishState, StatusAttempt,
    SourceSnapshot,
    Template, TemplateKey, PublishJob, MAX_PROFILE_TEMPLATES, MAX_BUNDLE_BYTES,
};
use crate::platform::store;
use crate::platform::wifi::{WifiState, Network as WifiNetwork, Job as WifiJob, Snapshot as WifiSnapshot, OperationReply, ReplySource};
use crate::platform::fonts::{FontLibrary, FontPlan};
use crate::platform::publish::{FrozenPublish, Capacity, CommittedRefs};

fn freeze_wire(bundle: &Bundle, caps: &DeviceCapabilities) -> Result<Option<String>> {
    caps.validate()?;
    let raw = bundle.canonical_bytes()?;
    let packed = if caps.bundle_compression == 1 {
        if raw.len() as u64 > caps.max_bundle_decoded_bytes { bail!("bundle exceeds device decoded limit"); }
        bridge_render::bundle_codec::encode(&raw)
    } else { None };
    let length = packed.as_ref().map_or(raw.len(), Vec::len);
    if length as u64 > caps.bundle_limit() {
        bail!("bundle wire is {length} bytes, device limit is {}", caps.bundle_limit());
    }
    Ok(packed.map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetJob {
    pub frozen: FrozenPublish,
    pub state: PublishState,
    pub created_at: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaJob {
    pub job_id: String,
    pub request_id: String,
    pub device_mac: String,
    pub bridge_id: String,
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
    pub state: String,
    pub firmware_target: String,
    pub expected_version: String,
    #[serde(default)]
    pub before_version: Option<String>,
    #[serde(default)]
    pub before_boot_id: Option<String>,
    #[serde(default)]
    pub confirmation_started_at: Option<u64>,
    pub sha256: String,
    pub size: usize,
    pub blob: String,
    pub last_error: Option<String>,
    pub upload_ack: bool,
    #[serde(default)]
    pub confirmation: Option<String>,
    #[serde(default)]
    pub attempt_count: u32,
    #[serde(default)]
    pub last_attempt_at: Option<u64>,
    pub cancel_requested: bool,
    #[serde(default)]
    pub confirmation_checks: u8,
    #[serde(default)]
    pub confirmation_note: Option<String>,
    #[serde(default)]
    pub last_running_image: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaHistoryEntry {
    pub job: OtaJob,
    pub outcome: String,
    pub closed_at: u64,
    #[serde(default)]
    pub running_image: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareRelease {
    pub firmware_target: String,
    pub version: String,
    pub sha256: String,
    pub size: usize,
    pub blob: String,
    pub published_at: u64,
}

impl OtaJob {
    fn pending(&self) -> bool {
        !matches!(self.state.as_str(), "succeeded" | "failed" | "cancelled")
    }

    /// Recovering Wi-Fi is necessary to verify the post-reboot image. Only an
    /// actual upload blocks this prerequisite; confirmation never authorizes
    /// another firmware upload or unrelated configuration writes.
    pub fn blocks_network_repair(&self) -> bool { self.state == "transferring" }

    /// Exact image verification may remain pending on current ROMs. Once the
    /// upload was acknowledged and the expected version was authenticated after
    /// it, other explicit work may proceed without treating the OTA as verified.
    pub fn blocks_following_work(&self) -> bool {
        self.state == "transferring"
            || (self.state == "awaiting_confirmation"
                && !(self.upload_ack
                    && matches!(self.confirmation.as_deref(),
                        Some("version_observed" | "version_seen_unproven"))))
    }
}

impl AssetJob {
    fn summary(&self) -> Value {
        json!({"job_id": self.frozen.manifest.job_id, "state": self.state,
            "manifest_id": self.frozen.manifest_id, "objects": self.frozen.objects.len(),
            "bytes": self.frozen.objects.iter().map(|o| o.length).sum::<u64>(),
            "transferred_bytes": 0, "ack": null,
            "last_error": self.last_error, "transport": "waiting_for_device_protocol"})
    }
}

/// In-memory plus persisted platform state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedState {
    #[serde(default)]
    pub wifi: BTreeMap<String, WifiState>,
    #[serde(default)]
    pub sources: Vec<DataSource>,
    #[serde(default)]
    pub templates: Vec<Template>,
    #[serde(default)]
    pub devices: Vec<DeviceRecord>,
    #[serde(default)]
    pub family_profiles: Vec<FamilyProfile>,
    /// Bounded terminal job summaries (no browsable content versions).
    #[serde(default)]
    pub jobs: Vec<Value>,
    #[serde(default)]
    pub asset_jobs: BTreeMap<String, AssetJob>,
    #[serde(default)]
    pub bundle_jobs: BTreeMap<String, PublishJob>,
    #[serde(default)]
    pub ota_jobs: BTreeMap<String, OtaJob>,
    #[serde(default)]
    pub ota_history: Vec<OtaHistoryEntry>,
    #[serde(default)]
    pub firmware_releases: BTreeMap<String, FirmwareRelease>,
    #[serde(default)]
    pub running_images: BTreeMap<String, Value>,
    /// Per-device next data_seq high water mark (never blindly restarted at 0).
    #[serde(default)]
    pub next_seq: BTreeMap<String, u64>,
    #[serde(default)]
    pub contexts: BTreeMap<String, Value>,
    /// Last sent/accepted plan per device (idempotent retry after restart).
    #[serde(default)]
    pub plans: BTreeMap<String, Value>,
}

pub struct PlatformService {
    dir: PathBuf,
    inner: Mutex<Inner>,
}

struct Inner {
    wifi: BTreeMap<String, WifiState>,
    sources: BTreeMap<String, DataSource>,
    snapshots: BTreeMap<String, SourceSnapshot>,
    templates: BTreeMap<TemplateKey, Template>,
    family_profiles: BTreeMap<(String, String), FamilyProfile>,
    devices: BTreeMap<String, DeviceRecord>,
    coordinators: BTreeMap<String, Coordinator>,
    codex_envelope: Option<Value>,
    jobs: Vec<Value>,
    asset_jobs: BTreeMap<String, AssetJob>,
    ota_jobs: BTreeMap<String, OtaJob>,
    ota_history: Vec<OtaHistoryEntry>,
    firmware_releases: BTreeMap<String, FirmwareRelease>,
    running_images: BTreeMap<String, Value>,
    /// Bounded checkpoint throttle: the 10 s status poll must not write flash.
    status_persist_at: BTreeMap<String, u64>,
}

pub const MAX_JOB_SUMMARIES: usize = 20;

impl PlatformService {
    pub fn data_root(&self) -> &Path { self.dir.parent().expect("platform directory has a root") }

    /// Load state from `<root>/platform/` (created on demand).
    pub fn open(root: &Path) -> Result<Self> {
        let dir = root.join("platform");
        store::ensure_dir(&dir)?;
        let persisted: PersistedState =
            store::read_json(&dir.join("state.json"))?.unwrap_or_default();
        let PersistedState {
            mut wifi,
            sources: persisted_sources,
            templates: persisted_templates,
            devices: persisted_devices,
            family_profiles: persisted_family_profiles,
            jobs: mut persisted_jobs,
            asset_jobs: mut persisted_asset_jobs,
            bundle_jobs: persisted_bundle_jobs,
            ota_jobs: mut persisted_ota_jobs,
            ota_history: mut persisted_ota_history,
            firmware_releases,
            running_images,
            next_seq,
            contexts,
            plans,
        } = persisted;
        for state in wifi.values_mut() { state.recover(); }
        for job in persisted_ota_jobs.values_mut() {
            if job.state == "transferring" { job.state = "awaiting_confirmation".into(); }
        }
        let mut ota_recovered = false;
        let abandoned: Vec<String> = persisted_ota_jobs.iter()
            .filter(|(_, job)| job.state == "awaiting_confirmation"
                && job.cancel_requested && !job.upload_ack)
            .map(|(mac, _)| mac.clone()).collect();
        for mac in abandoned {
            let job = persisted_ota_jobs.remove(&mac).unwrap();
            persisted_ota_history.push(OtaHistoryEntry {
                running_image: job.last_running_image.clone(), job,
                outcome: "cancelled_unconfirmed".into(),
                closed_at: crate::device_clock::wall_secs(&mac),
            });
            ota_recovered = true;
        }
        if persisted_ota_history.len() > MAX_JOB_SUMMARIES {
            let excess = persisted_ota_history.len() - MAX_JOB_SUMMARIES;
            persisted_ota_history.drain(..excess);
        }
        for job in persisted_asset_jobs.values_mut() {
            if let Err(error) = job.frozen.verify() {
                job.state = PublishState::Failed;
                job.last_error = Some(format!("persisted frozen job corrupt: {error}"));
            }
        }
        let mut history_recovered = false;
        for record in &persisted_devices {
            let committed = record.observed.committed_job_id.as_deref().filter(|id| !id.is_empty());
            let has_context = record.observed.active_context_id.as_deref().is_some_and(|id| !id.is_empty());
            if has_context {
                if let Some(entry) = persisted_jobs.iter_mut().rev().find(|entry| {
                    committed.is_some_and(|id| entry["job_id"] == id) && entry["state"] != "succeeded"
                }) {
                    entry["state"] = json!("succeeded");
                    history_recovered = true;
                }
            }
        }
        let mut templates = BTreeMap::new();
        for mut t in persisted_templates {
            // Old Rust render-plan records migrate from their retained source.
            // Saving the migration never publishes a bundle.
            if crate::compile::validate(&t.compiled).is_err() {
                match crate::compile::compile(&t.source, &t.key.render_target) {
                    Ok(compiled) => t.compiled = compiled,
                    Err(error) => tracing::warn!(template = %t.key.template_id, %error,
                        "stored template is unsupported; retain for editing, reject publication"),
                }
            }
            templates.insert(t.key.clone(), t);
        }
        let family_profiles = persisted_family_profiles
            .into_iter()
            .map(|profile| ((profile.render_target.clone(), profile.id.clone()), profile))
            .collect();
        let mut sources: BTreeMap<String, DataSource> = persisted_sources
            .into_iter()
            .map(|s| (s.source_id.clone(), s))
            .collect();
        if sources.is_empty() {
            for ds in crate::datasource::default_sources() {
                sources.insert(ds.source_id.clone(), ds);
            }
        }
        let mut devices = BTreeMap::new();
        let mut coordinators = BTreeMap::new();
        for mut record in persisted_devices {
            if let Some(profile) = record.profile.as_mut() {
                if profile.render_target.is_none() && !record.capabilities.render_target.is_empty() {
                    profile.render_target = Some(record.capabilities.render_target.clone());
                }
            }
            let mac = record.identity.device_mac.clone();
            let mut c = Coordinator::new(&mac, record.capabilities.clone());
            if let Some(mut job) = persisted_bundle_jobs.get(&mac).cloned() {
                if job.device_mac == mac
                    && job.frozen_bundle.verify().is_ok()
                    && job.frozen_bundle.render_target == record.capabilities.render_target
                    && job.frozen_bundle.firmware_target == record.capabilities.firmware_target
                {
                    if job.state == PublishState::Sending {
                        job.state = PublishState::Unknown;
                    }
                    c.job = Some(job);
                }
            }
            if let Some(seq) = next_seq.get(&mac) {
                // Lost local record: the authenticated device read precedes any
                // send; keep the per-device counter monotonic, never 0.
                if let Some(context) = contexts.get(&mac) {
                    if let Some(ctx) = context_from_json(context, seq.saturating_add(1)) {
                        c.context = Some(ctx);
                    }
                }
            }
            if let Some(plan) = plans.get(&mac) {
                if let Ok(state) =
                    serde_json::from_value::<crate::coordinator::PlanState>(plan.clone())
                {
                    c.plan = state;
                }
            }
            coordinators.insert(mac.clone(), c);
            devices.insert(mac, record);
        }
        let service = Self {
            dir,
            inner: Mutex::new(Inner {
                wifi,
                sources,
                snapshots: BTreeMap::new(),
                templates,
                family_profiles,
                devices,
                coordinators,
                codex_envelope: None,
                jobs: persisted_jobs,
                asset_jobs: persisted_asset_jobs,
                ota_jobs: persisted_ota_jobs,
                ota_history: persisted_ota_history,
                firmware_releases,
                running_images,
                status_persist_at: BTreeMap::new(),
            }),
        };
        service.refresh_all_contracts()?;
        if history_recovered || ota_recovered {
            let inner = service.inner.lock().unwrap();
            Self::persist(&inner, &service.state_path())?;
        }
        Ok(service)
    }

    fn state_path(&self) -> PathBuf {
        self.dir.join("state.json")
    }

    fn font_library(&self) -> Result<FontLibrary> {
        FontLibrary::open(&self.dir.parent().context("platform data root missing")?.join("fonts"))
    }

    pub fn font_list(&self) -> Result<Value> {
        Ok(json!({"fonts": self.font_library()?.list()?, "source_conversion": "unavailable", "note": "Import a CSFN .bin; TTF/OTF conversion is not packaged in this Bridge"}))
    }

    pub fn font_import(&self, path: &Path) -> Result<Value> {
        if path.extension().and_then(|x| x.to_str()) != Some("bin") {
            bail!("only CSFN .bin import is available; TTF/OTF conversion is not packaged");
        }
        let descriptor = self.font_library()?.add(&std::fs::read(path)?)?;
        Ok(json!({"imported": descriptor, "published": false}))
    }

    fn profile_fonts(&self, inner: &Inner, profile: &Profile) -> Result<FontPlan> {
        let target = profile.render_target.as_deref().context("profile target unbound")?;
        let mut names = Vec::new();
        for id in &profile.template_ids {
            let template = inner.templates.get(&TemplateKey::new(id, target))
                .with_context(|| format!("template {id} has no {target} variant"))?;
            collect_font_names(&template.source, &mut names);
        }
        let plan = FontPlan::for_selected(&self.font_library()?, &names, &profile.font_ids)?;
        if plan.required.len() != profile.font_ids.len() {
            bail!("profile contains a font_id selection unused by its templates");
        }
        let format = &inner.devices[&profile.device_mac].capabilities.pixel_format;
        for font in &plan.required { font.check_target(format)?; }
        Ok(plan)
    }

    fn freeze_assets(&self, caps: &DeviceCapabilities, profile: &Profile,
        templates: &[Template], fonts: &FontPlan, job_id: &str) -> Result<FrozenPublish> {
        if !fonts.required.is_empty() && caps.compiler_abi < 2 {
            bail!("asset fonts require a device/compiler ABI with explicit font references; ABI {} cannot publish them", caps.compiler_abi);
        }
        let library = self.font_library()?;
        let mut bytes = BTreeMap::new();
        for font in &fonts.required {
            bytes.insert(font.name.clone(), library.read(&font.id)?
                .with_context(|| format!("selected font {} disappeared", font.id))?);
        }
        FrozenPublish::build(job_id, caps, profile, templates, &bytes)
    }

    fn persist(inner: &Inner, path: &Path) -> Result<()> {
        let mut jobs = inner.jobs.clone();
        store::prune(&mut jobs, MAX_JOB_SUMMARIES);
        // Keep frozen in-flight Bundles across Bridge restarts. The device's
        // authenticated status and idempotent BEGIN decide whether to resume.
        let state = PersistedState {
            wifi: inner.wifi.clone(),
            sources: inner.sources.values().cloned().collect(),
            templates: inner.templates.values().cloned().collect(),
            family_profiles: inner.family_profiles.values().cloned().collect(),
            devices: inner.devices.values().cloned().collect(),
            jobs,
            asset_jobs: inner.asset_jobs.clone(),
            ota_jobs: inner.ota_jobs.clone(),
            ota_history: inner.ota_history.clone(),
            firmware_releases: inner.firmware_releases.clone(),
            running_images: inner.running_images.clone(),
            bundle_jobs: inner.coordinators.iter().filter_map(|(mac, c)| {
                c.job.as_ref().filter(|j| matches!(j.state, PublishState::Waiting | PublishState::Sending | PublishState::Unknown))
                    .map(|j| (mac.clone(), j.clone()))
            }).collect(),
            next_seq: inner
                .coordinators
                .iter()
                .map(|(mac, c)| {
                    (
                        mac.clone(),
                        c.context.as_ref().map(|x| x.next_seq).unwrap_or(1),
                    )
                })
                .collect(),
            contexts: inner
                .coordinators
                .iter()
                .filter_map(|(mac, c)| {
                    c.context
                        .as_ref()
                        .map(|x| (mac.clone(), serde_json::to_value(x).unwrap_or(Value::Null)))
                })
                .collect(),
            plans: inner
                .coordinators
                .iter()
                .map(|(mac, c)| {
                    (
                        mac.clone(),
                        serde_json::to_value(&c.plan).unwrap_or(Value::Null),
                    )
                })
                .collect(),
        };
        store::write_json(path, &state)
    }

    // All UI/MCP Wi-Fi mutations share the existing atomic platform store.
    fn wifi_mutate<T>(&self, mac: &str, change: impl FnOnce(&mut WifiState) -> Result<T>) -> Result<T> {
        let mac = DeviceIdentity::normalized_mac(mac).context("invalid_device_mac")?;
        let mut inner = self.inner.lock().unwrap();
        if !inner.devices.keys().any(|key| DeviceIdentity::normalized_mac(key).as_deref() == Some(&mac)) { bail!("device_not_registered"); }
        let old = inner.wifi.get(&mac).cloned();
        let mut next = old.clone().unwrap_or_default();
        if let Some(high)=inner.devices.get(&mac).and_then(|d|d.last_authenticated.as_ref())
            .and_then(|s|s.body["wifi_control"]["transaction_high_watermark"].as_u64()) {next.next_job_seq=next.next_job_seq.max(high);}
        let result = change(&mut next)?;
        inner.wifi.insert(mac.clone(), next);
        if let Err(error) = Self::persist(&inner, &self.state_path()) {
            if let Some(old) = old { inner.wifi.insert(mac, old); } else { inner.wifi.remove(&mac); }
            return Err(error);
        }
        Ok(result)
    }
    pub fn wifi_state(&self, mac: &str) -> WifiState {
        DeviceIdentity::normalized_mac(mac).and_then(|mac| self.inner.lock().unwrap().wifi.get(&mac).cloned()).unwrap_or_default()
    }
    pub fn wifi_clear_connection(&self, mac:&str) -> Result<()> {self.wifi_mutate(mac,|s|{s.connection_facts=None;Ok(())})}
    pub fn wifi_set_connection(&self, mac: &str, connection: super::wifi::ConnectionFacts) -> Result<()> {
        self.wifi_mutate(mac, |s| {
            if connection.schema!=2 {bail!("unsupported_wifi_connection_schema");}
            let target=s.matching_target(&connection.host_ssid).context("network_not_configured")?;
            if target.network_id!=connection.network_id || connection.local_revision!=s.local_revision { bail!("local_revision_conflict"); }
            if connection.admission_error.as_ref().is_some_and(|e|e.len()>64 || !e.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'_')) { bail!("invalid_error_code"); }
            s.connection_facts=Some(connection);Ok(())
        })
    }
    #[cfg(test)]
    pub fn wifi_note_connection(&self, mac:&str, c:super::wifi::Connection)->Result<()> {
        let mut facts=super::wifi::ConnectionFacts::from_legacy(c.clone());
        facts.observation=Some(super::wifi::NetworkObservation{boot_id:"null".into(),at:c.observed_at,
            state:c.state,error:c.error,reason:c.reason,ssid:c.actual_ssid,ip:c.ip});
        if c.http_ready || c.bridge_error.is_some() {
            facts.contact=Some(super::wifi::HttpContact{boot_id:"null".into(),at:c.observed_at,error:c.bridge_error});
        }
        self.wifi_set_connection(mac,facts)
    }
    pub fn wifi_save(&self, mac: &str, expected: u64, targets: Vec<WifiNetwork>, now: u64) -> Result<WifiState> {
        self.wifi_mutate(mac, |s| { s.save(expected, targets, now)?; Ok(s.clone()) })
    }
    pub fn wifi_queue(&self, mac: &str, bridge: &str, kind: &str, selected: Vec<String>, now: u64) -> Result<WifiJob> {
        self.wifi_queue_fields(mac, bridge, kind, selected, BTreeMap::new(), now)
    }
    pub fn wifi_queue_fields(&self, mac: &str, bridge: &str, kind: &str, selected: Vec<String>,
        fields: BTreeMap<String, Vec<String>>, now: u64) -> Result<WifiJob> {
        self.wifi_queue_fields_checked(mac, bridge, kind, selected, fields, None, now)
    }
    pub fn wifi_queue_fields_checked(&self, mac: &str, bridge: &str, kind: &str, selected: Vec<String>,
        fields: BTreeMap<String, Vec<String>>, expected: Option<u64>, now: u64) -> Result<WifiJob> {
        let mac = DeviceIdentity::normalized_mac(mac).context("invalid_device_mac")?;
        self.wifi_mutate(&mac, |s| {
            if expected.is_some_and(|v| v != s.local_revision) { bail!("local_revision_conflict"); }
            s.next_job_seq = s.next_job_seq.checked_add(1).context("wifi_id_exhausted")?;
            let id = crate::coordinator::new_context_id(&mac, now, s.next_job_seq);
            s.queue(&mac, bridge, kind, selected, now, id, fields)
        })
    }
    pub fn wifi_queue_control(&self,mac:&str,bridge:&str,kind:&str,body:Value,now:u64)->Result<WifiJob> {
        anyhow::ensure!(!bridge.is_empty() && bridge.len()<=64,"invalid_wifi_intent");
        anyhow::ensure!(matches!(kind,"trial"|"activate"),"invalid_wifi_intent");
        anyhow::ensure!(body.is_object() && serde_json::to_vec(&body)?.len()<=4096,"invalid_wifi_intent");
        anyhow::ensure!(body.as_object().unwrap().keys().all(|k|matches!(k.as_str(),"network_id"|"version"|"host_ssid"|"ssid"|"password"|"replace")),"invalid_wifi_intent");
        let id=body["network_id"].as_str().context("network_id_required")?.to_owned();
        anyhow::ensure!(!id.is_empty() && id.len()<=32,"network_id_required");
        let mac=DeviceIdentity::normalized_mac(mac).context("invalid_device_mac")?;
        self.wifi_mutate(&mac,|s| {
            s.expire(now);
            if let Some(job)=s.jobs.iter().find(|j|!j.terminal() && matches!(j.kind.as_str(),"trial"|"activate")) {
                anyhow::ensure!(job.bridge_id==bridge && job.kind==kind && job.command.as_ref()==Some(&body),"wifi_control_busy");return Ok(job.clone());
            }
            s.next_job_seq=s.next_job_seq.checked_add(1).context("wifi_id_exhausted")?;
            let job_id=crate::coordinator::new_context_id(&mac,now,s.next_job_seq);
            let mut job=s.queue(&mac,bridge,kind,vec![id],now,job_id,BTreeMap::new())?;
            job.command=Some(body);*s.jobs.iter_mut().find(|j|j.job_id==job.job_id).unwrap()=job.clone();Ok(job)
        })
    }
    fn wifi_complete_control(&self,mac:&str,job_id:&str,revision:Option<u64>,now:u64)->Result<WifiJob> {
        self.wifi_mutate(mac,|s| {
            let j=s.jobs.iter_mut().find(|j|j.job_id==job_id).context("wifi_job_not_found")?;
            j.state="completed".into();j.updated_at=now;j.last_stage="confirmed_operation".into();
            j.confirmation=Some("operation_result".into());j.error_code=None;j.receipt_ack_pending=j.request_id.starts_with("w3_");
            j.confirmed_revision=revision;Ok(j.clone())
        })
    }
    fn wifi_reject_receipt(&self, mac: &str, job_id: &str, code: &str, now: u64) -> Result<WifiJob> {
        self.wifi_mutate(mac, |s| {
            let j = s.jobs.iter_mut().find(|j| j.job_id == job_id).context("wifi_job_not_found")?;
            j.state = "failed".into(); j.updated_at = now; j.may_have_committed = false;
            j.confirmation = Some("durable_rejection".into()); j.last_stage = "confirmed_operation".into();
            j.error_code = Some(code.into()); j.receipt_ack_pending = j.request_id.starts_with("w3_");
            Ok(j.clone())
        })
    }
    pub fn wifi_queue_manual_sync(&self, mac: &str, bridge: &str, selected: Vec<String>, expected: u64,
        snapshot_id: &str, host: &str, now: u64) -> Result<WifiJob> {
        let mac = DeviceIdentity::normalized_mac(mac).context("invalid_device_mac")?;
        self.wifi_mutate(&mac, |s| {
            if expected != s.local_revision { bail!("local_revision_conflict"); }
            s.validate_manual_sync(&selected, host, snapshot_id)?;
            s.next_job_seq = s.next_job_seq.checked_add(1).context("wifi_id_exhausted")?;
            let id = crate::coordinator::new_context_id(&mac, now, s.next_job_seq);
            let mut job = s.queue(&mac, bridge, "sync", selected, now, id, BTreeMap::new())?;
            job.host_ssid = Some(host.into());
            *s.jobs.iter_mut().find(|j| j.job_id == job.job_id).unwrap() = job.clone();
            Ok(job)
        })
    }
    /// An unsent whole-row intent can already be satisfied by another confirmed
    /// write. Authenticated scoped digests prove the post-state, not a receipt.
    pub fn wifi_confirm_current(&self,mac:&str,job_id:&str,revision:u64,versions:&BTreeMap<String,u64>,now:u64)->Result<WifiJob> {
        self.wifi_mutate(mac,|s| {
            let job=s.jobs.iter().find(|j|j.job_id==job_id).context("wifi_job_not_found")?.clone();
            anyhow::ensure!(!job.sent && !job.terminal() && now<job.expires_at && job.fields.is_empty() && matches!(job.kind.as_str(),"sync"|"recovery_save"),"wifi_job_no_longer_active");
            anyhow::ensure!(versions.len()==job.selected.len() && job.selected.iter().all(|id|versions.contains_key(id)),"target_unconfirmed");
            for frozen in &job.frozen {
                s.baseline.insert(frozen.network_id.clone(),frozen.metadata());
                if s.local_revision==job.local_revision {
                    if let Some(target)=s.targets.iter_mut().find(|n|*n==frozen){target.version=versions[&frozen.network_id];target.credential="keep".into();}
                }
            }
            // Do not promote a stale full snapshot using a subset of digests.
            if let Some(snapshot)=s.snapshot.as_mut(){snapshot.complete=false;}
            let j=s.jobs.iter_mut().find(|j|j.job_id==job_id).unwrap();j.state="completed".into();j.updated_at=now;
            j.last_stage="already_current".into();j.confirmation=Some("verified_post_state".into());j.confirmed_revision=Some(revision);j.error_code=None;
            Ok(j.clone())
        })
    }
    fn wifi_confirm_receipt(&self, mac: &str, job: &str, revision: u64, bond: Option<&str>, now: u64) -> Result<WifiJob> {
        self.wifi_mutate(mac, |s| s.confirm_receipt(job, revision, bond, now))
    }
    pub fn wifi_receipt_acked(&self,mac:&str,job:&str,principal:&str,namespace:&str)->Result<()> {
        self.wifi_mutate(mac,|s| {
            let j=s.jobs.iter_mut().find(|j|j.job_id==job).context("wifi_job_not_found")?;
            j.check_transaction_context(principal,namespace)?;
            anyhow::ensure!(j.terminal() && matches!(j.confirmation.as_deref(),Some("durable_receipt"|"operation_result"|"durable_rejection")),"receipt_not_confirmed");
            j.receipt_ack_pending=false;Ok(())
        })
    }

    /// Explicitly renew only the read/ACK budget, never the original execution.
    pub fn wifi_retry_settlement(&self, mac: &str, job_id: &str) -> Result<WifiJob> {
        self.wifi_mutate(mac, |s| {
            let j=s.jobs.iter_mut().find(|j|j.job_id==job_id).context("wifi_job_not_found")?;
            anyhow::ensure!(j.terminal() && (j.confirmation_pending() || j.receipt_ack_pending), "settlement_not_required");
            anyhow::ensure!(j.request_id.starts_with("w3_"),"settlement_identity_unsupported");
            j.check_transaction_context(&j.bridge_id,j.transaction_namespace.as_deref().unwrap_or(""))?;
            if let Some(s)=j.settlement.as_mut(){s.attempts=0;s.next_attempt_at=0;s.error=None;}
            Ok(j.clone())
        })
    }

    /// Settle a stopped job using the frozen identity. No apply/activate, no
    /// wifi_begin, no cache reconstruction from an obsolete historical receipt.
    pub async fn wifi_settle_async(&self, mac: &str, job_id: &str, principal: &str, namespace: &str,
        now: u64, transport: &mut impl crate::platform::wifi::Transport) -> Result<Option<WifiJob>> {
        let job=self.wifi_mutate(mac,|s| {
            let j=s.jobs.iter_mut().find(|j|j.job_id==job_id).context("wifi_job_not_found")?;
            j.check_transaction_context(principal,namespace)?;
            if !j.settlement_due(now) { return Ok(None); }
            let attempts=j.settlement.as_ref().map_or(0,|s|s.attempts)+1;
            let result_error=j.settlement.as_ref().and_then(|s|s.result_error.clone());
            j.settlement=Some(crate::platform::wifi::Settlement { attempts,
                next_attempt_at:now.saturating_add(60), error:Some("settlement_incomplete".into()),result_error });
            Ok(Some(j.clone()))
        })?;
        let Some(job)=job else{return Ok(None);};
        let result:Result<()> = async {
            if !job.receipt_ack_pending {
                let reply=transport.exchange("result",json!({"query_id":job.request_id})).await?;
                let control=matches!(job.kind.as_str(),"trial"|"activate");
                let (confirmation,revision,rejection)=match OperationReply::decode(ReplySource::ResultQuery,&reply)? {
                    OperationReply::Committed {revision,bond} => {
                        anyhow::ensure!(bond.is_none_or(|b|b==namespace),"receipt_namespace_mismatch");
                        if !control {anyhow::ensure!(job.expected_revision.is_none_or(|r|r.checked_add(1)==Some(revision)),"receipt_revision_mismatch");}
                        (if control {"operation_result"}else{"durable_receipt"},Some(revision),None)
                    },
                    OperationReply::Completed {revision} if control => ("operation_result",revision,None),
                    OperationReply::Rejected {durable:true,code} if !matches!(code,"pending"|"result_unknown"|"storage_unknown") => {
                        anyhow::ensure!(!code.is_empty() && code.len()<=64 && code.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'),"receipt_result_invalid");
                        ("durable_rejection",None,Some(code.to_owned()))
                    },
                    _ => bail!("settlement_result_unknown"),
                };
                // Persist the proof before acknowledging the device's receipt.
                self.wifi_mutate(mac,|s| {
                    let j=s.jobs.iter_mut().find(|j|j.job_id==job_id).context("wifi_job_not_found")?;
                    j.check_transaction_context(principal,namespace)?;
                    j.confirmation=Some(confirmation.into());j.confirmed_revision=revision;
                    if rejection.is_some() {j.may_have_committed=false;}
                    if let Some(s)=j.settlement.as_mut(){s.result_error=rejection;}
                    j.receipt_ack_pending=true;j.last_stage="historical_result_confirmed".into();
                    Ok(())
                })?;
            }
            let reply=transport.exchange("result_ack",json!({"query_id":job.request_id})).await?;
            anyhow::ensure!(reply["ok"]==true && reply["outcome"]=="retired","settlement_ack_unknown");
            self.wifi_receipt_acked(mac,job_id,principal,namespace)
        }.await;
        let error=result.err().map(|e| {
            // Only fixed local codes or sanitized transport categories persist.
            let code=e.to_string();
            if matches!(code.as_str(),"settlement_result_unknown"|"settlement_ack_unknown"|"receipt_namespace_mismatch"|"receipt_revision_mismatch"|"receipt_result_invalid") {
                code
            } else {crate::rendezvous::wifi_error(&e).1.to_owned()}
        });
        self.wifi_mutate(mac,|s| {
            let j=s.jobs.iter_mut().find(|j|j.job_id==job_id).context("wifi_job_not_found")?;
            if let Some(settlement)=j.settlement.as_mut(){settlement.error=error;}
            Ok(Some(j.clone()))
        })
    }
    pub fn wifi_begin(&self, mac: &str, job: &str, now: u64) -> Result<Option<WifiJob>> {
        self.wifi_mutate(mac, |s| s.begin(job, now))
    }
    pub fn wifi_finish(&self, mac: &str, job: &str, state: &str, error: Option<&str>, now: u64) -> Result<WifiJob> {
        self.wifi_mutate(mac, |s| s.finish(job, state, error, now))
    }
    fn wifi_accepted_pending(&self, mac: &str, job: &str, now: u64) -> Result<WifiJob> {
        self.wifi_mutate(mac, |s| {
            s.finish(job, "awaiting_confirmation", None, now)?;
            let current = s.jobs.iter_mut().find(|j| j.job_id == job).context("wifi_job_not_found")?;
            if !current.terminal() { current.last_stage = "accepted".into(); }
            Ok(current.clone())
        })
    }
    pub fn wifi_complete_read(&self, mac: &str, job: &str, snapshot: WifiSnapshot, now: u64) -> Result<WifiJob> {
        self.wifi_mutate(mac, |s| s.complete_read(job, snapshot, now))
    }
    pub fn wifi_import(&self, mac: &str, selected: &[String], expected: u64, now: u64) -> Result<WifiState> {
        self.wifi_mutate(mac, |s| { s.import(selected, expected, now)?; Ok(s.clone()) })
    }

    pub fn wifi_import_fields(&self, mac: &str, selected: &[String], fields: &BTreeMap<String, Vec<String>>, expected: u64, now: u64) -> Result<WifiState> {
        self.wifi_import_fields_checked(mac, selected, fields, expected, None, now)
    }
    pub fn wifi_import_fields_checked(&self, mac: &str, selected: &[String], fields: &BTreeMap<String, Vec<String>>, expected: u64, snapshot_id: Option<&str>, now: u64) -> Result<WifiState> {
        self.wifi_mutate(mac, |s| {
            if snapshot_id.is_some_and(|id| s.snapshot.as_ref().map(|v| v.snapshot_id.as_str()) != Some(id)) {
                bail!("snapshot_conflict");
            }
            s.import_fields(selected, fields, expected, now)?; Ok(s.clone())
        })
    }
    pub fn wifi_expire(&self, mac: &str, now: u64) -> Result<()> {
        if self.wifi_state(mac).jobs.iter().any(|j| !j.terminal() && now >= j.expires_at) {
            self.wifi_mutate(mac, |s| { s.expire(now); Ok(()) })?;
        }
        Ok(())
    }
    fn wifi_definite_rejection(&self, mac: &str, job: &str) -> Result<()> {
        self.wifi_mutate(mac, |s| {
            let j=s.jobs.iter_mut().find(|j|j.job_id==job).context("wifi_job_not_found")?;
            j.sent=false;j.may_have_committed=false;Ok(())
        })
    }
    pub fn wifi_mark_sent(&self, mac: &str, job: &str, now: u64) -> Result<()> {
        self.wifi_mutate(mac, |s| s.mark_sent(job, now))
    }
    /// Freeze only from an authenticated carrier before the first dispatch.
    /// Missing identity on a previously sent job must never be guessed.
    pub fn wifi_bind_transaction(&self, mac: &str, job: &str, principal: &str, namespace: &str) -> Result<()> {
        self.wifi_mutate(mac, |s| {
            let j = s.jobs.iter_mut().find(|j| j.job_id == job).context("wifi_job_not_found")?;
            if j.transaction_namespace.is_none() && !j.sent && !j.may_have_committed && !j.receipt_ack_pending {
                j.transaction_namespace = Some(namespace.to_owned());
            }
            j.check_transaction_context(principal, namespace)
        })
    }
    /// One bounded opportunity, also exercised through Fake's real client path.
    /// Transport returns a full snapshot for "snapshot"; other calls are ACKs.
    pub async fn wifi_attempt_async(&self, mac: &str, job_id: &str, principal: &str, namespace: &str,
        write_allowed: bool, credentials: &BTreeMap<String, String>, now: u64,
        transport: &mut impl crate::platform::wifi::Transport) -> Result<Option<WifiJob>> {
        let Some(job) = self.wifi_begin(mac, job_id, now)? else { return Ok(None); };
        // Dropping carrier I/O must also release the persisted in-progress state.
        // A sent write remains query-only; cancellation is never proof of failure.
        struct AttemptGuard<'a> { service: &'a PlatformService, mac: &'a str, id: &'a str, now: u64, armed: bool }
        impl Drop for AttemptGuard<'_> {
            fn drop(&mut self) {
                if !self.armed { return; }
                let sent=self.service.wifi_state(self.mac).jobs.iter().any(|j| j.job_id==self.id && j.sent);
                if self.service.wifi_finish(self.mac,self.id,
                    if sent {"awaiting_confirmation"} else {"waiting_rendezvous"},Some("attempt_cancelled"),self.now).is_err() {
                    tracing::error!(event="wifi_cancel_persist_failed", "Cannot persist cancelled attempt; restart recovery required");
                }
            }
        }
        let mut guard=AttemptGuard {service:self,mac,id:job_id,now,armed:true};
        if !matches!(job.kind.as_str(),"read"|"read_import") {
            if let Err(error)=self.wifi_bind_transaction(mac,job_id,principal,namespace) {
                guard.armed=false;
                return self.wifi_finish(mac,job_id,if job.sent {"awaiting_confirmation"} else {"waiting_rendezvous"},Some(&error.to_string()),now).map(Some);
            }
        }
        let run: Result<WifiJob> = async {
            let control = matches!(job.kind.as_str(), "trial" | "activate");
            if job.sent {
                let reply = transport.exchange("result", json!({"query_id":job.request_id})).await?;
                return match OperationReply::decode(ReplySource::ResultQuery, &reply)? {
                    OperationReply::AcceptedPending => self.wifi_accepted_pending(mac, job_id, now),
                    OperationReply::Committed { revision, bond } if !control =>
                        self.wifi_confirm_receipt(mac, job_id, revision, bond, now),
                    OperationReply::Committed { revision, .. } =>
                        self.wifi_complete_control(mac, job_id, Some(revision), now),
                    OperationReply::Completed { revision } if control =>
                        self.wifi_complete_control(mac, job_id, revision, now),
                    OperationReply::Rejected { code, durable: true } =>
                        self.wifi_reject_receipt(mac, job_id, code, now),
                    OperationReply::Unknown { code } => self.wifi_finish(mac, job_id, "awaiting_confirmation",
                        Some(code.unwrap_or(if control { "result_unknown" } else { "receipt_unknown" })), now),
                    _ => self.wifi_finish(mac, job_id, "awaiting_confirmation", Some("receipt_unknown"), now),
                };
            }
            if control {
                if !write_allowed { return self.wifi_finish(mac,job_id,"waiting_rendezvous",Some("window_required"),now); }
                let mut body=job.command.clone().context("wifi_control_payload_required")?;body["request_id"]=json!(job.request_id);
                self.wifi_mark_sent(mac, job_id, now)?;
                let reply = transport.exchange(&job.kind, body).await?;
                return match OperationReply::decode(ReplySource::OriginalWrite, &reply)? {
                    OperationReply::AcceptedPending => self.wifi_accepted_pending(mac, job_id, now),
                    OperationReply::Committed { revision, .. } => self.wifi_complete_control(mac,job_id,Some(revision),now),
                    OperationReply::Completed { revision } => self.wifi_complete_control(mac,job_id,revision,now),
                    OperationReply::Rejected { code, durable: true } => self.wifi_reject_receipt(mac,job_id,code,now),
                    OperationReply::Rejected { code, durable: false } => {
                        self.wifi_definite_rejection(mac,job_id)?;
                        self.wifi_finish(mac,job_id,if matches!(code,"window_required"|"claim_required"|"occupied"|"session_expired"|"busy")
                            {"waiting_rendezvous"}else{"failed"},Some(code),now)
                    }
                    OperationReply::Unknown { code } => self.wifi_finish(mac,job_id,"awaiting_confirmation",
                        Some(code.unwrap_or("result_unknown")),now),
                };
            }
            if !matches!(job.kind.as_str(), "sync" | "recovery_save") {
                let snapshot: WifiSnapshot = serde_json::from_value(transport.exchange("snapshot", json!({})).await?)?;
                return self.wifi_complete_read(mac, job_id, snapshot, now);
            }
            if !write_allowed { return self.wifi_finish(mac, job_id, "waiting_rendezvous", Some("window_required"), now); }
            // Automatic whole-row provisioning needs only the current revision.
            // User edits still require their frozen full snapshot and conflict checks.
            let (revision, current) = if job.kind == "recovery_save" {
                let active = transport.exchange("active", json!({})).await?;
                anyhow::ensure!(active["ok"] == true, "revision_unavailable");
                (active["revision"].as_u64().context("revision_unavailable")?, BTreeMap::new())
            } else {
                let snapshot = self.wifi_state(mac).snapshot.context("snapshot_required")?;
                snapshot.validate(&job.device_mac)?;
                if job.expected_revision != Some(snapshot.revision) { return self.wifi_finish(mac, job_id, "conflict", Some("snapshot_conflict"), now); }
                if job.selected.iter().any(|id| snapshot.networks.iter().filter(|n| n.network.network_id == *id).count() > 1) {
                    return self.wifi_finish(mac, job_id, "conflict", Some("ambiguous_network_id"), now);
                }
                let current: BTreeMap<_,_> = snapshot.networks.iter().filter(|n| n.writable)
                    .map(|n| (n.network.network_id.clone(), n.network.metadata())).collect();
                if job.selected.iter().any(|id| job.fields.get(id).map_or(current.get(id) != job.before.get(id), |fields|
                    fields.iter().any(|f| f != "credential" && current.get(id).map(|v|&v[f]) != job.before.get(id).map(|v|&v[f])))) {
                    return self.wifi_finish(mac, job_id, "conflict", Some("target_revision_conflict"), now);
                }
                (snapshot.revision, current)
            };
            let mut changes = serde_json::to_value(&job.frozen)?;
            for n in changes.as_array_mut().unwrap() {
                let network_id = n["network_id"].as_str().unwrap().to_owned();
                if let Some(fields) = job.fields.get(&network_id) {
                    if let Some(actual) = current.get(&network_id) {
                        for f in ["host_ssid","ssid","label","deleted"] {
                            if !fields.iter().any(|selected| selected == f) { n[f] = actual[f].clone(); }
                        }
                        if !fields.iter().any(|f| f == "credential") { n["credential"] = json!("keep"); }
                    } else {
                        if ["host_ssid","ssid","label","deleted"].iter().any(|f| !fields.iter().any(|s|s==f)) { return self.wifi_finish(mac, job_id, "conflict", Some("new_network_requires_all_fields"), now); }
                    }
                }
                if n["deleted"] != true {
                    let id=n["network_id"].as_str().unwrap();
                    let password=credentials.get(id).cloned().or_else(||n["password"].as_str().map(str::to_owned));
                    let include=job.fields.get(id).is_none_or(|fields|fields.iter().any(|f|f=="credential"));
                    if include {
                        if let Some(password)=password {
                            n["credential"]=json!(if password.is_empty(){"open"}else{"replace"});n["password"]=json!(password);
                        } else if n["credential"]=="replace" {
                            return self.wifi_finish(mac,job_id,"waiting_input",Some("credentials_required"),now);
                        }
                    } else {n.as_object_mut().unwrap().remove("password");}
                }
            }
            if job.kind == "recovery_save" {
                self.wifi_mutate(mac, |s| {
                    s.jobs.iter_mut().find(|j| j.job_id==job_id).context("wifi_job_not_found")?.expected_revision=Some(revision);Ok(())
                })?;
            }
            self.wifi_mutate(mac, |s| s.mark_sent(job_id, now))?;
            let reply = transport.exchange("apply", json!({"request_id":job.request_id,"expected_revision":revision,
                "changes":changes})).await?;
            match OperationReply::decode(ReplySource::OriginalWrite, &reply)? {
                OperationReply::AcceptedPending => self.wifi_accepted_pending(mac, job_id, now),
                OperationReply::Committed { revision, bond } => self.wifi_confirm_receipt(mac,job_id,revision,bond,now),
                OperationReply::Rejected { code, durable: true } => self.wifi_reject_receipt(mac,job_id,code,now),
                OperationReply::Rejected { code, durable: false } => {
                    self.wifi_definite_rejection(mac,job_id)?;
                    let state = match code {
                        "physical_required" | "window_required" => "waiting_rendezvous",
                        "revision_conflict" => "conflict", "credentials_required" => "waiting_input", _ => "failed",
                    };
                    self.wifi_finish(mac,job_id,state,Some(code),now)
                }
                OperationReply::Unknown { code } => self.wifi_finish(mac,job_id,"awaiting_confirmation",Some(code.unwrap_or("receipt_unknown")),now),
                OperationReply::Completed { .. } => self.wifi_finish(mac,job_id,"awaiting_confirmation",Some("receipt_unknown"),now),
            }
        }.await;
        let result = match run {
            Ok(job) => Ok(Some(job)),
            Err(error) => {
                // A definite auth rejection proves this write was not applied.
                // Failure to query an older unknown write proves nothing about
                // that original transaction: keep its ID and query-only fence.
                let auth_rejected = error.chain().any(|cause|
                    cause.downcast_ref::<crate::device_client::HttpStatus>()
                        .is_some_and(|status| matches!(status.0, 401 | 403)));
                if !job.sent && (auth_rejected || crate::rendezvous::not_dispatched(&error)) {
                    self.wifi_definite_rejection(mac,job_id)?;
                }
                let (state, code) = crate::rendezvous::wifi_error(&error);
                tracing::warn!(event="wifi_attempt_failed",device_mac=%mac,job_id,
                    error_category=code,previously_sent=job.sent,
                    stage=crate::rendezvous::command_failure(&error).map(|f|f.stage).unwrap_or("transport"),
                    reason=crate::rendezvous::command_failure(&error).map(|f|f.reason).unwrap_or(code),
                    "Wi-Fi task attempt failed; original transaction retained");
                if let Some(failure) = crate::rendezvous::command_failure(&error) {
                    self.wifi_mutate(mac, |s| {
                        s.jobs.iter_mut().find(|j|j.job_id==job_id).context("wifi_job_not_found")?.last_stage=failure.stage.into();
                        Ok(())
                    })?;
                }
                let current = self.wifi_state(mac).jobs.into_iter().find(|j| j.job_id == job_id).context("wifi_job_not_found")?;
                self.wifi_finish(mac, job_id, if current.sent { "awaiting_confirmation" } else { state },
                    Some(code), now).map(Some)
            }
        };
        guard.armed=false;
        result
    }

    fn refresh_bundle_history(inner: &mut Inner, mac: &str) {
        let Some(summary) = inner.coordinators.get(mac).and_then(|c| c.job_snapshot()) else {
            return;
        };
        let job_id = summary["job_id"].as_str();
        if let Some(entry) = inner.jobs.iter_mut().rev().find(|entry| entry["job_id"].as_str() == job_id) {
            *entry = summary;
        }
    }

    pub fn templates(&self) -> Vec<Value> {
        let inner = self.inner.lock().unwrap();
        inner
            .templates
            .values()
            .map(|t| {
                json!({
                    "template_id": t.key.template_id,
                    "render_target": t.key.render_target,
                    "version": t.compiled.version,
                    "min_fw": t.compiled.min_fw,
                    "source_crc": t.source_crc,
                    "compiled_crc": crate::compile::compiled_crc(&t.compiled),
                    "bytes": crate::template::canonical_bytes(&t.source).len(),
                    "saved_at": t.saved_at,
                    "requirements": t.compiled.requirements.len(),
                    "render_ops": t.compiled.op_count,
                    "used_by": self.profiles_referencing(&inner, &t.key.template_id),
                })
            })
            .collect()
    }

    fn profiles_referencing(&self, inner: &Inner, template_id: &str) -> Vec<String> {
        inner
            .devices
            .iter()
            .filter(|(_, d)| {
                d.profile
                    .as_ref()
                    .map(|p| p.template_ids.iter().any(|t| t == template_id))
                    .unwrap_or(false)
            })
            .map(|(mac, _)| mac.clone())
            .collect()
    }

    pub fn template_get(&self, template_id: &str, render_target: Option<&str>) -> Option<Value> {
        let inner = self.inner.lock().unwrap();
        let key = match render_target {
            Some(t) => TemplateKey::new(template_id, t),
            None => inner
                .templates
                .keys()
                .find(|k| k.template_id == template_id)
                .cloned()?,
        };
        inner.templates.get(&key).map(|t| {
            json!({
                "template_id": t.key.template_id,
                "render_target": t.key.render_target,
                "source": t.source,
                "source_crc": t.source_crc,
                "compiled": t.compiled,
            })
        })
    }

    /// Save = validate + compile + replace latest content for
    /// `template_id + render_target`. Never touches a device.
    pub fn template_save(
        &self,
        template_id: &str,
        render_target: &str,
        source: &Value,
        now: u64,
    ) -> Result<Template> {
        validate_template_id(template_id)?;
        let declared = source
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if !declared.is_empty() && declared != template_id {
            bail!("template id {declared} does not match save id {template_id}");
        }
        if source.get("render_target").is_some() {
            let declared_target = source
                .get("render_target")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if declared_target != render_target {
                bail!("template render_target {declared_target} does not match {render_target}");
            }
        }
        let mut normalized = source.clone();
        if let Some(obj) = normalized.as_object_mut() {
            obj.insert("id".into(), json!(template_id));
        }
        let canonical = crate::template::canonical_bytes(&normalized);
        if canonical.len() > crate::platform::model::MAX_TEMPLATE_BYTES {
            bail!(
                "template is {} bytes; limit {}",
                canonical.len(),
                crate::platform::model::MAX_TEMPLATE_BYTES
            );
        }
        let compiled = crate::compile::compile(&normalized, render_target)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let template = Template {
            key: TemplateKey::new(template_id, render_target),
            source: normalized,
            source_crc: crate::template::template_hash(&canonical),
            compiled,
            saved_at: now,
        };
        let mut inner = self.inner.lock().unwrap();
        inner
            .templates
            .insert(template.key.clone(), template.clone());
        Self::persist(&inner, &self.state_path())?;
        Ok(template)
    }

    pub fn template_delete(&self, template_id: &str, render_target: &str) -> Result<bool> {
        let mut inner = self.inner.lock().unwrap();
        let key = TemplateKey::new(template_id, render_target);
        let removed = inner.templates.remove(&key).is_some();
        if removed {
            Self::persist(&inner, &self.state_path())?;
        }
        Ok(removed)
    }

    // ---- Profiles --------------------------------------------------------

    pub fn family_profiles(&self, render_target: Option<&str>) -> Vec<FamilyProfile> {
        self.inner
            .lock()
            .unwrap()
            .family_profiles
            .values()
            .filter(|profile| render_target.map_or(true, |target| profile.render_target == target))
            .cloned()
            .collect()
    }

    pub fn family_profile_get(&self, render_target: &str, id: &str) -> Option<FamilyProfile> {
        self.inner
            .lock()
            .unwrap()
            .family_profiles
            .get(&(render_target.into(), id.into()))
            .cloned()
    }

    fn validate_family_profile(&self, inner: &Inner, profile: &FamilyProfile) -> Result<()> {
        profile.validate()?;
        let mut font_names = Vec::new();
        for id in &profile.template_ids {
            let template = inner
                .templates
                .get(&TemplateKey::new(id, &profile.render_target))
                .with_context(|| {
                    format!("template {id} has no {} variant", profile.render_target)
                })?;
            collect_font_names(&template.source, &mut font_names);
        }
        let plan = FontPlan::for_selected(&self.font_library()?, &font_names, &profile.font_ids)?;
        if plan.required.len() != profile.font_ids.len() {
            bail!("family profile contains a font_id selection unused by its templates");
        }
        Ok(())
    }

    pub fn family_profile_save(
        &self,
        mut profile: FamilyProfile,
        now: u64,
    ) -> Result<FamilyProfile> {
        profile.initial_active_id = profile.enabled_ids().into_iter().next();
        profile.updated_at = now;
        let mut inner = self.inner.lock().unwrap();
        self.validate_family_profile(&inner, &profile)?;
        inner.family_profiles.insert(
            (profile.render_target.clone(), profile.id.clone()),
            profile.clone(),
        );
        Self::persist(&inner, &self.state_path())?;
        Ok(profile)
    }

    pub fn family_profile_delete(&self, render_target: &str, id: &str) -> Result<bool> {
        let mut inner = self.inner.lock().unwrap();
        let removed = inner
            .family_profiles
            .remove(&(render_target.into(), id.into()))
            .is_some();
        if removed {
            Self::persist(&inner, &self.state_path())?;
        }
        Ok(removed)
    }

    pub fn family_profile_copy_from_device(
        &self,
        mac: &str,
        id: &str,
        name: &str,
        now: u64,
    ) -> Result<FamilyProfile> {
        let mac = mac.to_uppercase();
        let mut inner = self.inner.lock().unwrap();
        let record = inner.devices.get(&mac).context("unknown device")?;
        record.capabilities.validate()?;
        let render_target = record.capabilities.render_target.clone();
        let source = record
            .profile
            .as_ref()
            .context("device has no Profile to copy")?;
        if source
            .render_target
            .as_deref()
            .is_some_and(|target| target != render_target)
        {
            bail!(
                "device Profile render_target does not match verified device capabilities"
            );
        }
        let key = (render_target.clone(), id.to_owned());
        if inner.family_profiles.contains_key(&key) {
            bail!("family Profile id {id} already exists for {render_target}");
        }
        let mut profile = FamilyProfile {
            render_target,
            id: id.into(),
            name: name.into(),
            template_ids: source.template_ids.clone(),
            enabled_template_ids: None,
            initial_active_id: None,
            font_ids: source.font_ids.clone(),
            bindings: source.bindings.clone(),
            sync_enabled: source.sync_enabled,
            full_sync_s: source.full_sync_s,
            updated_at: now,
        };
        profile.initial_active_id = profile.enabled_ids().into_iter().next();
        self.validate_family_profile(&inner, &profile)?;
        inner.family_profiles.insert(key, profile.clone());
        Self::persist(&inner, &self.state_path())?;
        Ok(profile)
    }

    pub fn profile_get(&self, mac: &str) -> Option<Profile> {
        let inner = self.inner.lock().unwrap();
        inner
            .devices
            .get(&mac.to_uppercase())
            .and_then(|d| d.profile.clone())
    }

    /// Assets selected by the device Profile for a pixel-accurate Bridge preview.
    pub fn profile_preview_fonts(&self, mac: &str) -> Result<Vec<Vec<u8>>> {
        let Some(profile) = self.profile_get(mac) else { return Ok(Vec::new()) };
        self.preview_fonts_by_ids(&profile.font_ids)
    }

    pub fn preview_fonts_by_ids(&self, font_ids: &std::collections::BTreeMap<String, String>) -> Result<Vec<Vec<u8>>> {
        let library = self.font_library()?;
        font_ids.values().map(|id| {
            library.read(id)?.with_context(|| format!("font asset {id} disappeared"))
        }).collect()
    }

    /// Save a Profile draft (1–8 ordered ids). Saving never publishes.
    pub fn profile_save(&self, mut profile: Profile, now: u64) -> Result<Profile> {
        profile.device_mac = profile.device_mac.to_uppercase();
        profile.validate()?;
        if profile.template_ids.len() > MAX_PROFILE_TEMPLATES {
            bail!("at most {MAX_PROFILE_TEMPLATES} templates are allowed");
        }
        let mut inner = self.inner.lock().unwrap();
        {
            let record = inner
                .devices
                .get(&profile.device_mac)
                .context("unknown device")?;
            if profile.render_target.is_none() {
                if record.capabilities.render_target.is_empty() {
                    bail!("device target is unknown; select a render_target after authenticated discovery");
                }
                profile.render_target = Some(record.capabilities.render_target.clone());
            }
            if profile.render_target.as_deref() != Some(record.capabilities.render_target.as_str()) {
                bail!("profile target {:?} differs from device {}; migrate the profile explicitly", profile.render_target, record.capabilities.render_target);
            }
            for id in &profile.template_ids {
                if !inner.templates.keys().any(|k| {
                    &k.template_id == id && Some(k.render_target.as_str()) == profile.render_target.as_deref()
                }) {
                    bail!(
                        "template {id} has no variant for render target {}",
                        record.capabilities.render_target
                    );
                }
            }
        }
        // Fill any missing bindings from a source that provides the field.
        let mut requirements: Vec<FieldRequirement> = Vec::new();
        for id in &profile.template_ids {
            let record = inner.devices.get(&profile.device_mac).unwrap();
            if let Some(t) = inner
                .templates
                .get(&TemplateKey::new(id, &record.capabilities.render_target))
            {
                for r in &t.compiled.requirements {
                    if !requirements.iter().any(|x| x.field == r.field) {
                        requirements.push(r.clone());
                    }
                }
            }
        }
        for r in requirements.iter().filter(|r| !r.local) {
            if profile.binding_for("", &r.field).is_some()
                || profile.bindings.iter().any(|b| b.field == r.field)
            {
                continue;
            }
            let (canonical, _) = crate::datasource::resolve_field(&r.field);
            let source_id = inner
                .sources
                .values()
                .find(|s| s.enabled && source_has_field(s, &canonical))
                .map(|s| s.source_id.clone())
                .unwrap_or_else(|| {
                    inner
                        .sources
                        .values()
                        .find(|s| s.kind == DataSourceKind::Codex)
                        .map(|s| s.source_id.clone())
                        .unwrap_or_else(|| "codex".into())
                });
            profile.bindings.push(Binding {
                template_id: String::new(),
                field: r.field.clone(),
                source_id,
                source_field: canonical,
            });
        }
        let _ = self.profile_fonts(&inner, &profile)?;
        profile.updated_at = now;
        let mac = profile.device_mac.clone();
        if let Some(record) = inner.devices.get_mut(&mac) {
            record.profile = Some(profile.clone());
            record.sync_enabled = profile.sync_enabled;
        }
        // Refresh delivery contract for the new profile.
        refresh_contract(&mut inner, &mac)?;
        Self::persist(&inner, &self.state_path())?;
        Ok(profile)
    }

    /// Device data permission only; never edits template order or publishes.
    pub fn set_data_sync(&self, mac: &str, enabled: bool) -> Result<()> {
        let mac = mac.to_uppercase();
        let mut inner = self.inner.lock().unwrap();
        let record = inner.devices.get_mut(&mac).context("unknown device")?;
        record.sync_enabled = enabled;
        if let Some(profile) = record.profile.as_mut() { profile.sync_enabled = enabled; }
        if let Some(coordinator) = inner.coordinators.get_mut(&mac) {
            coordinator.sync_enabled = enabled;
            coordinator.data.push_dirty = enabled;
        }
        Self::persist(&inner, &self.state_path())
    }

    // ---- Explicit publish ------------------------------------------------

    /// Read-only preflight. Reuse is unknown until an authenticated versioned
    /// status digest exists, so this uses an all-missing conservative estimate.
    pub fn publish_preview(&self, mac: &str) -> Result<Value> {
        let inner = self.inner.lock().unwrap();
        let record = inner.devices.get(&mac.to_uppercase()).context("unknown device")?;
        let profile = record.profile.as_ref().context("device has no profile")?;
        profile.validate_publishable(&record.capabilities)?;
        let fonts = self.profile_fonts(&inner, profile)?;
        if record.capabilities.asset_publish_protocol != 1 {
            return Ok(json!({"route": "full_bundle", "render_target": profile.render_target,
                "profile_order": profile.template_ids, "font_dependencies": fonts,
                "max_bundle_bytes": record.capabilities.bundle_limit(),
                "can_publish_fonts": record.capabilities.bundle_font_protocol >= 1 &&
                    fonts.required.iter().all(|f| u32::from(f.version) <= record.capabilities.bundle_font_protocol),
                "source_conversion": "unavailable", "note": "CSFN fonts travel inside the complete Bundle; Unicode fonts require bundle_font_protocol 2"}));
        }
        let templates = profile.template_ids.iter().map(|id| {
            inner.templates.get(&TemplateKey::new(id, &record.capabilities.render_target)).cloned()
                .with_context(|| format!("missing template {id}"))
        }).collect::<Result<Vec<_>>>()?;
        let frozen = self.freeze_assets(&record.capabilities, profile, &templates, &fonts, "preview")?;
        let plan = crate::platform::publish::plan(&frozen, &CommittedRefs::default(), &capacity_from_caps(&record.capabilities))?;
        Ok(json!({"route": "asset_manifest", "manifest_id": frozen.manifest_id,
            "target_id": frozen.target_id,
            "render_target": profile.render_target, "firmware_target": record.capabilities.firmware_target,
            "compiler_abi": record.capabilities.compiler_abi, "profile_order": profile.template_ids,
            "initial_active_id": frozen.manifest.initial_active_id,
            "templates": frozen.manifest.templates, "fonts": frozen.manifest.fonts,
            "objects": frozen.manifest.objects, "plan_all_missing": plan,
            "reuse": "unknown_until_authenticated_asset_status", "source_conversion": "unavailable"}))
    }

    /// Freeze the current profile into a complete Bundle and queue exactly one
    /// PublishJob. Later template/profile edits cannot change the frozen content.
    pub fn publish(&self, mac: &str, now: u64) -> Result<Value> {
        self.publish_checked(mac, now, None, None)
    }

    pub fn publish_checked(&self, mac: &str, now: u64, expected_target_id: Option<&str>, bridge_id: Option<&str>) -> Result<Value> {
        let mac = mac.to_uppercase();
        let mut inner = self.inner.lock().unwrap();
        if inner.asset_jobs.get(&mac).is_some_and(|j| !j.state.is_terminal()) {
            bail!("device has an unfinished asset publish job; cancel or recover it before starting another");
        }
        let caps = inner
            .devices
            .get(&mac)
            .map(|d| d.capabilities.clone())
            .context("unknown device")?;
        let profile = inner
            .devices
            .get(&mac)
            .and_then(|d| d.profile.clone())
            .context("device has no profile")?;
        profile.validate_publishable(&caps)?;
        let font_plan = self.profile_fonts(&inner, &profile)?;
        for font in &font_plan.required {
            if u32::from(font.version) > caps.bundle_font_protocol {
                bail!("Unicode font {} requires device bundle_font_protocol {}; upgrade firmware first", font.name, font.version);
            }
        }
        if !profile.sync_enabled {
            // Publishing itself is explicit; data sync is a separate switch, so a
            // publish is allowed with sync off. Kept visible in the job summary.
        }
        let mut templates = Vec::new();
        let mut resources: Vec<BundleResource> = Vec::new();
        let mut requirements: Vec<FieldRequirement> = Vec::new();
        for id in &profile.template_ids {
            let key = TemplateKey::new(id, &caps.render_target);
            let template = inner
                .templates
                .get(&key)
                .with_context(|| {
                    format!(
                        "template {id} has no variant for render target {}",
                        caps.render_target
                    )
                })?
                .clone();
            crate::template::validate_template(&template.source)
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("template {id} is unsupported; edit and save before publishing"))?;
            crate::compile::validate(&template.compiled)
                .with_context(|| format!("template {id} compiled content is invalid; save before publishing"))?;
            if template.source["elements"].as_array().is_some_and(|elements|
                elements.iter().any(|element| element.get("digit_x").is_some())) {
                let required = template.compiled.min_fw.as_deref()
                    .context("digit_x template must declare min_fw")?;
                let observed = inner.devices[&mac].last_authenticated.as_ref()
                    .and_then(|status| status.body["fw"].as_str())
                    .context("device firmware version is unknown; refresh authenticated status before publishing digit_x")?;
                let revision = |version: &str| -> Result<Vec<u32>> {
                    version.split('-').next().unwrap_or("").split('.')
                        .map(|part| part.parse::<u32>().map_err(Into::into)).collect()
                };
                if revision(observed)? < revision(required)? {
                    bail!("template {id} requires firmware {required}; device reports {observed}");
                }
            }
            for res in &template.compiled.resources {
                if !resources.iter().any(|r| r.id == res.id) {
                    resources.push(BundleResource {
                        id: res.id.clone(),
                        data: res.data.clone(),
                    });
                }
            }
            for r in &template.compiled.requirements {
                if !requirements.iter().any(|x| x.field == r.field) {
                    requirements.push(r.clone());
                }
            }
            templates.push(template);
        }
        for r in requirements.iter().filter(|r| !r.local) {
            if !profile.bindings.iter().any(|b| b.field == r.field) {
                bail!("profile has no binding for required field {}", r.field);
            }
        }
        if caps.asset_publish_protocol == 1 {
            let frozen = self.freeze_assets(&caps, &profile, &templates, &font_plan, &job_id_for(&mac, now, &profile))?;
            if expected_target_id.is_some_and(|id| id != frozen.target_id) {
                bail!("publish target changed since preview; review the new snapshot before publishing");
            }
            let capacity = capacity_from_caps(&caps);
            let _ = crate::platform::publish::plan(&frozen, &CommittedRefs::default(), &capacity)?;
            let job = AssetJob { frozen, state: PublishState::Waiting, created_at: now, last_error: None };
            let summary = job.summary();
            inner.asset_jobs.insert(mac.clone(), job);
            inner.jobs.push(summary.clone());
            store::prune(&mut inner.jobs, MAX_JOB_SUMMARIES);
            Self::persist(&inner, &self.state_path())?;
            return Ok(summary);
        }
        if !font_plan.required.is_empty() && caps.bundle_font_protocol == 0 {
            bail!("device does not advertise bundle font support; selected fonts cannot be published");
        }
        let library = self.font_library()?;
        let fonts = font_plan.required.iter().map(|font| {
            Ok(BundleFont {
                name: font.name.clone(),
                font_id: font.id.clone(),
                data: base64::engine::general_purpose::STANDARD.encode(
                    library.read(&font.id)?.with_context(|| format!("font asset {} disappeared", font.id))?
                ),
            })
        }).collect::<Result<Vec<_>>>()?;
        let initial = profile
            .initial_active_id
            .clone()
            .unwrap_or_else(|| profile.template_ids[0].clone());
        let job_id = format!(
            "{:08x}",
            crc32fast::hash(format!("{mac}{now}{}", profile.template_ids.join(",")).as_bytes())
        );
        let bundle = Bundle {
            job_id: job_id.clone(),
            device_mac: mac.clone(),
            bridge_id: bridge_id.unwrap_or_default().to_string(),
            firmware_target: caps.firmware_target.clone(),
            render_target: caps.render_target.clone(),
            compiler_abi: crate::compile::COMPILER_ABI,
            profile: BundleProfile {
                template_ids: profile.template_ids.clone(),
                initial_active_id: initial,
                bindings: profile.bindings.clone(),
                full_sync_s: profile.full_sync_s,
            },
            templates,
            resources,
            fonts,
            total_len: 0,
            crc: String::new(),
        }
        .seal()?;
        let frozen_wire = freeze_wire(&bundle, &caps)?;
        inner.asset_jobs.remove(&mac);
        let coordinator = inner
            .coordinators
            .entry(mac.clone())
            .or_insert_with(|| Coordinator::new(&mac, caps.clone()));
        coordinator
            .enqueue_bundle(bundle, now)
            .map_err(|e| anyhow::anyhow!(e))?;
        coordinator.full_sync_s = profile.full_sync_s.max(60);
        coordinator.job.as_mut().unwrap().frozen_wire = frozen_wire;
        let summary = coordinator.job_snapshot().unwrap_or(Value::Null);
        inner.jobs.push(summary.clone());
        store::prune(&mut inner.jobs, MAX_JOB_SUMMARIES);
        Self::persist(&inner, &self.state_path())?;
        Ok(summary)
    }

    /// Frozen Bundle payload for the queued job (canonical bytes).
    pub fn bundle_payload(&self, mac: &str) -> Result<Vec<u8>> {
        let inner = self.inner.lock().unwrap();
        let job = inner
            .coordinators
            .get(&mac.to_uppercase())
            .and_then(|c| c.job.as_ref())
            .context("no publish job for device")?;
        match &job.frozen_wire {
            Some(encoded) => base64::engine::general_purpose::STANDARD.decode(encoded).context("frozen Bundle wire"),
            None => job.frozen_bundle.canonical_bytes(),
        }
    }

    /// Bind pre-existing uncommitted Bundles that were frozen before the
    /// ownership field existed. Re-seal the same job ID before any retry.
    pub fn bind_pending_bundle_owner(&self, mac: &str, bridge_id: &str) -> Result<()> {
        if bridge_id.is_empty() {
            bail!("bridge id is required for bundle ownership");
        }
        let mut inner = self.inner.lock().unwrap();
        let mac = mac.to_uppercase();
        let caps = inner.devices.get(&mac).context("unknown device")?.capabilities.clone();
        let job = inner.coordinators.get_mut(&mac)
            .and_then(|c| c.job.as_mut()).context("no publish job for device")?;
        if job.frozen_bundle.bridge_id == bridge_id {
            return Ok(());
        }
        if !job.frozen_bundle.bridge_id.is_empty() || job.state.is_terminal() {
            bail!("frozen bundle belongs to another bridge or is already terminal");
        }
        let mut bundle = job.frozen_bundle.clone();
        bundle.bridge_id = bridge_id.to_string();
        bundle = bundle.seal()?;
        job.frozen_wire = freeze_wire(&bundle, &caps)?;
        job.frozen_bundle = bundle;
        job.updated_at = crate::device_clock::wall_secs(&mac);
        Self::refresh_bundle_history(&mut inner, &mac);
        Self::persist(&inner, &self.state_path())?;
        Ok(())
    }

    /// Canonical Data message body for the in-flight snapshot.
    pub fn data_message_body(&self, mac: &str) -> Option<Value> {
        let inner = self.inner.lock().unwrap();
        inner
            .coordinators
            .get(&mac.to_uppercase())
            .and_then(|c| c.data_message_body())
    }

    pub fn codex_envelope_available(&self) -> bool {
        self.inner.lock().unwrap().codex_envelope.is_some()
    }

    pub fn job(&self, mac: &str) -> Option<Value> {
        let inner = self.inner.lock().unwrap();
        if let Some(job) = inner.asset_jobs.get(&mac.to_uppercase()) { return Some(job.summary()); }
        inner
            .coordinators
            .get(&mac.to_uppercase())
            .and_then(|c| c.job_snapshot())
    }

    pub fn ota_job(&self, mac: &str) -> Option<OtaJob> {
        self.inner.lock().unwrap().ota_jobs.get(&mac.to_uppercase()).cloned()
    }

    fn archive_ota(inner: &mut Inner, mac: &str, outcome: &str, closed_at: u64) -> Option<OtaHistoryEntry> {
        let job = inner.ota_jobs.remove(mac)?;
        let entry = OtaHistoryEntry { running_image: job.last_running_image.clone(),
            job, outcome: outcome.into(), closed_at };
        inner.ota_history.push(entry.clone());
        if inner.ota_history.len() > MAX_JOB_SUMMARIES { inner.ota_history.remove(0); }
        Some(entry)
    }

    pub fn ota_history(&self, mac: &str) -> Vec<OtaHistoryEntry> {
        self.inner.lock().unwrap().ota_history.iter()
            .filter(|entry| entry.job.device_mac == mac.to_uppercase())
            .cloned().collect()
    }

    pub fn bundle_history(&self, mac: &str) -> Vec<Value> {
        self.inner.lock().unwrap().jobs.iter()
            .filter(|entry| entry["device_mac"] == mac.to_uppercase())
            .cloned().collect()
    }

    /// Explicitly publish one frozen release as the latest for its exact target.
    /// Version strings and queued OTA jobs never update this catalog.
    pub fn publish_firmware_release(&self, rom: &Path, version: &str,
        firmware_target: &str) -> Result<FirmwareRelease> {
        if version.is_empty() || version.len() > 32 || firmware_target.is_empty()
            || !firmware_target.bytes().all(|byte|
                byte.is_ascii_alphanumeric() || byte == b'-') {
            bail!("invalid firmware release version or target");
        }
        let bytes = std::fs::read(rom).with_context(|| format!("read ROM {}", rom.display()))?;
        if !(1024..=0x30_0000).contains(&bytes.len()) || bytes.first() != Some(&0xe9) {
            bail!("ROM is not a supported ESP image (size/header)");
        }
        let marker = format!("codex-status-ota-v1|{firmware_target}|{version}\0");
        if !bytes.windows(marker.len()).any(|window| window == marker.as_bytes()) {
            bail!("ROM lacks a matching embedded OTA target/version identity");
        }
        let sha256 = format!("{:x}", sha2::Sha256::digest(&bytes));
        let blob = format!("release-{firmware_target}-{sha256}.bin");
        let release = FirmwareRelease { firmware_target: firmware_target.into(),
            version: version.into(), sha256, size: bytes.len(), blob: blob.clone(),
            published_at: crate::now_secs() };
        store::atomic_write(&self.dir.join(&blob), &bytes)?;
        let mut inner = self.inner.lock().unwrap();
        let old = inner.firmware_releases.insert(firmware_target.into(), release.clone());
        if let Err(error) = Self::persist(&inner, &self.state_path()) {
            if let Some(old) = old { inner.firmware_releases.insert(firmware_target.into(), old); }
            else { inner.firmware_releases.remove(firmware_target); }
            return Err(error);
        }
        if let Some(old) = old.filter(|old| old.blob != blob) {
            let _ = std::fs::remove_file(self.dir.join(old.blob));
        }
        Ok(release)
    }

    pub fn latest_firmware(&self, firmware_target: &str) -> Option<FirmwareRelease> {
        self.inner.lock().unwrap().firmware_releases.get(firmware_target).cloned()
    }

    pub fn running_image(&self, mac: &str) -> Option<Value> {
        self.inner.lock().unwrap().running_images.get(&mac.to_uppercase()).cloned()
    }

    pub fn note_running_image(&self, mac: &str, status: &Value, image: &Value) -> Result<()> {
        let mac = DeviceIdentity::normalized_mac(mac).context("invalid image MAC")?;
        let target = status["firmware_target"].as_str().context("status target missing")?;
        let hash = image["sha256"].as_str().context("image hash missing")?;
        if status["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()
                != Some(mac.as_str()) ||
            image["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()
                != Some(mac.as_str()) ||
            image["algorithm"] != "sha256-running-prefix-v1" ||
            image["fw_target"] != target ||
            !image["image_bytes"].as_u64().is_some_and(|size| (1024..=0x30_0000).contains(&size)) ||
            hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("running image evidence identity or shape mismatch");
        }
        let observed = json!({"algorithm":image["algorithm"], "device_mac":mac,
            "fw_target":target, "fw":image["fw"], "running_slot":image["running_slot"],
            "image_bytes":image["image_bytes"], "sha256":hash,
            "boot_id":status["boot_id"], "observed_at":crate::device_clock::wall_secs(&mac)});
        let mut inner = self.inner.lock().unwrap();
        inner.running_images.insert(mac, observed);
        Self::persist(&inner, &self.state_path())
    }

    pub fn queue_ota(&self, mac: &str, bridge_id: &str, request_id: &str,
        rom: &Path, expected_version: &str, firmware_target: &str) -> Result<OtaJob> {
        let mac = DeviceIdentity::normalized_mac(mac).context("invalid device MAC")?;
        if request_id.is_empty() || request_id.len() > 128 || expected_version.is_empty() || expected_version.len() > 32 {
            bail!("request_id and expected_version are required and bounded");
        }
        if let Some(existing) = self.ota_job(&mac).filter(|j| j.request_id == request_id) {
            if existing.expected_version != expected_version || existing.firmware_target != firmware_target {
                bail!("request_id reused with different OTA metadata");
            }
            if !rom.exists() { return Ok(existing); }
        }
        if let Some(existing) = self.ota_history(&mac).into_iter().rev()
            .find(|entry| entry.job.request_id == request_id).map(|entry| entry.job) {
            if existing.expected_version != expected_version || existing.firmware_target != firmware_target {
                bail!("request_id reused with different OTA metadata");
            }
            if !rom.exists() { return Ok(existing); }
        }
        let bytes = std::fs::read(rom).with_context(|| format!("read ROM {}", rom.display()))?;
        if !(1024..=0x30_0000).contains(&bytes.len()) || bytes.first() != Some(&0xe9) {
            bail!("ROM is not a supported ESP image (size/header)");
        }
        let marker = format!("codex-status-ota-v1|{firmware_target}|{expected_version}\0");
        if !bytes.windows(marker.len()).any(|w| w == marker.as_bytes()) {
            bail!("ROM lacks a matching embedded OTA target/version identity");
        }
        let digest = format!("{:x}", sha2::Sha256::digest(&bytes));
        let mut inner = self.inner.lock().unwrap();
        let caps = &inner.devices.get(&mac).context("unknown device")?.capabilities;
        if caps.firmware_target != firmware_target { bail!("ROM target does not match registered device"); }
        if let Some(existing) = inner.ota_history.iter().rev()
            .find(|entry| entry.job.device_mac == mac && entry.job.request_id == request_id) {
            if existing.job.sha256 == digest && existing.job.expected_version == expected_version
                && existing.job.firmware_target == firmware_target { return Ok(existing.job.clone()); }
            bail!("request_id reused with different OTA content");
        }
        if let Some(existing) = inner.ota_jobs.get(&mac) {
            if existing.request_id == request_id {
                if existing.sha256 == digest && existing.expected_version == expected_version && existing.firmware_target == firmware_target {
                    return Ok(existing.clone());
                }
                bail!("request_id reused with different OTA content");
            }
            if existing.pending() && !(existing.state == "awaiting_confirmation"
                && existing.upload_ack
                && matches!(existing.confirmation.as_deref(),
                    Some("version_observed" | "version_seen_unproven"))) {
                bail!("device has an unfinished OTA job; cancel or confirm it first");
            }
        }
        let blob = format!("ota-{mac}-{digest}.bin");
        let path = self.dir.join(&blob);
        store::atomic_write(&path, &bytes)?;
        let now = crate::device_clock::wall_secs(&mac);
        let job = OtaJob {
            job_id: format!("{:08x}", crc32fast::hash(format!("{mac}{request_id}{digest}").as_bytes())),
            request_id: request_id.into(), device_mac: mac.clone(), bridge_id: bridge_id.into(),
            created_at: now, updated_at: now, state: "queued".into(), firmware_target: firmware_target.into(),
            expected_version: expected_version.into(), before_version: None,
            before_boot_id: None, confirmation_started_at: None,
            sha256: digest, size: bytes.len(), blob,
            last_error: None, upload_ack: false, confirmation: None, attempt_count: 0,
            last_attempt_at: None, cancel_requested: false,
            confirmation_checks: 0, confirmation_note: None, last_running_image: None,
        };
        let old_history = inner.ota_history.clone();
        let old = inner.ota_jobs.insert(mac.clone(), job.clone());
        let old_blob = old.as_ref().map(|previous| previous.blob.clone());
        if let Some(previous) = &old {
            inner.ota_history.push(OtaHistoryEntry {
                job: previous.clone(), outcome: "replaced_by_new_request".into(),
                closed_at: now, running_image: previous.last_running_image.clone(),
            });
            if inner.ota_history.len() > MAX_JOB_SUMMARIES { inner.ota_history.remove(0); }
        }
        // New explicit OTA work gets a bounded connection opportunity. A prior
        // automatic retry ceiling must not silently strand a fresh user request.
        // Queue replays returned above; unknown activation and bad credentials
        // remain query/repair-only instead of authorizing another write.
        let old_connection = inner.wifi.get(&mac).and_then(|w| w.connection_facts.clone());
        if let Some(wifi) = inner.wifi.get_mut(&mac) {
            let eligible=wifi.connection_at(now).is_some_and(|c|!c.http_ready &&
                !matches!(c.state.as_str(),"connecting"|"awaiting_confirmation") &&
                !matches!(c.error.as_deref(),Some("authentication_failed"|"security_mismatch")));
            if eligible {if let Some(c)=wifi.connection_facts.as_mut() {
                c.attempts=0;c.next_attempt_at=0;c.requested=true;c.admission_error=None;
            }}
        }
        if let Err(e) = Self::persist(&inner, &self.state_path()) {
            if let Some(wifi) = inner.wifi.get_mut(&mac) { wifi.connection_facts = old_connection; }
            let reused_blob = old.as_ref().is_some_and(|previous| previous.blob == job.blob);
            if let Some(old) = old { inner.ota_jobs.insert(mac, old); } else { inner.ota_jobs.remove(&mac); }
            inner.ota_history = old_history;
            if !reused_blob { let _ = std::fs::remove_file(self.dir.join(&job.blob)); }
            return Err(e);
        }
        if let Some(blob) = old_blob.filter(|blob| blob != &job.blob) {
            let _ = std::fs::remove_file(self.dir.join(blob));
        }
        Ok(job)
    }

    pub fn ota_cancel(&self, mac: &str) -> Result<Value> {
        let mac = mac.to_uppercase();
        let mut inner = self.inner.lock().unwrap();
        let job = inner.ota_jobs.get_mut(&mac).context("no OTA job")?;
        let queued = job.state == "queued";
        if queued { job.state = "cancelled".into(); }
        else if job.pending() { job.cancel_requested = true; }
        let now = crate::device_clock::wall_secs(&mac);
        job.updated_at = now;
        let close = if queued { Some("cancelled_before_upload") }
            else if job.state == "awaiting_confirmation" && !job.upload_ack {
                Some("cancelled_unconfirmed")
            } else { None };
        let cleanup = queued.then(|| job.blob.clone());
        let result = if let Some(outcome) = close {
            json!(Self::archive_ota(&mut inner, &mac, outcome, now))
        } else { json!(inner.ota_jobs.get(&mac)) };
        Self::persist(&inner, &self.state_path())?;
        if let Some(blob) = cleanup { let _ = std::fs::remove_file(self.dir.join(blob)); }
        Ok(result)
    }

    pub fn ota_begin(&self, mac: &str) -> Result<Option<PathBuf>> {
        let mac = mac.to_uppercase();
        let mut inner = self.inner.lock().unwrap();
        let Some(job) = inner.ota_jobs.get(&mac) else { return Ok(None); };
        if job.state != "queued" { return Ok(None); }
        let path = self.dir.join(&job.blob);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                let job = inner.ota_jobs.get_mut(&mac).unwrap();
                job.state = "failed".into();
                job.last_error = Some(format!("frozen ROM unavailable: {error}"));
                Self::persist(&inner, &self.state_path())?;
                bail!("frozen ROM unavailable: {error}");
            }
        };
        if bytes.len() != job.size || format!("{:x}", sha2::Sha256::digest(&bytes)) != job.sha256 {
            inner.ota_jobs.get_mut(&mac).unwrap().state = "failed".into();
            inner.ota_jobs.get_mut(&mac).unwrap().last_error = Some("frozen ROM hash/length mismatch".into());
            Self::persist(&inner, &self.state_path())?;
            bail!("frozen ROM hash/length mismatch");
        }
        let before_boot_id = inner.devices.get(&mac)
            .and_then(|d|d.last_authenticated.as_ref())
            .and_then(|s|s.body["boot_id"].as_str().map(str::to_string));
        let before_version = inner.devices.get(&mac)
            .and_then(|d| d.last_authenticated.as_ref())
            .and_then(|s| s.body["fw"].as_str().map(str::to_string));
        let job = inner.ota_jobs.get_mut(&mac).unwrap();
        job.state = "transferring".into();
        job.before_version = before_version;
        job.before_boot_id = before_boot_id;
        job.attempt_count = job.attempt_count.saturating_add(1);
        job.last_attempt_at = Some(crate::device_clock::wall_secs(&mac));
        job.updated_at = crate::device_clock::wall_secs(&mac);
        Self::persist(&inner, &self.state_path())?;
        Ok(Some(path))
    }

    pub fn ota_await_confirmation(&self, mac: &str, upload_ack: bool, error: Option<&str>) -> Result<()> {
        let mac = mac.to_uppercase();
        let mut inner = self.inner.lock().unwrap();
        let job = inner.ota_jobs.get_mut(&mac).context("no OTA job")?;
        job.state = "awaiting_confirmation".into();
        job.upload_ack = upload_ack;
        job.confirmation_started_at = Some(crate::device_clock::wall_secs(&mac));
        if upload_ack { job.confirmation = Some("upload_ack".into()); }
        job.last_error = error.map(str::to_string);
        job.updated_at = crate::device_clock::wall_secs(&mac);
        if job.cancel_requested && !upload_ack {
            Self::archive_ota(&mut inner, &mac, "cancelled_unconfirmed",
                crate::device_clock::wall_secs(&mac));
        }
        Self::persist(&inner, &self.state_path())
    }

    pub fn ota_preflight_failed(&self, mac: &str, error: &str, terminal: bool) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let job = inner.ota_jobs.get_mut(&mac.to_uppercase()).context("no OTA job")?;
        if job.state != "transferring" { bail!("OTA is not transferring"); }
        let cancelled = job.cancel_requested;
        let terminal = terminal || job.attempt_count >= 3 || cancelled;
        job.state = if terminal { "failed" } else { "queued" }.into();
        job.last_error = Some(error.chars().take(200).collect());
        job.updated_at = crate::device_clock::wall_secs(&mac);
        let cleanup = (terminal && !cancelled).then(|| job.blob.clone());
        if cancelled {
            Self::archive_ota(&mut inner, &mac.to_uppercase(), "cancelled_before_upload",
                crate::device_clock::wall_secs(&mac));
        }
        Self::persist(&inner, &self.state_path())?;
        if let Some(blob) = cleanup { let _ = std::fs::remove_file(self.dir.join(blob)); }
        Ok(())
    }

    pub fn ota_note_authenticated_version(&self, mac: &str, status: &Value) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let Some(job) = inner.ota_jobs.get_mut(&mac.to_uppercase()) else { return Ok(()); };
        if job.state == "awaiting_confirmation" && job.upload_ack
            && status["fw"] == job.expected_version {
            let changed = job.before_version.as_deref().is_some_and(|before| before != job.expected_version);
            let (level, note) = if changed {
                ("version_observed", "expected version observed; exact running image identity unavailable")
            } else {
                ("version_seen_unproven", "expected version reported, but pre-upload version was same or unknown")
            };
            if job.confirmation.as_deref() != Some(level) {
                job.confirmation_note = Some(note.into());
                job.confirmation = Some(level.into());
                job.updated_at = crate::device_clock::wall_secs(&mac);
                return Self::persist(&inner, &self.state_path());
            }
        }
        Ok(())
    }

    pub fn ota_note_running_image(&self, mac: &str, image: &Value) -> Result<bool> {
        let mac = mac.to_uppercase();
        let job = match self.ota_job(&mac) {
            Some(job) if job.state == "awaiting_confirmation" && job.upload_ack => job,
            _ => return Ok(false),
        };
        if image["algorithm"] != "sha256-running-prefix-v1" ||
            image["image_bytes"] != job.size || image["fw_target"] != job.firmware_target ||
            image["device_mac"] != mac ||
            !image["sha256"].as_str().is_some_and(|hash|
                hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())) {
            return Ok(false);
        }
        let matches_target = image["sha256"] == job.sha256;
        if matches_target {
            let bytes = std::fs::read(self.dir.join(&job.blob)).context("frozen OTA image unavailable")?;
            if bytes.len() != job.size || format!("{:x}", sha2::Sha256::digest(&bytes)) != job.sha256 {
                bail!("frozen OTA image changed");
            }
        }
        let observation = json!({"algorithm":image["algorithm"], "device_mac":mac,
            "image_bytes":image["image_bytes"], "fw_target":image["fw_target"],
            "fw":image["fw"], "sha256":image["sha256"]});
        let mut inner = self.inner.lock().unwrap();
        let current = inner.ota_jobs.get_mut(&mac).context("OTA job vanished")?;
        if current.job_id != job.job_id || current.state != "awaiting_confirmation" { return Ok(false); }
        current.last_running_image = Some(observation);
        current.updated_at = crate::device_clock::wall_secs(&mac);
        if matches_target {
            current.confirmation = Some("image_verified".into());
            current.confirmation_note = Some("specified image is running; upload causation is not claimed".into());
            current.state = "succeeded".into();
        } else {
            current.confirmation_checks = current.confirmation_checks.saturating_add(1);
            let previously_observed_target = matches!(current.confirmation.as_deref(),
                Some("version_observed" | "version_seen_unproven"));
            let different_version = image["fw"] != current.expected_version;
            if previously_observed_target && different_version || current.confirmation_checks >= 3 {
                let outcome = if different_version { "superseded_running_image" }
                    else { "unverified_after_checks" };
                Self::archive_ota(&mut inner, &mac, outcome,
                    crate::device_clock::wall_secs(&mac));
            }
        }
        Self::persist(&inner, &self.state_path())?;
        Ok(matches_target)
    }

    pub fn ota_note_confirmation_miss(&self, mac: &str) -> Result<()> {
        let mac = mac.to_uppercase();
        let mut inner = self.inner.lock().unwrap();
        let Some(job) = inner.ota_jobs.get_mut(&mac) else { return Ok(()); };
        if job.state != "awaiting_confirmation" { return Ok(()); }
        job.confirmation_checks = job.confirmation_checks.saturating_add(1);
        job.updated_at = crate::device_clock::wall_secs(&mac);
        if job.confirmation_checks >= 3 {
            let outcome = if job.upload_ack { "unverified_after_checks" }
                else { "upload_unacknowledged" };
            Self::archive_ota(&mut inner, &mac, outcome,
                crate::device_clock::wall_secs(&mac));
        }
        Self::persist(&inner, &self.state_path())
    }

    pub fn cancel_job(&self, mac: &str, now: u64) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(job) = inner.asset_jobs.get_mut(&mac.to_uppercase()) {
            if job.state == PublishState::Waiting { job.state = PublishState::Cancelled; }
            let _ = Self::persist(&inner, &self.state_path());
            return;
        }
        if let Some(c) = inner.coordinators.get_mut(&mac.to_uppercase()) {
            c.cancel_job(now);
            Self::refresh_bundle_history(&mut inner, &mac.to_uppercase());
            let _ = Self::persist(&inner, &self.state_path());
        }
    }

    pub fn asset_job_pending(&self, mac: &str) -> bool {
        self.inner.lock().unwrap().asset_jobs.get(&mac.to_uppercase())
            .is_some_and(|j| !j.state.is_terminal())
    }

    /// Idempotent retry result: the same committed job returns its prior result.
    pub fn retry_job(&self, mac: &str, job_id: &str, committed: bool, now: u64) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let matched = inner
            .coordinators
            .get_mut(&mac.to_uppercase())
            .map(|c| c.retry_bundle_ack(job_id, committed, now))
            .unwrap_or(false);
        if matched && committed {
            Self::refresh_bundle_history(&mut inner, &mac.to_uppercase());
            let _ = Self::persist(&inner, &self.state_path());
        }
        matched
    }

    /// Explicit remote activate (new context on success).
    pub fn activate(&self, mac: &str, template_id: &str) -> Result<()> {
        let mac = mac.to_uppercase();
        let mut inner = self.inner.lock().unwrap();
        let record = inner.devices.get(&mac).context("unknown device")?;
        let profile = record.profile.clone().context("device has no profile")?;
        if !profile.template_ids.iter().any(|t| t == template_id) {
            bail!("template {template_id} is not installed in the profile");
        }
        let caps = record.capabilities.clone();
        if !inner
            .templates
            .contains_key(&TemplateKey::new(template_id, &caps.render_target))
        {
            bail!(
                "template {template_id} has no variant for {}",
                caps.render_target
            );
        }
        let c = inner
            .coordinators
            .get_mut(&mac)
            .context("device coordinator missing")?;
        c.request_activate(template_id.to_string());
        Ok(())
    }

    /// Successful activation: clear the pending request and adopt the device's
    /// new context. Must be called exactly once per applied Activate, otherwise
    /// the coordinator would re-activate on every cycle (new context each time).
    pub fn note_activate_done(&self, mac: &str, context_id: &str, now: u64) -> Result<()> {
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(c) = inner.coordinators.get_mut(&mac.to_uppercase()) {
                c.pending_activate = None;
            }
        }
        self.adopt_activation_context(mac, context_id, now)
    }

    /// Device-generated context on activation: Bridge adopts it and rebuilds the
    /// full first packet for the new active template.
    pub fn adopt_activation_context(&self, mac: &str, context_id: &str, now: u64) -> Result<()> {
        let mac = mac.to_uppercase();
        let mut inner = self.inner.lock().unwrap();
        // Commit/Activate ACK carries a context, not its indexed field table.
        // Even the same template id may have a different contract after publish.
        if let Some(record) = inner.devices.get_mut(&mac) {
            if record.observed.active_context_id.as_deref() != Some(context_id) {
                record.data_contract = None;
            }
        }
        let template_id = inner
            .coordinators
            .get(&mac)
            .and_then(|c| c.active_template_id.clone())
            .or_else(|| {
                inner
                    .devices
                    .get(&mac)
                    .and_then(|d| d.profile.as_ref())
                    .and_then(|p| p.initial_active_id.clone())
            })
            .context("device has no active template")?;
        let requirements = device_requirements(&inner, &mac, &template_id);
        let triggers = triggers_for(&inner, &mac, &template_id);
        let c = inner.coordinators.get_mut(&mac).context("unknown device")?;
        if c.context.as_ref().map(|x| &x.context_id) != Some(&context_id.to_string()) {
            // Preserve the per-device counter; never restart from zero.
            let seq = c.context.as_ref().map(|x| x.next_seq).unwrap_or(1);
            c.context = Some(crate::coordinator::ContextState {
                context_id: context_id.to_string(),
                created_at: now,
                reason: "activate".into(),
                next_seq: seq,
                acked_push_fp: None,
                acked_full_fp: None,
                full_sync_deadline: now + c.full_sync_s,
                last_applied_seq: 0,
                last_content_crc: None,
            });
            c.data.in_flight = None;
            c.set_contract(&template_id, requirements, triggers);
        }
        Self::persist(&inner, &self.state_path())?;
        Ok(())
    }

    // ---- Data sources ----------------------------------------------------

    pub fn data_sources(&self) -> Vec<Value> {
        let inner = self.inner.lock().unwrap();
        inner
            .sources
            .values()
            .map(|s| {
                let snapshot = inner.snapshots.get(&s.source_id);
                json!({
                    "source_id": s.source_id,
                    "kind": s.kind,
                    "enabled": s.enabled,
                    "config": redact_config(&s.config),
                    "credential_ref": s.credential_ref,
                    "quality": snapshot.map(|x| x.quality),
                    "observed_at": snapshot.map(|x| x.observed_at),
                    "valid_until": snapshot.map(|x| x.valid_until),
                    "last_success_at": snapshot.map(|x| x.last_success_at),
                    "error": snapshot.and_then(|x| x.error.clone()),
                    "fields": snapshot.map(|x| {
                        x.fields.iter().map(|(k, v)| json!({
                            "field": k,
                            "value": v.value,
                            "quality": v.quality,
                            "trigger": s.trigger_for(k),
                            "kind": s.field_kind(k),
                            "observed_at": v.observed_at,
                            "valid_until": v.valid_until,
                        })).collect::<Vec<_>>()
                    }).unwrap_or_default(),
                    "used_by": self.devices_using_source(&inner, &s.source_id),
                    "full_sync_s": 3600,
                })
            })
            .collect()
    }

    fn devices_using_source(&self, inner: &Inner, source_id: &str) -> Vec<String> {
        inner
            .devices
            .iter()
            .filter(|(_, d)| {
                d.profile
                    .as_ref()
                    .map(|p| p.bindings.iter().any(|b| b.source_id == source_id))
                    .unwrap_or(false)
            })
            .map(|(mac, _)| mac.clone())
            .collect()
    }

    pub fn data_source_save(&self, source: DataSource) -> Result<()> {
        source.validate()?;
        let mut inner = self.inner.lock().unwrap();
        inner.sources.insert(source.source_id.clone(), source);
        Self::persist(&inner, &self.state_path())
    }

    pub fn data_source_delete(&self, source_id: &str) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.sources.remove(source_id);
        Self::persist(&inner, &self.state_path())
    }

    /// Probe (collect once) a source: Codex uses the latest envelope, static
    /// sources evaluate their config.
    pub fn data_probe(&self, source_id: &str) -> Result<SourceSnapshot> {
        let mut inner = self.inner.lock().unwrap();
        let source = inner
            .sources
            .get(source_id)
            .cloned()
            .with_context(|| format!("unknown source {source_id}"))?;
        let snapshot = source.snapshot(inner.codex_envelope.as_ref());
        inner
            .snapshots
            .insert(source_id.to_string(), snapshot.clone());
        let coordinators: Vec<String> = inner.coordinators.keys().cloned().collect();
        for mac in coordinators {
            if let Some(c) = inner.coordinators.get_mut(&mac) {
                c.note_snapshot(snapshot.clone());
            }
        }
        Ok(snapshot)
    }

    /// Feed the Codex app-server envelope: refreshes the Codex source snapshot
    /// and every coordinator bound to it.
    pub fn note_codex_envelope(&self, envelope: &Value) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.codex_envelope = Some(envelope.clone());
        let codex_sources: Vec<DataSource> = inner
            .sources
            .values()
            .filter(|s| s.kind == DataSourceKind::Codex && s.enabled)
            .cloned()
            .collect();
        for source in codex_sources {
            let snapshot = source.snapshot(Some(envelope));
            inner
                .snapshots
                .insert(source.source_id.clone(), snapshot.clone());
            for c in inner.coordinators.values_mut() {
                c.note_snapshot(snapshot.clone());
            }
        }
        Ok(())
    }

    // ---- Devices ---------------------------------------------------------

    pub fn devices(&self) -> Vec<Value> {
        let inner = self.inner.lock().unwrap();
        inner
            .devices
            .values()
            .map(|d| {
                let mac = &d.identity.device_mac;
                let coordinator = inner.coordinators.get(mac);
                json!({
                    "device_mac": mac,
                    "name": d.identity.name,
                    "ip": d.identity.ip,
                    "discovered_via": d.identity.discovered_via,
                    "last_seen_at": d.identity.last_seen_at,
                    "capabilities": d.capabilities,
                    "hardware_verified": d.capabilities.hardware_verified,
                    "owner": coordinator.map(|c| c.session.clone()),
                    "profile": d.profile,
                    "profile_count": d.profile.as_ref().map(|p| p.template_ids.len()).unwrap_or(0),
                    "sync_enabled": d.sync_enabled,
                    "data_contract": d.data_contract,
                    "active_template_id": coordinator.and_then(|c| c.active_template_id.clone()),
                    "job": inner.asset_jobs.get(mac).map(AssetJob::summary)
                        .or_else(|| coordinator.and_then(|c| c.job_snapshot())),
                    "ota_job": inner.ota_jobs.get(mac),
                    "ota_history": inner.ota_history.iter().rev()
                        .filter(|entry| entry.job.device_mac == mac.as_str())
                        .collect::<Vec<_>>(),
                    "latest_firmware": inner.firmware_releases.get(&d.capabilities.firmware_target),
                    "running_image": inner.running_images.get(mac),
                    "data": coordinator.map(|c| json!({
                        "push_dirty": c.data.push_dirty,
                        "pull_only_change": c.pull_only_change(),
                        "full_sync_due": c.full_sync_due(crate::device_clock::wall_secs(&mac)),
                        "in_flight": c.data.in_flight.as_ref().map(|f| json!({
                            "seq": f.seq, "kind": f.kind, "attempts": f.attempts,
                        })),
                    })),
                    "power": coordinator.map(|c| json!({
                        "plan": c.plan.last_sent,
                        "prepared_at": (c.plan.last_sent_at != 0).then_some(c.plan.last_sent_at),
                        "accepted_plan_id": c.plan.last_accepted_id,
                        "accepted_remaining_s": c.plan.last_accepted_remaining_s,
                        "accepted_at": (c.plan.last_accepted_at != 0).then_some(c.plan.last_accepted_at),
                        "observed": d.observed.power,
                        "observed_at": d.observed.power_observed_at,
                        "observed_transport": d.observed.power_transport,
                    })),
                    "observed": coordinator.map(|c| c.session.clone()),
                    "last_authenticated": d.last_authenticated,
                    "last_authenticated_contact_at": d.last_authenticated_contact_at,
                    "last_authenticated_transport": d.last_authenticated_transport,
                    "last_status_attempt": d.last_status_attempt,
                    "record_observed": d.observed,
                })
            })
            .collect()
    }

    pub fn device_get(&self, mac: &str) -> Option<Value> {
        self.devices()
            .into_iter()
            .find(|d| d["device_mac"] == mac.to_uppercase())
    }

    /// Register or update a device; identity is always the Wi-Fi MAC.
    pub fn device_upsert(
        &self,
        identity: DeviceIdentity,
        capabilities: DeviceCapabilities,
    ) -> Result<()> {
        capabilities.validate()?;
        let mac = identity.device_mac.clone();
        let mut inner = self.inner.lock().unwrap();
        let existing = inner.devices.get(&mac).cloned();
        let record = DeviceRecord {
            identity,
            capabilities: capabilities.clone(),
            profile: existing.as_ref().and_then(|d| d.profile.clone()),
            observed: existing
                .as_ref()
                .map(|d| d.observed.clone())
                .unwrap_or_default(),
            last_authenticated: existing.as_ref().and_then(|d| d.last_authenticated.clone()),
            last_authenticated_contact_at: existing.as_ref().and_then(|d| d.last_authenticated_contact_at),
            last_authenticated_transport: existing.as_ref().and_then(|d| d.last_authenticated_transport.clone()),
            last_status_attempt: existing.as_ref().and_then(|d| d.last_status_attempt.clone()),
            sync_enabled: existing.as_ref().map(|d| d.sync_enabled).unwrap_or(false),
            data_contract: existing.as_ref().and_then(|d| d.data_contract.clone()),
        };
        inner.devices.insert(mac.clone(), record);
        let caps = inner.devices[&mac].capabilities.clone();
        let coordinator = inner
            .coordinators
            .entry(mac.clone())
            .or_insert_with(|| Coordinator::new(&mac, caps.clone()));
        coordinator.caps = caps;
        Self::persist(&inner, &self.state_path())
    }

    /// The caller must have verified the endpoint token and returned MAC.
    /// Bound the saved document so a malformed peer cannot grow state.json forever.
    pub fn note_authenticated_status(&self, mac: &str, status: &Value) -> Result<()> {
        self.note_authenticated_status_at(mac, status, crate::device_clock::wall_secs(mac))
    }

    pub fn note_authenticated_status_at(&self, mac: &str, status: &Value, now: u64) -> Result<()> {
        self.note_authenticated_status_via_at(mac,status,"http",now)
    }
    pub fn note_authenticated_status_via_at(&self,mac:&str,status:&Value,transport:&str,now:u64)->Result<()> {
        anyhow::ensure!(matches!(transport,"http"|"ble"),"invalid_status_transport");
        fn merge_field(previous: &Value, fresh: &Value, path: &str, now: u64,
                       boot: &Value, transport:&str,samples: &mut serde_json::Map<String, Value>) -> Value {
            if let Some(map) = fresh.as_object() {
                if fresh["value"].is_null() && map.contains_key("value") &&
                   matches!(fresh["reason"].as_str(), Some("not_sampled" | "unsupported" |
                       "not_applicable" | "read_error")) {
                    let old = if previous.get("reason").is_some() {
                        &previous["previous"]
                    } else { previous };
                    return json!({"value":null, "reason":fresh["reason"],
                        "previous":if old.is_null() { Value::Null } else { old.clone() }});
                }
                let mut merged = previous.as_object().cloned().unwrap_or_default();
                for (key, value) in map {
                    let child = format!("{path}.{key}");
                    merged.insert(key.clone(), merge_field(previous.get(key).unwrap_or(&Value::Null),
                        value, &child, now, boot, transport,samples));
                }
                return Value::Object(merged);
            }
            if !fresh.is_null() {
                samples.insert(path.into(), json!({"received_at":now,
                    "sampled_boot_id":boot, "transport":transport}));
            }
            fresh.clone()
        }
        let mac = mac.to_uppercase();
        if status["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref() != Some(mac.as_str()) {
            bail!("authenticated status MAC does not match {mac}");
        }
        if serde_json::to_vec(status)?.len() > 16 * 1024 {
            bail!("authenticated status exceeds 16 KiB");
        }
        let mut inner = self.inner.lock().unwrap();
        let record = inner.devices.get_mut(&mac).context("unknown device")?;
        if let Some(previous)=record.last_authenticated.as_ref() {
            if previous.body["boot_id"]==status["boot_id"] &&
                previous.body["status_seq"].as_u64().zip(status["status_seq"].as_u64()).is_some_and(|(old,new)|new<old) {
                bail!("stale_device_status");
            }
        }
        let mut body = record.last_authenticated.as_ref().map(|saved| saved.body.clone())
            .unwrap_or_else(|| json!({}));
        for key in ["presence","transports","wifi_control","status_seq","network"] {
            body.as_object_mut().unwrap().remove(key);
        }
        body["boot_id"] = status["boot_id"].clone();
        let mut field_samples = body["field_samples"].as_object().cloned().unwrap_or_default();
        let groups: [(&str, &[&str]); 7] = [
            ("identity", &["device_mac", "configured"]),
            ("firmware", &["fw", "firmware_target", "running_slot", "reset_reason",
                "image_identity", "sync_v1", "diag_format", "diag_capacity", "wifi_management"]),
            ("radio", &["radio", "presence", "transports", "wifi_control", "status_seq", "network"]),
            ("runtime", &["heap_free", "heap_min", "uptime_ms"]),
            ("display", &["display", "active_context_id", "active_template_id",
                "template_ids", "display_state", "data_contract"]),
            ("power", &["power", "sync"]),
            ("jobs", &["committed_job_id", "commit_seq", "data_seq", "applied_seq"]),
        ];
        for (group, fields) in groups {
            let mut sampled = false;
            let mut available = false;
            for key in fields {
                if let Some(value) = status.get(*key) {
                    body[*key] = if matches!(*key,"presence"|"transports"|"wifi_control"|"status_seq"|"network") {value.clone()}
                        else {merge_field(&body[*key], value, key, now, &status["boot_id"],transport,&mut field_samples)};
                    sampled = true;
                    available |= !value.is_null();
                }
            }
            if sampled {
                body["groups"][group] = json!({
                    "received_at": now, "sampled_boot_id": status.get("boot_id"),
                    "sampled_uptime_ms": status.get("uptime_ms"),
                    "sampled_wall": status.get("sampled_wall"),
                    "transport": transport,
                    "quality": if available { "observed" } else { "unavailable" },
                });
            }
        }
        body["field_samples"] = Value::Object(field_samples);
        body["received_at"] = json!(now);
        if serde_json::to_vec(&body)?.len() > 16 * 1024 {
            bail!("authenticated cached status exceeds 16 KiB");
        }
        let previous = (record.last_authenticated.clone(), record.last_status_attempt.clone(),
            record.last_authenticated_contact_at, record.last_authenticated_transport.clone());
        record.last_authenticated = Some(AuthenticatedStatus {
            observed_at: now, transport: transport.into(), schema_version: 2, body,
        });
        record.last_authenticated_contact_at = Some(now);
        record.last_authenticated_transport = Some(transport.into());
        record.last_status_attempt = Some(StatusAttempt { at: now, outcome: "ok".into(), error: None });
        // Every validated HTTP status path confirms connectivity, including
        // background polling and BLE hints followed by an HTTP read.
        let previous_connection = inner.wifi.get(&mac).and_then(|wifi| wifi.connection_facts.clone());
        if let Some(wifi)=inner.wifi.get_mut(&mac) {
            if let Some(c)=wifi.connection_facts.as_mut().filter(|c|c.local_revision==wifi.local_revision) {
                let boot_id=status["boot_id"].to_string();
                if transport=="http" {c.note_http(boot_id,now,None);}
                else if c.observation.as_ref().is_some_and(|o|o.boot_id!=boot_id) ||
                    c.contact.as_ref().is_some_and(|h|h.boot_id!=boot_id) {
                    c.observation=None;c.contact=None;
                }
            }
        }
        if let Err(error) = Self::persist(&inner, &self.state_path()) {
            let record = inner.devices.get_mut(&mac).unwrap();
            (record.last_authenticated, record.last_status_attempt,
                record.last_authenticated_contact_at, record.last_authenticated_transport) = previous;
            if let Some(wifi) = inner.wifi.get_mut(&mac) {
                wifi.connection_facts = previous_connection;
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn note_status_attempt(&self, mac: &str, outcome: &str, error: Option<&str>) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let record = inner.devices.get_mut(&mac.to_uppercase()).context("unknown device")?;
        let now = crate::device_clock::wall_secs(&mac);
        if record.last_status_attempt.as_ref().is_some_and(|last|
            last.outcome == outcome && last.error.as_deref() == error
                && now.saturating_sub(last.at) < 60) { return Ok(()); }
        record.last_status_attempt = Some(StatusAttempt {
            at: now, outcome: outcome.into(),
            error: error.map(|s| s.chars().take(200).collect()),
        });
        Self::persist(&inner, &self.state_path())
    }

    /// Persist a new endpoint only after the caller verifies authenticated MAC/session.
    pub fn note_device_endpoint(&self, mac: &str, ip: &str) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let record = inner.devices.get_mut(&mac.to_uppercase()).context("unknown device")?;
        if record.identity.ip.as_deref() == Some(ip) { return Ok(()); }
        record.identity.ip = Some(ip.to_owned());
        Self::persist(&inner, &self.state_path())
    }

    pub fn device_rename(&self, mac: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 24 {
            bail!("device name must be 1–24 characters");
        }
        let mac = mac.to_uppercase();
        let mut inner = self.inner.lock().unwrap();
        let record = inner.devices.get_mut(&mac).context("unknown device")?;
        record.identity.name = name.to_string();
        Self::persist(&inner, &self.state_path())
    }

    /// Authenticated Status digest; the device is authoritative for its context.
    pub fn note_device_status(&self, mac: &str, status: &Value) -> Result<()> {
        self.note_device_status_at(mac, status, "unknown", crate::device_clock::wall_secs(mac))
    }

    pub fn note_device_status_at(&self, mac: &str, status: &Value, transport: &str, now: u64) -> Result<()> {
        let mac = mac.to_uppercase();
        if status.get("device_mac").and_then(Value::as_str)
            .and_then(DeviceIdentity::normalized_mac)
            .is_some_and(|reported| reported != mac) {
            bail!("status MAC does not match {mac}");
        }
        let mut inner = self.inner.lock().unwrap();
        let reported = status.get("active_template_id").and_then(Value::as_str).unwrap_or("");
        let record = inner.devices.get(&mac).context("unknown device")?;
        if let Some(previous)=record.last_authenticated.as_ref() {
            if previous.body["boot_id"]==status["boot_id"] && previous.body["status_seq"].as_u64().zip(status["status_seq"].as_u64()).is_some_and(|(old,new)|new<old){bail!("stale_device_status");}
        }
        let storage_capacity = match status.get("bundle_storage_protocol") {
            Some(value) => {
                let protocol = value.as_u64().filter(|v| *v <= 2)
                    .context("unsupported bundle_storage_protocol")? as u32;
                let limit = status["max_bundle_bytes"].as_u64()
                    .filter(|v| *v <= crate::platform::model::MAX_BUNDLE_BYTES as u64)
                    .context("invalid dynamic Bundle capacity")?;
                Some((protocol, limit))
            }
            None => None,
        };
        let compression_capacity = if transport == "http" || status.get("bundle_compression").is_some() {
            let protocol = status.get("bundle_compression").map(|v| v.as_u64().filter(|n| *n <= 1)
                .context("unsupported bundle_compression")).transpose()?.unwrap_or(0) as u32;
            let decoded = if protocol == 1 {
                if storage_capacity.map_or(record.capabilities.bundle_storage_protocol, |(v, _)| v) != 2 {
                    bail!("Bundle compression requires single-copy storage");
                }
                status["max_bundle_decoded_bytes"].as_u64()
                    .filter(|n| *n > 0 && *n <= MAX_BUNDLE_BYTES as u64).context("invalid Bundle decoded limit")?
            } else { 0 };
            Some((protocol, decoded))
        } else { None };
        let contract = match status.get("data_contract").filter(|value| !value.is_null()) {
            _ if status["configured"] == false => None,
            Some(value) => {
                let table: DeviceDataContract = serde_json::from_value(value.clone())?;
                table.validate(&record.capabilities)?;
                validate_template_id(&table.template_id)?;
                if table.template_id != reported ||
                    status["active_context_id"].as_str().is_none_or(str::is_empty) {
                    bail!("device data contract does not match active status");
                }
                Some(table)
            }
            // BLE's existing context/active digest identifies the installed
            // immutable contract. A new context or active requires a fresh HTTP
            // field table; never guess it from local templates.
            None if transport == "ble" || status["status_projection"] == "compact" => record.data_contract.clone().filter(|table|
                table.template_id == reported &&
                record.observed.active_context_id.as_deref() ==
                    status["active_context_id"].as_str()),
            None => None,
        };
        let contract_changed = record.data_contract != contract ||
            record.observed.active_template_id.as_deref() != Some(reported);
        if let Some(record) = inner.devices.get_mut(&mac) {
            if let Some((protocol, limit)) = storage_capacity {
                record.capabilities.bundle_storage_protocol = protocol;
                record.capabilities.max_bundle_bytes = limit;
            }
            if let Some((protocol, decoded)) = compression_capacity {
                record.capabilities.bundle_compression = protocol;
                record.capabilities.max_bundle_decoded_bytes = decoded;
            }
            record.data_contract = contract;
            record.last_authenticated_contact_at = Some(now);
            record.observed.observed_at = Some(now);
            record.observed.transport = Some(transport.into());
            record.observed.fw = status.get("fw").and_then(Value::as_str).map(str::to_string);
            record.observed.boot_id = status.get("boot_id").and_then(Value::as_str).map(str::to_string);
            record.observed.active_context_id = status
                .get("active_context_id")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            record.observed.active_template_id = status
                .get("active_template_id")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            record.observed.committed_job_id = status
                .get("committed_job_id")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            record.observed.data_seq = status.get("data_seq").and_then(|v| v.as_u64());
            record.observed.applied_seq = status.get("applied_seq").and_then(|v| v.as_u64());
            record.observed.last_acked_at = status
                .get("last_acked_at")
                .and_then(|v| v.as_u64())
                .or(Some(now));
            if status["power"].as_object().is_some_and(|power| !power.is_empty()) {
                record.observed.power = Some(status["power"].clone());
                record.observed.power_observed_at = Some(now);
                record.observed.power_transport = Some(transport.into());
            }
            record.observed.display_state = status
                .get("display_state")
                .and_then(|v| v.as_str())
                .map(str::to_string);
        }
        if let Some(c) = inner.coordinators.get_mut(&mac) {
            if let Some((protocol, limit)) = storage_capacity {
                c.caps.bundle_storage_protocol = protocol;
                c.caps.max_bundle_bytes = limit;
            }
            if let Some((protocol, decoded)) = compression_capacity {
                c.caps.bundle_compression = protocol;
                c.caps.max_bundle_decoded_bytes = decoded;
            }
            c.note_status(status, now);
            if let Some(seq) = c.session.data_seq.or(c.session.applied_seq) {
                if let Some(context) = c.context.as_mut() {
                    context.next_seq = context.next_seq.max(seq.saturating_add(1));
                }
            }
            c.active_template_id = (!reported.is_empty()).then(|| reported.to_owned());
            if contract_changed {
                c.data.in_flight = None;
                c.data.acked_push_fp = None;
                c.data.acked_full_fp = None;
                c.data.push_dirty = c.sync_enabled;
            }
        }
        // OTA/software reset can retain the Bundle context while losing RAM Data.
        // A lower authenticated sequence invalidates that delivery baseline,
        // even after a Bridge restart. Keep any uncertain snapshot and its seq.
        let mut data_baseline_lost = false;
        if let Some(c) = inner.coordinators.get_mut(&mac) {
            if let (Some(context), Some(seq)) = (c.context.as_mut(), c.session.data_seq.or(c.session.applied_seq)) {
                if c.session.active_context_id.as_deref() == Some(context.context_id.as_str())
                    && seq < context.last_applied_seq && context.acked_full_fp.is_some() {
                    context.acked_push_fp = None;
                    context.acked_full_fp = None;
                    c.data.acked_push_fp = None;
                    c.data.acked_full_fp = None;
                    c.data.push_dirty = c.sync_enabled;
                    data_baseline_lost = true;
                }
            }
        }
        let mut power_reconciled = false;
        if let Some(c) = inner.coordinators.get_mut(&mac) {
            if let Some(pending) = c.plan.last_sent.clone() {
                let power = &status["power"];
                let mode = if pending.mode == PlanMode::Light { "light" } else { "sleep" };
                if power["plan_id"] == pending.plan_id && power["mode"] == mode {
                    if let Some(remaining) = power["remaining_s"].as_u64().filter(|n| *n <= pending.light_duration_s as u64) {
                        power_reconciled = c.note_plan_ack(pending.plan_id, remaining as u32, false, now)
                            == crate::coordinator::PlanAck::Accepted;
                    }
                }
            }
        }
        let mut job_reconciled = false;
        if status["configured"] == true {
            if let Some(job_id) = status["committed_job_id"].as_str().filter(|s| !s.is_empty()) {
                let local_pending = inner.coordinators.get(&mac)
                    .and_then(|c| c.job.as_ref())
                    .is_some_and(|j| j.job_id == job_id && j.state != PublishState::Succeeded);
                if local_pending {
                    inner.coordinators.get_mut(&mac).expect("coordinator")
                        .retry_bundle_ack(job_id, true, now);
                    Self::refresh_bundle_history(&mut inner, &mac);
                    job_reconciled = true;
                } else if let Some(entry) = inner.jobs.iter_mut().rev()
                    .find(|entry| entry["job_id"] == job_id && entry["state"] != "succeeded") {
                    entry["state"] = json!("succeeded");
                    job_reconciled = true;
                }
            }
        }
        // Reconcile a device context we do not know (cold start / recovery):
        // adopt it but keep the per-device seq counter monotonic.
        let reconcile = inner.coordinators.get(&mac).and_then(|c| {
            let device_ctx = c.session.active_context_id.clone()?;
            let known = c.context.as_ref().map(|x| x.context_id.clone());
            (known.as_deref() != Some(device_ctx.as_str())).then_some((
                device_ctx,
                c.session.data_seq.or(c.session.applied_seq).unwrap_or(0),
                c.context.as_ref().map(|x| x.next_seq).unwrap_or(1),
                c.full_sync_s,
            ))
        });
        let reconcile_needed = reconcile.is_some();
        if let Some((device_ctx, device_seq, local_next, full_sync_s)) = reconcile {
            let c = inner.coordinators.get_mut(&mac).expect("coordinator");
            c.context = Some(crate::coordinator::ContextState {
                context_id: device_ctx,
                created_at: now,
                reason: "reconciled_from_device".into(),
                next_seq: local_next.max(device_seq.saturating_add(1)),
                acked_push_fp: None,
                acked_full_fp: None,
                full_sync_deadline: now + full_sync_s,
                last_applied_seq: device_seq,
                last_content_crc: None,
            });
            c.data.in_flight = None;
            // ACKs belong to their context, even if the installed fields and
            // current values are unchanged after a device reboot.
            c.data.acked_push_fp = None;
            c.data.acked_full_fp = None;
            c.data.push_dirty = c.sync_enabled;
        }
        refresh_contract(&mut inner, &mac)?;
        if reconcile_needed || job_reconciled || power_reconciled || contract_changed || data_baseline_lost {
            let _ = Self::persist(&inner, &self.state_path());
            inner
                .status_persist_at
                .insert(mac.clone(), crate::device_clock::wall_secs(&mac));
        } else {
            let last = inner.status_persist_at.get(&mac).copied().unwrap_or(0);
            if crate::device_clock::wall_secs(&mac).saturating_sub(last) >= 60 {
                inner
                    .status_persist_at
                    .insert(mac.clone(), crate::device_clock::wall_secs(&mac));
                let _ = Self::persist(&inner, &self.state_path());
            }
        }
        Ok(())
    }

    pub fn note_ble_contact(&self, mac: &str) -> Result<()> {
        self.note_ble_contact_at(mac, crate::device_clock::wall_secs(mac))
    }

    pub fn note_ble_contact_at(&self, mac: &str, at: u64) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let record = inner.devices.get_mut(&mac.to_uppercase()).context("unknown device")?;
        record.last_authenticated_contact_at = Some(at);
        record.last_authenticated_transport = Some("ble".into());
        Self::persist(&inner, &self.state_path())
    }

    // ---- Data delivery ---------------------------------------------------

    /// Decide and pin the next delivery for one device. `reachable` is an
    /// authenticated rendezvous opportunity right now.
    pub fn next_delivery(&self, mac: &str, reachable: bool, now: u64) -> Value {
        self.delivery_for_transport(mac, reachable, now, true)
    }

    pub fn next_http_delivery(&self, mac: &str, reachable: bool, now: u64) -> Value {
        self.delivery_for_transport(mac, reachable, now, false)
    }

    fn delivery_for_transport(&self, mac: &str, reachable: bool, now: u64, ble: bool) -> Value {
        let mut inner = self.inner.lock().unwrap();
        let mac = mac.to_uppercase();
        let contract_known = inner.devices.get(&mac)
            .is_some_and(|record| record.data_contract.is_some());
        let sources_ready = data_sources_ready(&inner, &mac);
        let Some(c) = inner.coordinators.get_mut(&mac) else {
            return json!({"decision": "unknown_device"});
        };
        let decision = match c.next_delivery(now, reachable) {
            Delivery::BleData(snapshot) if !ble => Delivery::LightData(snapshot),
            other => other,
        };
        if matches!(decision, Delivery::BleData(_) | Delivery::LightData(_)) {
            if !contract_known {
                return json!({"decision":"waiting_for_device_contract"});
            }
            if c.requirements.is_empty() {
                return json!({"decision":"idle"});
            }
            // Retry pinned content even if a source is currently unavailable;
            // only new snapshots must wait for their sources' first success.
            if c.data.in_flight.is_none() && !sources_ready {
                tracing::info!(event="data_source_wait", device_mac=%mac,
                    next_stage="read_data_source", "new Data deferred until required source is ready");
                return json!({"decision":"waiting_for_data_source",
                    "next_stage":"read_data_source",
                    "reason":"required source has no successful snapshot yet"});
            }
        }
        let mut out = match &decision {
            Delivery::Idle => json!({"decision": "idle"}),
            Delivery::WaitingForRendezvous { reason } => {
                json!({"decision": "waiting_for_rendezvous", "reason": reason})
            }
            Delivery::Noop { time_sync } => {
                json!({"decision": "noop", "time_sync": time_sync})
            }
            Delivery::BleData(_) => json!({"decision": "ble_data"}),
            Delivery::LightData(_) => json!({"decision": "light_data"}),
            Delivery::Bundle { job_id } => json!({"decision": "bundle", "job_id": job_id}),
            Delivery::Activate {
                template_id,
                context_id,
            } => json!({
                "decision": "activate",
                "template_id": template_id,
                "expected_active_context_id": context_id,
            }),
            Delivery::Plan(plan) => json!({"decision": "plan", "plan": plan}),
        };
        if matches!(decision, Delivery::BleData(_) | Delivery::LightData(_)) {
            let new_snapshot = c.data.in_flight.is_none();
            match c.note_delivery_started(&decision, now) {
                Ok(flight) => {
                    if new_snapshot {
                        let missing_fields: Vec<_> = flight.payload.fields.iter()
                            .filter(|(_, field)| field.value.is_null())
                            .map(|(name, _)| name.as_str()).collect();
                        tracing::info!(event="data_snapshot_prepared", device_mac=%mac,
                            seq=flight.seq, content_crc=%flight.content_crc,
                            field_count=flight.payload.fields.len(), missing_fields=?missing_fields,
                            "complete Data snapshot pinned from initialized sources");
                    }
                    out["temporary_data"] = json!(c.data.acked_push_fp == Some(flight.push_fp));
                    out["data_seq"] = json!(flight.seq);
                    out["active_context_id"] = json!(flight.payload.active_context_id);
                    out["content_crc"] = json!(flight.content_crc);
                    out["fields"] = json!(flight.payload.fields);
                }
                Err(e) => out["error"] = json!(e),
            }
        }
        if matches!(decision, Delivery::Bundle { .. } | Delivery::BleData(_) | Delivery::LightData(_))
            && out.get("error").is_none()
        {
            if let Err(e) = Self::persist(&inner, &self.state_path()) {
                return json!({"decision": "storage_error", "error": e.to_string()});
            }
        }
        out
    }

    pub fn note_ack(
        &self,
        mac: &str,
        kind: DeliveryKind,
        seq: u64,
        content_crc: &str,
        applied: bool,
        display_state: &str,
    ) -> Value {
        let mut inner = self.inner.lock().unwrap();
        let Some(c) = inner.coordinators.get_mut(&mac.to_uppercase()) else {
            return json!({"outcome": "unknown_device"});
        };
        let outcome = c.note_ack(
            kind,
            seq,
            content_crc,
            applied,
            display_state,
            crate::device_clock::wall_secs(&mac),
        );
        // The confirmed fingerprint / data_seq checkpoint must survive a restart.
        let _ = Self::persist(&inner, &self.state_path());
        json!({
            "outcome": match outcome {
                AckOutcome::Applied => "applied",
                AckOutcome::Rejected => "rejected",
                AckOutcome::Stale => "stale",
                AckOutcome::IgnoredNoFlight => "no_in_flight",
            },
            "display_state": display_state,
        })
    }

    /// Formal plan for a rendezvous. A physical (BOOT/button) wake opens a 300 s
    /// window that later rendezvous must not cut short: the effective deadline is
    /// `max(formal plan, t_boot + BOOT_PROVISIONAL_S)`. Pending work may extend
    /// it; only the explicit entry points may end it earlier.
    pub fn uses_power_plan(&self, mac: &str) -> Result<bool> {
        Ok(self.inner.lock().unwrap().devices.get(&mac.to_uppercase())
            .context("unknown device")?.capabilities.uses_power_plan())
    }

    pub fn plan_for_rendezvous(&self,mac:&str,now:u64,wake_reason:&str,provisional_remaining_s:u32)->Result<PowerPlan> {
        self.plan_for_rendezvous_network_work(mac,now,wake_reason,provisional_remaining_s,false)
    }

    pub fn plan_for_rendezvous_network_work(
        &self,
        mac: &str,
        now: u64,
        wake_reason: &str,
        provisional_remaining_s: u32,
        network_work: bool,
    ) -> Result<PowerPlan> {
        anyhow::ensure!(self.uses_power_plan(mac)?, "power_plan_unsupported");
        let mac = mac.to_uppercase();
        let mut inner = self.inner.lock().unwrap();
        if let Some(pending) = inner.coordinators.get(&mac)
            .and_then(|c| c.plan.pending_explicit_light.as_ref()) {
            return Ok(pending.clone());
        }
        let manual_boot = if wake_reason == "manual" {
            // The device reports what is left of its window, so the physical wake
            // instant (`t_boot`) can be recovered and remembered: it stops
            // reporting `provisional_remaining_s` as soon as a formal plan is
            // accepted, but the window itself is still running.
            let elapsed = crate::coordinator::BOOT_PROVISIONAL_S
                .saturating_sub(provisional_remaining_s) as u64;
            let t_boot = now.saturating_sub(elapsed);
            if let Some(c) = inner.coordinators.get_mut(&mac) {
                c.note_manual_window(t_boot);
            }
            Some(t_boot)
        } else {
            None
        };
        // Light is only granted for real pending work: sync alone does not keep
        // the radio on (v2 §7). Pending data/jobs/activation do.
        let contract_known = inner.devices.get(&mac)
            .is_some_and(|record| record.data_contract.is_some());
        let sources_ready = data_sources_ready(&inner, &mac);
        let contract_needed = inner.devices.get(&mac).is_some_and(|record|
            record.sync_enabled && !contract_known &&
            record.observed.active_context_id.as_deref().is_some_and(|id| !id.is_empty()) &&
            record.observed.active_template_id.as_deref().is_some_and(|id| !id.is_empty()));
        // W4-1: a queued (non-terminal, backoff-eligible) Wi-Fi read/sync job is
        // real pending work. It is admitted into the window work set and gets a
        // *bounded* formal light plan, so a read no longer has to borrow somebody
        // else's window. The Wi-Fi job store is keyed by the normalized MAC.
        let wifi_window_wanted = DeviceIdentity::normalized_mac(&mac).is_some_and(|wifi_mac| {
            inner
                .wifi
                .get(&wifi_mac)
                .is_some_and(|state| state.jobs.iter().any(|job| wifi_job_wants_window(job, now)) || state.connection_at(now).as_ref().is_some_and(|c|
                    (network_work || c.state=="requested") &&
                    c.local_revision==state.local_revision && state.matching_target(&c.host_ssid).is_some_and(|n|n.network_id==c.network_id) &&
                    matches!(c.state.as_str(),"requested"|"waiting_for_window"|"connecting"|"awaiting_confirmation"|"connection_failed") &&
                    !matches!(c.error.as_deref(),Some("authentication_failed"|"security_mismatch")) &&
                    c.attempts<3 && (c.state=="connecting" || now>=c.next_attempt_at)))
        });
        let other_work_want_light = contract_needed || inner
            .coordinators
            .get(&mac)
            .map(|c| {
                (contract_known && sources_ready && c.sync_enabled && !c.sources.is_empty() && c.data.push_dirty)
                    || c.data.in_flight.as_ref().is_some_and(|flight|
                        c.data.acked_push_fp != Some(flight.push_fp))
                    || c.pending_activate.is_some()
                    || c.job
                        .as_ref()
                        .map(|j| !j.state.is_terminal())
                        .unwrap_or(false)
            })
            .unwrap_or(false)
            || inner.ota_jobs.get(&mac).is_some_and(|j| matches!(j.state.as_str(),"queued"|"awaiting_confirmation"));
        let work_want_light = other_work_want_light || wifi_window_wanted;
        let c = inner.coordinators.get_mut(&mac).context("unknown device")?;
        let manual_remaining = c.manual_remaining(now);
        let hold_remaining = c.plan.light_hold_until.saturating_sub(now).min(u32::MAX as u64) as u32;
        let want_light =
            work_want_light || manual_remaining.max(hold_remaining) >= crate::coordinator::MIN_LIGHT_S;
        let plan = match manual_boot {
            Some(t_boot) => c.boot_plan(
                now,
                t_boot,
                want_light,
                crate::coordinator::BOOT_PROVISIONAL_S,
            ),
            None => {
                // Without pending work, light lasts exactly as long as the
                // physical wake's own window: re-sending MAX_LIGHT_S every
                // rendezvous would extend it forever. Data/publish work keeps the
                // existing MAX_LIGHT_S; a Wi-Fi read alone only gets the bounded
                // read window, and manual/boot time gets no extension at all.
                let light_s = if other_work_want_light {
                    crate::coordinator::MAX_LIGHT_S
                } else if wifi_window_wanted {
                    crate::coordinator::WIFI_READ_LIGHT_S
                } else {
                    manual_remaining.max(hold_remaining)
                };
                c.plan_for(now, want_light, light_s.max(hold_remaining), "rendezvous")
            }
        };
        Self::persist(&inner, &self.state_path())?;
        Ok(plan)
    }

    /// Raise the bridge-side light hold for one device (post-OTA control
    /// window). Rendezvous plans grant light until this epoch.
    pub fn hold_light(&self, mac: &str, until: u64) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let c = inner
            .coordinators
            .get_mut(&mac.to_uppercase())
            .context("unknown device")?;
        c.hold_light(until);
        Self::persist(&inner, &self.state_path())
    }

    /// Durable explicit light request, shared by UI and MCP. The plan is
    /// frozen until an authenticated HTTP/BLE ACK or status reconciliation.
    pub fn queue_explicit_light(&self, mac: &str, now: u64) -> Result<(PowerPlan, bool)> {
        anyhow::ensure!(self.uses_power_plan(mac)?, "power_plan_unsupported");
        let mut inner = self.inner.lock().unwrap();
        let c = inner.coordinators.get_mut(&mac.to_uppercase()).context("unknown device")?;
        let already_pending = c.plan.pending_explicit_light.is_some();
        let plan = c.queue_explicit_light(now, crate::coordinator::MAX_LIGHT_S);
        if !already_pending {
            Self::persist(&inner, &self.state_path())?;
        }
        Ok((plan, already_pending))
    }

    /// Explicit user/debug power action with a freshly allocated plan id.
    pub fn explicit_plan(
        &self,
        mac: &str,
        mode: crate::platform::model::PlanMode,
        light_s: u32,
        reason: &str,
    ) -> Result<PowerPlan> {
        anyhow::ensure!(self.uses_power_plan(mac)?, "power_plan_unsupported");
        let mut inner = self.inner.lock().unwrap();
        let c = inner
            .coordinators
            .get_mut(&mac.to_uppercase())
            .context("unknown device")?;
        let now = crate::device_clock::wall_secs(&mac);
        if mode == crate::platform::model::PlanMode::Light {
            if let Some(pending) = &c.plan.pending_explicit_light {
                return Ok(pending.clone());
            }
        }
        if mode == crate::platform::model::PlanMode::Sleep {
            c.cancel_explicit_light();
        }
        let plan = c.plan_for(
            now,
            mode == crate::platform::model::PlanMode::Light,
            if light_s == 0 {
                crate::coordinator::MAX_LIGHT_S
            } else {
                light_s
            },
            reason,
        );
        Self::persist(&inner, &self.state_path())?;
        Ok(plan)
    }

    pub fn note_plan_ack(
        &self,
        mac: &str,
        plan_id: u64,
        remaining_s: u32,
        provisional: bool,
        now: u64,
    ) -> Value {
        let mut inner = self.inner.lock().unwrap();
        let Some(c) = inner.coordinators.get_mut(&mac.to_uppercase()) else {
            return json!({"outcome": "unknown_device"});
        };
        let outcome = c.note_plan_ack(plan_id, remaining_s, provisional, now);
        if outcome == crate::coordinator::PlanAck::Accepted {
            let _ = Self::persist(&inner, &self.state_path());
        }
        json!({
            "outcome": match outcome {
                crate::coordinator::PlanAck::Accepted => "accepted",
                crate::coordinator::PlanAck::Stale => "stale",
                crate::coordinator::PlanAck::Conflict => "conflict",
            }
        })
    }

    pub fn coordinator_summary(&self, mac: &str, now: u64) -> Option<Value> {
        let inner = self.inner.lock().unwrap();
        inner
            .coordinators
            .get(&mac.to_uppercase())
            .map(|c| c.summary(now))
    }

    // ---- Recovery / migration -------------------------------------------

    /// Minimal recovery read from a device digest: builds a profile draft with
    /// sync disabled. Never auto-wakes, claims, publishes or changes active.
    pub fn recovery_import(&self, mac: &str, digest: &Value, now: u64) -> Result<Profile> {
        let mac = mac.to_uppercase();
        let ids: Vec<String> = digest
            .get("template_ids")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .take(MAX_PROFILE_TEMPLATES)
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            bail!("device digest has no recoverable template order");
        }
        let active = digest
            .get("active_template_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let mut profile = Profile {
            device_mac: mac.clone(),
            template_ids: ids,
            render_target: None,
            font_ids: BTreeMap::new(),
            initial_active_id: active,
            bindings: Vec::new(),
            sync_enabled: false,
            full_sync_s: DEFAULT_FULL_SYNC_S,
            updated_at: now,
        };
        if let Some(active) = profile.initial_active_id.clone() {
            if !profile.template_ids.contains(&active) {
                profile.initial_active_id = profile.template_ids.first().cloned();
            }
        }
        self.profile_save(profile.clone(), now)?;
        Ok(profile)
    }

    // ---- Overview --------------------------------------------------------

    pub fn overview(&self) -> Value {
        let inner = self.inner.lock().unwrap();
        let mut pending = Vec::new();
        for (mac, c) in &inner.coordinators {
            if c.data.push_dirty {
                pending.push(json!({"device_mac": mac, "state": "data_pending"}));
            }
            if c.full_sync_due(crate::device_clock::wall_secs(&mac)) {
                pending.push(json!({"device_mac": mac, "state": "full_sync_due"}));
            }
            if let Some(job) = &c.job {
                match job.state {
                    PublishState::Waiting => {
                        pending.push(json!({"device_mac": mac, "state": "waiting_for_rendezvous", "job_id": job.job_id}))
                    }
                    PublishState::Sending => {
                        pending.push(json!({"device_mac": mac, "state": "publishing", "job_id": job.job_id}))
                    }
                    PublishState::Unknown => {
                        pending.push(json!({"device_mac": mac, "state": "publish_unknown", "job_id": job.job_id}))
                    }
                    _ => {}
                }
            }
            if c.session.display_state.as_deref() == Some("failed") {
                pending.push(json!({"device_mac": mac, "state": "applied_display_failed"}));
            }
            if pending
                .last()
                .and_then(|v| v.get("device_mac"))
                .and_then(|v| v.as_str())
                == Some(mac)
                && c.session.last_status_at.is_none()
            {
                pending.push(json!({"device_mac": mac, "state": "never_seen"}));
            }
        }
        let templates: Vec<Value> = inner
            .templates
            .values()
            .map(|t| {
                json!({
                    "template_id": t.key.template_id,
                    "render_target": t.key.render_target,
                    "source_crc": t.source_crc,
                    "compiled_crc": crate::compile::compiled_crc(&t.compiled),
                    "saved_at": t.saved_at,
                })
            })
            .collect();
        json!({
            "templates": templates,
            "devices": inner.devices.values().map(|d| json!({
                "device_mac": d.identity.device_mac,
                "name": d.identity.name,
                "hardware_verified": d.capabilities.hardware_verified,
                "profile_count": d.profile.as_ref().map(|p| p.template_ids.len()).unwrap_or(0),
            })).collect::<Vec<_>>(),
            "sources": inner.sources.values().map(|s| json!({
                "source_id": s.source_id, "kind": s.kind, "enabled": s.enabled,
                "quality": inner.snapshots.get(&s.source_id).map(|x| x.quality),
                "error": inner.snapshots.get(&s.source_id).and_then(|x| x.error.clone()),
            })).collect::<Vec<_>>(),
            "pending": pending,
            "jobs": inner.jobs.iter().rev().take(10).collect::<Vec<_>>(),
        })
    }

    fn refresh_all_contracts(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let macs: Vec<String> = inner.coordinators.keys().cloned().collect();
        for mac in macs {
            refresh_contract(&mut inner, &mac)?;
        }
        Ok(())
    }
}

/// A Wi-Fi job counts as window work while it still has a rendezvous-eligible
/// attempt left: not terminal, not blocked on user input or a conflict, inside
/// its TTL and past its retry backoff (W4-1). Dead/expired/blocked jobs must not
/// keep the radio on.
fn wifi_job_wants_window(job: &WifiJob, now: u64) -> bool {
    job.execution_active(now) && crate::rendezvous::admit_wifi(job, now)
}

fn collect_font_names(value: &Value, names: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(name) = map.get("font").and_then(Value::as_str) {
                if !names.iter().any(|n| n == name) { names.push(name.to_string()); }
            }
            for child in map.values() { collect_font_names(child, names); }
        }
        Value::Array(items) => for child in items { collect_font_names(child, names); },
        _ => {}
    }
}

fn job_id_for(mac: &str, now: u64, profile: &Profile) -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default().as_nanos();
    let fingerprint = format!("{:x}", sha2::Sha256::digest(format!("{mac}:{now}:{nanos}:{}", profile.template_ids.join(",")).as_bytes()));
    fingerprint[..24].to_string()
}

fn capacity_from_caps(caps: &DeviceCapabilities) -> Capacity {
    Capacity { max_object_bytes: caps.max_object_bytes,
        max_manifest_bytes: caps.max_manifest_bytes, free_bytes: caps.free_bytes,
        install_peak_bytes: caps.install_peak_bytes,
        filesystem_overhead_bytes: caps.filesystem_overhead_bytes }
}

fn context_from_json(value: &Value, next_seq: u64) -> Option<crate::coordinator::ContextState> {
    let mut ctx: crate::coordinator::ContextState = serde_json::from_value(value.clone()).ok()?;
    if ctx.context_id.is_empty() {
        return None;
    }
    ctx.next_seq = ctx.next_seq.max(next_seq);
    Some(ctx)
}

fn source_has_field(source: &DataSource, canonical: &str) -> bool {
    source
        .config
        .get("triggers")
        .and_then(|v| v.as_object())
        .map(|t| {
            t.keys().any(|k| {
                canonical == k
                    || canonical.ends_with(&format!(".{k}"))
                    || k == "usedPercent" && canonical.ends_with("usedPercent")
            })
        })
        .unwrap_or(false)
}

/// Never persist credentials: config is shown with secret-looking keys removed.
pub fn redact_config(config: &Value) -> Value {
    fn walk(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut out = serde_json::Map::new();
                for (k, v) in map {
                    let key = k.to_lowercase();
                    if key.contains("token")
                        || key.contains("password")
                        || key.contains("secret")
                        || key.contains("key")
                    {
                        out.insert(k.clone(), json!("<redacted>"));
                    } else {
                        out.insert(k.clone(), walk(v));
                    }
                }
                Value::Object(out)
            }
            Value::Array(arr) => Value::Array(arr.iter().map(walk).collect()),
            other => other.clone(),
        }
    }
    walk(config)
}

fn validate_template_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 16
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("invalid template id: use [A-Za-z0-9_-]{{1,16}}");
    }
    Ok(())
}

fn device_requirements(inner: &Inner, mac: &str, template_id: &str) -> Vec<FieldRequirement> {
    inner.devices.get(mac).and_then(|record| record.data_contract.as_ref())
        .filter(|contract| contract.template_id == template_id)
        .map(|contract| contract.requirements.clone()).unwrap_or_default()
}

/// Missing fields are meaningful only after their source has been read. Source
/// snapshots are volatile across Bridge restarts; the device's values are not.
fn data_sources_ready(inner: &Inner, mac: &str) -> bool {
    let Some(c) = inner.coordinators.get(mac) else { return false; };
    let profile = inner.devices.get(mac).and_then(|d| d.profile.as_ref());
    c.requirements.iter().all(|r| {
        let (canonical, _) = crate::datasource::resolve_field(&r.field);
        let bound_source = profile.and_then(|p|
            p.binding_for(c.active_template_id.as_deref().unwrap_or(""), &r.field)
                .or_else(|| p.binding_for("", &r.field)))
            .and_then(|b| inner.sources.get(&b.source_id));
        // Without an explicit binding, an initialized source that supplies the
        // field is sufficient (e.g. static-only displays without Codex).
        if bound_source.is_none() && c.sources.values().any(|s|
            s.last_success_at > 0 && s.fields.contains_key(&canonical)) {
            return true;
        }
        let source = bound_source.or_else(|| inner.sources.values()
            .find(|s| s.enabled && source_has_field(s, &canonical)))
            .or_else(|| inner.sources.values().find(|s|
                s.enabled && s.kind == DataSourceKind::Codex));
        match source {
            Some(s) => !s.enabled || c.sources.get(&s.source_id)
                .is_some_and(|snapshot| snapshot.last_success_at > 0),
            None => c.sources.values().any(|s| s.last_success_at > 0),
        }
    })
}

fn triggers_for(
    inner: &Inner,
    mac: &str,
    template_id: &str,
) -> BTreeMap<String, crate::platform::model::FieldTrigger> {
    let mut out = BTreeMap::new();
    let requirements = device_requirements(inner, mac, template_id);
    let profile = inner
        .devices
        .get(mac)
        .and_then(|d| d.profile.clone())
        .unwrap_or_else(|| Profile::draft(mac));
    for r in requirements.iter().filter(|r| !r.local) {
        let source = profile
            .bindings
            .iter()
            .find(|b| b.field == r.field)
            .and_then(|b| inner.sources.get(&b.source_id))
            .or_else(|| {
                let (canonical, _) = crate::datasource::resolve_field(&r.field);
                inner
                    .sources
                    .values()
                    .find(|s| s.enabled && source_has_field(s, &canonical))
            });
        if let Some(source) = source {
            let (canonical, _) = crate::datasource::resolve_field(&r.field);
            out.insert(r.field.clone(), source.trigger_for(&canonical));
        }
    }
    out
}

/// Set the active template contract on the coordinator (active-only delivery).
fn refresh_contract(inner: &mut Inner, mac: &str) -> Result<()> {
    let record = inner.devices.get(mac).context("unknown device")?;
    let sync_enabled = record.sync_enabled;
    let active_id = inner.coordinators.get(mac).and_then(|c| c.active_template_id.clone())
        .or_else(|| record.observed.active_template_id.clone());
    let requirements = active_id.as_deref().map(|id| device_requirements(inner, mac, id))
        .unwrap_or_default();
    let triggers = active_id.as_deref().map(|id| triggers_for(inner, mac, id))
        .unwrap_or_default();
    if let Some(c) = inner.coordinators.get_mut(mac) {
        c.sync_enabled = sync_enabled;
        let remote: Vec<_> = requirements.iter().filter(|r| !r.local).cloned().collect();
        if c.requirements != remote || c.triggers != triggers {
            c.set_contract(active_id.as_deref().unwrap_or(""), requirements, triggers);
        }
        c.active_template_id = active_id;
    }
    Ok(())
}

/// Helper used by the device page: compare the device's authoritative state
/// with the bridge's expected state (v2 §10 "预期 deep/等待会合/确认故障").
pub fn expectation_gap(summary: &Value) -> Value {
    let observed_ctx = summary
        .get("session")
        .and_then(|s| s.get("active_context_id"))
        .and_then(|v| v.as_str());
    let expected_ctx = summary
        .get("context")
        .and_then(|c| c.get("context_id"))
        .and_then(|v| v.as_str());
    let mut gaps = Vec::new();
    if observed_ctx.is_none() {
        gaps.push("device_never_seen");
    } else if observed_ctx != expected_ctx {
        gaps.push("context_mismatch");
    }
    if summary
        .get("job")
        .and_then(|j| j.get("state"))
        .and_then(|v| v.as_str())
        == Some("sending")
    {
        gaps.push("publish_in_progress");
    }
    json!({"gaps": gaps})
}

/// Compile a template into a source+compiled pair for a Bundle (test/helper).
pub fn compile_source(
    source: &Value,
    render_target: &str,
) -> Result<(Value, crate::compile::CompiledTemplate)> {
    let t = crate::compile::compile(source, render_target)?;
    Ok((source.clone(), t))
}

/// Collect the unique resources of a set of compiled templates.
pub fn collect_resources(templates: &[crate::compile::CompiledTemplate]) -> Vec<BundleResource> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for t in templates {
        for r in &t.resources {
            if seen.insert(r.id.clone()) {
                out.push(BundleResource {
                    id: r.id.clone(),
                    data: r.data.clone(),
                });
            }
        }
    }
    out
}

pub fn plan_mode(value: &str) -> Result<PlanMode> {
    match value {
        "sleep" => Ok(PlanMode::Sleep),
        "light" => Ok(PlanMode::Light),
        other => bail!("unknown plan mode {other}"),
    }
}

pub use crate::coordinator::new_context_id as context_id_for;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::DeliveryKind;
    use serde_json::json;

    fn service_with_device(dir: &Path) -> PlatformService {
        let svc = PlatformService::open(dir).unwrap();
        svc.device_upsert(
            DeviceIdentity::new("AA:BB:CC:DD:EE:FF", "Test").unwrap(),
            DeviceCapabilities::ssd1681_154g(),
        )
        .unwrap();
        svc
    }

    #[test]
    fn wifi_offline_intents_survive_restart_and_keep_mac_isolation() {
        let dir = tempfile::tempdir().unwrap(); let svc = service_with_device(dir.path());
        let a = "AABBCCDDEEFF"; let b = "001122334455";
        svc.device_upsert(DeviceIdentity::new(b, "Other").unwrap(), DeviceCapabilities::ssd1681_154g()).unwrap();
        let targets = serde_json::from_value(json!([{"network_id":"home","host_ssid":"Host","ssid":"Device"}])).unwrap();
        assert_eq!(svc.wifi_save(a, 0, targets, 100).unwrap().local_revision, 1);
        assert!(svc.wifi_save(a, 0, vec![], 101).is_err());
        assert!(svc.job(a).is_none()); assert!(svc.wifi_state(a).snapshot.is_none());
        let j = svc.wifi_queue(a, "installation", "read", vec![], 100).unwrap();
        assert_eq!(svc.wifi_queue(a, "installation", "read", vec![], 101).unwrap().job_id, j.job_id);
        let other = svc.wifi_queue(b, "installation", "read", vec![], 100).unwrap();
        assert_ne!(j.job_id, other.job_id);
        assert!(svc.wifi_begin(a, &j.job_id, 101).unwrap().is_some());
        drop(svc); let svc = PlatformService::open(dir.path()).unwrap();
        assert_eq!(svc.wifi_state(a).jobs[0].state, "waiting_rendezvous");
        svc.wifi_finish(a, &j.job_id, "waiting_rendezvous", Some("http_timeout"), 102).unwrap();
        assert!(svc.wifi_begin(a, &j.job_id, 106).unwrap().is_none());
        assert!(svc.wifi_begin(a, &j.job_id, 107).unwrap().is_some());
        svc.wifi_finish(a, &j.job_id, "cancelled", None, 108).unwrap();
        assert!(svc.wifi_begin(a, &j.job_id, 200).unwrap().is_none());
        assert_eq!(svc.wifi_state(b).jobs[0].state, "queued");
        assert!(svc.wifi_begin(b, &other.job_id, 86500).unwrap().is_none());
        assert_eq!(svc.wifi_state(b).jobs[0].state, "expired");
        let bytes = std::fs::read_to_string(dir.path().join("platform/state.json")).unwrap();
        assert!(!bytes.contains("password") && !bytes.contains("token"));
    }

    #[test]
    fn wifi_read_only_replaces_valid_complete_snapshot_and_import_checks_revision() {
        let dir = tempfile::tempdir().unwrap(); let svc = service_with_device(dir.path()); let mac = "AABBCCDDEEFF";
        let snapshot: WifiSnapshot = serde_json::from_value(json!({"schema":2,"device_mac":mac,"revision":3,
            "snapshot_id":"snap","complete":true,"observed_at":101,"networks":[{"bond":"own","writable":true,
            "network":{"network_id":"home","host_ssid":"Host","ssid":"Device"}}],"capacity":{"total":12}})).unwrap();
        let j = svc.wifi_queue(mac, "installation", "read", vec![], 100).unwrap();
        let mut partial = snapshot.clone(); partial.complete = false;
        assert!(svc.wifi_complete_read(mac, &j.job_id, partial, 101).is_err());
        assert!(svc.wifi_state(mac).snapshot.is_none());
        let mut wrong = snapshot.clone(); wrong.device_mac = "001122334455".into();
        assert!(svc.wifi_complete_read(mac, &j.job_id, wrong, 101).is_err());
        svc.wifi_complete_read(mac, &j.job_id, snapshot.clone(), 101).unwrap();
        assert!(svc.wifi_state(mac).targets.is_empty());
        svc.wifi_import(mac, &["home".into()], 0, 102).unwrap();
        let j = svc.wifi_queue(mac, "installation", "read_import", vec!["home".into()], 103).unwrap();
        svc.wifi_save(mac, 1, vec![], 104).unwrap();
        assert_eq!(svc.wifi_complete_read(mac, &j.job_id, snapshot, 105).unwrap().state, "conflict");
        assert!(svc.wifi_state(mac).targets.is_empty());
    }

    #[test]
    fn wifi_selected_fields_preserve_unrelated_changes_and_confirm_only_sent_version() {
        let dir = tempfile::tempdir().unwrap(); let svc = service_with_device(dir.path()); let mac="AABBCCDDEEFF";
        let mut snapshot: WifiSnapshot = serde_json::from_value(json!({"schema":2,"device_mac":mac,"revision":1,
            "snapshot_id":"one","complete":true,"observed_at":100,"networks":[{"bond":"own","writable":true,
                "network":{"network_id":"home","host_ssid":"Host","ssid":"Old","label":"Old label"}}],"capacity":{"total":12}})).unwrap();
        let read=svc.wifi_queue(mac,"bridge","read",vec![],100).unwrap();svc.wifi_complete_read(mac,&read.job_id,snapshot.clone(),101).unwrap();
        svc.wifi_import(mac,&["home".into()],0,102).unwrap();
        let mut targets=svc.wifi_state(mac).targets;targets[0].ssid="Approved".into();svc.wifi_save(mac,1,targets.clone(),103).unwrap();
        let fields=BTreeMap::from([("home".into(),vec!["ssid".into()])]);
        assert!(svc.wifi_queue_fields_checked(mac,"bridge","sync",vec!["home".into()],fields.clone(),Some(1),104).is_err());
        let job=svc.wifi_queue_fields_checked(mac,"bridge","sync",vec!["home".into()],fields,Some(2),104).unwrap();
        // Local edit after queueing must not change the frozen intent.
        targets[0].ssid="Later local draft".into();svc.wifi_save(mac,2,targets,105).unwrap();
        // The frozen cache is used; external revision changes must be rejected, never reread/rebase.
        let mut apply_count=0;
        let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&BTreeMap::new(),106,&mut |op: &str, body: Value| {
            if op=="apply" {
                assert!(body.get("permit").is_none(), "obsolete authorization must not enter the business request");
                apply_count+=1;assert_eq!(body["changes"][0]["ssid"],"Approved");
                assert_eq!(body["changes"][0]["label"],"Old label");assert_eq!(body["expected_revision"],1);
                snapshot.networks[0].network.ssid="Approved".into();snapshot.revision=2;
                Ok(json!({"ok":true,"outcome":"committed","revision":2}))
            } else { panic!("unexpected config reread: {op}") }
        })).unwrap().unwrap();
        assert_eq!(result.state,"completed");assert_eq!(apply_count,1);
        let state=svc.wifi_state(mac);assert_eq!(state.targets[0].ssid,"Later local draft");
        assert_eq!(state.baseline["home"]["ssid"],"Approved");assert_eq!(state.baseline["home"]["label"],"Old label");
        // A changed approved field pauses; no apply despite unrelated revision handling.
        let job=svc.wifi_queue_fields(mac,"bridge","sync",vec!["home".into()],BTreeMap::from([("home".into(),vec!["ssid".into()])]),107).unwrap();
        snapshot.networks[0].network.ssid="Changed by device".into();
        let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&BTreeMap::new(),108,&mut |op: &str, _: Value| {
            assert_eq!(op,"apply");Ok(json!({"ok":false,"error":"revision_conflict"}))
        })).unwrap().unwrap();assert_eq!(result.state,"conflict");
    }
    #[test]
    fn recovery_without_snapshot_is_atomic_and_lost_ack_is_query_only() {
        for outcome in ["committed", "lost", "conflict"] {
            let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
            let targets=serde_json::from_value(json!([{"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"replace","password":"password123"}])).unwrap();
            svc.wifi_save(mac,0,targets,100).unwrap();
            let job=svc.wifi_queue(mac,"bridge","recovery_save",vec!["home".into()],101).unwrap();
            let mut ops=vec![];
            let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&BTreeMap::new(),102,&mut |op: &str, body: Value| {
                ops.push(op.to_owned());
                if op=="active" {return Ok(json!({"ok":true,"revision":7}));}
                assert_eq!(op,"apply");assert_eq!(body["expected_revision"],7);assert_eq!(body["request_id"],job.request_id);
                assert_eq!(body["changes"].as_array().unwrap().len(),1);
                match outcome {"lost"=>anyhow::bail!("lost_ack"),"conflict"=>Ok(json!({"ok":false,"error":"revision_conflict"})),_=>Ok(json!({"ok":true,"outcome":"committed","revision":8,"bond":"own"}))}
            })).unwrap().unwrap();
            assert_eq!(ops,vec!["active","apply"]);
            assert_eq!(result.state,match outcome {"lost"=>"awaiting_confirmation","conflict"=>"conflict",_=>"completed"});
            if outcome=="lost" {
                let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&BTreeMap::new(),170,&mut |op: &str, _: Value| {
                    assert_eq!(op,"result");Ok(json!({"ok":true,"outcome":"committed","revision":8,"bond":"own"}))
                })).unwrap().unwrap();assert_eq!(result.state,"completed");
            }
        }
    }
    #[test]
    fn wifi_cancellation_input_expiry_and_unknown_receipt_never_reapply() {
        let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
        let snapshot:WifiSnapshot=serde_json::from_value(json!({"schema":2,"device_mac":mac,"revision":0,"snapshot_id":"snap",
            "complete":true,"observed_at":100,"networks":[],"capacity":{"total":12}})).unwrap();
        let read=svc.wifi_queue(mac,"bridge","read",vec![],100).unwrap();svc.wifi_complete_read(mac,&read.job_id,snapshot.clone(),101).unwrap();
        let targets=serde_json::from_value(json!([{"network_id":"new","host_ssid":"Host","ssid":"Device","credential":"replace"}])).unwrap();
        svc.wifi_save(mac,0,targets,102).unwrap();let j=svc.wifi_queue(mac,"bridge","sync",vec!["new".into()],103).unwrap();
        assert_eq!(ready(svc.wifi_attempt_async(mac,&j.job_id, &j.bridge_id, "own",false,&BTreeMap::new(),104,&mut |op: &str, _: Value|{assert_eq!(op,"snapshot");Ok(serde_json::to_value(&snapshot)?)})).unwrap().unwrap().state,"waiting_rendezvous");
        assert_eq!(ready(svc.wifi_attempt_async(mac,&j.job_id, &j.bridge_id, "own",true,&BTreeMap::new(),110,&mut |op: &str, _: Value|{assert_eq!(op,"snapshot");Ok(serde_json::to_value(&snapshot)?)})).unwrap().unwrap().state,"waiting_input");
        let input=BTreeMap::from([("new".into(),(0..12).map(|i|char::from(b'a'+i)).collect::<String>())]);
        let result=ready(svc.wifi_attempt_async(mac,&j.job_id, &j.bridge_id, "own",true,&input,126,&mut |op: &str, _: Value|{
            assert_eq!(op,"apply");anyhow::bail!("lost_ack")
        })).unwrap().unwrap();assert_eq!(result.state,"awaiting_confirmation");
        drop(svc);let svc=PlatformService::open(dir.path()).unwrap();
        let mut matching=snapshot.clone();
        matching.networks=serde_json::from_value(json!([{"bond":"own","writable":true,"network":{"network_id":"new","host_ssid":"Host","ssid":"Device","credential":"device"}}])).unwrap();
        let result=ready(svc.wifi_attempt_async(mac,&j.job_id, &j.bridge_id, "own",true,&input,187,&mut |op: &str, _: Value|{
            if op=="result"{Ok(json!({"ok":true,"outcome":"unknown"}))}else{panic!("unexpected config reread: {op}")}
        })).unwrap().unwrap();assert_eq!(result.state,"awaiting_confirmation");assert_eq!(result.error_code.as_deref(),Some("receipt_unknown"));
        // A definite rejected receipt cannot be relabelled as an unknown success.
        let result=ready(svc.wifi_attempt_async(mac,&j.job_id, &j.bridge_id, "own",false,&BTreeMap::new(),248,&mut |op: &str, _: Value| {
            assert_eq!(op,"result");Ok(json!({"ok":true,"outcome":"rejected","error":"capacity"}))
        })).unwrap().unwrap();assert_eq!(result.state,"failed");assert!(!result.may_have_committed);
        let cancelled=svc.wifi_queue(mac,"bridge","sync",vec!["new".into()],249).unwrap();
        drop(svc);let svc=PlatformService::open(dir.path()).unwrap();
        assert_eq!(svc.wifi_state(mac).jobs.iter().find(|x|x.job_id==cancelled.job_id).unwrap().state,"waiting_input");
        svc.wifi_mark_sent(mac,&cancelled.job_id,249).unwrap();
        assert!(svc.wifi_finish(mac,&cancelled.job_id,"cancelled",None,250).unwrap().may_have_committed);
        assert!(ready(svc.wifi_attempt_async(mac,&cancelled.job_id, &cancelled.bridge_id, "own",false,&input,251,&mut |_: &str, _: Value|panic!("cancelled task sent"))).unwrap().is_none());
        let next=svc.wifi_queue(mac,"bridge","read",vec![],300).unwrap();
        assert!(ready(svc.wifi_attempt_async(mac,&next.job_id, &next.bridge_id, "own",false,&BTreeMap::new(),86700,&mut |_: &str, _: Value|panic!("expired task sent"))).unwrap().is_none());
    }

    fn quad_source() -> Value {
        serde_json::from_str(include_str!(
            "../../../../../tools/test-bridge/templates/quad.json"
        ))
        .unwrap()
    }

    fn mini_source() -> Value {
        serde_json::from_str(include_str!(
            "../../../../../tools/test-bridge/templates/mini.json"
        ))
        .unwrap()
    }

    #[test]
    fn device_reported_fields_work_without_local_template_or_profile_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "AABBCCDDEEFF";
        svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(),
            DeviceCapabilities::ssd1681_154g()).unwrap();
        let requirements = vec![FieldRequirement { index: 3, field: "account.plan".into(),
            kind: crate::platform::model::FieldKind::Text,
            missing: crate::platform::model::MissingPolicy::Hide, local: false }];
        let status = json!({"device_mac":mac, "configured":true,
            "active_context_id":"device-context", "active_template_id":"device-only",
            "applied_seq":99,
            "data_contract":{"template_id":"device-only", "compiler_abi":2,
                "source_crc":"12345678", "requirements":requirements}});
        svc.note_device_status(mac, &status).unwrap();
        svc.set_data_sync(mac, true).unwrap();
        svc.note_codex_envelope(&json!({"schema":1,"account":{"plan":"plus"}})).unwrap();
        assert_eq!(svc.inner.lock().unwrap().coordinators[mac].requirements, requirements);
        drop(svc);
        let svc = PlatformService::open(dir.path()).unwrap();
        svc.note_device_status(mac, &status).unwrap();
        svc.note_codex_envelope(&json!({"schema":1,"account":{"plan":"plus"}})).unwrap();
        assert!(svc.inner.lock().unwrap().coordinators[mac].sync_enabled);
        assert!(svc.device_get(mac).unwrap()["profile"].is_null());
        assert_eq!(svc.next_delivery(mac, true, crate::now_secs())["decision"], "ble_data");
        let body = svc.data_message_body(mac).unwrap();
        assert!(body["seq"].as_u64().unwrap() > 99);
        assert_eq!(body["fields"].as_array().unwrap().len(), 1);
        assert_eq!(body["fields"][0]["i"], 3);
        assert_eq!(body["fields"][0]["k"], "account.plan");
        assert_eq!(body["fields"][0]["v"], "plus");
    }


    #[test]
    fn same_context_device_data_loss_replays_after_bridge_restart() {
        for restart in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let svc = PlatformService::open(dir.path()).unwrap();
            let mac = "AABBCCDDEEFF";
            let now = crate::now_secs();
            svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(),
                DeviceCapabilities::ssd1681_154g()).unwrap();
            let mut status = json!({"device_mac":mac,"configured":true,"boot_id":"old",
                "active_context_id":"same-context","active_template_id":"device-only","data_seq":0,
                "data_contract":{"template_id":"device-only","compiler_abi":2,
                    "source_crc":"12345678","requirements":[
                        {"index":0,"field":"account.plan","kind":"text","missing":"hide","local":false}]}});
            let envelope = json!({"schema":1,"account":{"plan":"plus"}});
            svc.note_device_status_at(mac,&status,"http",now).unwrap();
            svc.set_data_sync(mac,true).unwrap();
            svc.note_codex_envelope(&envelope).unwrap();
            svc.next_http_delivery(mac,true,now);
            let first = svc.data_message_body(mac).unwrap();
            let mut seq = first["seq"].as_u64().unwrap();
            assert_eq!(svc.note_ack(mac,DeliveryKind::LightData,seq,first["crc"].as_str().unwrap(),true,"displayed")["outcome"],"applied");
            let svc = if restart {drop(svc);PlatformService::open(dir.path()).unwrap()} else {svc};
            status["data_seq"]=json!(seq);
            svc.note_device_status_at(mac,&status,"http",now+1).unwrap();
            svc.note_codex_envelope(&envelope).unwrap();
            if restart && svc.next_http_delivery(mac,true,now+1)["decision"] == "light_data" {
                // Startup may reconfirm the cached source; establish its ACK
                // before testing the later device-only reboot.
                let initial=svc.data_message_body(mac).unwrap();seq=initial["seq"].as_u64().unwrap();
                assert_eq!(svc.note_ack(mac,DeliveryKind::LightData,seq,initial["crc"].as_str().unwrap(),true,"displayed")["outcome"],"applied");
            }
            assert_eq!(svc.next_http_delivery(mac,true,now+1)["decision"],"idle");
            let before=svc.coordinator_summary(mac,now+1).unwrap();
            status["boot_id"]=json!("new");status["data_seq"]=json!(0);
            svc.note_device_status_at(mac,&status,"http",now+2).unwrap();
            let after=svc.coordinator_summary(mac,now+2).unwrap();
            assert_eq!(after["context"]["full_sync_deadline"],before["context"]["full_sync_deadline"]);
            assert_eq!(after["plan"],before["plan"]);
            assert_eq!(svc.next_http_delivery(mac,true,now+2)["decision"],"light_data");
            let replay=svc.data_message_body(mac).unwrap();
            assert_eq!(replay["fields"],first["fields"]);
            assert!(replay["seq"].as_u64().unwrap()>seq);
            svc.note_device_status_at(mac,&status,"http",now+3).unwrap();
            svc.next_http_delivery(mac,true,now+3);
            assert_eq!(svc.data_message_body(mac).unwrap(),replay);
            let replay_seq=replay["seq"].as_u64().unwrap();
            assert_eq!(svc.note_ack(mac,DeliveryKind::LightData,replay_seq,replay["crc"].as_str().unwrap(),true,"displayed")["outcome"],"applied");
            status["data_seq"]=json!(replay_seq);
            svc.note_device_status_at(mac,&status,"http",now+4).unwrap();
            assert_eq!(svc.next_http_delivery(mac,true,now+4)["decision"],"idle");
        }
    }

    #[test]
    fn remaining_changes_push_without_local_profile_after_ack_and_restart() {
        for ble in [false, true] {
            for restart in [false, true] {
                check_remaining_delivery(ble, restart, None);
            }
        }
    }

    #[test]
    fn remaining_trigger_overrides_survive_without_local_profile() {
        for ble in [false, true] {
            for restart in [false, true] {
                for trigger in ["push", "pull"] {
                    check_remaining_delivery(ble, restart, Some(trigger));
                }
            }
        }
    }

    fn first_contact_service(dir: &Path) -> PlatformService {
        let svc = PlatformService::open(dir).unwrap();
        let mac = "AABBCCDDEEFF";
        svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(),
            DeviceCapabilities::ssd1681_154g()).unwrap();
        svc.note_device_status(mac, &json!({"device_mac":mac,"configured":true,
            "active_context_id":"first-contact","active_template_id":"device-only",
            "data_contract":{"template_id":"device-only","compiler_abi":2,
                "source_crc":"12345678","requirements":[
                    {"index":0,"field":"account.plan","kind":"text","missing":"hide","local":false},
                    {"index":1,"field":"buckets[codex].weekly.remaining","kind":"number","missing":"hide","local":false}
                ]}})).unwrap();
        svc.set_data_sync(mac, true).unwrap();
        svc
    }

    #[test]
    fn restart_first_contact_waits_for_source_before_pinning_data() {
        let mac = "AABBCCDDEEFF";
        let env = json!({"account":{"plan":"plus"},"buckets":[{"id":"codex",
            "windows":[{"usedPercent":0,"windowMins":10080}]}]});
        for ble in [false, true] {
            for missing in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let svc = first_contact_service(dir.path());
                let now = crate::now_secs();
                let deliver = |svc: &PlatformService| if ble {
                    svc.next_delivery(mac, true, now)
                } else { svc.next_http_delivery(mac, true, now) };
                let kind = if ble { DeliveryKind::BleData } else { DeliveryKind::LightData };
                svc.note_codex_envelope(&env).unwrap();
                let first = deliver(&svc);
                assert_eq!(svc.note_ack(mac, kind, first["data_seq"].as_u64().unwrap(),
                    first["content_crc"].as_str().unwrap(), true, "displayed")["outcome"], "applied");
                drop(svc);
                let svc = PlatformService::open(dir.path()).unwrap();
                let before = svc.coordinator_summary(mac, now).unwrap();
                for probe in [false, true] {
                    if probe {
                        assert_eq!(svc.data_probe("codex").unwrap().last_success_at, 0);
                    }
                    let waiting = deliver(&svc);
                    assert_eq!(waiting["decision"], "waiting_for_data_source", "ble={ble}, {waiting}");
                    assert_eq!(waiting["next_stage"], "read_data_source");
                    assert!(svc.data_message_body(mac).is_none());
                    let waiting_state = svc.coordinator_summary(mac, now).unwrap();
                    assert_eq!(waiting_state["context"], before["context"]);
                    assert_eq!(waiting_state["acked_push_fp"], before["acked_push_fp"]);
                    assert_eq!(waiting_state["acked_full_fp"], before["acked_full_fp"]);
                    assert_eq!(svc.plan_for_rendezvous(mac, now, "rendezvous", 0).unwrap().mode, PlanMode::Sleep);
                }
                // A successful read with absent fields is business missing, not startup.
                let ready_env = if missing { json!({}) } else { env.clone() };
                svc.note_codex_envelope(&ready_env).unwrap();
                let ready = deliver(&svc);
                assert_eq!(ready["decision"], if ble { "ble_data" } else { "light_data" });
                let body = svc.data_message_body(mac).unwrap();
                assert_eq!(body["fields"][1]["v"], if missing { Value::Null } else { json!(100) });
                assert_eq!(body["fields"][1]["q"], if missing { "missing" } else { "good" });
                assert_eq!(svc.note_ack(mac, kind, ready["data_seq"].as_u64().unwrap(),
                    ready["content_crc"].as_str().unwrap(), true, "displayed")["outcome"], "applied");
                assert_eq!(deliver(&svc)["decision"], "idle");
            }
        }
    }

    #[test]
    fn first_contact_static_source_and_partial_readiness() {
        for ble in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let svc = first_contact_service(dir.path());
            let mac = "AABBCCDDEEFF";
            let now = crate::now_secs();
            let deliver = || if ble { svc.next_delivery(mac, true, now) }
                else { svc.next_http_delivery(mac, true, now) };
            svc.data_source_save(DataSource { source_id:"static".into(), kind:DataSourceKind::StaticJson,
                config:json!({"values":{"account.plan":"static"}}), credential_ref:None, enabled:true }).unwrap();
            svc.data_probe("static").unwrap();
            assert_eq!(deliver()["decision"], "waiting_for_data_source");
            assert_eq!(svc.plan_for_rendezvous(mac, now, "rendezvous", 0).unwrap().mode, PlanMode::Sleep);
            svc.data_source_save(DataSource { source_id:"static".into(), kind:DataSourceKind::StaticJson,
                config:json!({"values":{"account.plan":"static","buckets[codex].weekly.usedPercent":0}}),
                credential_ref:None, enabled:true }).unwrap();
            svc.data_probe("static").unwrap();
            assert!(!svc.codex_envelope_available());
            assert_eq!(deliver()["decision"], if ble { "ble_data" } else { "light_data" });
            let body = svc.data_message_body(mac).unwrap();
            assert_eq!(body["fields"][0]["v"], "static");
            assert_eq!(body["fields"][1]["v"], 100);
        }
    }

    #[test]
    fn source_wait_preserves_original_unconfirmed_data() {
        for ble in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let svc = first_contact_service(dir.path());
            let mac = "AABBCCDDEEFF";
            let now = crate::now_secs();
            let deliver = || if ble { svc.next_delivery(mac, true, now) }
                else { svc.next_http_delivery(mac, true, now) };
            svc.note_codex_envelope(&json!({"account":{"plan":"plus"}})).unwrap();
            let original = deliver();
            let body = svc.data_message_body(mac).unwrap();
            // A failed refresh must not replace an already pinned transaction.
            svc.inner.lock().unwrap().codex_envelope = None;
            svc.data_probe("codex").unwrap();
            let retry = deliver();
            assert_eq!(retry["data_seq"], original["data_seq"]);
            assert_eq!(retry["content_crc"], original["content_crc"]);
            assert_eq!(svc.data_message_body(mac).unwrap(), body);
        }
    }

    #[test]
    fn first_contact_bound_static_source_can_report_real_missing() {
        for ble in [false, true] {
            for other_source_ready in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let svc = first_contact_service(dir.path());
                let mac = "AABBCCDDEEFF";
                let now = crate::now_secs();
                let deliver = || if ble { svc.next_delivery(mac, true, now) }
                    else { svc.next_http_delivery(mac, true, now) };
                svc.data_source_save(DataSource { source_id:"static".into(), kind:DataSourceKind::StaticJson,
                    config:json!({"values":{}}), credential_ref:None, enabled:true }).unwrap();
                {
                    let mut inner = svc.inner.lock().unwrap();
                    let mut profile = Profile::draft(mac);
                    for r in &inner.coordinators[mac].requirements {
                        // Active-template bindings take precedence over generic ones.
                        for (template_id, source_id) in [("", "codex"), ("device-only", "static")] {
                            profile.bindings.push(Binding {
                                template_id:template_id.into(), field:r.field.clone(), source_id:source_id.into(),
                                source_field:crate::datasource::resolve_field(&r.field).0,
                            });
                        }
                    }
                    inner.devices.get_mut(mac).unwrap().profile = Some(profile);
                }
                if other_source_ready {
                    svc.note_codex_envelope(&json!({"account":{"plan":"plus"}})).unwrap();
                }
                assert_eq!(deliver()["decision"], "waiting_for_data_source");
                assert_eq!(svc.plan_for_rendezvous(mac, now, "rendezvous", 0).unwrap().mode, PlanMode::Sleep);
                svc.data_probe("static").unwrap();
                assert_eq!(deliver()["decision"], if ble { "ble_data" } else { "light_data" });
                let body = svc.data_message_body(mac).unwrap();
                assert!(body["fields"][1]["v"].is_null());
                assert_eq!(body["fields"][1]["q"], "missing");
            }
        }
    }

    fn check_remaining_delivery(ble: bool, restart: bool, trigger: Option<&str>) {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "AABBCCDDEEFF";
        let now = crate::now_secs();
        svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(),
            DeviceCapabilities::ssd1681_154g()).unwrap();
        if let Some(trigger) = trigger {
            svc.data_source_save(DataSource {
                source_id: "codex".into(), kind: DataSourceKind::Codex,
                config: json!({"triggers":{"usedPercent":trigger}}),
                credential_ref: None, enabled: true,
            }).unwrap();
        }
        svc.note_device_status_at(mac, &json!({
            "device_mac":mac, "configured":true,
            "active_context_id":"device-context", "active_template_id":"device-only",
            "data_contract":{"template_id":"device-only", "compiler_abi":2,
                "source_crc":"12345678", "requirements":[
                    {"index":8, "field":"buckets[codex].weekly.remaining",
                        "kind":"number", "missing":"hide", "local":false},
                    {"index":9, "field":"buckets[codex].weekly.resetsAt",
                        "kind":"number", "missing":"hide", "local":false}
                ]}
        }), "http", now).unwrap();
        svc.set_data_sync(mac, true).unwrap();
        let envelope = |used, reset| json!({"buckets":[{"id":"codex","windows":[
            {"kind":"weekly","usedPercent":used,"resetsAt":reset,"windowMins":10080}
        ]}]});
        let delivery = |svc: &PlatformService, at| if ble {
            svc.next_delivery(mac, true, at)
        } else {
            svc.next_http_delivery(mac, true, at)
        };
        let expected = if ble { "ble_data" } else { "light_data" };
        let kind = if ble { DeliveryKind::BleData } else { DeliveryKind::LightData };
        let ack = |svc: &PlatformService, decision: &Value| {
            let body = svc.data_message_body(mac).unwrap();
            assert_eq!(svc.note_ack(mac, kind, body["seq"].as_u64().unwrap(),
                decision["content_crc"].as_str().unwrap(), true, "displayed")["outcome"], "applied");
        };

        svc.note_codex_envelope(&envelope(7, now + 86400)).unwrap();
        let first = delivery(&svc, now);
        assert_eq!(first["decision"], expected);
        assert_eq!(svc.data_message_body(mac).unwrap()["fields"][0]["v"], 93);
        ack(&svc, &first);
        let svc = if restart {
            drop(svc);
            PlatformService::open(dir.path()).unwrap()
        } else { svc };
        assert!(svc.device_get(mac).unwrap()["profile"].is_null());
        assert!(svc.templates().is_empty());
        svc.note_codex_envelope(&envelope(7, now + 86400)).unwrap();
        let resumed = delivery(&svc, now + 1);
        if restart && resumed["decision"] == expected {
            // Startup may reconfirm the cached snapshot; test subsequent changes
            // against its successful ACK, rather than assuming no startup replay.
            assert_eq!(svc.data_message_body(mac).unwrap()["fields"][0]["v"], 93);
            ack(&svc, &resumed);
        } else {
            assert_eq!(resumed["decision"], "idle");
        }
        let confirmed = svc.coordinator_summary(mac, now).unwrap();
        assert_eq!(delivery(&svc, now + 1)["decision"], "idle");

        // Pull-only metadata changes stay cached and cannot extend the PowerPlan.
        let plan = serde_json::to_value(&svc.inner.lock().unwrap().coordinators[mac].plan).unwrap();
        svc.note_codex_envelope(&envelope(7, now + 86500)).unwrap();
        let pull = svc.coordinator_summary(mac, now + 2).unwrap();
        assert_eq!(pull["push_dirty"], false);
        assert_eq!(pull["pull_only_change"], true);
        assert_eq!(delivery(&svc, now + 2)["decision"], "idle");
        assert_eq!(serde_json::to_value(&svc.inner.lock().unwrap().coordinators[mac].plan).unwrap(), plan);
        assert_eq!(pull["context"]["full_sync_deadline"],
            confirmed["context"]["full_sync_deadline"]);

        svc.note_codex_envelope(&envelope(8, now + 86500)).unwrap();
        let changed = svc.coordinator_summary(mac, now + 3).unwrap();
        let should_push = trigger != Some("pull");
        assert_eq!(changed["push_dirty"], should_push,
            "ble={ble}, restart={restart}, override={trigger:?}");
        assert_eq!(changed["acked_push_fp"], confirmed["acked_push_fp"]);
        let next = if should_push {
            delivery(&svc, now + 3)
        } else {
            assert_eq!(changed["pull_only_change"], true);
            assert_eq!(delivery(&svc, now + 3)["decision"], "idle");
            delivery(&svc, changed["context"]["full_sync_deadline"].as_u64().unwrap() + 1)
        };
        assert_eq!(next["decision"], expected);
        let body = svc.data_message_body(mac).unwrap();
        assert_eq!(body["fields"][0]["v"], 92);
        assert_eq!(body["fields"][1]["v"], now + 86500);
        ack(&svc, &next);
        assert_eq!(delivery(&svc, now + 4)["decision"], "idle");
    }


    #[test]
    fn device_context_change_requeues_unchanged_remaining_after_ack() {
        for ble in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let svc = PlatformService::open(dir.path()).unwrap();
            let mac = "AABBCCDDEEFF";
            let now = crate::now_secs();
            svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(),
                DeviceCapabilities::ssd1681_154g()).unwrap();
            let mut status = json!({"device_mac":mac, "configured":true,
                "active_context_id":"before-reboot", "active_template_id":"device-only",
                "data_seq":0, "applied_seq":0,
                "data_contract":{"template_id":"device-only","compiler_abi":2,
                    "source_crc":"12345678","requirements":[{"index":8,
                        "field":"buckets[codex].weekly.remaining","kind":"number",
                        "missing":"hide","local":false}]}});
            svc.note_device_status_at(mac, &status, "http", now).unwrap();
            svc.set_data_sync(mac, true).unwrap();
            svc.note_codex_envelope(&json!({"buckets":[{"id":"codex","windows":[
                {"kind":"weekly","usedPercent":7,"windowMins":10080}]}]})).unwrap();
            let delivery = |at| if ble { svc.next_delivery(mac, true, at) }
                else { svc.next_http_delivery(mac, true, at) };
            let kind = if ble { DeliveryKind::BleData } else { DeliveryKind::LightData };
            let expected = if ble { "ble_data" } else { "light_data" };
            let first = delivery(now);
            let old_seq = first["data_seq"].as_u64().unwrap();
            let old_crc = first["content_crc"].as_str().unwrap();
            assert_eq!(svc.note_ack(mac, kind, old_seq, old_crc, true, "displayed")["outcome"], "applied");
            assert_eq!(delivery(now + 1)["decision"], "idle");
            status["active_context_id"] = json!("after-reboot");
            svc.note_device_status_at(mac, &status, "http", now + 2).unwrap();
            let next = delivery(now + 2);
            assert_eq!(next["decision"], expected);
            assert!(next["data_seq"].as_u64().unwrap() > old_seq);
            let body = svc.data_message_body(mac).unwrap();
            assert_eq!(body["active_context_id"], "after-reboot");
            assert_eq!(body["fields"][0]["v"], 93);
            assert_eq!(svc.note_ack(mac, kind, old_seq, old_crc, true, "displayed")["outcome"], "stale");
            assert_eq!(svc.note_ack(mac, kind, next["data_seq"].as_u64().unwrap(),
                next["content_crc"].as_str().unwrap(), true, "displayed")["outcome"], "applied");
            assert_eq!(delivery(now + 3)["decision"], "idle");
        }
    }

    #[test]
    fn single_copy_status_refreshes_capacity_and_clears_contract_in_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "AABBCCDDEEFF";
        let mut caps = DeviceCapabilities::ssd1681_154g();
        caps.max_bundle_bytes = 800_000;
        assert_eq!(caps.bundle_limit(), 256 * 1024); // old firmware stays bounded
        svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(), caps).unwrap();
        let mut status = json!({"device_mac":mac,"configured":true,
            "bundle_storage_protocol":2,"max_bundle_bytes":700_000,
            "bundle_compression":1,"max_bundle_decoded_bytes":2*1024*1024,
            "active_context_id":"ctx-a","active_template_id":"quad",
            "data_contract":{"template_id":"quad","compiler_abi":2,"source_crc":"12345678",
                "requirements":[{"index":0,"field":"account.plan","kind":"text","missing":"hide","local":false}]}});
        svc.note_device_status_at(mac, &status, "http", 100).unwrap();
        assert_eq!(svc.inner.lock().unwrap().devices[mac].capabilities.bundle_limit(), 700_000);
        assert_eq!(svc.inner.lock().unwrap().devices[mac].capabilities.bundle_compression, 1);
        status["configured"] = json!(false);
        status["max_bundle_bytes"] = json!(0);
        status["data_contract"] = Value::Null;
        status["active_context_id"] = json!(""); status["active_template_id"] = json!("");
        svc.note_device_status_at(mac, &status, "http", 101).unwrap();
        assert!(svc.device_get(mac).unwrap()["data_contract"].is_null());
        assert_eq!(svc.inner.lock().unwrap().devices[mac].capabilities.bundle_limit(), 0);
        status["bundle_compression"] = json!(99);
        assert!(svc.note_device_status_at(mac, &status, "http", 102).is_err());
        status["bundle_compression"] = json!(1);
        status["bundle_storage_protocol"] = json!(99);
        assert!(svc.note_device_status_at(mac, &status, "http", 102).is_err());
    }

    #[test]
    fn device_contract_ignores_local_drafts_and_expires_on_ble_context_switch() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "AABBCCDDEEFF";
        svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(),
            DeviceCapabilities::ssd1681_154g()).unwrap();
        let mut status = json!({"device_mac":mac,"configured":true,
            "active_context_id":"ctx-a","active_template_id":"quad",
            "data_contract":{"template_id":"quad","compiler_abi":2,"source_crc":"12345678",
                "requirements":[{"index":4,"field":"account.plan","kind":"text",
                    "missing":"hide","local":false}]}});
        svc.note_device_status_at(mac, &status, "http", 100).unwrap();
        svc.set_data_sync(mac, true).unwrap();
        let before = svc.device_get(mac).unwrap()["data_contract"].clone();
        svc.template_save("quad", crate::platform::model::RENDER_TARGET_154G,
            &quad_source(), 101).unwrap();
        let mut profile = Profile::draft(mac);
        profile.template_ids = vec!["quad".into()];
        profile.initial_active_id = Some("quad".into());
        profile.sync_enabled = true;
        svc.profile_save(profile, 102).unwrap();
        assert_eq!(svc.device_get(mac).unwrap()["data_contract"], before);
        assert_eq!(svc.inner.lock().unwrap().coordinators[mac].requirements.len(), 1);
        let mut invalid = status.clone();
        invalid["data_contract"]["requirements"][0]["index"] = json!(32);
        assert!(svc.note_device_status_at(mac, &invalid, "http", 103).is_err());
        assert_eq!(svc.device_get(mac).unwrap()["data_contract"], before);
        invalid = status.clone();
        invalid["data_contract"]["template_id"] = json!("other");
        assert!(svc.note_device_status_at(mac, &invalid, "http", 103).is_err());
        assert_eq!(svc.device_get(mac).unwrap()["data_contract"], before);
        status.as_object_mut().unwrap().remove("data_contract");
        svc.note_device_status_at(mac, &status, "ble", 104).unwrap();
        assert_eq!(svc.device_get(mac).unwrap()["data_contract"], before);
        status["status_projection"] = json!("compact");
        svc.note_device_status_at(mac, &status, "http", 104).unwrap();
        assert_eq!(svc.device_get(mac).unwrap()["data_contract"], before);
        status["active_context_id"] = json!("ctx-b");
        svc.note_device_status_at(mac, &status, "http", 105).unwrap();
        assert!(svc.device_get(mac).unwrap()["data_contract"].is_null());
        assert_eq!(svc.next_delivery(mac, true, 105)["decision"], "waiting_for_device_contract");
        assert_eq!(svc.plan_for_rendezvous(mac, 105, "rendezvous", 0).unwrap().mode, PlanMode::Light);
    }

    #[test]
    fn publish_ack_invalidates_same_template_contract_until_authenticated_readback() {
        let dir=tempfile::tempdir().unwrap();
        let svc=PlatformService::open(dir.path()).unwrap();
        let mac="AABBCCDDEEFF";
        svc.device_upsert(DeviceIdentity::new(mac,"Test").unwrap(),DeviceCapabilities::ssd1681_154g()).unwrap();
        let mut status=json!({"device_mac":mac,"configured":true,
            "active_context_id":"old","active_template_id":"quad",
            "data_contract":{"template_id":"quad","compiler_abi":2,"source_crc":"12345678",
                "requirements":[{"index":4,"field":"account.plan","kind":"text","missing":"hide","local":false}]}});
        svc.note_device_status_at(mac,&status,"http",100).unwrap();
        svc.set_data_sync(mac,true).unwrap();
        svc.adopt_activation_context(mac,"published",101).unwrap();
        assert!(svc.device_get(mac).unwrap()["data_contract"].is_null());
        assert_eq!(svc.next_http_delivery(mac,true,101)["decision"],"waiting_for_device_contract");
        drop(svc);
        let svc=PlatformService::open(dir.path()).unwrap();
        assert!(svc.device_get(mac).unwrap()["data_contract"].is_null());
        status["active_context_id"]=json!("published");
        status["data_contract"]["source_crc"]=json!("87654321");
        status["data_contract"]["requirements"][0]["index"]=json!(5);
        svc.note_device_status_at(mac,&status,"http",102).unwrap();
        svc.adopt_activation_context(mac,"published",103).unwrap();
        assert_eq!(svc.device_get(mac).unwrap()["data_contract"]["requirements"][0]["index"],5);
        assert_eq!(svc.inner.lock().unwrap().coordinators[mac].requirements[0].index,5);
    }

    #[test]
    fn authenticated_snapshot_survives_failed_attempt_and_restart() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "AABBCCDDEEFF";
        svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(), DeviceCapabilities::ssd1681_154g()).unwrap();
        let body = json!({"device_mac": mac, "fw": "test", "power": {"battery": 45}});
        svc.note_authenticated_status(mac, &body).unwrap();
        let before = svc.device_get(mac).unwrap()["last_authenticated"].clone();
        svc.note_status_attempt(mac, "offline", Some("timeout")).unwrap();
        assert_eq!(svc.device_get(mac).unwrap()["last_authenticated"], before);
        svc.note_ble_contact(mac).unwrap();
        assert_eq!(svc.device_get(mac).unwrap()["last_authenticated"], before);
        assert_eq!(svc.device_get(mac).unwrap()["last_authenticated_transport"], "ble");
        assert!(svc.note_authenticated_status(mac, &json!({"device_mac": "FFFFFFFFFFFF"})).is_err());
        drop(svc);
        let resumed = PlatformService::open(dir.path()).unwrap();
        assert_eq!(resumed.device_get(mac).unwrap()["last_authenticated"], before);
        assert_eq!(resumed.device_get(mac).unwrap()["last_status_attempt"]["outcome"], "offline");
    }

    #[test]
    fn http_receipt_and_digest_share_time_ble_keeps_wifi_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "AABBCCDDEEFF";
        svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(),
            DeviceCapabilities::ssd1681_154g()).unwrap();
        let status = json!({"device_mac":mac,"fw":"1.0","power":{"battery":0},
            "active_template_id":"quad"});
        svc.note_device_status_at(mac, &status, "http", 100).unwrap();
        svc.note_authenticated_status_at(mac, &status, 100).unwrap();
        let wifi = svc.device_get(mac).unwrap();
        assert_eq!(wifi["last_authenticated"]["observed_at"], 100);
        assert_eq!(wifi["last_authenticated"]["body"]["field_samples"]["fw"]["received_at"], 100);
        assert_eq!(wifi["record_observed"]["transport"], "http");
        assert_eq!(wifi["power"]["observed_at"], 100);
        svc.note_device_status_at(mac, &json!({"device_mac":mac,
            "active_template_id":"other"}), "ble", 101).unwrap();
        svc.note_ble_contact_at(mac, 101).unwrap();
        let ble = svc.device_get(mac).unwrap();
        assert_eq!(ble["last_authenticated"], wifi["last_authenticated"]);
        assert_eq!(ble["record_observed"]["transport"], "ble");
        assert_eq!(ble["record_observed"]["fw"], Value::Null);
        assert_eq!(ble["last_authenticated_contact_at"], 101);
        assert_eq!(ble["power"]["observed_at"], 100);
        svc.note_device_status_at(mac, &json!({"device_mac":mac,
            "power":{"mode":"light","remaining_s":30}}), "ble", 102).unwrap();
        let powered = svc.device_get(mac).unwrap();
        assert_eq!(powered["power"]["observed_at"], 102);
        assert_eq!(powered["power"]["observed_transport"], "ble");
    }

    #[test]
    fn sync_v1_samples_preserve_omitted_groups_and_zero_values() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "0200000000F1";
        svc.device_upsert(DeviceIdentity::new(mac, "Fake").unwrap(),
            DeviceCapabilities::ssd1681_154g()).unwrap();
        crate::device_clock::configure(mac, 0, 1_000_000, 0).unwrap();
        svc.note_authenticated_status(mac, &json!({"device_mac":mac, "fw":"a",
            "boot_id":"boot-a", "uptime_ms":0, "radio":{"wifi_connected":false,
            "ble_connected":false}, "heap_free":0, "template_ids":[]})).unwrap();
        let first = svc.device_get(mac).unwrap()["last_authenticated"]["body"].clone();
        assert_eq!(first["radio"]["wifi_connected"], false);
        assert_eq!(first["heap_free"], 0);
        assert_eq!(first["template_ids"], json!([]));
        crate::device_clock::step(mac, 60_000).unwrap();
        svc.note_authenticated_status(mac, &json!({"device_mac":mac,"fw":"b",
            "boot_id":"boot-b","uptime_ms":100})).unwrap();
        let second = svc.device_get(mac).unwrap()["last_authenticated"]["body"].clone();
        assert_eq!(second["radio"], first["radio"]);
        assert_eq!(second["groups"]["radio"], first["groups"]["radio"]);
        assert_ne!(second["groups"]["firmware"], first["groups"]["firmware"]);
        crate::device_clock::clear(mac);
    }

    #[test]
    fn data_sync_permission_keeps_profile_order_and_does_not_publish() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "0200000000F2";
        svc.device_upsert(DeviceIdentity::new(mac, "Fake").unwrap(),
            DeviceCapabilities::ssd1681_154g()).unwrap();
        let before = svc.device_get(mac).unwrap();
        svc.set_data_sync(mac, true).unwrap();
        let after = svc.device_get(mac).unwrap();
        assert_eq!(after["sync_enabled"], true);
        assert_eq!(after["profile"], before["profile"]);
        assert_eq!(after["job"], before["job"]);
        drop(svc);
        let resumed = PlatformService::open(dir.path()).unwrap();
        assert_eq!(resumed.device_get(mac).unwrap()["sync_enabled"], true);
    }

    #[test]
    fn null_reason_keeps_last_value_and_its_original_sample_time() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "0200000000F3";
        svc.device_upsert(DeviceIdentity::new(mac, "Fake").unwrap(),
            DeviceCapabilities::ssd1681_154g()).unwrap();
        crate::device_clock::configure(mac, 0, 1_000_000, 0).unwrap();
        svc.note_authenticated_status(mac, &json!({"device_mac":mac,"boot_id":"boot-a",
            "fw":"v1", "heap_free":0,"power":{"battery":75}})).unwrap();
        let first = svc.device_get(mac).unwrap()["last_authenticated"]["body"].clone();
        crate::device_clock::step(mac, 60_000).unwrap();
        svc.note_authenticated_status(mac, &json!({"device_mac":mac,"boot_id":"boot-b",
            "fw":"v2","heap_free":{"value":null,"reason":"read_error"},
            "power":{"battery":{"value":null,"reason":"unsupported"}}})).unwrap();
        let second = svc.device_get(mac).unwrap()["last_authenticated"]["body"].clone();
        assert_eq!(second["heap_free"]["reason"], "read_error");
        assert_eq!(second["heap_free"]["previous"], 0);
        assert_eq!(second["power"]["battery"]["previous"], 75);
        assert_eq!(second["field_samples"]["heap_free"], first["field_samples"]["heap_free"]);
        assert_eq!(second["field_samples"]["power.battery"], first["field_samples"]["power.battery"]);
        assert_ne!(second["field_samples"]["fw"], first["field_samples"]["fw"]);
        crate::device_clock::clear(mac);
    }

    #[test]
    fn authenticated_timestamps_use_only_the_target_mac_clock() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let a = "0200000000E1";
        let b = "0200000000E2";
        for mac in [a,b] {
            svc.device_upsert(DeviceIdentity::new(mac,"Fake").unwrap(),
                DeviceCapabilities::ssd1681_154g()).unwrap();
        }
        crate::device_clock::configure(a,1000,2_000_000,0).unwrap();
        crate::device_clock::configure(b,5000,7_000_000,0).unwrap();
        svc.note_authenticated_status(a,&json!({"device_mac":a})).unwrap();
        svc.note_authenticated_status(b,&json!({"device_mac":b})).unwrap();
        assert_eq!(svc.device_get(a).unwrap()["last_authenticated"]["observed_at"],2000);
        assert_eq!(svc.device_get(b).unwrap()["last_authenticated"]["observed_at"],7000);
        crate::device_clock::step(a,10_000).unwrap();
        svc.note_status_attempt(a,"offline",Some("timeout")).unwrap();
        assert_eq!(svc.device_get(a).unwrap()["last_status_attempt"]["at"],2010);
        assert_eq!(svc.device_get(b).unwrap()["last_authenticated"]["observed_at"],7000);
        crate::device_clock::clear(a);
        crate::device_clock::clear(b);
    }

    #[test]
    fn ota_freezes_per_mac_and_never_reuploads_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "AABBCCDDEEFF";
        svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(), DeviceCapabilities::ssd1681_154g()).unwrap();
        let target = svc.device_get(mac).unwrap()["capabilities"]["firmware_target"].as_str().unwrap().to_string();
        let version = "0.99.0-bw";
        let mut bytes = vec![0u8; 2048];
        bytes[0] = 0xe9;
        let marker = format!("codex-status-ota-v1|{target}|{version}\0");
        bytes[32..32 + marker.len()].copy_from_slice(marker.as_bytes());
        let source = dir.path().join("candidate.bin");
        std::fs::write(&source, &bytes).unwrap();
        assert!(svc.queue_ota(mac, "bridge", "wrong", &source, version, "zectrix-note4-400x300").is_err());
        let job = svc.queue_ota(mac, "bridge", "request-1", &source, version, &target).unwrap();
        std::fs::remove_file(&source).unwrap();
        assert_eq!(svc.queue_ota(mac, "bridge", "request-1", &source, version, &target).unwrap().job_id, job.job_id);
        assert!(svc.queue_ota(mac, "bridge", "request-2", &svc.dir.join(&job.blob), version, &target).is_err());
        svc.note_authenticated_status(mac, &json!({"device_mac":mac,"boot_id":"before-restart"})).unwrap();
        let frozen = svc.ota_begin(mac).unwrap().unwrap();
        assert_eq!(std::fs::read(frozen).unwrap(), bytes);
        drop(svc);
        let resumed = PlatformService::open(dir.path()).unwrap();
        assert_eq!(resumed.ota_job(mac).unwrap().state, "awaiting_confirmation");
        assert_eq!(resumed.ota_job(mac).unwrap().before_boot_id.as_deref(), Some("before-restart"));
        assert!(resumed.ota_job(mac).unwrap().blocks_following_work());
        assert!(resumed.ota_begin(mac).unwrap().is_none());
        assert_eq!(resumed.ota_job(mac).unwrap().sha256, job.sha256);
        resumed.ota_note_authenticated_version(mac, &json!({"fw": version})).unwrap();
        assert!(resumed.ota_job(mac).unwrap().confirmation.is_none());
        resumed.ota_await_confirmation(mac, true, None).unwrap();
        assert!(resumed.ota_job(mac).unwrap().blocks_following_work());
        resumed.ota_note_authenticated_version(mac, &json!({"fw": "unexpected"})).unwrap();
        assert!(resumed.ota_job(mac).unwrap().blocks_following_work());
        resumed.ota_note_authenticated_version(mac, &json!({"fw": version})).unwrap();
        assert_eq!(resumed.ota_job(mac).unwrap().confirmation.as_deref(), Some("version_seen_unproven"));
        assert_eq!(resumed.ota_job(mac).unwrap().state, "awaiting_confirmation");
        assert!(!resumed.ota_job(mac).unwrap().blocks_following_work());
        let next_version = "0.99.1-bw";
        let mut next_bytes = bytes;
        let next_marker = format!("codex-status-ota-v1|{target}|{next_version}\0");
        next_bytes[32..32 + next_marker.len()].copy_from_slice(next_marker.as_bytes());
        std::fs::write(&source, next_bytes).unwrap();
        let next = resumed.queue_ota(mac, "bridge", "request-2", &source, next_version, &target).unwrap();
        assert_eq!(next.state, "queued");
        assert_ne!(next.job_id, job.job_id);
        assert!(!resumed.dir.join(job.blob).exists());
    }

    #[test]
    fn cancelled_unacked_ota_stays_unconfirmed_and_is_not_rescheduled() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "AABBCCDDEEFF";
        svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(),
            DeviceCapabilities::ssd1681_154g()).unwrap();
        let target = svc.device_get(mac).unwrap()["capabilities"]["firmware_target"]
            .as_str().unwrap().to_owned();
        let version = "0.99.0-bw";
        let mut bytes = vec![0u8; 2048];
        bytes[0] = 0xe9;
        let marker = format!("codex-status-ota-v1|{target}|{version}\0");
        bytes[32..32 + marker.len()].copy_from_slice(marker.as_bytes());
        let source = dir.path().join("candidate.bin");
        std::fs::write(&source, bytes).unwrap();
        svc.queue_ota(mac, "bridge", "request-1", &source, version, &target).unwrap();
        svc.ota_begin(mac).unwrap();
        svc.ota_await_confirmation(mac, false, Some("sync arm HTTP 409: batch_conflict")).unwrap();
        svc.ota_cancel(mac).unwrap();
        drop(svc);

        let resumed = PlatformService::open(dir.path()).unwrap();
        resumed.ota_note_authenticated_version(mac, &json!({"fw": version})).unwrap();
        assert!(resumed.ota_job(mac).is_none());
        let entry = resumed.ota_history(mac).pop().unwrap();
        assert_eq!(entry.outcome, "cancelled_unconfirmed");
        assert!(!entry.job.upload_ack && entry.job.cancel_requested && entry.job.confirmation.is_none());
        assert!(resumed.ota_begin(mac).unwrap().is_none());
        assert_eq!(resumed.queue_ota(mac, "bridge", "request-2", &source, version, &target).unwrap().state, "queued");
    }

    #[test]
    fn sync_v1_running_image_proves_exact_frozen_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "AABBCCDDEEFF";
        svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(),
            DeviceCapabilities::ssd1681_154g()).unwrap();
        let target = svc.device_get(mac).unwrap()["capabilities"]["firmware_target"]
            .as_str().unwrap().to_owned();
        let version = "0.99.0-bw";
        let mut x = vec![7u8; 2048];
        x[0] = 0xe9;
        let marker = format!("codex-status-ota-v1|{target}|{version}\0");
        x[32..32 + marker.len()].copy_from_slice(marker.as_bytes());
        let source = dir.path().join("image.bin");
        std::fs::write(&source, &x).unwrap();
        let job = svc.queue_ota(mac, "bridge", "request-1", &source, version, &target).unwrap();
        svc.ota_begin(mac).unwrap();
        svc.ota_await_confirmation(mac, true, None).unwrap();
        let mut y = x.clone();
        let tail = y.len() - 1;
        y[tail] ^= 1;
        let wrong = json!({"algorithm":"sha256-running-prefix-v1", "device_mac":mac,
            "image_bytes":x.len(), "sha256":format!("{:x}", sha2::Sha256::digest(&y)),
            "fw_target":target, "fw":version});
        assert!(!svc.ota_note_running_image(mac, &wrong).unwrap());
        assert_eq!(svc.ota_job(mac).unwrap().confirmation.as_deref(), Some("upload_ack"));
        let mut exact = wrong;
        exact["sha256"] = json!(job.sha256);
        assert!(svc.ota_note_running_image(mac, &exact).unwrap());
        assert_eq!(svc.ota_job(mac).unwrap().confirmation.as_deref(), Some("image_verified"));
        assert_eq!(svc.ota_job(mac).unwrap().state, "succeeded");
    }

    #[test]
    fn ota_corrupt_blob_fails_without_touching_another_mac() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let a = "AABBCCDDEEFF";
        let b = "112233445566";
        for mac in [a, b] {
            svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(), DeviceCapabilities::ssd1681_154g()).unwrap();
        }
        let target = svc.device_get(a).unwrap()["capabilities"]["firmware_target"].as_str().unwrap().to_string();
        let version = "0.99.0-bw";
        let mut bytes = vec![0u8; 2048];
        bytes[0] = 0xe9;
        let marker = format!("codex-status-ota-v1|{target}|{version}\0");
        bytes[32..32 + marker.len()].copy_from_slice(marker.as_bytes());
        let source = dir.path().join("candidate.bin");
        std::fs::write(&source, &bytes).unwrap();
        let a_job = svc.queue_ota(a, "bridge", "a", &source, version, &target).unwrap();
        let b_job = svc.queue_ota(b, "bridge", "b", &source, version, &target).unwrap();
        assert_ne!(a_job.blob, b_job.blob);
        std::fs::write(svc.dir.join(&a_job.blob), b"corrupt").unwrap();
        assert!(svc.ota_begin(a).is_err());
        assert_eq!(svc.ota_job(a).unwrap().state, "failed");
        assert!(svc.ota_begin(b).unwrap().is_some());
        assert_eq!(svc.ota_job(b).unwrap().state, "transferring");
        svc.ota_cancel(b).unwrap();
        assert_eq!(svc.ota_job(b).unwrap().state, "transferring");
        assert!(svc.ota_job(b).unwrap().cancel_requested);
    }

    #[test]
    fn queued_ota_cancel_removes_frozen_blob() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "AABBCCDDEEFF";
        svc.device_upsert(DeviceIdentity::new(mac, "Test").unwrap(), DeviceCapabilities::ssd1681_154g()).unwrap();
        let target = svc.device_get(mac).unwrap()["capabilities"]["firmware_target"].as_str().unwrap().to_string();
        let mut bytes = vec![0u8; 2048];
        bytes[0] = 0xe9;
        let marker = format!("codex-status-ota-v1|{target}|v1\0");
        bytes[32..32 + marker.len()].copy_from_slice(marker.as_bytes());
        let source = dir.path().join("candidate.bin");
        std::fs::write(&source, bytes).unwrap();
        let job = svc.queue_ota(mac, "bridge", "request-1", &source, "v1", &target).unwrap();
        assert!(svc.dir.join(&job.blob).exists());
        svc.ota_cancel(mac).unwrap();
        assert!(svc.ota_job(mac).is_none());
        assert_eq!(svc.ota_history(mac).pop().unwrap().outcome, "cancelled_before_upload");
        assert!(!svc.dir.join(&job.blob).exists());
    }

    /// A template switched on the device (same context!) must rebuild the data
    /// contract, or every frame is rejected with `incomplete` on the device.
    #[test]
    fn device_side_template_switch_refreshes_the_data_contract() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        let rt = crate::platform::model::RENDER_TARGET_154G;
        svc.template_save("mini", rt, &mini_source(), 1000).unwrap();
        svc.template_save("quad", rt, &quad_source(), 1001).unwrap();
        let mut profile = Profile::draft("AA:BB:CC:DD:EE:FF");
        profile.template_ids = vec!["mini".into(), "quad".into()];
        profile.initial_active_id = Some("mini".into());
        svc.profile_save(profile, 1002).unwrap();

        let status = |template: &str| {
            let compiled = crate::compile::compile(
                &if template == "mini" { mini_source() } else { quad_source() }, rt).unwrap();
            json!({
                "configured": true,
                "active_context_id": "ctx-1",
                "active_template_id": template,
                "committed_job_id": "",
                "data_seq": 0,
                "applied_seq": 0,
                "data_contract":{"template_id":template, "compiler_abi":compiled.compiler_abi,
                    "source_crc":compiled.source_crc,"requirements":compiled.requirements},
            })
        };
        let fields = |svc: &PlatformService| -> Vec<String> {
            let inner = svc.inner.lock().unwrap();
            inner.coordinators["AA:BB:CC:DD:EE:FF"]
                .requirements
                .iter()
                .map(|r| r.field.clone())
                .collect()
        };
        let active = |svc: &PlatformService| -> Option<String> {
            let inner = svc.inner.lock().unwrap();
            inner.coordinators["AA:BB:CC:DD:EE:FF"].active_template_id.clone()
        };

        // First status adopts the device context and its template (mini).
        svc.note_device_status("AA:BB:CC:DD:EE:FF", &status("mini")).unwrap();
        assert_eq!(active(&svc).as_deref(), Some("mini"));
        assert!(!fields(&svc).iter().any(|f| f == "buckets[codex].monthly.remaining"));

        // The device switches locally to quad: same context, new template.
        svc.note_device_status("AA:BB:CC:DD:EE:FF", &status("quad")).unwrap();
        assert_eq!(active(&svc).as_deref(), Some("quad"));
        let after = fields(&svc);
        assert!(
            after.iter().any(|f| f == "buckets[codex].monthly.remaining"),
            "quad's requirement set must be active after a device-side switch: {after:?}"
        );
    }

    #[test]
    fn family_profile_copy_and_save_persist_without_publishing() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        svc.template_save(
            "quad",
            crate::platform::model::RENDER_TARGET_154G,
            &quad_source(),
            1000,
        )
            .unwrap();
        let mut device_profile = Profile::draft("AA:BB:CC:DD:EE:FF");
        device_profile.template_ids = vec!["quad".into()];
        device_profile.initial_active_id = Some("quad".into());
        let device_profile = svc.profile_save(device_profile, 1001).unwrap();

        let copied = svc
            .family_profile_copy_from_device("aa:bb:cc:dd:ee:ff", "copied", "Copied", 1002)
            .unwrap();
        assert_eq!(copied.render_target, crate::platform::model::RENDER_TARGET_154G);
        assert_eq!(copied.template_ids, ["quad"]);
        assert_eq!(svc.profile_get("AA:BB:CC:DD:EE:FF").unwrap(), device_profile);
        assert!(svc.job("AA:BB:CC:DD:EE:FF").is_none());
        assert!(svc
            .family_profile_copy_from_device("AA:BB:CC:DD:EE:FF", "copied", "Duplicate", 1003)
            .is_err());

        let draft = FamilyProfile {
            render_target: crate::platform::model::RENDER_TARGET_154G.into(),
            id: "default".into(),
            name: "Default".into(),
            template_ids: vec!["quad".into()],
            enabled_template_ids: None,
            initial_active_id: Some("quad".into()),
            font_ids: BTreeMap::new(),
            bindings: Vec::new(),
            sync_enabled: false,
            full_sync_s: 3600,
            updated_at: 0,
        };
        svc.family_profile_save(draft, 1004).unwrap();
        drop(svc);

        let restored = PlatformService::open(dir.path()).unwrap();
        assert!(restored
            .family_profile_get(crate::platform::model::RENDER_TARGET_154G, "default")
            .is_some());
        assert!(restored
            .family_profile_get(crate::platform::model::RENDER_TARGET_154G, "copied")
            .is_some());
        assert_eq!(restored.profile_get("AA:BB:CC:DD:EE:FF").unwrap(), device_profile);
        assert!(restored.job("AA:BB:CC:DD:EE:FF").is_none());
    }

    #[test]
    fn profile_order_1_to_8_and_rejections() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        for id in ["a", "b", "c", "d", "e", "f", "g", "h"] {
            let mut src = quad_source();
            src["id"] = json!(id);
            svc.template_save(id, "epd-ssd1681-200x200-1bpp", &src, 1000)
                .unwrap();
        }
        let mut p = Profile::draft("AA:BB:CC:DD:EE:FF");
        p.template_ids = (1..=8)
            .map(|i| ((b'a' + i - 1) as char).to_string())
            .collect();
        p.initial_active_id = Some("a".into());
        let saved = svc.profile_save(p, 1000).unwrap();
        assert_eq!(saved.template_ids.len(), 8);
        assert_eq!(saved.template_ids, ["a", "b", "c", "d", "e", "f", "g", "h"]);
        // 9 items rejected
        let mut p9 = saved.clone();
        p9.template_ids.push("a".into());
        assert!(svc.profile_save(p9, 1000).is_err());
        // duplicate rejected
        let mut dup = saved.clone();
        dup.template_ids[1] = "a".into();
        assert!(svc.profile_save(dup, 1000).is_err());
        // empty draft may be saved but never published
        let mut empty = Profile::draft("AA:BB:CC:DD:EE:FF");
        empty.template_ids.clear();
        svc.profile_save(empty, 1000).unwrap();
        assert!(svc.publish("AA:BB:CC:DD:EE:FF", 1000).is_err());
        // restore an 8-item profile and publish
        svc.profile_save(saved, 1000).unwrap();
        let job = svc.publish("AA:BB:CC:DD:EE:FF", 1000).unwrap();
        assert_eq!(job["state"], "waiting");
        let bundle = svc.inner.lock().unwrap().coordinators["AA:BB:CC:DD:EE:FF"]
            .job
            .as_ref()
            .unwrap()
            .frozen_bundle
            .clone();
        assert_eq!(bundle.profile.template_ids.len(), 8);
        assert_eq!(bundle.templates.len(), 8);
    }

    #[test]
    fn unsupported_stored_template_keeps_bridge_available_and_blocks_publish() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        svc.template_save("quad", "epd-ssd1681-200x200-1bpp", &quad_source(), 1000).unwrap();
        let mut profile = Profile::draft("AA:BB:CC:DD:EE:FF");
        profile.template_ids = vec!["quad".into()];
        profile.initial_active_id = Some("quad".into());
        svc.profile_save(profile, 1000).unwrap();
        drop(svc);
        let path = dir.path().join("platform/state.json");
        let mut saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        saved["templates"][0]["source"]["elements"][0]["when"] = json!({"bind":"device.state","exists":false});
        saved["templates"][0]["compiled"]["compiler_abi"] = json!(0);
        std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        assert_eq!(svc.inner.lock().unwrap().templates.values().next().unwrap().source,
            saved["templates"][0]["source"]);
        let error = svc.publish("AA:BB:CC:DD:EE:FF", 1001).unwrap_err();
        assert!(error.to_string().contains("unsupported"), "{error:#}");
        assert!(svc.job("AA:BB:CC:DD:EE:FF").is_none());
        svc.template_save("quad", "epd-ssd1681-200x200-1bpp", &quad_source(), 1002).unwrap();
        svc.publish("AA:BB:CC:DD:EE:FF", 1003).unwrap();
    }

    #[test]
    fn save_does_not_publish_and_publish_freezes() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        svc.template_save("quad", "epd-ssd1681-200x200-1bpp", &quad_source(), 1000)
            .unwrap();
        let mut p = Profile::draft("AA:BB:CC:DD:EE:FF");
        p.template_ids = vec!["quad".into()];
        p.initial_active_id = Some("quad".into());
        svc.profile_save(p, 1000).unwrap();
        assert!(svc.job("AA:BB:CC:DD:EE:FF").is_none());
        svc.publish("AA:BB:CC:DD:EE:FF", 1000).unwrap();
        let frozen_before = svc.inner.lock().unwrap().coordinators["AA:BB:CC:DD:EE:FF"]
            .job
            .as_ref()
            .unwrap()
            .frozen_bundle
            .templates[0]
            .source
            .clone();
        // Edit the template after queueing: the frozen job must not drift.
        let mut edited = quad_source();
        edited["elements"][0]["rect"] = json!([0, 0, 10, 10]);
        svc.template_save("quad", "epd-ssd1681-200x200-1bpp", &edited, 2000)
            .unwrap();
        let frozen_after = svc.inner.lock().unwrap().coordinators["AA:BB:CC:DD:EE:FF"]
            .job
            .as_ref()
            .unwrap()
            .frozen_bundle
            .templates[0]
            .source
            .clone();
        assert_eq!(frozen_before, frozen_after);
        assert_ne!(frozen_after, edited);
    }

    #[test]
    fn full_bundle_job_survives_restart_with_frozen_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        svc.template_save("quad", "epd-ssd1681-200x200-1bpp", &quad_source(), 1000)
            .unwrap();
        let mut profile = Profile::draft("AA:BB:CC:DD:EE:FF");
        profile.template_ids = vec!["quad".into()];
        profile.initial_active_id = Some("quad".into());
        svc.profile_save(profile, 1000).unwrap();
        let job = svc.publish("AA:BB:CC:DD:EE:FF", 1000).unwrap();
        let old_bytes = svc.bundle_payload("AA:BB:CC:DD:EE:FF").unwrap();
        drop(svc);
        let svc = PlatformService::open(dir.path()).unwrap();
        svc.bind_pending_bundle_owner("AA:BB:CC:DD:EE:FF", "bridge-1").unwrap();
        let frozen = svc.bundle_payload("AA:BB:CC:DD:EE:FF").unwrap();
        assert_ne!(frozen, old_bytes);
        assert_eq!(serde_json::from_slice::<Value>(&frozen).unwrap()["bridge_id"], "bridge-1");
        let mut edited = quad_source();
        edited["elements"][0]["rect"] = json!([0, 0, 10, 10]);
        svc.template_save("quad", "epd-ssd1681-200x200-1bpp", &edited, 2000)
            .unwrap();
        drop(svc);

        let resumed = PlatformService::open(dir.path()).unwrap();
        assert_eq!(resumed.job("AA:BB:CC:DD:EE:FF").unwrap()["job_id"], job["job_id"]);
        assert_eq!(resumed.bundle_payload("AA:BB:CC:DD:EE:FF").unwrap(), frozen);
        assert_eq!(resumed.next_http_delivery("AA:BB:CC:DD:EE:FF", true, 3000)["decision"], "bundle");
        let in_flight: PersistedState = store::read_json(&dir.path().join("platform/state.json"))
            .unwrap().unwrap();
        assert_eq!(in_flight.bundle_jobs["AA:BB:CC:DD:EE:FF"].state,
            PublishState::Sending);
        drop(resumed);
        let recovered = PlatformService::open(dir.path()).unwrap();
        assert_eq!(recovered.job("AA:BB:CC:DD:EE:FF").unwrap()["state"], "unknown");
        assert_eq!(recovered.next_http_delivery("AA:BB:CC:DD:EE:FF", true, 3002)["decision"], "idle");
        assert_eq!(recovered.bundle_payload("AA:BB:CC:DD:EE:FF").unwrap(), frozen);
        recovered.note_device_status("AA:BB:CC:DD:EE:FF", &json!({
            "configured": true, "committed_job_id": job["job_id"],
            "active_context_id": "device-context-after-lost-ack",
            "active_template_id": "quad", "data_seq": 0,
        })).unwrap();
        assert_eq!(recovered.job("AA:BB:CC:DD:EE:FF").unwrap()["state"], "succeeded");
        assert_eq!(recovered.next_http_delivery("AA:BB:CC:DD:EE:FF", true, 4001)["decision"], "idle");
        let expected_crc = recovered.job("AA:BB:CC:DD:EE:FF").unwrap()["crc"].clone();
        drop(recovered);
        let after_ack = PlatformService::open(dir.path()).unwrap();
        assert_eq!(after_ack.next_http_delivery("AA:BB:CC:DD:EE:FF", true, 4001)["decision"], "idle");
        let persisted: PersistedState = store::read_json(&dir.path().join("platform/state.json"))
            .unwrap().unwrap();
        let summary = persisted.jobs.iter().find(|entry| entry["job_id"] == job["job_id"]).unwrap();
        assert_eq!(summary["state"], "succeeded");
        assert_eq!(summary["crc"], expected_crc);
    }

    #[test]
    fn compressed_publish_uses_wire_capacity_and_freezes_across_capability_change() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        let mac = "AA:BB:CC:DD:EE:FF";
        {
            let mut inner = svc.inner.lock().unwrap();
            let caps = &mut inner.devices.get_mut(mac).unwrap().capabilities;
            caps.bundle_storage_protocol = 2;
            caps.bundle_compression = 1;
            caps.max_bundle_decoded_bytes = 1024*1024;
            caps.max_bundle_bytes = 8192;
        }
        svc.template_save("quad", "epd-ssd1681-200x200-1bpp", &quad_source(), 1000).unwrap();
        let mut profile = Profile::draft(mac);
        profile.template_ids = vec!["quad".into()];
        svc.profile_save(profile, 1000).unwrap();
        svc.publish(mac, 1000).unwrap();
        let wire = svc.bundle_payload(mac).unwrap();
        assert!(wire.starts_with(b"CSZ1") && wire.len() < 8192);
        assert!(bridge_render::bundle_codec::decode(&wire, 1024*1024).unwrap().len() > 8192);
        drop(svc);
        let resumed = PlatformService::open(dir.path()).unwrap();
        resumed.inner.lock().unwrap().devices.get_mut(mac).unwrap().capabilities.bundle_compression = 0;
        assert_eq!(resumed.bundle_payload(mac).unwrap(), wire);
    }

    #[test]
    fn device_status_reconciles_lost_bundle_ack() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        svc.template_save("quad", "epd-ssd1681-200x200-1bpp", &quad_source(), 1000).unwrap();
        let mut profile = Profile::draft("AA:BB:CC:DD:EE:FF");
        profile.template_ids = vec!["quad".into()];
        svc.profile_save(profile, 1000).unwrap();
        let job = svc.publish("AA:BB:CC:DD:EE:FF", 1000).unwrap();
        drop(svc);

        let resumed = PlatformService::open(dir.path()).unwrap();
        resumed.note_device_status("AA:BB:CC:DD:EE:FF", &json!({
            "configured": true, "committed_job_id": job["job_id"],
            "active_context_id": "ctx-1", "active_template_id": "quad",
        })).unwrap();
        assert_eq!(resumed.job("AA:BB:CC:DD:EE:FF").unwrap()["state"], "succeeded");
        drop(resumed);
        let state_path = dir.path().join("platform/state.json");
        let mut state: PersistedState = store::read_json(&state_path).unwrap().unwrap();
        state.jobs.iter_mut().find(|entry| entry["job_id"] == job["job_id"]).unwrap()["state"] = json!("waiting");
        store::write_json(&state_path, &state).unwrap(); // Simulates an older stale history checkpoint.
        let restarted = PlatformService::open(dir.path()).unwrap();
        assert_eq!(restarted.next_http_delivery("AA:BB:CC:DD:EE:FF", true, 2000)["decision"], "idle");
        let state: PersistedState = store::read_json(&state_path).unwrap().unwrap();
        assert_eq!(state.jobs.iter().find(|entry| entry["job_id"] == job["job_id"]).unwrap()["state"], "succeeded");
    }

    #[test]
    fn explicit_light_queues_one_plan_across_restart_and_ack() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        let mac = "AA:BB:CC:DD:EE:FF";
        let now = crate::now_secs();
        let ordinary = svc.plan_for_rendezvous(mac, now, "rendezvous", 0).unwrap();
        assert_eq!(ordinary.mode, PlanMode::Sleep);
        let (light, already) = svc.queue_explicit_light(mac, now + 1).unwrap();
        assert!(!already);
        assert_eq!(light.mode, PlanMode::Light);
        assert!(light.plan_id > ordinary.plan_id);
        assert_eq!(svc.queue_explicit_light(mac, now + 2).unwrap(), (light.clone(), true));
        drop(svc);

        let resumed = PlatformService::open(dir.path()).unwrap();
        assert_eq!(resumed.plan_for_rendezvous(mac, now + 3, "rendezvous", 0).unwrap(), light);
        assert_eq!(resumed.note_plan_ack(mac, light.plan_id, 500, false, now + 4)["outcome"], "accepted");
        let summary = resumed.coordinator_summary(mac, now + 4).unwrap();
        assert!(summary["plan"]["pending_explicit_light"].is_null());
        assert_eq!(summary["plan"]["last_explicit_light_ack"]["plan_id"], light.plan_id);
        let hold_until = summary["plan"]["light_hold_until"].as_u64().unwrap();
        assert_eq!(resumed.plan_for_rendezvous(mac, hold_until - 1, "rendezvous", 0).unwrap().mode, PlanMode::Light);
        assert_eq!(resumed.plan_for_rendezvous(mac, hold_until + 1, "rendezvous", 0).unwrap().mode, PlanMode::Sleep);
    }

    #[test]
    fn physical_wake_window_survives_follow_up_rendezvous() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        let mac = "AA:BB:CC:DD:EE:FF";
        let now = crate::now_secs();
        // Button wake reported 15 s into its 300 s window, with nothing pending.
        let boot = svc.plan_for_rendezvous(mac, now, "manual", 285).unwrap();
        assert_eq!(boot.mode, PlanMode::Light);
        assert_eq!(boot.light_duration_s, 285);
        assert_eq!(
            svc.note_plan_ack(mac, boot.plan_id, 285, true, now)["outcome"],
            "accepted"
        );
        // The device stops reporting the provisional once a formal plan is
        // accepted, but the next rendezvous must still hold light until the same
        // physical-wake deadline instead of cutting it to an early sleep.
        let follow = svc.plan_for_rendezvous(mac, now + 60, "rendezvous", 0).unwrap();
        assert_eq!(follow.mode, PlanMode::Light);
        assert_eq!(follow.plan_id, boot.plan_id); // replay keeps the original deadline
        assert_eq!(follow.light_duration_s, 285);
        // One second past the window the ordinary sleep decision returns.
        let after = svc.plan_for_rendezvous(mac, now + 301, "rendezvous", 0).unwrap();
        assert_eq!(after.mode, PlanMode::Sleep);
    }

    /// W4-1 admission: a queued (non-terminal) Wi-Fi read/sync job is real
    /// pending work, so a rendezvous must grant it a *bounded* formal light
    /// window instead of waiting for somebody else's window. The window is the
    /// dedicated `WIFI_READ_LIGHT_S` (pinned here as 120 s): at least
    /// `MIN_LIGHT_S`, at most `MAX_LIGHT_S`, and strictly below the data/publish
    /// maximum so a stuck read cannot pin the radio for 600 s.
    ///
    /// Red on the pre-fix tree: `plan_for_rendezvous` returns `sleep`.
    #[test]
    fn wifi_read_admission_grants_bounded_formal_light() {
        const EXPECTED_READ_LIGHT_S: u32 = 120;
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        // Registered with separators: the Wi-Fi job store is keyed by the
        // normalized MAC, so admission must normalize too.
        let mac = "AA:BB:CC:DD:EE:FF";
        let now = crate::now_secs();
        // Nothing pending: the rendezvous is a plain sleep plan.
        assert_eq!(
            svc.plan_for_rendezvous(mac, now, "rendezvous", 0).unwrap().mode,
            PlanMode::Sleep
        );
        // Queueing a read is real pending work and must not need a borrowed window.
        let job = svc.wifi_queue(mac, "bridge", "read", vec![], now).unwrap();
        let plan = svc.plan_for_rendezvous(mac, now + 1, "rendezvous", 0).unwrap();
        assert_eq!(
            plan.mode, PlanMode::Light,
            "a queued Wi-Fi read must open a formal light window"
        );
        assert_eq!(plan.light_duration_s, EXPECTED_READ_LIGHT_S);
        assert!(plan.light_duration_s >= crate::coordinator::MIN_LIGHT_S);
        assert!(plan.light_duration_s <= crate::coordinator::MAX_LIGHT_S);
        assert!(plan.light_duration_s < crate::coordinator::MAX_LIGHT_S);
        assert_eq!(plan.reason, "rendezvous");
        // Admission is a plan decision only: it must not retry, complete or
        // otherwise mutate the job (no implicit activity, no implicit renewal).
        let job = svc
            .wifi_state(mac)
            .jobs
            .into_iter()
            .find(|j| j.job_id == job.job_id)
            .unwrap();
        assert_eq!(job.state, "queued");
        assert_eq!(job.attempt_count, 0);
        assert!(!job.sent);
        // A completed read leaves no hold behind: the next rendezvous sleeps.
        let snapshot: WifiSnapshot = serde_json::from_value(json!({"schema":2,"device_mac":mac,"revision":1,
            "snapshot_id":"snap","complete":true,"observed_at":now+2,"networks":[],"capacity":{"total":12}})).unwrap();
        svc.wifi_complete_read(mac, &job.job_id, snapshot, now + 2).unwrap();
        assert_eq!(
            svc.plan_for_rendezvous(mac, now + 3, "rendezvous", 0).unwrap().mode,
            PlanMode::Sleep
        );
    }

    #[test]
    fn rendezvous_admission_excludes_physical_recovery_and_blocked_work() {
        let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());
        let now=crate::now_secs();
        let mut job=svc.wifi_queue("AA:BB:CC:DD:EE:FF","bridge","read",vec![],now).unwrap();
        assert!(wifi_job_wants_window(&job,now));
        job.kind="recovery_save".into();assert!(wifi_job_wants_window(&job,now));
        job.kind="sync".into();
        for state in ["waiting_input","conflict","failed","cancelled"] {
            job.state=state.into();assert!(!wifi_job_wants_window(&job,now),"{state}");
        }
        job.state="queued".into();job.next_attempt_at=now+1;assert!(!wifi_job_wants_window(&job,now));
    }

    #[test]
    fn rendezvous_wifi_errors_keep_complete_snapshot_and_safe_distinct_codes() {
        for (message,state,code) in [("snapshot_expired","waiting_rendezvous","snapshot_expired"),
            ("wifi_snapshot_mismatch","failed","snapshot_mismatch"),("invalid_wifi_snapshot","failed","snapshot_shape"),
            ("wifi_window_budget","waiting_rendezvous","window_budget_exhausted"),("wifi_http_401","failed","unauthorized")] {
            let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AA:BB:CC:DD:EE:FF";let now=crate::now_secs();
            let first=svc.wifi_queue(mac,"bridge","read",vec![],now).unwrap();
            let snapshot=serde_json::from_value(json!({"schema":2,"device_mac":mac,"revision":1,"snapshot_id":"old",
                "complete":true,"observed_at":now,"networks":[],"capacity":{"total":12}})).unwrap();
            svc.wifi_complete_read(mac,&first.job_id,snapshot,now).unwrap();
            let job=svc.wifi_queue(mac,"bridge","read",vec![],now+1).unwrap();
            let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",false,&Default::default(),now+1,&mut |_: &str, _: Value|Err(anyhow::anyhow!(message)))).unwrap().unwrap();
            assert_eq!((result.state.as_str(),result.error_code.as_deref()),(state,Some(code)));
            assert_eq!(svc.wifi_state(mac).snapshot.unwrap().snapshot_id,"old");
        }
    }

    #[test]
    fn unknown_persisted_transaction_fields_are_rejected_without_rewriting_state() {
        let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
        let job=svc.wifi_queue_control(mac,"original","activate",json!({"network_id":"home"}),100).unwrap();
        svc.wifi_mark_sent(mac,&job.job_id,101).unwrap();
        let path=svc.state_path();let original=std::fs::read(&path).unwrap();
        let state:Value=serde_json::from_slice(&original).unwrap();drop(svc);
        for location in ["root","wifi","job"] {
            let mut unknown=state.clone();
            let object=match location {
                "root"=>&mut unknown,
                "wifi"=>&mut unknown["wifi"][mac],
                _=>&mut unknown["wifi"][mac]["jobs"][0],
            };
            object["future_transaction_fence"]=json!({"blocked":true});
            let bytes=serde_json::to_vec(&unknown).unwrap();std::fs::write(&path,&bytes).unwrap();
            assert!(PlatformService::open(dir.path()).is_err(),"unknown {location} field must not be discarded");
            assert_eq!(std::fs::read(&path).unwrap(),bytes);
        }
        std::fs::write(&path,&original).unwrap();
        let svc=PlatformService::open(dir.path()).unwrap();
        let restored=svc.wifi_state(mac).jobs[0].clone();
        assert!(restored.confirmation_pending());assert!(restored.transaction_namespace.is_none());
        assert!(restored.terminal());
        assert_eq!(std::fs::read(&path).unwrap(),original);
        assert!(svc.wifi_queue_control(mac,"original","activate",json!({"network_id":"other"}),102).is_ok());
    }

    #[test]
    fn wifi_attempt_and_ack_require_original_identity_without_adapter_guards() {
        for sent in [false,true] {
            for (principal,namespace,code) in [("other","own","transaction_principal_changed"),
                ("original","other-bond","transaction_namespace_changed"),
                ("original","","transaction_namespace_invalid")] {
                let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
                let job=svc.wifi_queue_control(mac,"original","activate",json!({"network_id":"home"}),100).unwrap();
                svc.wifi_bind_transaction(mac,&job.job_id,"original","own").unwrap();
                if sent {svc.wifi_mark_sent(mac,&job.job_id,101).unwrap();svc.wifi_finish(mac,&job.job_id,"awaiting_confirmation",None,101).unwrap();}
                let blocked=ready(svc.wifi_attempt_async(mac,&job.job_id,principal,namespace,true,&Default::default(),200,&mut
                    |_: &str, _: Value|panic!("mismatched context must never reach transport"))).unwrap().unwrap();
                assert_eq!(blocked.error_code.as_deref(),Some(code));
                assert_eq!(blocked.sent,sent);assert_eq!(blocked.confirmation_pending(),sent);
                assert_eq!(blocked.request_id,job.request_id);
                drop(svc);let svc=PlatformService::open(dir.path()).unwrap();
                assert_eq!(svc.wifi_state(mac).jobs[0].state,"failed");
                assert!(ready(svc.wifi_attempt_async(mac,&job.job_id,"original","own",true,&Default::default(),300,&mut
                    |_: &str, _: Value|panic!("identity failure requires explicit handling, not automatic resumption"))).unwrap().is_none());
                assert!(svc.wifi_receipt_acked(mac,&job.job_id,principal,namespace).is_err());
            }
        }
        let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
        let job=svc.wifi_queue_control(mac,"original","activate",json!({"network_id":"home"}),100).unwrap();
        svc.wifi_mark_sent(mac,&job.job_id,101).unwrap();
        svc.wifi_finish(mac,&job.job_id,"awaiting_confirmation",None,101).unwrap();
        assert!(ready(svc.wifi_attempt_async(mac,&job.job_id,"original","own",true,&Default::default(),200,&mut
            |_: &str, _: Value|panic!("legacy identity must not be guessed"))).unwrap().is_none());
        let unknown=svc.wifi_state(mac).jobs[0].clone();
        assert_eq!(unknown.error_code.as_deref(),Some("transaction_identity_unknown"));
        assert_eq!(unknown.state,"failed");
        assert!(unknown.confirmation_pending());assert!(unknown.transaction_namespace.is_none());
    }

    #[tokio::test]
    async fn cancelled_async_carrier_preserves_original_query_after_restart() {
        struct SuspendedWrite { request_id: String, op: &'static str, entered: bool }
        impl crate::platform::wifi::Transport for SuspendedWrite {
            async fn exchange(&mut self, op: &str, body: Value) -> Result<Value> {
                assert_eq!(op, self.op);
                if op=="activate" { assert_eq!(body["request_id"], self.request_id); }
                self.entered = true;
                std::future::pending().await
            }
        }
        let dir=tempfile::tempdir().unwrap();
        let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
        let job=svc.wifi_queue_control(mac,"original","activate",json!({"network_id":"home"}),100).unwrap();
        let mut carrier=SuspendedWrite {request_id:job.request_id.clone(),op:"activate",entered:false};
        assert!(tokio::time::timeout(std::time::Duration::from_millis(10),
            svc.wifi_attempt_async(mac,&job.job_id,"original","own",true,&Default::default(),101,&mut carrier)).await.is_err());
        assert!(carrier.entered);
        assert!(svc.wifi_state(mac).jobs[0].confirmation_pending());
        assert_eq!(svc.wifi_state(mac).jobs[0].state,"awaiting_confirmation");
        let queried=ready(svc.wifi_attempt_async(mac,&job.job_id,"original","own",false,&Default::default(),150,&mut |op: &str, body: Value| {
            assert_eq!(op,"result");assert_eq!(body["query_id"],job.request_id);
            Ok(json!({"ok":true,"outcome":"unknown"}))
        })).unwrap().unwrap();
        assert!(queried.confirmation_pending());
        drop(svc);
        let svc=PlatformService::open(dir.path()).unwrap();
        assert!(svc.wifi_queue_control(mac,"original","trial",json!({"ssid":"new"}),200).is_err());
        let done=ready(svc.wifi_attempt_async(mac,&job.job_id,"original","own",false,&Default::default(),200,&mut |op: &str, body: Value| {
            assert_eq!(op,"result");assert_eq!(body["query_id"],job.request_id);
            Ok(json!({"ok":true,"outcome":"completed","revision":1}))
        })).unwrap().unwrap();
        assert!(!done.confirmation_pending());assert!(done.receipt_ack_pending);
        let read=svc.wifi_queue(mac,"original","read",vec![],300).unwrap();
        carrier.op="snapshot";carrier.entered=false;
        assert!(tokio::time::timeout(std::time::Duration::from_millis(10),
            svc.wifi_attempt_async(mac,&read.job_id,"original","",false,&Default::default(),301,&mut carrier)).await.is_err());
        assert!(carrier.entered);
        let state=svc.wifi_state(mac);
        let read_state=state.jobs.iter().find(|j|j.job_id==read.job_id).unwrap();
        assert!(!read_state.sent);assert_eq!(read_state.state,"waiting_rendezvous");
        assert!(svc.wifi_begin(mac,&read.job_id,400).unwrap().is_some());
    }

    #[test]
    fn wifi_transaction_namespace_is_frozen_and_missing_sent_identity_is_not_guessed() {
        for already_sent in [false, true] {
            let dir=tempfile::tempdir().unwrap();
            let svc=service_with_device(dir.path());
            let mac="AABBCCDDEEFF";
            let job=svc.wifi_queue_control(mac,"original","activate",json!({"network_id":"home"}),100).unwrap();
            if already_sent {svc.wifi_mark_sent(mac,&job.job_id,101).unwrap();}
            let before=serde_json::to_value(svc.wifi_state(mac)).unwrap();
            for (principal,namespace,code) in [("other","bond-a","transaction_principal_changed"),
                ("original","","transaction_namespace_invalid")] {
                assert_eq!(svc.wifi_bind_transaction(mac,&job.job_id,principal,namespace).unwrap_err().to_string(),code);
                assert_eq!(serde_json::to_value(svc.wifi_state(mac)).unwrap(),before);
            }
            if already_sent {
                assert_eq!(svc.wifi_bind_transaction(mac,&job.job_id,"original","bond-a").unwrap_err().to_string(),"transaction_identity_unknown");
                assert_eq!(serde_json::to_value(svc.wifi_state(mac)).unwrap(),before);
            } else {
                svc.wifi_bind_transaction(mac,&job.job_id,"original","bond-a").unwrap();
                svc.wifi_mark_sent(mac,&job.job_id,101).unwrap();
            }
            drop(svc);
            let svc=PlatformService::open(dir.path()).unwrap();
            let restored=svc.wifi_state(mac).jobs[0].clone();
            assert_eq!(restored.request_id,job.request_id);
            assert!(restored.confirmation_pending());
            if already_sent {
                assert!(restored.transaction_namespace.is_none());
                assert!(svc.wifi_bind_transaction(mac,&job.job_id,"original","bond-a").is_err());
            } else {
                assert_eq!(restored.transaction_namespace.as_deref(),Some("bond-a"));
                svc.wifi_bind_transaction(mac,&job.job_id,"original","bond-a").unwrap();
                assert_eq!(svc.wifi_bind_transaction(mac,&job.job_id,"original","bond-b").unwrap_err().to_string(),"transaction_namespace_changed");
                assert!(restored.check_transaction_context("original","bond-a").is_ok());
                assert!(restored.check_transaction_context("other","bond-a").is_err());
                assert!(restored.check_transaction_context("original","bond-b").is_err());
            }
        }
    }

    #[test]
    fn wifi_control_dedup_preserves_principal_after_restart() {
        for kind in ["trial", "activate"] {
            let dir = tempfile::tempdir().unwrap();
            let svc = service_with_device(dir.path());
            let mac = "AABBCCDDEEFF";
            let body = json!({"network_id":"home","version":7});
            let job = svc.wifi_queue_control(mac, "original", kind, body.clone(), 100).unwrap();
            drop(svc);
            let svc = PlatformService::open(dir.path()).unwrap();
            let before = serde_json::to_value(svc.wifi_state(mac)).unwrap();
            for principal in ["other", "", &"x".repeat(65)] {
                let error = svc.wifi_queue_control(mac, principal, kind, body.clone(), 101).unwrap_err();
                assert_eq!(error.to_string(), if principal == "other" { "wifi_control_busy" } else { "invalid_wifi_intent" });
                assert_eq!(serde_json::to_value(svc.wifi_state(mac)).unwrap(), before);
            }
            let replay = svc.wifi_queue_control(mac, "original", kind, body, 102).unwrap();
            assert_eq!(replay.job_id, job.job_id);
            assert_eq!(replay.request_id, job.request_id);
            assert_eq!(replay.bridge_id, "original");
            drop(svc);
            let svc = PlatformService::open(dir.path()).unwrap();
            assert_eq!(serde_json::to_value(svc.wifi_state(mac)).unwrap(), before);
        }
    }

    #[test]
    fn cancelled_and_expired_writes_preserve_unknown_without_automatic_retries() {
        use crate::platform::wifi::ExecutionIntent;
        for kind in ["sync", "recovery_save", "trial", "activate"] {
            for cancelled in [false, true] {
                    let dir = tempfile::tempdir().unwrap();
                    let svc = service_with_device(dir.path());
                    let mac = "AABBCCDDEEFF";
                    svc.wifi_save(mac, 0, serde_json::from_value(json!([
                        {"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"}
                    ])).unwrap(), 100).unwrap();
                    let read = svc.wifi_queue(mac, "bridge", "read", vec![], 100).unwrap();
                    svc.wifi_complete_read(mac, &read.job_id, serde_json::from_value(json!({
                        "schema":2,"device_mac":mac,"revision":7,"snapshot_id":"before","complete":true,
                        "observed_at":100,"networks":[],"capacity":{"total":12}
                    })).unwrap(), 100).unwrap();
                    let job = if matches!(kind, "trial" | "activate") {
                        svc.wifi_queue_control(mac, "bridge", kind, json!({"network_id":"home","version":7}), 101).unwrap()
                    } else { svc.wifi_queue(mac, "bridge", kind, vec!["home".into()], 101).unwrap() };
                    ready(svc.wifi_attempt_async(mac, &job.job_id, &job.bridge_id, "own", true, &Default::default(), 102,&mut  |op: &str, _: Value| {
                        if op == "active" { return Ok(json!({"ok":true,"revision":7})); }
                        assert_eq!(op, if matches!(kind, "sync" | "recovery_save") { "apply" } else { kind });
                        Err(anyhow::anyhow!("transport_timeout"))
                    })).unwrap();
                    let now = if cancelled { 200 } else { job.expires_at + 1 };
                    if cancelled { svc.wifi_finish(mac, &job.job_id, "cancelled", None, now - 10).unwrap(); }
                    else { svc.wifi_expire(mac, now).unwrap(); }
                    drop(svc);
                    let svc = PlatformService::open(dir.path()).unwrap();
                    let pending = svc.wifi_state(mac).jobs.into_iter().find(|j| j.job_id == job.job_id).unwrap();
                    assert!(pending.confirmation_pending() && !pending.runnable(now));
                    assert!(!wifi_job_wants_window(&pending, now));
                    let next=svc.wifi_queue_control(mac, "bridge", "activate", json!({"network_id":"another"}), now).unwrap();
                    assert_ne!(next.request_id, pending.request_id);
                    assert_eq!(serde_json::to_value(svc.wifi_state(mac).jobs.into_iter().find(|j|j.job_id==pending.job_id).unwrap()).unwrap(),serde_json::to_value(&pending).unwrap());
                    for later in [now, now+61, now+86400] {
                        assert!(ready(svc.wifi_attempt_async(mac, &job.job_id, &job.bridge_id, "own", false, &Default::default(), later,&mut
                            |_: &str, _: Value|panic!("cancelled/expired transactions must not automatically query or write"))).unwrap().is_none());
                        svc.wifi_finish(mac,&job.job_id,"awaiting_confirmation",Some("rendezvous_failed"),later).unwrap();
                    }
                    drop(svc);
                    let svc=PlatformService::open(dir.path()).unwrap();
                    let result=svc.wifi_state(mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap();
                    assert_eq!(result.execution_intent, if cancelled { ExecutionIntent::Cancelled } else { ExecutionIntent::Expired });
                    assert_eq!(result.state, if cancelled {"cancelled"} else {"expired"});
                    assert_eq!(result.request_id,job.request_id);
                    assert!(result.confirmation_pending());
                    assert!(!result.receipt_ack_pending);
            }
        }
    }

    #[test]
    fn historical_settlement_preserves_execution_and_cache() {
        for kind in ["sync","recovery_save","trial","activate"] {
            let dir=tempfile::tempdir().unwrap();let mac="AABBCCDDEEFF";
            let svc=service_with_device(dir.path());
            svc.wifi_save(mac,0,serde_json::from_value(json!([
                {"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"}
            ])).unwrap(),100).unwrap();
            let read=svc.wifi_queue(mac,"bridge","read",vec![],100).unwrap();
            svc.wifi_complete_read(mac,&read.job_id,serde_json::from_value(json!({
                "schema":2,"device_mac":mac,"revision":7,"snapshot_id":"before","complete":true,
                "observed_at":100,"networks":[],"capacity":{"total":12}
            })).unwrap(),100).unwrap();
            let job=if matches!(kind,"trial"|"activate") {
                svc.wifi_queue_control(mac,"bridge",kind,json!({"network_id":"home","version":7}),101).unwrap()
            } else {svc.wifi_queue(mac,"bridge",kind,vec!["home".into()],101).unwrap()};
            svc.wifi_bind_transaction(mac,&job.job_id,"bridge","own").unwrap();
            svc.wifi_mark_sent(mac,&job.job_id,102).unwrap();
            svc.wifi_finish(mac,&job.job_id,"cancelled",None,103).unwrap();
            let before=svc.wifi_state(mac);let mut ops=Vec::new();
            let settled=ready(svc.wifi_settle_async(mac,&job.job_id,"bridge","own",104,&mut |op: &str, body: Value| {
                ops.push(op.to_owned());assert_eq!(body["query_id"],job.request_id);
                Ok(if op=="result" {json!({"ok":true,"outcome":"committed","revision":8,"bond":"own"})}
                    else {json!({"ok":true,"outcome":"retired"})})
            })).unwrap().unwrap();
            assert_eq!(ops,["result","result_ack"]);assert_eq!(settled.state,"cancelled");
            assert_eq!(settled.execution_intent,crate::platform::wifi::ExecutionIntent::Cancelled);
            assert!(!settled.confirmation_pending() && !settled.receipt_ack_pending && !settled.runnable(200));
            let after=svc.wifi_state(mac);
            assert_eq!(serde_json::to_value(after.snapshot).unwrap(),serde_json::to_value(before.snapshot).unwrap());
            assert!(after.targets==before.targets);assert_eq!(after.baseline,before.baseline);
            drop(svc);let svc=PlatformService::open(dir.path()).unwrap();
            assert!(ready(svc.wifi_settle_async(mac,&job.job_id,"bridge","own",200,&mut |_: &str, _: Value|panic!("already settled"))).unwrap().is_none());
        }
    }

    #[test]
    fn historical_settlement_ack_loss_restart_and_finite_budget() {
        let dir=tempfile::tempdir().unwrap();let mac="AABBCCDDEEFF";
        let svc=service_with_device(dir.path());
        let job=svc.wifi_queue_control(mac,"bridge","activate",json!({"network_id":"home","version":7}),100).unwrap();
        svc.wifi_bind_transaction(mac,&job.job_id,"bridge","own").unwrap();
        svc.wifi_mark_sent(mac,&job.job_id,101).unwrap();
        svc.wifi_finish(mac,&job.job_id,"cancelled",None,102).unwrap();
        for (principal,namespace) in [("other","own"),("bridge","other")] {
            assert!(ready(svc.wifi_settle_async(mac,&job.job_id,principal,namespace,103,&mut |_: &str, _: Value|panic!("identity mismatch must not send"))).is_err());
        }
        for now in [103,164,225] {
            ready(svc.wifi_settle_async(mac,&job.job_id,"bridge","own",now,&mut |op: &str, _: Value| {
                assert_eq!(op,"result");Ok(json!({"ok":true,"outcome":"unknown"}))
            })).unwrap();
        }
        drop(svc);let svc=PlatformService::open(dir.path()).unwrap();
        assert!(ready(svc.wifi_settle_async(mac,&job.job_id,"bridge","own",1000,&mut |_: &str, _: Value|panic!("finite maintenance budget"))).unwrap().is_none());
        svc.wifi_retry_settlement(mac,&job.job_id).unwrap();
        let pending=ready(svc.wifi_settle_async(mac,&job.job_id,"bridge","own",1001,&mut |op: &str, _: Value| {
            if op=="result" {Ok(json!({"ok":true,"outcome":"committed","revision":8}))}
            else {Err(anyhow::anyhow!("transport_timeout"))}
        })).unwrap().unwrap();
        assert!(!pending.confirmation_pending() && pending.receipt_ack_pending);
        drop(svc);let svc=PlatformService::open(dir.path()).unwrap();
        let done=ready(svc.wifi_settle_async(mac,&job.job_id,"bridge","own",1062,&mut |op: &str, body: Value| {
            assert_eq!(op,"result_ack");assert_eq!(body["query_id"],job.request_id);
            Ok(json!({"ok":true,"outcome":"retired"}))
        })).unwrap().unwrap();
        assert_eq!(done.state,"cancelled");assert!(!done.receipt_ack_pending);
    }

    #[test]
    fn historical_settlement_unknown_evidence_never_acknowledges() {
        for reply in [json!({"ok":false,"error":"storage"}),
            json!({"ok":true,"outcome":"retired"}),json!({"ok":true,"outcome":"accepted"}),
            json!({"ok":false,"outcome":"rejected","error":"result_unknown"}),
            json!({"ok":true,"outcome":"committed","revision":8,"bond":"another"})] {
            let dir=tempfile::tempdir().unwrap();let mac="AABBCCDDEEFF";
            let svc=service_with_device(dir.path());
            let job=svc.wifi_queue_control(mac,"bridge","activate",json!({"network_id":"home","version":7}),100).unwrap();
            svc.wifi_bind_transaction(mac,&job.job_id,"bridge","own").unwrap();svc.wifi_mark_sent(mac,&job.job_id,101).unwrap();
            svc.wifi_finish(mac,&job.job_id,"cancelled",None,102).unwrap();
            let done=ready(svc.wifi_settle_async(mac,&job.job_id,"bridge","own",103,&mut |op: &str, _: Value| {
                assert_eq!(op,"result","ambiguous evidence must not release device receipt");Ok(reply.clone())
            })).unwrap().unwrap();
            assert!(done.confirmation_pending() && !done.receipt_ack_pending);
            assert!(done.settlement.unwrap().error.is_some());
        }
    }

    #[test]
    fn accepted_control_is_normal_and_queries_keep_original_identity() {
        let dir=tempfile::tempdir().unwrap();let mac="AABBCCDDEEFF";
        let svc=service_with_device(dir.path());
        let job=svc.wifi_queue_control(mac,"bridge","activate",json!({"network_id":"home","version":7}),100).unwrap();
        let accepted=ready(svc.wifi_attempt_async(mac,&job.job_id,"bridge","own",true,&Default::default(),101,&mut |op: &str, body: Value| {
            assert_eq!(op,"activate");assert_eq!(body["request_id"],job.request_id);
            Ok(json!({"ok":true,"stage":"accepted"}))
        })).unwrap().unwrap();
        assert!(accepted.permits_network_start());assert!(accepted.error_code.is_none());
        assert!(accepted.blocks_mutations());assert!(!accepted.receipt_ack_pending);
        drop(svc);let svc=PlatformService::open(dir.path()).unwrap();
        let restored=svc.wifi_state(mac).jobs.remove(0);
        assert!(restored.permits_network_start());assert_eq!(restored.transaction_namespace.as_deref(),Some("own"));
        for (now,reply,allowed) in [(107,json!({"ok":true,"outcome":"accepted"}),true),
            (125,json!({"ok":false,"error":"busy"}),false),
            (190,json!({"ok":true,"outcome":"accepted"}),false)] {
            let result=ready(svc.wifi_attempt_async(mac,&job.job_id,"bridge","own",true,&Default::default(),now,&mut |op: &str, body: Value| {
                assert_eq!(op,"result");assert_eq!(body["query_id"],job.request_id);Ok(reply.clone())
            })).unwrap().unwrap();
            assert_eq!(result.permits_network_start(),allowed);
            assert!(result.confirmation_pending());assert!(!result.receipt_ack_pending);
        }
        assert_eq!(svc.wifi_state(mac).jobs[0].error_code.as_deref(),Some("confirmation_budget_exhausted"));
    }

    #[test]
    fn ambiguous_control_reply_never_permits_network_start() {
        for reply in [json!({"ok":true}),json!({"ok":true,"outcome":"unknown"}),
            json!({"ok":false,"error":"storage_unknown"})] {
            let dir=tempfile::tempdir().unwrap();let mac="AABBCCDDEEFF";
            let svc=service_with_device(dir.path());
            let job=svc.wifi_queue_control(mac,"bridge","activate",json!({"network_id":"home","version":7}),100).unwrap();
            let result=ready(svc.wifi_attempt_async(mac,&job.job_id,"bridge","own",true,&Default::default(),101,&mut |_: &str, _: Value|Ok(reply.clone()))).unwrap().unwrap();
            assert!(!result.permits_network_start());assert!(result.blocks_mutations());
            assert!(result.error_code.is_some());
            // Explicit migration of the previous ambiguous error projection.
            let mut state=svc.wifi_state(mac);state.jobs[0].error_code=Some("operation_pending".into());
            state.recover();assert_eq!(state.jobs[0].error_code.as_deref(),Some("result_unknown"));
            assert_eq!(state.jobs[0].request_id,job.request_id);assert!(state.jobs[0].sent);
            assert!(!state.jobs[0].permits_network_start());
        }
    }

    #[test]
    fn historical_settlement_releases_job_capacity_only_after_ack() {
        let dir=tempfile::tempdir().unwrap();let mac="AABBCCDDEEFF";
        let svc=service_with_device(dir.path());let mut first=None;
        for n in 0..32 {
            let job=svc.wifi_queue_control(mac,"bridge","activate",json!({"network_id":"home","version":7}),100+n).unwrap();
            svc.wifi_bind_transaction(mac,&job.job_id,"bridge","own").unwrap();svc.wifi_mark_sent(mac,&job.job_id,101+n).unwrap();
            svc.wifi_finish(mac,&job.job_id,"cancelled",None,102+n).unwrap();
            if first.is_none(){first=Some(job);}
        }
        assert!(svc.wifi_queue_control(mac,"bridge","activate",json!({"network_id":"home","version":7}),200).is_err());
        let first=first.unwrap();
        let pending=ready(svc.wifi_settle_async(mac,&first.job_id,"bridge","own",201,&mut |op: &str, _: Value| {
            if op=="result" {Ok(json!({"ok":true,"outcome":"committed","revision":8}))}
            else {Err(anyhow::anyhow!("transport_timeout"))}
        })).unwrap().unwrap();
        assert!(pending.receipt_ack_pending);
        assert!(svc.wifi_queue_control(mac,"bridge","activate",json!({"network_id":"home","version":7}),202).is_err());
        ready(svc.wifi_settle_async(mac,&first.job_id,"bridge","own",262,&mut |op: &str, _: Value| {
            assert_eq!(op,"result_ack");Ok(json!({"ok":true,"outcome":"retired"}))
        })).unwrap();
        svc.wifi_queue_control(mac,"bridge","activate",json!({"network_id":"home","version":7}),263).unwrap();
        assert_eq!(svc.wifi_state(mac).jobs.len(),32);
        assert!(!svc.wifi_state(mac).jobs.iter().any(|j|j.job_id==first.job_id));
    }

    #[test]
    fn unknown_confirmation_budget_survives_restart_and_releases_execution() {
        for query_fails in [false,true] {
            let dir=tempfile::tempdir().unwrap();let mac="AABBCCDDEEFF";
            let svc=service_with_device(dir.path());
            let job=svc.wifi_queue_control(mac,"bridge","activate",json!({"network_id":"home","version":7}),100).unwrap();
            drop(svc);
            for attempt in 0..4 {
                let svc=PlatformService::open(dir.path()).unwrap();
                let result=ready(svc.wifi_attempt_async(mac,&job.job_id,"bridge","own",true,&Default::default(),101+attempt*100,&mut |op: &str, body: Value| {
                    if attempt==0 {
                        assert_eq!(op,"activate");assert_eq!(body["request_id"],job.request_id);
                        return Err(anyhow::anyhow!("transport_timeout"));
                    }
                    assert_eq!(op,"result");assert_eq!(body["query_id"],job.request_id);
                    if query_fails {Err(anyhow::anyhow!("transport_timeout"))}
                    else {Ok(json!({"ok":true,"outcome":"unknown"}))}
                })).unwrap().unwrap();
                assert_eq!(result.terminal(),attempt==3);
                assert!(result.confirmation_pending());
            }
            let svc=PlatformService::open(dir.path()).unwrap();
            let historical=svc.wifi_state(mac).jobs[0].clone();
            assert_eq!(historical.error_code.as_deref(),Some("confirmation_budget_exhausted"));
            assert!(ready(svc.wifi_attempt_async(mac,&job.job_id,"bridge","own",true,&Default::default(),600,&mut |_: &str, _: Value|panic!("ended execution must never query or resend"))).unwrap().is_none());
            let next=svc.wifi_queue_control(mac,"bridge","activate",json!({"network_id":"office","version":8}),600).unwrap();
            assert_ne!(next.request_id,historical.request_id);
            assert_eq!(serde_json::to_value(svc.wifi_state(mac).jobs[0].clone()).unwrap(),serde_json::to_value(historical).unwrap());
        }
    }

    #[test]
    fn protocol_query_failure_preserves_control_transaction_across_restart() {
        for kind in ["trial", "activate"] {
            for code in ["storage", "session_expired", "claim_required", "occupied", "busy", "unauthorized", "storage_unknown"] {
                let dir = tempfile::tempdir().unwrap();
                let svc = service_with_device(dir.path());
                let mac = "AABBCCDDEEFF";
                let job = svc.wifi_queue_control(mac, "bridge", kind,
                    json!({"network_id":"home","version":1}), 100).unwrap();
                let unknown = ready(svc.wifi_attempt_async(mac, &job.job_id, &job.bridge_id, "own", true, &Default::default(), 101,&mut  |op: &str, _: Value| {
                    assert_eq!(op, kind);
                    Err(anyhow::anyhow!("transport_timeout"))
                })).unwrap().unwrap();
                assert!(unknown.sent && unknown.may_have_committed);
                drop(svc);
                let svc = PlatformService::open(dir.path()).unwrap();
                let failed_query = ready(svc.wifi_attempt_async(mac, &job.job_id, &job.bridge_id, "own", false, &Default::default(), 200,&mut  |op: &str, body: Value| {
                    assert_eq!(op, "result");
                    assert_eq!(body["query_id"], job.request_id);
                    Ok(json!({"ok":false,"error":code}))
                })).unwrap().unwrap();
                assert_eq!(failed_query.state, "awaiting_confirmation", "{kind}/{code}");
                assert!(failed_query.sent && failed_query.may_have_committed, "{kind}/{code}");
                assert_eq!(failed_query.error_code.as_deref(), Some(code));
                assert!(!failed_query.receipt_ack_pending);
                drop(svc);
                let svc = PlatformService::open(dir.path()).unwrap();
                let confirmed = ready(svc.wifi_attempt_async(mac, &job.job_id, &job.bridge_id, "own", true, &Default::default(), 300,&mut  |op: &str, body: Value| {
                    assert_eq!(op, "result", "query failure must never authorize another write");
                    assert_eq!(body["query_id"], job.request_id);
                    Ok(json!({"ok":true,"outcome":"completed","revision":2}))
                })).unwrap().unwrap();
                assert_eq!(confirmed.state, "completed");
                assert!(confirmed.receipt_ack_pending);
                drop(svc);
                let svc = PlatformService::open(dir.path()).unwrap();
                let persisted = svc.wifi_state(mac).jobs.into_iter().find(|j| j.job_id == job.job_id).unwrap();
                assert_eq!(persisted.confirmation.as_deref(), Some("operation_result"));
                assert!(persisted.receipt_ack_pending);
            }
        }
    }

    #[test]
    fn protocol_control_rejections_distinguish_write_from_result_query() {
        for kind in ["trial", "activate"] {
            for previously_sent in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let svc = service_with_device(dir.path());
                let mac = "AABBCCDDEEFF";
                let job = svc.wifi_queue_control(mac, "bridge", kind,
                    json!({"network_id":"home","version":1}), 100).unwrap();
                if previously_sent {
                    ready(svc.wifi_attempt_async(mac, &job.job_id, &job.bridge_id, "own", true, &Default::default(), 101,&mut  |op: &str, _: Value| {
                        assert_eq!(op, kind);
                        Err(anyhow::anyhow!("transport_timeout"))
                    })).unwrap();
                }
                let result = ready(svc.wifi_attempt_async(mac, &job.job_id, &job.bridge_id, "own", true, &Default::default(), 200,&mut  |op: &str, _: Value| {
                    assert_eq!(op, if previously_sent { "result" } else { kind });
                    // An unsuccessful query's envelope is not a durable refusal.
                    Ok(json!({"ok":false,"outcome":"rejected","error":"storage"}))
                })).unwrap().unwrap();
                assert_eq!(result.state, if previously_sent { "awaiting_confirmation" } else { "failed" });
                if previously_sent {
                    assert!(result.sent && result.may_have_committed);
                    assert!(result.confirmation.is_none() && !result.receipt_ack_pending);
                    let confirmed = ready(svc.wifi_attempt_async(mac, &job.job_id, &job.bridge_id, "own", false, &Default::default(), 300,&mut  |op: &str, body: Value| {
                        assert_eq!(op, "result");
                        assert_eq!(body["query_id"], job.request_id);
                        Ok(json!({"ok":true,"outcome":"rejected","error":"authentication_failed"}))
                    })).unwrap().unwrap();
                    assert_eq!(confirmed.confirmation.as_deref(), Some("durable_rejection"));
                    assert!(confirmed.receipt_ack_pending);
                }
            }
        }
    }

    #[test]
    fn protocol_auth_rejection_is_terminal_only_for_the_current_write() {
        for code in [401,403] {
            for previously_sent in [false,true] {
                let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
                let job=svc.wifi_queue_control(mac,"bridge","activate",json!({"network_id":"home","version":1}),100).unwrap();
                if previously_sent {
                    let unknown=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&Default::default(),101,&mut |op: &str, _: Value| {
                        assert_eq!(op,"activate");Err(anyhow::anyhow!("transport_timeout"))
                    })).unwrap().unwrap();
                    assert_eq!(unknown.state,"awaiting_confirmation");
                }
                let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&Default::default(),200,&mut |op: &str, _: Value| {
                    assert_eq!(op,if previously_sent {"result"} else {"activate"});
                    Err(crate::device_client::HttpStatus(code).into())
                })).unwrap().unwrap();
                assert_eq!(result.request_id,job.request_id);
                assert_eq!(result.sent,previously_sent);
                assert_eq!(result.state,if previously_sent {"awaiting_confirmation"} else {"failed"});
                assert_eq!(result.error_code.as_deref(),Some("unauthorized"));
            }
        }
    }

    #[test]
    fn wifi_only_local_non_dispatch_can_clear_this_attempt_sent_marker() {
        for local in [true,false] {
            let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
            let read=svc.wifi_queue(mac,"bridge","read",vec![],100).unwrap();
            let snapshot=serde_json::from_value(json!({"schema":2,"device_mac":mac,"revision":1,"snapshot_id":"budget",
                "complete":true,"observed_at":100,"networks":[],"capacity":{"total":12}})).unwrap();
            svc.wifi_complete_read(mac,&read.job_id,snapshot,100).unwrap();
            let targets=serde_json::from_value(json!([{"network_id":"home","host_ssid":"host","ssid":"device","credential":"open"}])).unwrap();
            svc.wifi_save(mac,0,targets,101).unwrap();let job=svc.wifi_queue(mac,"bridge","sync",vec!["home".into()],102).unwrap();
            let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&Default::default(),103,&mut |op: &str, _: Value| {
                assert_eq!(op,"apply");
                Err(if local {crate::rendezvous::NotDispatched.into()} else {anyhow::anyhow!("wifi_window_budget")})
            })).unwrap().unwrap();
            assert_eq!(result.sent,!local);assert_eq!(result.may_have_committed,!local);
            assert_eq!(result.state,if local {"waiting_rendezvous"} else {"awaiting_confirmation"});
            if !local {
                let query=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",false,&Default::default(),200,&mut |op: &str, _: Value| {
                    assert_eq!(op,"result");Err(crate::rendezvous::NotDispatched.into())
                })).unwrap().unwrap();assert!(query.sent);
            }
        }
    }

    #[test]
    fn wifi_dispatch_failure_preserves_original_transaction_fence() {
        use crate::rendezvous::{CommandFailure,DispatchOutcome};
        for outcome in [DispatchOutcome::NotSent,DispatchOutcome::WriteUnknown,DispatchOutcome::AckUnknown] {
            let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
            let job=svc.wifi_queue_control(mac,"bridge","activate",json!({"network_id":"home","version":1}),100).unwrap();
            let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&Default::default(),101,&mut |op: &str, _: Value| {
                assert_eq!(op,"activate");Err(CommandFailure{outcome,stage:"write",reason:"window_budget_exhausted"}.into())
            })).unwrap().unwrap();
            let sent=outcome!=DispatchOutcome::NotSent;
            assert_eq!(result.sent,sent);assert_eq!(result.may_have_committed,sent);
            assert_eq!(result.state,if sent {"awaiting_confirmation"}else{"waiting_rendezvous"});
            assert_eq!(result.error_code.as_deref(),Some(if sent {"result_unknown"}else{"not_sent"}));
            let next=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&Default::default(),200,&mut |op: &str, body: Value| {
                assert_eq!(op,if sent {"result"}else{"activate"});
                assert_eq!(body[if sent {"query_id"}else{"request_id"}],job.request_id);
                Err(CommandFailure{outcome:DispatchOutcome::NotSent,stage:"before_write",reason:"window_budget_exhausted"}.into())
            })).unwrap().unwrap();
            assert_eq!(next.request_id,job.request_id);assert_eq!(next.sent,sent);
        }
    }

    #[test]
    fn wifi_whole_row_rename_then_delete_preserves_identity_after_restart() {
        let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
        let mut snapshot:WifiSnapshot=serde_json::from_value(json!({"schema":2,"device_mac":mac,"revision":7,"snapshot_id":"edit-fixture",
            "complete":true,"observed_at":100,"networks":[{"bond":"own","writable":true,"network":{"network_id":"home","host_ssid":"Host","ssid":"Device","label":"Original","version":7,"password":"fixture-pass"}}],"capacity":{"total":12}})).unwrap();
        let read=svc.wifi_queue(mac,"bridge","read",vec![],100).unwrap();svc.wifi_complete_read(mac,&read.job_id,snapshot.clone(),101).unwrap();
        svc.wifi_import(mac,&["home".into()],0,102).unwrap();
        for deleted in [false,true] {
            let state=svc.wifi_state(mac);let mut targets=state.targets;
            targets[0].label="Renamed".into();targets[0].deleted=deleted;
            svc.wifi_save(mac,state.local_revision,targets,103).unwrap();
            let job=svc.wifi_queue(mac,"bridge","sync",vec!["home".into()],104).unwrap();
            let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&BTreeMap::new(),105,&mut |op: &str, body: Value| {
                if op=="apply" {
                    assert_eq!(body["changes"].as_array().unwrap().len(),1);
                    assert_eq!(body["changes"][0]["network_id"],"home");assert_eq!(body["changes"][0]["deleted"],deleted);
                    snapshot.revision+=1;
                    if deleted {snapshot.networks.clear();}
                    else {snapshot.networks[0].network.label="Renamed".into();snapshot.networks[0].network.version=snapshot.revision;}
                    Ok(json!({"ok":true,"outcome":"committed","revision":snapshot.revision}))
                }else {Ok(serde_json::to_value(&snapshot)?)}
            })).unwrap().unwrap();assert_eq!(result.state,"completed");
            assert_eq!(svc.wifi_state(mac).targets.len(),1);
        }
        drop(svc);let state=PlatformService::open(dir.path()).unwrap().wifi_state(mac);
        assert_eq!(state.targets[0].network_id,"home");assert_eq!(state.targets[0].label,"Renamed");assert!(state.targets[0].deleted);
        assert!(state.snapshot.unwrap().networks.is_empty());
    }

    #[test]
    fn wifi_imported_other_bond_is_editable_locally_and_preserves_device_snapshot() {
        let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
        let snapshot:WifiSnapshot=serde_json::from_value(json!({"schema":2,"device_mac":mac,"revision":3,"snapshot_id":"other-bond",
            "complete":true,"observed_at":100,"networks":[{"bond":"other","writable":false,"network":{"network_id":"home","host_ssid":"Host","ssid":"Device","label":"Imported","version":7}}],"capacity":{"total":12}})).unwrap();
        let read=svc.wifi_queue(mac,"bridge","read",vec![],100).unwrap();svc.wifi_complete_read(mac,&read.job_id,snapshot.clone(),101).unwrap();
        svc.wifi_import(mac,&["home".into()],0,102).unwrap();
        let state=svc.wifi_state(mac);assert_eq!(state.targets[0].origin,"local");
        let mut targets=state.targets;targets[0].label="Edited".into();targets[0].ssid="Changed-2G".into();targets[0].password=Some("fixture-pass".into());
        // Old persisted imports also become local configurations on explicit save.
        targets[0].origin="other".into();
        let saved=svc.wifi_save(mac,state.local_revision,targets,103).unwrap();
        assert_eq!(saved.targets.len(),1);assert_eq!(saved.targets[0].origin,"local");
        assert_eq!(saved.snapshot.as_ref().unwrap().networks[0].network.label,"Imported");
        assert!(!saved.snapshot.as_ref().unwrap().networks[0].writable);
        assert!(!saved.differences()[0]["writable"].as_bool().unwrap());
        // Local editing must not relax the device transport's binding checks.
        let job=svc.wifi_queue(mac,"bridge","sync",vec!["home".into()],104).unwrap();
        let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",false,&BTreeMap::new(),105,&mut |op: &str, _: Value| {
            assert_ne!(op,"apply","Missing write permission must never reach apply");
            Ok(serde_json::to_value(&snapshot)?)
        })).unwrap().unwrap();assert_ne!(result.state,"completed");
        drop(svc);let state=PlatformService::open(dir.path()).unwrap().wifi_state(mac);
        assert_eq!(state.targets[0].network_id,"home");assert_eq!(state.targets[0].version,7);
        assert_eq!(state.targets[0].ssid,"Changed-2G");assert_eq!(state.targets[0].password.as_deref(),Some("fixture-pass"));
        assert_eq!(state.snapshot.unwrap().networks[0].network.label,"Imported");
    }

    #[test]
    fn wifi_import_edit_sync_roundtrip_is_independent_of_source_bond() {
        for foreign in [false,true] {
            let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
            let snapshot:WifiSnapshot=serde_json::from_value(json!({"schema":2,"device_mac":mac,"revision":3,"snapshot_id":"roundtrip",
                "complete":true,"observed_at":100,"networks":[{"bond":if foreign{"other"}else{"own"},"writable":!foreign,
                "network":{"network_id":"home","host_ssid":"Host","ssid":"Device","label":"Source","version":3}}],"capacity":{"total":12}})).unwrap();
            let read=svc.wifi_queue(mac,"bridge","read",vec![],100).unwrap();svc.wifi_complete_read(mac,&read.job_id,snapshot,101).unwrap();
            svc.wifi_import(mac,&["home".into()],0,102).unwrap();
            let mut targets=svc.wifi_state(mac).targets;targets[0].ssid="Edited-2G".into();targets[0].label="Edited".into();targets[0].password=Some("fixture-pass".into());
            svc.wifi_save(mac,1,targets,103).unwrap();
            let job=svc.wifi_queue_manual_sync(mac,"bridge",vec!["home".into()],2,"roundtrip","Host",104).unwrap();
            let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&BTreeMap::new(),105,&mut |op: &str, body: Value|{
                assert_eq!(op,"apply");assert_eq!(body["changes"].as_array().unwrap().len(),1);
                assert_eq!(body["changes"][0]["network_id"],"home");assert_eq!(body["changes"][0]["ssid"],"Edited-2G");
                assert_eq!(body["changes"][0]["password"],"fixture-pass");
                Ok(json!({"ok":true,"outcome":"committed","revision":4,"bond":"own"}))
            })).unwrap().unwrap();assert_eq!(result.state,"completed");
            let state=svc.wifi_state(mac);let rows=&state.snapshot.as_ref().unwrap().networks;
            assert_eq!(rows.len(),if foreign{2}else{1});
            assert_eq!(rows.iter().find(|r|r.writable).unwrap().network.ssid,"Edited-2G");
            if foreign {assert_eq!(rows.iter().find(|r|!r.writable).unwrap().network.ssid,"Device");}
            assert_eq!(state.differences()[0]["state"],"equal","Current target comparison must not use the old source row");
            svc.wifi_import(mac,&["home".into()],2,106).unwrap();
            assert_eq!(svc.wifi_state(mac).targets[0].ssid,"Edited-2G","Reimport must choose the current target version");
            drop(svc);let reopened=PlatformService::open(dir.path()).unwrap().wifi_state(mac);
            assert_eq!(reopened.targets.len(),1);assert_eq!(reopened.targets[0].network_id,"home");assert_eq!(reopened.targets[0].origin,"local");
        }
    }

    #[test]
    fn wifi_shared_configuration_sync_preserves_creator_provenance() {
        for foreign in [false,true] {
            let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
            let snapshot:WifiSnapshot=serde_json::from_value(json!({"schema":2,"device_mac":mac,"revision":3,"snapshot_id":"roundtrip",
                "complete":true,"observed_at":100,"networks":[{"bond":if foreign{"other"}else{"own"},"writable":true,
                "network":{"network_id":"home","host_ssid":"Host","ssid":"Device","label":"Source","version":3}}],"capacity":{"total":12}})).unwrap();
            let read=svc.wifi_queue(mac,"bridge","read",vec![],100).unwrap();svc.wifi_complete_read(mac,&read.job_id,snapshot,101).unwrap();
            svc.wifi_import(mac,&["home".into()],0,102).unwrap();
            let mut targets=svc.wifi_state(mac).targets;targets[0].ssid="Edited-2G".into();targets[0].label="Edited".into();targets[0].password=Some("fixture-pass".into());
            svc.wifi_save(mac,1,targets,103).unwrap();
            let job=svc.wifi_queue_manual_sync(mac,"bridge",vec!["home".into()],2,"roundtrip","Host",104).unwrap();
            let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&BTreeMap::new(),105,&mut |op: &str, body: Value|{
                assert_eq!(op,"apply");assert_eq!(body["changes"].as_array().unwrap().len(),1);
                assert_eq!(body["changes"][0]["network_id"],"home");assert_eq!(body["changes"][0]["ssid"],"Edited-2G");
                assert_eq!(body["changes"][0]["password"],"fixture-pass");
                Ok(json!({"ok":true,"outcome":"committed","revision":4,"bond":"own"}))
            })).unwrap().unwrap();assert_eq!(result.state,"completed");
            let state=svc.wifi_state(mac);let rows=&state.snapshot.as_ref().unwrap().networks;
            assert_eq!(rows.len(),1);
            assert_eq!(rows[0].bond,if foreign{"other"}else{"own"});
            assert_eq!(rows.iter().find(|r|r.writable).unwrap().network.ssid,"Edited-2G");
            assert_eq!(state.differences()[0]["state"],"equal","Current target comparison must not use the old source row");
            svc.wifi_import(mac,&["home".into()],2,106).unwrap();
            assert_eq!(svc.wifi_state(mac).targets[0].ssid,"Edited-2G","Reimport must choose the current target version");
            drop(svc);let reopened=PlatformService::open(dir.path()).unwrap().wifi_state(mac);
            assert_eq!(reopened.targets.len(),1);assert_eq!(reopened.targets[0].network_id,"home");assert_eq!(reopened.targets[0].origin,"local");
        }
    }

    #[test]
    fn wifi_cached_password_survives_restart_import_and_sync_without_input() {
        let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
        let snapshot:WifiSnapshot=serde_json::from_value(json!({"schema":2,"device_mac":mac,"revision":1,"snapshot_id":"cached-password",
            "complete":true,"observed_at":100,"networks":[{"bond":"own","writable":true,"network":{"network_id":"home","host_ssid":"Host","ssid":"Device","password":"fixture-pass"}}],"capacity":{"total":12}})).unwrap();
        let read=svc.wifi_queue(mac,"bridge","read",vec![],100).unwrap();svc.wifi_complete_read(mac,&read.job_id,snapshot.clone(),101).unwrap();
        svc.wifi_import_fields_checked(mac,&["home".into()],&BTreeMap::new(),0,Some("cached-password"),102).unwrap();
        assert_eq!(svc.wifi_state(mac).targets[0].password.as_deref(),Some("fixture-pass"));
        drop(svc);let svc=PlatformService::open(dir.path()).unwrap();
        let mut targets=svc.wifi_state(mac).targets;assert_eq!(targets[0].password.as_deref(),Some("fixture-pass"));
        assert!(!format!("{:?}",targets[0]).contains("fixture-pass"));
        let mut compared=svc.wifi_state(mac);compared.targets[0].password=Some("changed-pass".into());
        assert_eq!(compared.differences()[0]["state"],"conflict");
        assert_eq!(compared.differences()[0]["password"],"different");
        targets[0].credential="replace".into();svc.wifi_save(mac,1,targets,103).unwrap();
        let job=svc.wifi_queue(mac,"bridge","sync",vec!["home".into()],104).unwrap();
        let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&BTreeMap::new(),105,&mut |op: &str, body: Value| {
            if op=="apply" {assert_eq!(body["changes"][0]["password"],"fixture-pass");Ok(json!({"ok":true,"outcome":"committed","revision":2}))}
            else {Ok(serde_json::to_value(&snapshot)?)}
        })).unwrap().unwrap();assert_eq!(result.state,"completed");
        #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;assert_eq!(std::fs::metadata(svc.state_path()).unwrap().permissions().mode()&0o777,0o600);}
    }

    #[test]
    fn wifi_recovery_consumes_confirmed_credentials_but_preserves_new_local_edits() {
        for edit in [false,true] {
            let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
            let target:WifiNetwork=serde_json::from_value(json!({"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"replace"})).unwrap();
            svc.wifi_save(mac,0,vec![target.clone()],100).unwrap();
            let job=svc.wifi_queue(mac,"bridge","recovery_save",vec!["home".into()],101).unwrap();
            svc.wifi_bind_transaction(mac,&job.job_id,"bridge","own").unwrap();
            svc.wifi_mark_sent(mac,&job.job_id,102).unwrap();
            svc.wifi_finish(mac,&job.job_id,"awaiting_confirmation",None,102).unwrap();
            if edit {let mut changed=target.clone();changed.ssid="New intent".into();svc.wifi_save(mac,1,vec![changed],103).unwrap();}
            ready(svc.wifi_attempt_async(mac,&job.job_id,"bridge","own",false,&BTreeMap::new(),200,&mut |op: &str, body: Value| {
                assert_eq!(op,"result");assert_eq!(body["query_id"],job.request_id);
                Ok(json!({"ok":true,"outcome":"committed","revision":7,"bond":"own"}))
            })).unwrap().unwrap();
            let state=svc.wifi_state(mac);
            assert_eq!(state.targets[0].credential,if edit{"replace"}else{"keep"});
            assert_eq!(state.targets[0].version,if edit{0}else{7});
            assert_eq!(state.targets[0].ssid,if edit{"New intent"}else{"Device"});
            assert!(state.snapshot.is_none()); // no prerequisite full-device read
        }
    }

    #[test]
    fn saved_network_connection_work_has_a_bounded_plan_and_stale_or_rejected_work_does_not() {
        let now=crate::now_secs();let mac="AA:BB:CC:DD:EE:FF";
        for (kind,error,attempts,due,wanted) in [("waiting_for_window",None,0,true,true),("selected",None,0,true,false),
            ("connection_failed",Some("network_not_found"),1,true,true),("connection_failed",Some("authentication_failed"),1,true,false),
            ("connection_failed",Some("network_not_found"),3,true,false),("connection_failed",Some("network_not_found"),1,false,false)] {
            let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());
            svc.wifi_save(mac,0,vec![serde_json::from_value(json!({"network_id":"home","host_ssid":"Host","ssid":"Device"})).unwrap()],now).unwrap();
            let c=super::super::wifi::Connection{host_ssid:"Host".into(),network_id:"home".into(),local_revision:1,version:1,
                state:kind.into(),error:error.map(str::to_owned),reason:0,observed_at:now,attempts,next_attempt_at:if due{now}else{now+60},http_ready:false,bridge_error:None,actual_ssid:None,ip:None};
            svc.wifi_note_connection(mac,c.clone()).unwrap();
            assert_eq!(svc.plan_for_rendezvous(mac,now,"rendezvous",0).unwrap().mode,PlanMode::Sleep);
            let plan=svc.plan_for_rendezvous_network_work(mac,now,"rendezvous",0,true).unwrap();
            assert_eq!(plan.mode==PlanMode::Light,wanted,"{kind}/{error:?}/{attempts}/{due}");
            if wanted {assert!(plan.light_duration_s<=120);}
            let mut stale=c;stale.local_revision=0;assert!(svc.wifi_note_connection(mac,stale).is_err());
        }
    }

    #[test]
    fn rendezvous_ordinary_plan_persists_and_http_recovers_lost_ack() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        let mac = "AA:BB:CC:DD:EE:FF";
        let now = crate::now_secs();
        svc.note_device_status_at(mac, &json!({"power":{"plan_id":100,"mode":"sleep","remaining_s":0}}), "ble", now).unwrap();
        svc.wifi_queue(mac, "bridge", "read", vec![], now).unwrap();
        let plan = svc.plan_for_rendezvous(mac, now, "rendezvous", 0).unwrap();
        assert_eq!(plan.plan_id, 101);
        drop(svc);
        let resumed = PlatformService::open(dir.path()).unwrap();
        assert_eq!(resumed.plan_for_rendezvous(mac, now+1, "rendezvous", 0).unwrap(), plan);
        resumed.note_device_status_at(mac, &json!({"power":{"plan_id":plan.plan_id,"mode":"light","remaining_s":115}}), "http", now+5).unwrap();
        let summary = resumed.coordinator_summary(mac, now+5).unwrap();
        assert_eq!(summary["plan"]["last_accepted_id"], plan.plan_id);
        assert_eq!(summary["plan"]["last_accepted_remaining_s"], 115);
    }

    #[test]
    fn authenticated_status_recovers_lost_explicit_light_ack() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        let mac = "AA:BB:CC:DD:EE:FF";
        let (plan, _) = svc.queue_explicit_light(mac, crate::now_secs()).unwrap();
        svc.note_device_status(mac, &json!({
            "power": {"mode": "light", "plan_id": plan.plan_id, "remaining_s": 540},
        })).unwrap();
        drop(svc);
        let resumed = PlatformService::open(dir.path()).unwrap();
        let state = resumed.coordinator_summary(mac, crate::now_secs()).unwrap();
        assert!(state["plan"]["pending_explicit_light"].is_null());
        assert_eq!(state["plan"]["last_explicit_light_ack"]["plan_id"], plan.plan_id);
    }

    #[test]
    fn explicit_sleep_cancels_queued_light_without_changing_normal_rendezvous() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        let mac = "AA:BB:CC:DD:EE:FF";
        let now = crate::now_secs();
        svc.queue_explicit_light(mac, now).unwrap();
        let sleep = svc.explicit_plan(mac, PlanMode::Sleep, 0, "explicit").unwrap();
        assert_eq!(sleep.mode, PlanMode::Sleep);
        assert!(svc.coordinator_summary(mac, now + 1).unwrap()["plan"]["pending_explicit_light"].is_null());
        assert_eq!(svc.plan_for_rendezvous(mac, now + 1, "rendezvous", 0).unwrap().mode, PlanMode::Sleep);
    }

    #[test]
    fn incremental_job_persists_frozen_bytes_across_restart_and_edit() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mut caps = DeviceCapabilities::ssd1681_154g();
        caps.asset_publish_protocol = 1;
        caps.max_object_bytes = 100_000;
        caps.max_manifest_bytes = 100_000;
        caps.free_bytes = 1_000_000;
        caps.install_peak_bytes = 1_000_000;
        svc.device_upsert(DeviceIdentity::new("AA:BB:CC:DD:EE:FF", "Test").unwrap(), caps).unwrap();
        svc.template_save("quad", crate::platform::model::RENDER_TARGET_154G, &quad_source(), 1000).unwrap();
        let mut profile = Profile::draft("AA:BB:CC:DD:EE:FF");
        profile.template_ids = vec!["quad".into()];
        let saved = svc.profile_save(profile, 1000).unwrap();
        assert_eq!(saved.render_target.as_deref(), Some(crate::platform::model::RENDER_TARGET_154G));
        let preview = svc.publish_preview("AA:BB:CC:DD:EE:FF").unwrap();
        let expected = preview["target_id"].as_str().unwrap();
        let before = svc.publish_checked("AA:BB:CC:DD:EE:FF", 1001, Some(expected), None).unwrap();
        assert_eq!(before["state"], "waiting");
        let mut edited = quad_source();
        edited["elements"][0]["rect"] = json!([0, 0, 10, 10]);
        svc.template_save("quad", crate::platform::model::RENDER_TARGET_154G, &edited, 1002).unwrap();
        drop(svc);
        let restored = PlatformService::open(dir.path()).unwrap();
        let after = restored.job("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(before["manifest_id"], after["manifest_id"]);
        assert!(restored.asset_job_pending("AA:BB:CC:DD:EE:FF"));
        restored.cancel_job("AA:BB:CC:DD:EE:FF", 1001);
        assert_eq!(restored.job("AA:BB:CC:DD:EE:FF").unwrap()["state"], "cancelled");
    }

    #[test]
    fn profile_target_change_requires_explicit_migration() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        svc.template_save("quad", crate::platform::model::RENDER_TARGET_154G, &quad_source(), 1000).unwrap();
        let mut profile = Profile::draft("AA:BB:CC:DD:EE:FF");
        profile.template_ids = vec!["quad".into()];
        profile.render_target = Some(crate::platform::model::RENDER_TARGET_NOTE4.into());
        assert!(svc.profile_save(profile, 1001).unwrap_err().to_string().contains("differs from device"));
    }

    #[test]
    fn note4_builtin_font_profile_uses_existing_full_bundle_route() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mut caps = DeviceCapabilities::ssd1681_154g();
        caps.firmware_target = crate::platform::model::FIRMWARE_TARGET_NOTE4.into();
        caps.render_target = crate::platform::model::RENDER_TARGET_NOTE4.into();
        caps.width = 400;
        caps.height = 300;
        caps.partial = false;
        caps.hardware_verified = false;
        svc.device_upsert(DeviceIdentity::new("7C4FADB93408", "Note4").unwrap(), caps).unwrap();
        let source: Value = serde_json::from_str(include_str!("../../tests/fixtures/codex-status-a-400x300.json")).unwrap();
        svc.template_save("codex-status-a", crate::platform::model::RENDER_TARGET_NOTE4, &source, 1000).unwrap();
        let mut profile = Profile::draft("7C4FADB93408");
        profile.template_ids = vec!["codex-status-a".into()];
        profile.initial_active_id = Some("codex-status-a".into());
        svc.profile_save(profile, 1000).unwrap();
        assert!(svc.publish("7C4FADB93408", 1001).unwrap_err().to_string().contains("version is unknown"));
        svc.inner.lock().unwrap().devices.get_mut("7C4FADB93408").unwrap().last_authenticated = Some(
            crate::platform::model::AuthenticatedStatus { observed_at: 1000, transport: "wifi".into(), schema_version: 1,
                body: json!({"fw":"0.18.35-note4-b-ota1"}) });
        assert!(svc.publish("7C4FADB93408", 1001).unwrap_err().to_string().contains("requires firmware"));
        svc.inner.lock().unwrap().devices.get_mut("7C4FADB93408").unwrap().last_authenticated.as_mut().unwrap().body["fw"] = json!("0.18.75-note4");
        let job = svc.publish("7C4FADB93408", 1001).unwrap();
        assert_eq!(job["state"], "waiting");
        let inner = svc.inner.lock().unwrap();
        let bundle = &inner.coordinators["7C4FADB93408"].job.as_ref().unwrap().frozen_bundle;
        assert_eq!(bundle.render_target, crate::platform::model::RENDER_TARGET_NOTE4);
        assert_eq!(bundle.compiler_abi, crate::compile::COMPILER_ABI);
        assert!(bundle.total_len <= 262_144);
        assert_eq!(bundle.profile.template_ids, ["codex-status-a"]);
        bundle.verify().unwrap();
    }

    #[test]
    fn unicode_fonts_require_capability_before_publish() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        let mac = "AA:BB:CC:DD:EE:FF";
        let target = "epd-ssd1681-200x200-1bpp";
        let source = json!({"schema":1,"id":"unicode","canvas":{"w":200,"h":200},
            "elements":[{"type":"text","x":0,"y":0,"font":"ntthin18","text":"中文"}]});
        svc.template_save("unicode", target, &source, 1000).unwrap();
        let font = svc.font_library().unwrap().add(include_bytes!("../../tests/fixtures/unicode-subset.bin")).unwrap();
        let mut profile = Profile::draft(mac);
        profile.render_target = Some(target.into());
        profile.template_ids = vec!["unicode".into()];
        profile.initial_active_id = Some("unicode".into());
        profile.font_ids.insert("ntthin18".into(), font.id);
        svc.profile_save(profile, 1000).unwrap();
        svc.inner.lock().unwrap().devices.get_mut(mac).unwrap().capabilities.bundle_font_protocol = 1;
        assert_eq!(svc.publish_preview(mac).unwrap()["can_publish_fonts"], false);
        assert!(svc.publish(mac, 1001).unwrap_err().to_string().contains("requires device bundle_font_protocol 2"));
        assert!(svc.inner.lock().unwrap().coordinators.get(mac).and_then(|c| c.job.as_ref()).is_none());
        svc.inner.lock().unwrap().devices.get_mut(mac).unwrap().capabilities.bundle_font_protocol = 2;
        assert_eq!(svc.publish_preview(mac).unwrap()["can_publish_fonts"], true);
        assert_eq!(svc.publish(mac, 1002).unwrap()["state"], "waiting");
        let inner = svc.inner.lock().unwrap();
        assert_eq!(inner.coordinators[mac].job.as_ref().unwrap().frozen_bundle.fonts.len(), 1);
    }

    #[test]
    fn full_sync_deadline_only_moves_on_ack() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        svc.template_save("quad", "epd-ssd1681-200x200-1bpp", &quad_source(), 1000)
            .unwrap();
        let mut p = Profile::draft("AA:BB:CC:DD:EE:FF");
        p.template_ids = vec!["quad".into()];
        p.initial_active_id = Some("quad".into());
        p.sync_enabled = true;
        p.full_sync_s = 60;
        svc.profile_save(p, 1000).unwrap();
        svc.publish("AA:BB:CC:DD:EE:FF", 1000).unwrap();
        let mut env = json!({
            "schema": 1, "server_time": 1_700_000_000,
            "account": {"plan": "plus"}, "bridge": {"label": "t", "hostId": "h"},
            "buckets": [{"id": "codex", "windows": [{"kind": "weekly", "usedPercent": 30, "resetsAt": 1111, "windowMins": 10080}]}],
            "resetCredits": {"availableCount": 1, "nextExpiresAt": 2222}
        });
        svc.note_codex_envelope(&env).unwrap();
        let mac = "AA:BB:CC:DD:EE:FF";
        // The device commits the bundle and reports its new context.
        let t = crate::now_secs();
        svc.cancel_job(mac, t);
        let compiled = crate::compile::compile(&quad_source(),
            crate::platform::model::RENDER_TARGET_154G).unwrap();
        svc.note_device_status_at(mac, &json!({"configured":true,
            "active_context_id":"ctx-ack-test","active_template_id":"quad",
            "data_contract":{"template_id":"quad","compiler_abi":compiled.compiler_abi,
                "source_crc":compiled.source_crc,"requirements":compiled.requirements}}),
            "http", t).unwrap();
        let decision = svc.next_delivery(mac, true, t);
        assert_eq!(decision["decision"], "ble_data");
        let seq = decision["data_seq"].as_u64().unwrap();
        // Exercise the actual app delivery contract: ACK correlation uses the
        // CRC from the device message, not a test-only coordinator fingerprint.
        let body = svc.data_message_body(mac).unwrap();
        let crc = body["crc"].as_str().unwrap().to_string();
        assert_eq!(decision["content_crc"], body["crc"]);
        // ACK confirms and starts the deadline clock.
        let ack = svc.note_ack(mac, DeliveryKind::BleData, seq, &crc, true, "displayed");
        assert_eq!(ack["outcome"], "applied");
        assert!(svc.coordinator_summary(mac, t).unwrap()["in_flight"].is_null());
        let deadline_before = svc.coordinator_summary(mac, t).unwrap()["context"]
            ["full_sync_deadline"]
            .as_u64()
            .unwrap();

        // Pull-only change: no immediate push, deadline unchanged.
        env["buckets"][0]["windows"][0]["resetsAt"] = json!(9999);
        svc.note_codex_envelope(&env).unwrap();
        assert_eq!(svc.next_delivery(mac, true, t + 1)["decision"], "idle");
        assert_eq!(
            svc.coordinator_summary(mac, t + 1).unwrap()["context"]["full_sync_deadline"].as_u64(),
            Some(deadline_before)
        );

        // Push change: sends the complete snapshot including the newer pull value.
        env["buckets"][0]["windows"][0]["usedPercent"] = json!(31);
        svc.note_codex_envelope(&env).unwrap();
        let decision = svc.next_delivery(mac, true, t + 2);
        assert_eq!(decision["decision"], "ble_data");
        assert_eq!(
            decision["fields"]["buckets[codex].weekly.remaining"]["value"],
            json!(69)
        );
        assert_eq!(
            decision["fields"]["buckets[codex].weekly.resetsAt"]["value"],
            json!(9999)
        );
        assert_eq!(decision["temporary_data"], false);
        // The separate push regression covers formal Light intent; don't
        // accept a Light plan here, so the periodic case starts without a hold.
        let seq2 = decision["data_seq"].as_u64().unwrap();
        let crc2 = decision["content_crc"].as_str().unwrap().to_string();
        assert_eq!(seq2, seq + 1);
        svc.note_ack(mac, DeliveryKind::BleData, seq2, &crc2, true, "displayed");

        // A periodic full snapshot (including HTTP-sized/retried delivery)
        // must not turn the pinned Data into a formal Light intent.
        let periodic_at = t + 1 + 61;
        let periodic = svc.next_delivery(mac, true, periodic_at);
        assert_eq!(periodic["decision"], "ble_data");
        assert_eq!(periodic["temporary_data"], true);
        assert_eq!(svc.plan_for_rendezvous(mac, periodic_at, "rendezvous", 0).unwrap().mode, PlanMode::Sleep);
        let retry = svc.next_http_delivery(mac, true, periodic_at + 1);
        assert_eq!(retry["data_seq"], periodic["data_seq"]);
        assert_eq!(retry["temporary_data"], true);
        assert_eq!(svc.plan_for_rendezvous_network_work(mac, periodic_at + 1, "rendezvous", 0, true).unwrap().mode, PlanMode::Sleep);
        let resumed = PlatformService::open(dir.path()).unwrap();
        assert_eq!(resumed.plan_for_rendezvous(mac, periodic_at + 2, "rendezvous", 0).unwrap().mode, PlanMode::Sleep);
        // A stale ACK for the old content never confirms the new snapshot.
        let stale = svc.note_ack(mac, DeliveryKind::BleData, seq, &crc, true, "displayed");
        assert_eq!(stale["outcome"], "stale");
    }

    #[test]
    fn restart_preserves_data_seq() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        svc.template_save("quad", "epd-ssd1681-200x200-1bpp", &quad_source(), 1000)
            .unwrap();
        let mut p = Profile::draft("AA:BB:CC:DD:EE:FF");
        p.template_ids = vec!["quad".into()];
        p.initial_active_id = Some("quad".into());
        p.sync_enabled = true;
        svc.profile_save(p, 1000).unwrap();
        svc.publish("AA:BB:CC:DD:EE:FF", 1000).unwrap();
        svc.cancel_job("AA:BB:CC:DD:EE:FF", 1001);
        let mac = "AA:BB:CC:DD:EE:FF";
        let compiled = crate::compile::compile(&quad_source(),
            crate::platform::model::RENDER_TARGET_154G).unwrap();
        svc.note_device_status_at(mac, &json!({"configured":true,
            "active_context_id":"ctx-1","active_template_id":"quad",
            "data_contract":{"template_id":"quad","compiler_abi":compiled.compiler_abi,
                "source_crc":compiled.source_crc,"requirements":compiled.requirements}}),
            "http", 1000).unwrap();
        svc.note_codex_envelope(&json!({
            "account": {"plan": "plus"}, "bridge": {"label": "t", "hostId": "h"},
            "buckets": [{"id": "codex", "windows": [{"kind": "weekly", "usedPercent": 30, "windowMins": 10080}]}],
        }))
        .unwrap();
        let d = svc.next_delivery(mac, true, 1000);
        let seq = d["data_seq"].as_u64().unwrap();
        let crc = d["content_crc"].as_str().unwrap().to_string();
        svc.note_ack(mac, DeliveryKind::BleData, seq, &crc, true, "displayed");
        let next_seq = svc.coordinator_summary(mac, 1000).unwrap()["context"]["next_seq"]
            .as_u64()
            .unwrap();
        drop(svc);
        // Reopen the service: seq continues, never restarts at 0.
        let svc2 = PlatformService::open(dir.path()).unwrap();
        let summary = svc2.coordinator_summary(mac, 1000).unwrap();
        assert!(summary["context"]["next_seq"].as_u64().unwrap() >= next_seq);
        assert_eq!(summary["context"]["context_id"], "ctx-1");
    }

    #[test]
    fn mcp_and_ui_share_the_same_service_behavior() {
        // Both interfaces call these methods; a save is never a publish.
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        svc.template_save("quad", "epd-ssd1681-200x200-1bpp", &quad_source(), 0)
            .unwrap();
        assert!(svc.job("AA:BB:CC:DD:EE:FF").is_none());
        let overview = svc.overview();
        assert_eq!(overview["templates"].as_array().unwrap().len(), 1);
    }
    #[test]
    fn manual_wifi_sync_retains_current_environment_and_freezes_preview() {
        let dir=tempfile::tempdir().unwrap();let svc=service_with_device(dir.path());let mac="AABBCCDDEEFF";
        let snapshot:WifiSnapshot=serde_json::from_value(json!({"schema":2,"device_mac":mac,"revision":7,"snapshot_id":"manual",
            "complete":true,"observed_at":100,"networks":[{"bond":"own","writable":true,"network":{"network_id":"old","host_ssid":"Host-5G","ssid":"Old-2G"}}],"capacity":{"total":12}})).unwrap();
        let read=svc.wifi_queue(mac,"bridge","read",vec![],100).unwrap();svc.wifi_complete_read(mac,&read.job_id,snapshot,101).unwrap();
        let targets=serde_json::from_value(json!([{"network_id":"old","host_ssid":"Host-5G","ssid":"Old-2G","deleted":true},
            {"network_id":"current","host_ssid":"Host-5G","ssid":"Current-2G","credential":"open"}])).unwrap();
        svc.wifi_save(mac,0,targets,102).unwrap();
        assert_eq!(svc.wifi_queue_manual_sync(mac,"bridge",vec!["old".into()],1,"manual","Host-5G",103).unwrap_err().to_string(),"current_network_required");
        assert!(svc.wifi_queue_manual_sync(mac,"bridge",vec!["current".into()],1,"stale","Host-5G",103).is_err());
        let job=svc.wifi_queue_manual_sync(mac,"bridge",vec!["old".into(),"current".into()],1,"manual","Host-5G",103).unwrap();
        assert_eq!(job.expected_revision,Some(7));assert_eq!(job.host_ssid.as_deref(),Some("Host-5G"));
        let mut newer=svc.wifi_state(mac).targets;newer[1].label="Later draft".into();svc.wifi_save(mac,1,newer,104).unwrap();
        let outcome=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",true,&BTreeMap::new(),105,&mut |op: &str, body: Value| {
            assert_eq!(op,"apply","must not reread device configuration");assert_eq!(body["expected_revision"],7);
            assert_eq!(body["changes"].as_array().unwrap().len(),2);anyhow::bail!("lost_ack")
        })).unwrap().unwrap();assert_eq!(outcome.state,"awaiting_confirmation");
        drop(svc);let svc=PlatformService::open(dir.path()).unwrap();
        let result=ready(svc.wifi_attempt_async(mac,&job.job_id, &job.bridge_id, "own",false,&BTreeMap::new(),166,&mut |op: &str, _: Value| {
            assert_eq!(op,"result","lost ACK must only query the original transaction");Ok(json!({"ok":true,"outcome":"committed","revision":8,"bond":"own"}))
        })).unwrap().unwrap();assert_eq!(result.state,"completed");assert_eq!(result.confirmation.as_deref(),Some("durable_receipt"));
        let state=svc.wifi_state(mac);assert_eq!(state.targets[1].label,"Later draft");
        let cache=state.snapshot.unwrap();assert_eq!(cache.networks.len(),1);assert_eq!(cache.networks[0].network.network_id,"current");
        assert_eq!(cache.revision,8);assert_eq!(cache.networks[0].network.label,"");
    }

}

#[cfg(test)]
mod ota_connection_tests {
    use super::*;
    #[test]
    fn authenticated_http_status_resets_wifi_budget_but_ble_and_rejected_status_do_not() {
        let dir = tempfile::tempdir().unwrap();
        let svc = PlatformService::open(dir.path()).unwrap();
        let mac = "AABBCCDDEEFF";
        svc.device_upsert(DeviceIdentity::new(mac,"Test").unwrap(),DeviceCapabilities::ssd1681_154g()).unwrap();
        svc.wifi_save(mac,0,vec![serde_json::from_value(json!({"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"})).unwrap()],100).unwrap();
        let connection: crate::platform::wifi::Connection = serde_json::from_value(json!({
            "host_ssid":"Host","network_id":"home","local_revision":1,"version":1,
            "state":"connection_failed","error":"connect_timeout","reason":0,"observed_at":100,
            "attempts":3,"next_attempt_at":200,"http_ready":false})).unwrap();
        svc.wifi_note_connection(mac,connection.clone()).unwrap();
        let status = json!({"device_mac":mac,"boot_id":"boot","status_seq":2});
        svc.note_authenticated_status_via_at(mac,&status,"ble",101).unwrap();
        assert_eq!(svc.wifi_state(mac).connection().unwrap().attempts,3);
        assert!(svc.note_authenticated_status_at(mac,&json!({"device_mac":"FFFFFFFFFFFF"}),102).is_err());
        assert!(svc.note_authenticated_status_at(mac,&json!({"device_mac":mac,"boot_id":"boot","status_seq":1}),102).is_err());
        assert_eq!(svc.wifi_state(mac).connection().unwrap().attempts,3);
        svc.note_authenticated_status_at(mac,&status,103).unwrap();
        let confirmed = svc.wifi_state(mac).connection().unwrap();
        assert_eq!((confirmed.attempts,confirmed.next_attempt_at),(0,0));
        assert_eq!(confirmed.state,"connected");
        assert!(confirmed.http_ready);
        assert!(confirmed.error.is_none());
        drop(svc);
        let svc = PlatformService::open(dir.path()).unwrap();
        assert_eq!(svc.wifi_state(mac).connection().unwrap().attempts,0);
        let mut pending = connection.clone();pending.state="requested".into();
        svc.wifi_note_connection(mac,pending).unwrap();
        svc.note_authenticated_status_at(mac,&status,104).unwrap();
        assert_eq!(svc.wifi_state(mac).connection().unwrap().attempts,3);
    }
    #[test]
    fn new_ota_renews_bounded_connection_but_replay_and_unknown_do_not() {
        for (state,error,reset) in [("waiting_for_window",None,true),("connection_failed",Some("connect_timeout"),true),
            ("awaiting_confirmation",Some("wifi_command_unknown"),false),("connecting",None,false),
            ("connection_failed",Some("authentication_failed"),false),("connection_failed",Some("security_mismatch"),false)] {
            let dir=tempfile::tempdir().unwrap();let svc=PlatformService::open(dir.path()).unwrap();let mac="AABBCCDDEEFF";
            svc.device_upsert(DeviceIdentity::new(mac,"Test").unwrap(),DeviceCapabilities::ssd1681_154g()).unwrap();
            svc.wifi_save(mac,0,vec![serde_json::from_value(json!({"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"})).unwrap()],100).unwrap();
            let connection: crate::platform::wifi::Connection=serde_json::from_value(json!({"host_ssid":"Host","network_id":"home","local_revision":1,"version":1,
                "state":state,"error":error,"reason":0,"observed_at":crate::device_clock::wall_secs(mac),"attempts":3,"next_attempt_at":200,"http_ready":false})).unwrap();
            svc.wifi_note_connection(mac,connection.clone()).unwrap();
            let target=svc.device_get(mac).unwrap()["capabilities"]["firmware_target"].as_str().unwrap().to_owned();
            let mut rom=vec![7u8;2048];rom[0]=0xe9;let marker=format!("codex-status-ota-v1|{target}|test-ota\0");rom[32..32+marker.len()].copy_from_slice(marker.as_bytes());
            let path=dir.path().join("image.bin");std::fs::write(&path,rom).unwrap();
            svc.queue_ota(mac,"bridge","ota-request",&path,"test-ota",&target).unwrap();
            let mut ota=svc.ota_job(mac).unwrap();
            for (phase,blocked) in [("queued",false),("transferring",true),("awaiting_confirmation",false),("succeeded",false)] {
                ota.state=phase.into();assert_eq!(ota.blocks_network_repair(),blocked);
            }
            let next=svc.wifi_state(mac).connection().unwrap();assert_eq!(next.attempts,if reset{0}else{3});
            assert_eq!(next.state,if reset{"requested"}else{state});assert_eq!(next.error,if reset{None}else{error.map(str::to_owned)});
            svc.wifi_note_connection(mac,connection.clone()).unwrap();
            svc.queue_ota(mac,"bridge","ota-request",&path,"test-ota",&target).unwrap();
            assert_eq!(svc.wifi_state(mac).connection().unwrap().attempts,3);
            drop(svc);assert_eq!(PlatformService::open(dir.path()).unwrap().wifi_state(mac).connection().unwrap().attempts,3);
        }
    }
}

#[cfg(test)]
mod wifi_idle_admission_tests {
    use super::*;
    #[test]
    fn wifi_idle_connection_does_not_authorize_periodic_light_but_real_work_does() {
        let root=tempfile::tempdir().unwrap();let svc=PlatformService::open(root.path()).unwrap();
        let mac="0200000000E1";
        svc.device_upsert(DeviceIdentity::new(mac,"Idle test").unwrap(),DeviceCapabilities::ssd1681_154g()).unwrap();
        svc.wifi_save(mac,0,serde_json::from_value(json!([{"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"}])).unwrap(),100).unwrap();
        for state in ["waiting_for_window","connection_failed","connecting","awaiting_confirmation"] {
            svc.wifi_note_connection(mac,serde_json::from_value(json!({"host_ssid":"Host","network_id":"home","local_revision":1,
                "version":8,"state":state,"error":null,"reason":0,"observed_at":100,"attempts":0,"next_attempt_at":0,"http_ready":false})).unwrap()).unwrap();
            for now in [100,280,460,640] {
                assert_eq!(svc.plan_for_rendezvous(mac,now,"rendezvous",0).unwrap().mode,PlanMode::Sleep);
            }
            assert_eq!(svc.plan_for_rendezvous_network_work(mac,641,"rendezvous",0,true).unwrap().mode,PlanMode::Light);
        }
        let mut c=svc.wifi_state(mac).connection().unwrap();c.state="requested".into();svc.wifi_note_connection(mac,c).unwrap();
        assert_eq!(svc.plan_for_rendezvous(mac,700,"rendezvous",0).unwrap().mode,PlanMode::Light);
        let mut c=svc.wifi_state(mac).connection().unwrap();c.state="waiting_for_window".into();svc.wifi_note_connection(mac,c).unwrap();
        svc.wifi_queue(mac,"bridge-test","read",vec![],710).unwrap();
        assert_eq!(svc.plan_for_rendezvous(mac,711,"rendezvous",0).unwrap().mode,PlanMode::Light);
    }
}

#[cfg(test)]
fn ready<F: std::future::Future>(future:F)->F::Output {
    let mut future=std::pin::pin!(future);
    match future.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())) {
        std::task::Poll::Ready(value)=>value,
        std::task::Poll::Pending=>panic!("synchronous fixture suspended"),
    }
}
