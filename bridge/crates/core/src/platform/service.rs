//! Shared application service (v2 §2/§11): the four UI pages and MCP call these
//! exact operations; there is no second business model and no direct file edit
//! path around them. Keeping the state in one mutex-serialized struct also gives
//! the per-device serial coordination the design requires.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;

use crate::coordinator::{AckOutcome, Coordinator, Delivery, DeliveryKind, DEFAULT_FULL_SYNC_S};
use crate::datasource::{DataSource, DataSourceKind};
use crate::platform::model::{
    Binding, Bundle, BundleProfile, BundleResource, DeviceCapabilities, DeviceIdentity,
    AuthenticatedStatus, DeviceRecord, FamilyProfile, FieldRequirement, PlanMode, PowerPlan, Profile, PublishState, StatusAttempt,
    SourceSnapshot,
    Template, TemplateKey, PublishJob, MAX_PROFILE_TEMPLATES,
};
use crate::platform::store;
use crate::platform::fonts::{FontLibrary, FontPlan};
use crate::platform::publish::{FrozenPublish, Capacity, CommittedRefs};

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
}

impl OtaJob {
    fn pending(&self) -> bool {
        !matches!(self.state.as_str(), "succeeded" | "failed" | "cancelled")
    }

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
pub struct PersistedState {
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
    /// Bounded checkpoint throttle: the 10 s status poll must not write flash.
    status_persist_at: BTreeMap<String, u64>,
}

pub const MAX_JOB_SUMMARIES: usize = 20;

impl PlatformService {
    /// Load state from `<root>/platform/` (created on demand).
    pub fn open(root: &Path) -> Result<Self> {
        let dir = root.join("platform");
        store::ensure_dir(&dir)?;
        let persisted: PersistedState =
            store::read_json(&dir.join("state.json"))?.unwrap_or_default();
        let PersistedState {
            sources: persisted_sources,
            templates: persisted_templates,
            devices: persisted_devices,
            family_profiles: persisted_family_profiles,
            jobs: mut persisted_jobs,
            asset_jobs: mut persisted_asset_jobs,
            bundle_jobs: persisted_bundle_jobs,
            ota_jobs: mut persisted_ota_jobs,
            next_seq,
            contexts,
            plans,
        } = persisted;
        for job in persisted_ota_jobs.values_mut() {
            if job.state == "transferring" { job.state = "awaiting_confirmation".into(); }
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
                t.compiled = crate::compile::compile(&t.source, &t.key.render_target)?;
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
                status_persist_at: BTreeMap::new(),
            }),
        };
        service.refresh_all_contracts()?;
        if history_recovered {
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
            sources: inner.sources.values().cloned().collect(),
            templates: inner.templates.values().cloned().collect(),
            family_profiles: inner.family_profiles.values().cloned().collect(),
            devices: inner.devices.values().cloned().collect(),
            jobs,
            asset_jobs: inner.asset_jobs.clone(),
            ota_jobs: inner.ota_jobs.clone(),
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
                "max_bundle_bytes": record.capabilities.max_bundle_bytes.min(256 * 1024),
                "can_publish_fonts": false,
                "source_conversion": "unavailable", "note": "CSFN import is available; TTF/OTF conversion and incremental device protocol are not available"}));
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
        if !font_plan.required.is_empty() {
            bail!("device does not advertise the versioned asset protocol; selected fonts cannot be published through the old bundle path");
        }
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
            total_len: 0,
            crc: String::new(),
        }
        .seal()?;
        if u64::from(bundle.total_len) > caps.max_bundle_bytes.min(256 * 1024) {
            bail!("bundle is {} bytes, device full-bundle limit is {}", bundle.total_len, caps.max_bundle_bytes.min(256 * 1024));
        }
        inner.asset_jobs.remove(&mac);
        let coordinator = inner
            .coordinators
            .entry(mac.clone())
            .or_insert_with(|| Coordinator::new(&mac, caps.clone()));
        coordinator
            .enqueue_bundle(bundle, now)
            .map_err(|e| anyhow::anyhow!(e))?;
        coordinator.full_sync_s = profile.full_sync_s.max(60);
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
        job.frozen_bundle.canonical_bytes()
    }

    /// Bind pre-existing uncommitted Bundles that were frozen before the
    /// ownership field existed. Re-seal the same job ID before any retry.
    pub fn bind_pending_bundle_owner(&self, mac: &str, bridge_id: &str) -> Result<()> {
        if bridge_id.is_empty() {
            bail!("bridge id is required for bundle ownership");
        }
        let mut inner = self.inner.lock().unwrap();
        let mac = mac.to_uppercase();
        let limit = inner.devices.get(&mac).context("unknown device")?
            .capabilities.max_bundle_bytes.min(256 * 1024);
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
        if u64::from(bundle.total_len) > limit {
            bail!("bundle is {} bytes, device full-bundle limit is {limit}", bundle.total_len);
        }
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
            sha256: digest, size: bytes.len(), blob,
            last_error: None, upload_ack: false, confirmation: None, attempt_count: 0,
            last_attempt_at: None, cancel_requested: false,
        };
        let old = inner.ota_jobs.insert(mac.clone(), job.clone());
        let old_blob = old.as_ref().map(|previous| previous.blob.clone());
        if let Err(e) = Self::persist(&inner, &self.state_path()) {
            let reused_blob = old.as_ref().is_some_and(|previous| previous.blob == job.blob);
            if let Some(old) = old { inner.ota_jobs.insert(mac, old); } else { inner.ota_jobs.remove(&mac); }
            if !reused_blob { let _ = std::fs::remove_file(self.dir.join(&job.blob)); }
            return Err(e);
        }
        if let Some(blob) = old_blob.filter(|blob| blob != &job.blob) {
            let _ = std::fs::remove_file(self.dir.join(blob));
        }
        Ok(job)
    }

    pub fn ota_cancel(&self, mac: &str) -> Result<Value> {
        let mut inner = self.inner.lock().unwrap();
        let job = inner.ota_jobs.get_mut(&mac.to_uppercase()).context("no OTA job")?;
        if job.state == "queued" { job.state = "cancelled".into(); }
        else if job.pending() { job.cancel_requested = true; }
        job.updated_at = crate::device_clock::wall_secs(&mac);
        let cleanup = (job.state == "cancelled").then(|| job.blob.clone());
        Self::persist(&inner, &self.state_path())?;
        if let Some(blob) = cleanup { let _ = std::fs::remove_file(self.dir.join(blob)); }
        Ok(json!(inner.ota_jobs.get(&mac.to_uppercase())))
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
        let before_version = inner.devices.get(&mac)
            .and_then(|d| d.last_authenticated.as_ref())
            .and_then(|s| s.body["fw"].as_str().map(str::to_string));
        let job = inner.ota_jobs.get_mut(&mac).unwrap();
        job.state = "transferring".into();
        job.before_version = before_version;
        job.attempt_count = job.attempt_count.saturating_add(1);
        job.last_attempt_at = Some(crate::device_clock::wall_secs(&mac));
        job.updated_at = crate::device_clock::wall_secs(&mac);
        Self::persist(&inner, &self.state_path())?;
        Ok(Some(path))
    }

    pub fn ota_await_confirmation(&self, mac: &str, upload_ack: bool, error: Option<&str>) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let job = inner.ota_jobs.get_mut(&mac.to_uppercase()).context("no OTA job")?;
        job.state = "awaiting_confirmation".into();
        job.upload_ack = upload_ack;
        if upload_ack { job.confirmation = Some("upload_ack".into()); }
        job.last_error = error.map(str::to_string);
        job.updated_at = crate::device_clock::wall_secs(&mac);
        Self::persist(&inner, &self.state_path())
    }

    pub fn ota_preflight_failed(&self, mac: &str, error: &str, terminal: bool) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let job = inner.ota_jobs.get_mut(&mac.to_uppercase()).context("no OTA job")?;
        if job.state != "transferring" { bail!("OTA is not transferring"); }
        job.state = if terminal { "failed" } else { "queued" }.into();
        job.last_error = Some(error.chars().take(200).collect());
        job.updated_at = crate::device_clock::wall_secs(&mac);
        let cleanup = terminal.then(|| job.blob.clone());
        Self::persist(&inner, &self.state_path())?;
        if let Some(blob) = cleanup { let _ = std::fs::remove_file(self.dir.join(blob)); }
        Ok(())
    }

    pub fn ota_note_authenticated_version(&self, mac: &str, status: &Value) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let Some(job) = inner.ota_jobs.get_mut(&mac.to_uppercase()) else { return Ok(()); };
        if job.state == "awaiting_confirmation" && status["fw"] == job.expected_version {
            let changed = job.before_version.as_deref().is_some_and(|before| before != job.expected_version);
            let (level, note) = if changed {
                ("version_observed", "expected version observed; exact running image identity unavailable")
            } else {
                ("version_seen_unproven", "expected version reported, but pre-upload version was same or unknown")
            };
            if job.confirmation.as_deref() != Some(level) {
                job.last_error = Some(note.into());
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
            image["fw"] != job.expected_version || image["sha256"] != job.sha256 ||
            image["device_mac"] != mac {
            return Ok(false);
        }
        let bytes = std::fs::read(self.dir.join(&job.blob)).context("frozen OTA image unavailable")?;
        if bytes.len() != job.size || format!("{:x}", sha2::Sha256::digest(&bytes)) != job.sha256 {
            bail!("frozen OTA image changed");
        }
        let mut inner = self.inner.lock().unwrap();
        let current = inner.ota_jobs.get_mut(&mac).context("OTA job vanished")?;
        if current.job_id != job.job_id || current.state != "awaiting_confirmation" { return Ok(false); }
        current.confirmation = Some("image_verified".into());
        current.state = "succeeded".into();
        current.last_error = None;
        current.updated_at = crate::device_clock::wall_secs(&mac);
        Self::persist(&inner, &self.state_path())?;
        Ok(true)
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
        let requirements = compiled_requirements(&inner, &mac, &template_id);
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
                    "sync_enabled": d.profile.as_ref().map(|p| p.sync_enabled).unwrap_or(false),
                    "active_template_id": coordinator.and_then(|c| c.active_template_id.clone()),
                    "job": inner.asset_jobs.get(mac).map(AssetJob::summary)
                        .or_else(|| coordinator.and_then(|c| c.job_snapshot())),
                    "ota_job": inner.ota_jobs.get(mac),
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
                        "accepted_plan_id": c.plan.last_accepted_id,
                        "accepted_remaining_s": c.plan.last_accepted_remaining_s,
                        "observed": c.session.power,
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
        let mac = mac.to_uppercase();
        if status["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref() != Some(mac.as_str()) {
            bail!("authenticated status MAC does not match {mac}");
        }
        if serde_json::to_vec(status)?.len() > 16 * 1024 {
            bail!("authenticated status exceeds 16 KiB");
        }
        let mut inner = self.inner.lock().unwrap();
        let record = inner.devices.get_mut(&mac).context("unknown device")?;
        let now = crate::device_clock::wall_secs(&mac);
        let mut body = record.last_authenticated.as_ref().map(|saved| saved.body.clone())
            .unwrap_or_else(|| json!({}));
        let groups: [(&str, &[&str]); 7] = [
            ("identity", &["device_mac", "configured"]),
            ("firmware", &["fw", "firmware_target", "running_slot", "reset_reason",
                "image_identity", "sync_v1", "diag_format", "diag_capacity"]),
            ("radio", &["radio"]),
            ("runtime", &["heap_free", "heap_min", "uptime_ms"]),
            ("display", &["display", "active_context_id", "active_template_id",
                "template_ids", "display_state"]),
            ("power", &["power", "sync"]),
            ("jobs", &["committed_job_id", "commit_seq", "data_seq", "applied_seq"]),
        ];
        for (group, fields) in groups {
            let mut sampled = false;
            let mut available = false;
            for key in fields {
                if let Some(value) = status.get(*key) {
                    body[*key] = value.clone();
                    sampled = true;
                    available |= !value.is_null();
                }
            }
            if sampled {
                body["groups"][group] = json!({
                    "received_at": now, "sampled_boot_id": status.get("boot_id"),
                    "sampled_uptime_ms": status.get("uptime_ms"),
                    "sampled_wall": status.get("sampled_wall"),
                    "transport": "http",
                    "quality": if available { "observed" } else { "unavailable" },
                });
            }
        }
        if serde_json::to_vec(&body)?.len() > 16 * 1024 {
            bail!("authenticated cached status exceeds 16 KiB");
        }
        let previous = (record.last_authenticated.clone(), record.last_status_attempt.clone(),
            record.last_authenticated_contact_at, record.last_authenticated_transport.clone());
        record.last_authenticated = Some(AuthenticatedStatus {
            observed_at: now, transport: "http".into(), schema_version: 2, body,
        });
        record.last_authenticated_contact_at = Some(now);
        record.last_authenticated_transport = Some("http".into());
        record.last_status_attempt = Some(StatusAttempt { at: now, outcome: "ok".into(), error: None });
        if let Err(error) = Self::persist(&inner, &self.state_path()) {
            let record = inner.devices.get_mut(&mac).unwrap();
            (record.last_authenticated, record.last_status_attempt,
                record.last_authenticated_contact_at, record.last_authenticated_transport) = previous;
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
        let mac = mac.to_uppercase();
        if status.get("device_mac").and_then(Value::as_str)
            .and_then(DeviceIdentity::normalized_mac)
            .is_some_and(|reported| reported != mac) {
            bail!("status MAC does not match {mac}");
        }
        let now = crate::device_clock::wall_secs(&mac);
        let mut inner = self.inner.lock().unwrap();
        if let Some(record) = inner.devices.get_mut(&mac) {
            record.last_authenticated_contact_at = Some(now);
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
            record.observed.display_state = status
                .get("display_state")
                .and_then(|v| v.as_str())
                .map(str::to_string);
        }
        let profile = inner.devices.get(&mac).and_then(|d| d.profile.clone());
        if let Some(c) = inner.coordinators.get_mut(&mac) {
            c.note_status(status, now);
        }
        // A template switched ON THE DEVICE (button/local switch) changes which
        // compiled requirements data frames are validated against, usually
        // WITHOUT changing the context. Without this refresh the bridge keeps
        // sending the previous template's field set and the device answers every
        // frame with `incomplete` (firmware v2_runtime.cpp: fields.size() !=
        // remoteCount). Only refresh for templates the profile actually installs.
        let reported = status
            .get("active_template_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let contract_template = inner
            .coordinators
            .get(&mac)
            .and_then(|c| c.active_template_id.clone());
        let installed = profile
            .as_ref()
            .is_some_and(|p| p.template_ids.iter().any(|t| t == reported));
        if !reported.is_empty()
            && contract_template.as_deref() != Some(reported)
            && installed
        {
            let requirements = compiled_requirements(&inner, &mac, reported);
            let triggers = triggers_for(&inner, &mac, reported);
            if !requirements.is_empty() {
                if let Some(c) = inner.coordinators.get_mut(&mac) {
                    c.set_contract(reported, requirements, triggers);
                }
            }
        }
        let mut power_reconciled = false;
        if let Some(c) = inner.coordinators.get_mut(&mac) {
            if let Some(pending) = c.plan.pending_explicit_light.clone() {
                let power = &status["power"];
                if power["plan_id"] == pending.plan_id && power["mode"] == "light" {
                    if let Some(remaining) = power["remaining_s"].as_u64().filter(|n| *n > 0) {
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
                c.session.data_seq.unwrap_or(0),
                c.context.as_ref().map(|x| x.next_seq).unwrap_or(1),
                c.active_template_id.clone().unwrap_or_default(),
                c.full_sync_s,
            ))
        });
        let reconcile_needed = reconcile.is_some();
        if let Some((device_ctx, device_seq, local_next, template_id, full_sync_s)) = reconcile {
            let installed = profile
                .as_ref()
                .map(|p| p.template_ids.contains(&template_id))
                .unwrap_or(false);
            let contract = if !template_id.is_empty() && installed {
                Some((
                    compiled_requirements(&inner, &mac, &template_id),
                    triggers_for(&inner, &mac, &template_id),
                ))
            } else {
                None
            };
            let c = inner.coordinators.get_mut(&mac).expect("coordinator");
            c.context = Some(crate::coordinator::ContextState {
                context_id: device_ctx,
                created_at: now,
                reason: "reconciled_from_device".into(),
                next_seq: local_next.max(device_seq + 1),
                acked_push_fp: None,
                acked_full_fp: None,
                full_sync_deadline: now + full_sync_s,
                last_applied_seq: device_seq,
                last_content_crc: None,
            });
            c.data.in_flight = None;
            if let Some((requirements, triggers)) = contract {
                c.set_contract(&template_id, requirements, triggers);
            }
        }
        if reconcile_needed || job_reconciled || power_reconciled {
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
        let mut inner = self.inner.lock().unwrap();
        let record = inner.devices.get_mut(&mac.to_uppercase()).context("unknown device")?;
        record.last_authenticated_contact_at = Some(crate::device_clock::wall_secs(&mac));
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
        let Some(c) = inner.coordinators.get_mut(&mac.to_uppercase()) else {
            return json!({"decision": "unknown_device"});
        };
        let decision = match c.next_delivery(now, reachable) {
            Delivery::BleData(snapshot) if !ble => Delivery::LightData(snapshot),
            other => other,
        };
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
            match c.note_delivery_started(&decision, now) {
                Ok(flight) => {
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
    pub fn plan_for_rendezvous(
        &self,
        mac: &str,
        now: u64,
        wake_reason: &str,
        provisional_remaining_s: u32,
    ) -> Result<PowerPlan> {
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
        let work_want_light = inner
            .coordinators
            .get(&mac)
            .map(|c| {
                now < c.plan.light_hold_until
                    || (c.sync_enabled && c.data.push_dirty)
                    || c.data.in_flight.is_some()
                    || c.pending_activate.is_some()
                    || c.job
                        .as_ref()
                        .map(|j| !j.state.is_terminal())
                        .unwrap_or(false)
            })
            .unwrap_or(false)
            || inner.ota_jobs.get(&mac).is_some_and(|j| j.state == "queued");
        let c = inner.coordinators.get_mut(&mac).context("unknown device")?;
        let manual_remaining = c.manual_remaining(now);
        let want_light =
            work_want_light || manual_remaining >= crate::coordinator::MIN_LIGHT_S;
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
                // rendezvous would extend it forever.
                let light_s = if work_want_light {
                    crate::coordinator::MAX_LIGHT_S
                } else {
                    manual_remaining
                };
                c.plan_for(now, want_light, light_s, "rendezvous")
            }
        };
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

fn compiled_requirements(inner: &Inner, mac: &str, template_id: &str) -> Vec<FieldRequirement> {
    let Some(record) = inner.devices.get(mac) else {
        return Vec::new();
    };
    inner
        .templates
        .get(&TemplateKey::new(
            template_id,
            &record.capabilities.render_target,
        ))
        .map(|t| t.compiled.requirements.clone())
        .unwrap_or_default()
}

fn triggers_for(
    inner: &Inner,
    mac: &str,
    template_id: &str,
) -> BTreeMap<String, crate::platform::model::FieldTrigger> {
    let mut out = BTreeMap::new();
    let requirements = compiled_requirements(inner, mac, template_id);
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
                inner
                    .sources
                    .values()
                    .find(|s| s.kind == DataSourceKind::Codex)
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
    let sync_enabled = inner
        .devices
        .get(mac)
        .and_then(|d| d.profile.as_ref())
        .map(|p| p.sync_enabled)
        .unwrap_or(false);
    let active_id = inner
        .coordinators
        .get(mac)
        .and_then(|c| c.active_template_id.clone())
        .or_else(|| {
            inner
                .devices
                .get(mac)
                .and_then(|d| d.profile.as_ref())
                .and_then(|p| p.initial_active_id.clone())
        });
    let Some(active_id) = active_id else {
        return Ok(());
    };
    let requirements = compiled_requirements(inner, mac, &active_id);
    let triggers = triggers_for(inner, mac, &active_id);
    if let Some(c) = inner.coordinators.get_mut(mac) {
        c.sync_enabled = sync_enabled;
        c.set_contract(&active_id, requirements, triggers);
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
        let frozen = svc.ota_begin(mac).unwrap().unwrap();
        assert_eq!(std::fs::read(frozen).unwrap(), bytes);
        drop(svc);
        let resumed = PlatformService::open(dir.path()).unwrap();
        assert_eq!(resumed.ota_job(mac).unwrap().state, "awaiting_confirmation");
        assert!(resumed.ota_job(mac).unwrap().blocks_following_work());
        assert!(resumed.ota_begin(mac).unwrap().is_none());
        assert_eq!(resumed.ota_job(mac).unwrap().sha256, job.sha256);
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
        assert_eq!(svc.ota_job(mac).unwrap().state, "cancelled");
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
            json!({
                "configured": true,
                "active_context_id": "ctx-1",
                "active_template_id": template,
                "committed_job_id": "",
                "data_seq": 0,
                "applied_seq": 0,
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
        assert_eq!(follow.light_duration_s, 225);
        // One second past the window the ordinary sleep decision returns.
        let after = svc.plan_for_rendezvous(mac, now + 301, "rendezvous", 0).unwrap();
        assert_eq!(after.mode, PlanMode::Sleep);
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
        let job = svc.publish("7C4FADB93408", 1001).unwrap();
        assert_eq!(job["state"], "waiting");
        let inner = svc.inner.lock().unwrap();
        let bundle = &inner.coordinators["7C4FADB93408"].job.as_ref().unwrap().frozen_bundle;
        assert_eq!(bundle.render_target, crate::platform::model::RENDER_TARGET_NOTE4);
        assert_eq!(bundle.compiler_abi, 2);
        assert!(bundle.total_len <= 262_144);
        assert_eq!(bundle.profile.template_ids, ["codex-status-a"]);
        bundle.verify().unwrap();
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
        svc.adopt_activation_context(mac, "ctx-ack-test", t)
            .unwrap();
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
        let seq2 = decision["data_seq"].as_u64().unwrap();
        let crc2 = decision["content_crc"].as_str().unwrap().to_string();
        assert_eq!(seq2, seq + 1);
        svc.note_ack(mac, DeliveryKind::BleData, seq2, &crc2, true, "displayed");

        // Reaching the confirmed deadline schedules a complete sync.
        assert!(matches!(
            svc.next_delivery(mac, true, t + 1 + 61)["decision"].as_str(),
            Some("ble_data")
        ));
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
        let a = svc.adopt_activation_context(mac, "ctx-1", 1000).unwrap();
        let _ = a;
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
}
