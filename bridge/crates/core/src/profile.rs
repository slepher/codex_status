//! Push profiles: an ordered selection of templates. Each entry carries its
//! own enabled flag (order and enabled state are independent); only enabled
//! entries are pushed (at most three, matching the device slots), and the
//! first enabled entry is the default template shown after a push.

use std::fs;
use std::path::Path;

use anyhow::{bail, Result};
use serde::{Deserialize, Deserializer, Serialize};

pub const MAX_PROFILE_TEMPLATES: usize = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileEntry {
    pub id: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Accepts both `"id"` and `{"id": ..., "enabled": ...}` so profiles written
/// with the previous format keep loading.
#[derive(Deserialize)]
#[serde(untagged)]
enum EntryDef {
    Plain(String),
    Full(ProfileEntry),
}

fn de_entries<'de, D>(deserializer: D) -> Result<Vec<ProfileEntry>, D::Error>
where
    D: Deserializer<'de>,
{
    let defs = Vec::<EntryDef>::deserialize(deserializer)?;
    Ok(defs
        .into_iter()
        .map(|def| match def {
            EntryDef::Plain(id) => ProfileEntry { id, enabled: true },
            EntryDef::Full(entry) => entry,
        })
        .collect())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfilesFile {
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "de_entries")]
    pub templates: Vec<ProfileEntry>,
}

impl Profile {
    pub fn enabled_ids(&self) -> Vec<String> {
        self.templates
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| entry.id.clone())
            .collect()
    }
}

impl ProfilesFile {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn upsert(&mut self, profile: Profile, known: &[String]) -> Result<()> {
        validate(&profile, known)?;
        match self.profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(existing) => *existing = profile,
            None => self.profiles.push(profile),
        }
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        self.profiles.len() != before
    }
}

pub fn validate(profile: &Profile, known: &[String]) -> Result<()> {
    if profile.id.is_empty()
        || !profile
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("invalid profile id: use [a-z0-9_-]");
    }
    if profile.templates.iter().filter(|e| e.enabled).count() > MAX_PROFILE_TEMPLATES {
        bail!("at most {MAX_PROFILE_TEMPLATES} templates can be enabled (device has 3 slots)");
    }
    let mut seen: Vec<&str> = Vec::new();
    for entry in &profile.templates {
        if !known.iter().any(|k| k == &entry.id) {
            bail!("unknown template in profile: {}", entry.id);
        }
        if seen.contains(&entry.id.as_str()) {
            bail!("duplicate template in profile: {}", entry.id);
        }
        seen.push(&entry.id);
    }
    Ok(())
}
