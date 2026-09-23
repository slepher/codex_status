//! Frozen complete target and pure differential planner for the versioned
//! asset protocol. The HTTP transport is gated until the device contract lands.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::model::{Binding, DeviceCapabilities, Profile, Template};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Object {
    pub id: String,
    pub kind: String,
    pub length: u64,
    pub crc32: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

impl Object {
    pub fn new(kind: &str, bytes: Vec<u8>) -> Self {
        let length = bytes.len() as u64;
        let crc32 = format!("{:08x}", crc32fast::hash(&bytes));
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        let id = format!("{kind}:{length}:{crc32}:{sha256}");
        Self { id, kind: kind.into(), length, crc32, sha256, bytes }
    }

    pub fn verify(&self) -> Result<()> {
        let actual = Self::new(&self.kind, self.bytes.clone());
        if actual.id != self.id || actual.length != self.length || actual.crc32 != self.crc32 || actual.sha256 != self.sha256 {
            bail!("object {} has changed bytes", self.id);
        }
        Ok(())
    }

    fn descriptor(&self) -> ObjectDescriptor {
        ObjectDescriptor { id: self.id.clone(), kind: self.kind.clone(), length: self.length,
            crc32: self.crc32.clone(), sha256: self.sha256.clone() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectDescriptor {
    pub id: String,
    pub kind: String,
    pub length: u64,
    pub crc32: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateRef {
    pub template_id: String,
    pub source_id: String,
    pub compiled_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontRef {
    pub name: String,
    pub font_id: String,
    pub object_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub protocol: u32,
    pub job_id: String,
    pub device_mac: String,
    pub firmware_target: String,
    pub render_target: String,
    pub compiler_abi: u32,
    pub profile_order: Vec<String>,
    pub initial_active_id: String,
    pub full_sync_s: u64,
    pub bindings: Vec<Binding>,
    pub templates: Vec<TemplateRef>,
    pub fonts: Vec<FontRef>,
    pub objects: Vec<ObjectDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrozenPublish {
    pub manifest: Manifest,
    pub manifest_id: String,
    pub target_id: String,
    pub manifest_bytes: Vec<u8>,
    pub objects: Vec<Object>,
}

impl FrozenPublish {
    pub fn build(
        job_id: &str, caps: &DeviceCapabilities, profile: &Profile,
        templates: &[Template], font_bytes: &BTreeMap<String, Vec<u8>>,
    ) -> Result<Self> {
        profile.validate_publishable(caps)?;
        if caps.asset_publish_protocol != 1 { bail!("device does not advertise asset publish protocol 1"); }
        let mut objects = BTreeMap::<String, Object>::new();
        let mut template_refs = Vec::new();
        for id in &profile.template_ids {
            let t = templates.iter().find(|t| &t.key.template_id == id && t.key.render_target == caps.render_target)
                .ok_or_else(|| anyhow::anyhow!("template {id} has no {} variant", caps.render_target))?;
            if t.compiled.compiler_abi != caps.compiler_abi { bail!("template {id} ABI mismatch"); }
            let source = Object::new("source", crate::template::canonical_bytes(&t.source));
            let compiled = Object::new("compiled", crate::compile::binary_bytes(&t.compiled)?);
            template_refs.push(TemplateRef { template_id: id.clone(), source_id: source.id.clone(), compiled_id: compiled.id.clone() });
            insert_object(&mut objects, source)?;
            insert_object(&mut objects, compiled)?;
        }
        let mut font_refs = Vec::new();
        for (name, id) in &profile.font_ids {
            let bytes = font_bytes.get(name).ok_or_else(|| anyhow::anyhow!("font {name} has no frozen bytes"))?;
            let descriptor = super::fonts::FontLibrary::validate(bytes)?;
            descriptor.check_target(&caps.pixel_format)?;
            if descriptor.name != *name || descriptor.id != *id { bail!("font {name} changed after selection"); }
            let object = Object::new("font", bytes.clone());
            font_refs.push(FontRef { name: name.clone(), font_id: id.clone(), object_id: object.id.clone() });
            insert_object(&mut objects, object)?;
        }
        let manifest = Manifest {
            protocol: 1, job_id: job_id.into(), device_mac: profile.device_mac.clone(),
            firmware_target: caps.firmware_target.clone(), render_target: caps.render_target.clone(),
            compiler_abi: caps.compiler_abi, profile_order: profile.template_ids.clone(),
            initial_active_id: profile.initial_active_id.clone().unwrap_or_else(|| profile.template_ids[0].clone()),
            full_sync_s: profile.full_sync_s,
            bindings: profile.bindings.clone(), templates: template_refs, fonts: font_refs,
            objects: objects.values().map(Object::descriptor).collect(),
        };
        let mut target = manifest.clone();
        target.job_id.clear();
        let target_id = Object::new("target", crate::template::canonical_bytes(&serde_json::to_value(&target)?)).id;
        let manifest_bytes = crate::template::canonical_bytes(&serde_json::to_value(&manifest)?);
        let manifest_id = Object::new("manifest", manifest_bytes.clone()).id;
        Ok(Self { manifest, manifest_id, target_id, manifest_bytes, objects: objects.into_values().collect() })
    }

    pub fn verify(&self) -> Result<()> {
        if Object::new("manifest", self.manifest_bytes.clone()).id != self.manifest_id { bail!("frozen manifest changed"); }
        if self.manifest_bytes != crate::template::canonical_bytes(&serde_json::to_value(&self.manifest)?) { bail!("manifest/body mismatch"); }
        let mut target = self.manifest.clone(); target.job_id.clear();
        if Object::new("target", crate::template::canonical_bytes(&serde_json::to_value(&target)?)).id != self.target_id { bail!("target ID mismatch"); }
        for object in &self.objects { object.verify()?; }
        let expected: BTreeSet<String> = self.manifest.objects.iter().map(|o| o.id.clone()).collect();
        let actual: BTreeSet<String> = self.objects.iter().map(|o| o.id.clone()).collect();
        if expected != actual || self.manifest.objects.len() != self.objects.len() { bail!("manifest object list mismatch"); }
        for descriptor in &self.manifest.objects {
            if !self.objects.iter().any(|o| &o.descriptor() == descriptor) { bail!("manifest object descriptor mismatch"); }
        }
        Ok(())
    }
}

fn insert_object(objects: &mut BTreeMap<String, Object>, object: Object) -> Result<()> {
    if let Some(old) = objects.get(&object.id) {
        if old.bytes != object.bytes { bail!("content ID collision {}", object.id); }
    } else { objects.insert(object.id.clone(), object); }
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommittedRefs {
    pub active_target_id: Option<String>,
    pub active_manifest_id: Option<String>,
    pub rollback_manifest_id: Option<String>,
    pub object_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capacity {
    pub max_object_bytes: u64,
    pub max_manifest_bytes: u64,
    pub free_bytes: u64,
    pub install_peak_bytes: u64,
    pub filesystem_overhead_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferPlan {
    pub manifest_id: String,
    pub target_id: String,
    pub already_active: bool,
    pub missing: Vec<String>,
    pub reused: Vec<String>,
    pub transfer_bytes: u64,
    pub reused_bytes: u64,
    pub font_transfer_bytes: u64,
    pub template_transfer_bytes: u64,
    pub peak_new_bytes: u64,
}

pub fn plan(frozen: &FrozenPublish, refs: &CommittedRefs, capacity: &Capacity) -> Result<TransferPlan> {
    frozen.verify()?;
    if refs.active_target_id.as_deref() == Some(frozen.target_id.as_str()) {
        return Ok(TransferPlan { manifest_id: frozen.manifest_id.clone(), target_id: frozen.target_id.clone(), already_active: true,
            missing: Vec::new(), reused: frozen.objects.iter().map(|o| o.id.clone()).collect(), transfer_bytes: 0,
            reused_bytes: frozen.objects.iter().map(|o| o.length).sum(), font_transfer_bytes: 0,
            template_transfer_bytes: 0, peak_new_bytes: 0 });
    }
    if frozen.manifest_bytes.len() as u64 > capacity.max_manifest_bytes { bail!("manifest exceeds device max_manifest_bytes"); }
    let mut missing = Vec::new();
    let mut reused = Vec::new();
    let (mut transfer_bytes, mut reused_bytes, mut font_bytes, mut template_bytes) = (0, 0, 0, 0);
    let mut max_object = 0;
    for object in &frozen.objects {
        if object.length > capacity.max_object_bytes { bail!("{} exceeds device max_object_bytes", object.id); }
        if refs.object_ids.contains(&object.id) {
            reused.push(object.id.clone()); reused_bytes += object.length;
        } else {
            missing.push(object.id.clone()); transfer_bytes += object.length;
            max_object = max_object.max(object.length);
            if object.kind == "font" { font_bytes += object.length; } else { template_bytes += object.length; }
        }
    }
    // Reserve the new objects, one temporary copy of the largest object, two
    // manifest writes, and filesystem overhead per new file. Active/rollback
    // objects are retained and are not counted as free space.
    let peak_new_bytes = transfer_bytes.saturating_add(max_object)
        .saturating_add(2 * frozen.manifest_bytes.len() as u64)
        .saturating_add(capacity.filesystem_overhead_bytes.saturating_mul(missing.len() as u64 + 2));
    if peak_new_bytes > capacity.free_bytes || peak_new_bytes > capacity.install_peak_bytes {
        bail!("insufficient space: peak {peak_new_bytes}, free {}, install budget {}", capacity.free_bytes, capacity.install_peak_bytes);
    }
    Ok(TransferPlan { manifest_id: frozen.manifest_id.clone(), target_id: frozen.target_id.clone(), already_active: false, missing, reused,
        transfer_bytes, reused_bytes, font_transfer_bytes: font_bytes,
        template_transfer_bytes: template_bytes, peak_new_bytes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(template: &[u8], font: &[u8]) -> FrozenPublish {
        let source = Object::new("source", template.to_vec());
        let compiled = Object::new("compiled", template.to_vec());
        let font = Object::new("font", font.to_vec());
        let manifest = Manifest { protocol: 1, job_id: "job-1".into(), device_mac: "AABBCCDDEEFF".into(),
            firmware_target: "zectrix-note4-400x300".into(), render_target: "epd-ssd2683-400x300-1bpp".into(),
            compiler_abi: 2, profile_order: vec!["a".into()], initial_active_id: "a".into(), full_sync_s: 3600,
            bindings: vec![], templates: vec![TemplateRef { template_id: "a".into(), source_id: source.id.clone(), compiled_id: compiled.id.clone() }],
            fonts: vec![FontRef { name: "ntthin18".into(), font_id: "12345678".into(), object_id: font.id.clone() }],
            objects: vec![source.descriptor(), compiled.descriptor(), font.descriptor()] };
        let manifest_bytes = crate::template::canonical_bytes(&serde_json::to_value(&manifest).unwrap());
        let manifest_id = Object::new("manifest", manifest_bytes.clone()).id;
        let mut target = manifest.clone(); target.job_id.clear();
        let target_id = Object::new("target", crate::template::canonical_bytes(&serde_json::to_value(&target).unwrap())).id;
        FrozenPublish { manifest, manifest_id, target_id, manifest_bytes, objects: vec![source, compiled, font] }
    }

    fn room() -> Capacity {
        Capacity { max_object_bytes: 10000, max_manifest_bytes: 10000, free_bytes: 100000,
            install_peak_bytes: 100000, filesystem_overhead_bytes: 32 }
    }

    #[test]
    fn template_and_font_changes_transfer_only_missing_content() {
        let old = sample(b"template-a", b"font-a");
        let template_edit = sample(b"template-b", b"font-a");
        let refs = CommittedRefs { object_ids: old.objects.iter().map(|o| o.id.clone()).collect(), ..Default::default() };
        let t = plan(&template_edit, &refs, &room()).unwrap();
        assert_eq!(t.font_transfer_bytes, 0);
        assert!(t.template_transfer_bytes > 0);

        let font_edit = sample(b"template-a", b"font-b");
        let f = plan(&font_edit, &refs, &room()).unwrap();
        assert_eq!(f.template_transfer_bytes, 0);
        assert_eq!(f.missing.len(), 1);
        assert_ne!(font_edit.manifest.fonts[0].object_id, old.manifest.fonts[0].object_id);

        let repeat = plan(&old, &refs, &room()).unwrap();
        assert_eq!(repeat.transfer_bytes, 0);
        assert!(repeat.missing.is_empty());
        let mut new_job = old.clone();
        new_job.manifest.job_id = "job-2".into();
        new_job.manifest_bytes = crate::template::canonical_bytes(&serde_json::to_value(&new_job.manifest).unwrap());
        new_job.manifest_id = Object::new("manifest", new_job.manifest_bytes.clone()).id;
        assert_eq!(new_job.target_id, old.target_id);
        let active = CommittedRefs { active_target_id: Some(old.target_id.clone()), ..refs };
        assert!(plan(&new_job, &active, &room()).unwrap().already_active);
    }

    #[test]
    fn frozen_bytes_survive_source_change_and_restart_serialization() {
        let frozen = sample(b"before", b"font-v1");
        let original = frozen.manifest_id.clone();
        let _source_now = sample(b"after", b"font-v2");
        let restored: FrozenPublish = serde_json::from_slice(&serde_json::to_vec(&frozen).unwrap()).unwrap();
        restored.verify().unwrap();
        assert_eq!(restored.manifest_id, original);
        assert_eq!(restored.objects[2].bytes, b"font-v1");
    }

    #[test]
    fn capacity_and_corruption_fail_before_install() {
        let frozen = sample(b"template", b"font");
        let mut small = room(); small.free_bytes = 1;
        assert!(plan(&frozen, &CommittedRefs::default(), &small).unwrap_err().to_string().contains("insufficient space"));
        let mut max = room(); max.max_object_bytes = 3;
        assert!(plan(&frozen, &CommittedRefs::default(), &max).unwrap_err().to_string().contains("max_object_bytes"));
        let mut corrupt = frozen.clone(); corrupt.objects[2].bytes[0] ^= 1;
        assert!(plan(&corrupt, &CommittedRefs::default(), &room()).is_err());
    }

    // Host-only model of the draft transaction. This is not Note4 firmware
    // validation; it checks interruption and ACK reasoning against the contract.
    #[derive(Default)]
    struct HostDevice {
        active: Option<String>,
        committed_job: Option<String>,
        staged: BTreeMap<String, Vec<u8>>,
        switches: u32,
    }
    impl HostDevice {
        fn chunk(&mut self, object: &Object, offset: usize, bytes: &[u8], crc: u32) -> Result<usize> {
            if crc32fast::hash(bytes) != crc { bail!("bad_crc"); }
            let entry = self.staged.entry(object.id.clone()).or_default();
            if offset < entry.len() && entry.get(offset..offset + bytes.len()) == Some(bytes) { return Ok(entry.len()); }
            if offset != entry.len() || entry.len() + bytes.len() > object.bytes.len() { bail!("bad_offset"); }
            entry.extend_from_slice(bytes); Ok(entry.len())
        }
        fn commit(&mut self, frozen: &FrozenPublish) -> Result<String> {
            if self.committed_job.as_deref() == Some(frozen.manifest.job_id.as_str()) { return Ok(self.active.clone().unwrap()); }
            for object in &frozen.objects {
                if self.staged.get(&object.id) != Some(&object.bytes) { bail!("missing_object"); }
            }
            self.active = Some(frozen.manifest_id.clone());
            self.committed_job = Some(frozen.manifest.job_id.clone());
            self.switches += 1;
            Ok(frozen.manifest_id.clone())
        }
    }

    #[test]
    fn host_transaction_interruption_crc_power_loss_and_lost_ack() {
        let frozen = sample(b"new-template", b"new-font");
        let mut device = HostDevice { active: Some("old-manifest".into()), ..Default::default() };
        let first = &frozen.objects[0];
        let split = first.bytes.len() / 2;
        assert_eq!(device.chunk(first, 0, &first.bytes[..split], crc32fast::hash(&first.bytes[..split])).unwrap(), split);
        assert_eq!(device.active.as_deref(), Some("old-manifest"));
        assert!(device.chunk(first, split, &first.bytes[split..], 0).is_err());
        assert_eq!(device.staged[&first.id].len(), split);
        // Simulated power loss retains staged offsets; old manifest stays active.
        assert!(device.commit(&frozen).is_err());
        for object in &frozen.objects {
            let at = device.staged.get(&object.id).map_or(0, Vec::len);
            device.chunk(object, at, &object.bytes[at..], crc32fast::hash(&object.bytes[at..])).unwrap();
        }
        assert_eq!(device.commit(&frozen).unwrap(), frozen.manifest_id);
        // Lost HTTP ACK and Bridge restart: committed job/status is authoritative.
        assert_eq!(device.committed_job.as_deref(), Some("job-1"));
        assert_eq!(device.commit(&frozen).unwrap(), frozen.manifest_id);
        assert_eq!(device.switches, 1);
    }
}
