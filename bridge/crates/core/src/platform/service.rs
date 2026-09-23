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
    DeviceRecord, FieldRequirement, PlanMode, PowerPlan, Profile, PublishState, SourceSnapshot,
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
    /// Bounded terminal job summaries (no browsable content versions).
    #[serde(default)]
    pub jobs: Vec<Value>,
    #[serde(default)]
    pub asset_jobs: BTreeMap<String, AssetJob>,
    #[serde(default)]
    pub bundle_jobs: BTreeMap<String, PublishJob>,
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
    devices: BTreeMap<String, DeviceRecord>,
    coordinators: BTreeMap<String, Coordinator>,
    codex_envelope: Option<Value>,
    jobs: Vec<Value>,
    asset_jobs: BTreeMap<String, AssetJob>,
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
            jobs: mut persisted_jobs,
            asset_jobs: mut persisted_asset_jobs,
            bundle_jobs: persisted_bundle_jobs,
            next_seq,
            contexts,
            plans,
        } = persisted;
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
                        job.state = PublishState::Waiting;
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
                devices,
                coordinators,
                codex_envelope: None,
                jobs: persisted_jobs,
                asset_jobs: persisted_asset_jobs,
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
            devices: inner.devices.values().cloned().collect(),
            jobs,
            asset_jobs: inner.asset_jobs.clone(),
            bundle_jobs: inner.coordinators.iter().filter_map(|(mac, c)| {
                c.job.as_ref().filter(|j| !j.state.is_terminal())
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
        if let Some(record) = inner.devices.get(&mac) {
            if record.legacy {
                bail!("device {mac} runs legacy firmware; publish uses the legacy template path");
            }
        }
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
            .enqueue_bundle(bundle)
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
        job.updated_at = crate::now_secs();
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

    pub fn cancel_job(&self, mac: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(job) = inner.asset_jobs.get_mut(&mac.to_uppercase()) {
            if !job.state.is_terminal() { job.state = PublishState::Cancelled; }
            let _ = Self::persist(&inner, &self.state_path());
            return;
        }
        if let Some(c) = inner.coordinators.get_mut(&mac.to_uppercase()) {
            c.cancel_job();
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
                    "legacy": d.legacy,
                    "capabilities": d.capabilities,
                    "hardware_verified": d.capabilities.hardware_verified,
                    "owner": coordinator.map(|c| c.session.clone()),
                    "profile": d.profile,
                    "profile_count": d.profile.as_ref().map(|p| p.template_ids.len()).unwrap_or(0),
                    "sync_enabled": d.profile.as_ref().map(|p| p.sync_enabled).unwrap_or(false),
                    "active_template_id": coordinator.and_then(|c| c.active_template_id.clone()),
                    "job": inner.asset_jobs.get(mac).map(AssetJob::summary)
                        .or_else(|| coordinator.and_then(|c| c.job_snapshot())),
                    "data": coordinator.map(|c| json!({
                        "push_dirty": c.data.push_dirty,
                        "pull_only_change": c.pull_only_change(),
                        "full_sync_due": c.full_sync_due(crate::now_secs()),
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
        legacy: bool,
    ) -> Result<()> {
        capabilities.validate()?;
        let mac = identity.device_mac.clone();
        let mut inner = self.inner.lock().unwrap();
        let existing = inner.devices.get(&mac).cloned();
        let record = DeviceRecord {
            identity,
            capabilities: capabilities.clone(),
            legacy,
            profile: existing.as_ref().and_then(|d| d.profile.clone()),
            observed: existing
                .as_ref()
                .map(|d| d.observed.clone())
                .unwrap_or_default(),
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
        let now = crate::now_secs();
        let mut inner = self.inner.lock().unwrap();
        if let Some(record) = inner.devices.get_mut(&mac) {
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
        let mut power_reconciled = false;
        if let Some(c) = inner.coordinators.get_mut(&mac) {
            if let Some(pending) = c.plan.pending_explicit_light.clone() {
                let power = &status["power"];
                if power["plan_id"] == pending.plan_id && power["mode"] == "light" {
                    if let Some(remaining) = power["remaining_s"].as_u64().filter(|n| *n > 0) {
                        power_reconciled = c.note_plan_ack(pending.plan_id, remaining as u32, false)
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
                .insert(mac.clone(), crate::now_secs());
        } else {
            let last = inner.status_persist_at.get(&mac).copied().unwrap_or(0);
            if crate::now_secs().saturating_sub(last) >= 60 {
                inner
                    .status_persist_at
                    .insert(mac.clone(), crate::now_secs());
                let _ = Self::persist(&inner, &self.state_path());
            }
        }
        Ok(())
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
            crate::now_secs(),
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

    /// Formal plan for a rendezvous. BOOT keeps its provisional semantics: the
    /// bridge answers with remaining time or a deliberately different window.
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
        // Light is only granted for real pending work: sync alone does not keep
        // the radio on (v2 §7). Pending data/jobs/activation do.
        let want_light = inner
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
            .unwrap_or(false);
        let c = inner.coordinators.get_mut(&mac).context("unknown device")?;
        let plan = if wake_reason == "manual" {
            let t_boot = now.saturating_sub(
                (crate::coordinator::BOOT_PROVISIONAL_S as u32)
                    .saturating_sub(provisional_remaining_s) as u64,
            );
            c.boot_plan(
                now,
                t_boot,
                want_light,
                crate::coordinator::BOOT_PROVISIONAL_S,
            )
        } else {
            c.plan_for(
                now,
                want_light,
                crate::coordinator::MAX_LIGHT_S,
                "rendezvous",
            )
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
        let now = crate::now_secs();
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
    ) -> Value {
        let mut inner = self.inner.lock().unwrap();
        let Some(c) = inner.coordinators.get_mut(&mac.to_uppercase()) else {
            return json!({"outcome": "unknown_device"});
        };
        let outcome = c.note_plan_ack(plan_id, remaining_s, provisional);
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

    pub fn coordinator_summary(&self, mac: &str) -> Option<Value> {
        let inner = self.inner.lock().unwrap();
        inner
            .coordinators
            .get(&mac.to_uppercase())
            .map(|c| c.summary())
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

    /// Legacy migration: the ≤3 enabled legacy profile upgrades to a 1–8 order
    /// without silent truncation; entries beyond 8 are an explicit error.
    pub fn migrate_legacy_profile(
        &self,
        mac: &str,
        legacy_ids: &[String],
        now: u64,
    ) -> Result<Profile> {
        if legacy_ids.len() > MAX_PROFILE_TEMPLATES {
            bail!(
                "legacy profile has {} entries; refusing to truncate to {MAX_PROFILE_TEMPLATES}",
                legacy_ids.len()
            );
        }
        if legacy_ids.is_empty() {
            bail!("legacy profile is empty");
        }
        let mut profile = Profile::draft(&mac.to_uppercase());
        profile.template_ids = legacy_ids.to_vec();
        profile.initial_active_id = legacy_ids.first().cloned();
        profile.sync_enabled = false;
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
            if c.full_sync_due(crate::now_secs()) {
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
                "legacy": d.legacy,
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
            false,
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
        resumed.template_save("quad", "epd-ssd1681-200x200-1bpp", &quad_source(), 3001)
            .unwrap(); // checkpoints the in-flight Sending state
        drop(resumed);
        let recovered = PlatformService::open(dir.path()).unwrap();
        assert_eq!(recovered.job("AA:BB:CC:DD:EE:FF").unwrap()["state"], "waiting");
        assert_eq!(recovered.bundle_payload("AA:BB:CC:DD:EE:FF").unwrap(), frozen);
        assert!(recovered.retry_job("AA:BB:CC:DD:EE:FF", job["job_id"].as_str().unwrap(), true, 4000));
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
        assert_eq!(resumed.note_plan_ack(mac, light.plan_id, 500, false)["outcome"], "accepted");
        let summary = resumed.coordinator_summary(mac).unwrap();
        assert!(summary["plan"]["pending_explicit_light"].is_null());
        assert_eq!(summary["plan"]["last_explicit_light_ack"]["plan_id"], light.plan_id);
        let hold_until = summary["plan"]["light_hold_until"].as_u64().unwrap();
        assert_eq!(resumed.plan_for_rendezvous(mac, hold_until - 1, "rendezvous", 0).unwrap().mode, PlanMode::Light);
        assert_eq!(resumed.plan_for_rendezvous(mac, hold_until + 1, "rendezvous", 0).unwrap().mode, PlanMode::Sleep);
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
        let state = resumed.coordinator_summary(mac).unwrap();
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
        assert!(svc.coordinator_summary(mac).unwrap()["plan"]["pending_explicit_light"].is_null());
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
        svc.device_upsert(DeviceIdentity::new("AA:BB:CC:DD:EE:FF", "Test").unwrap(), caps, false).unwrap();
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
        restored.cancel_job("AA:BB:CC:DD:EE:FF");
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
        svc.device_upsert(DeviceIdentity::new("7C4FADB93408", "Note4").unwrap(), caps, false).unwrap();
        let source: Value = serde_json::from_str(include_str!("../../../../../project-workflow/generic-display-platform-implementation/concepts-400x300/codex-status-a-400x300.json")).unwrap();
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
        assert_eq!(bundle.compiler_abi, 1);
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
        svc.cancel_job(mac);
        let t = crate::now_secs();
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
        assert!(svc.coordinator_summary(mac).unwrap()["in_flight"].is_null());
        let deadline_before = svc.coordinator_summary(mac).unwrap()["context"]
            ["full_sync_deadline"]
            .as_u64()
            .unwrap();

        // Pull-only change: no immediate push, deadline unchanged.
        env["buckets"][0]["windows"][0]["resetsAt"] = json!(9999);
        svc.note_codex_envelope(&env).unwrap();
        assert_eq!(svc.next_delivery(mac, true, t + 1)["decision"], "idle");
        assert_eq!(
            svc.coordinator_summary(mac).unwrap()["context"]["full_sync_deadline"].as_u64(),
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
        svc.cancel_job("AA:BB:CC:DD:EE:FF");
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
        let next_seq = svc.coordinator_summary(mac).unwrap()["context"]["next_seq"]
            .as_u64()
            .unwrap();
        drop(svc);
        // Reopen the service: seq continues, never restarts at 0.
        let svc2 = PlatformService::open(dir.path()).unwrap();
        let summary = svc2.coordinator_summary(mac).unwrap();
        assert!(summary["context"]["next_seq"].as_u64().unwrap() >= next_seq);
        assert_eq!(summary["context"]["context_id"], "ctx-1");
    }

    #[test]
    fn migration_rejects_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service_with_device(dir.path());
        let long: Vec<String> = (0..9).map(|i| format!("t{i}")).collect();
        assert!(svc
            .migrate_legacy_profile("AA:BB:CC:DD:EE:FF", &long, 0)
            .is_err());
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
