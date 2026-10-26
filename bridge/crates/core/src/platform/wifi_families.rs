//! Bridge-only Wi-Fi library. Device wire/storage still uses host_ssid as the family name.
use super::{model::{self, DeviceIdentity}, wifi::Network};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Family {
    pub id: String,
    #[serde(default)] pub name: String,
    #[serde(default)] pub ssid_2g: String,
    #[serde(default)] pub ssid_5g: String,
    #[serde(default)] pub password_2g: Option<String>,
    #[serde(default)] pub password_5g: Option<String>,
}
// Credentials must never enter Debug/log output.
impl std::fmt::Debug for Family {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Family").field("id", &self.id).finish_non_exhaustive()
    }
}
impl Family {
    pub fn validate(&mut self) -> Result<()> {
        if self.name.is_empty() { self.name = if self.ssid_5g.is_empty() { &self.ssid_2g } else { &self.ssid_5g }.clone(); }
        ensure!(!self.ssid_2g.is_empty() || !self.ssid_5g.is_empty(), "wifi_family_empty");
        for ssid in [&self.ssid_2g, &self.ssid_5g] {
            ensure!(ssid.len() <= 32 && !ssid.contains('\0'), "invalid_wifi_network");
        }
        // Reuse exactly the existing wire limits/credential validation.
        for password in [&self.password_2g, &self.password_5g] {
            let n = Network { network_id:self.id.clone(),host_ssid:self.name.clone(),ssid:self.name.clone(),
                label:String::new(),credential:"keep".into(),password:password.clone(),origin:"local".into(),version:0,deleted:false };
            n.validate()?;
        }
        if !self.ssid_2g.is_empty() && self.ssid_2g == self.ssid_5g {
            ensure!(self.password_2g == self.password_5g, "wifi_family_credential_conflict");
        }
        Ok(())
    }
    pub fn network(&self, firmware_target: &str) -> Result<Network> {
        // All three current radios support 2.4 GHz. Unknown targets must declare
        // their capability through a future extension rather than guessing 5 GHz.
        ensure!(matches!(firmware_target, model::FIRMWARE_TARGET_NOTE4 | model::FIRMWARE_TARGET_154G |
            model::FIRMWARE_TARGET_AP01 | model::FIRMWARE_TARGET_GRAY4), "wifi_capability_unknown");
        ensure!(!self.ssid_2g.is_empty(), "wifi_family_no_compatible_network");
        Ok(Network { network_id:self.id.clone(),host_ssid:self.name.clone(),ssid:self.ssid_2g.clone(),
            label:self.name.clone(),credential:match self.password_2g.as_deref() {Some("")=>"open",Some(_)=>"replace",None=>"keep"}.into(),
            password:self.password_2g.clone(),origin:"local".into(),version:0,deleted:false })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    #[serde(default="library_schema")] pub schema: u32,
    pub revision: u64,
    pub families: Vec<Family>,
    pub devices: BTreeMap<String, Vec<String>>,
    pub wired_family: Option<String>,
}
fn library_schema()->u32 {1}
impl Default for Library {
    fn default()->Self {Self{schema:1,revision:0,families:Vec::new(),devices:BTreeMap::new(),wired_family:None}}
}
impl Library {
    pub fn validate(&mut self) -> Result<()> {
        ensure!(self.schema==1,"unsupported_wifi_library_schema");
        ensure!(self.families.len() <= 64, "capacity");
        let mut ids=BTreeSet::new();let mut names=BTreeSet::new();let mut ssids=BTreeMap::new();
        for f in &mut self.families {
            f.validate()?;
            ensure!(ids.insert(f.id.clone()) && names.insert(f.name.clone()), "wifi_family_conflict");
            for ssid in [&f.ssid_2g,&f.ssid_5g].into_iter().filter(|s|!s.is_empty()) {
                ensure!(ssids.insert(ssid.clone(),f.id.clone()).is_none_or(|id|id==f.id), "wifi_family_ssid_conflict");
            }
        }
        ensure!(self.wired_family.as_ref().is_none_or(|id|ids.contains(id)), "wifi_family_not_found");
        for (mac, selected) in &self.devices {
            ensure!(DeviceIdentity::normalized_mac(mac).as_ref()==Some(mac), "invalid_device_mac");
            ensure!(selected.len()<=12 && selected.iter().collect::<BTreeSet<_>>().len()==selected.len() &&
                selected.iter().all(|id|ids.contains(id)), "invalid_wifi_selection");
        }
        Ok(())
    }
    pub fn current_name<'a>(&'a self, host:&Value) -> Option<&'a str> {
        match host["state"].as_str()? {
            "connected" => {
                let ssid=host["networks"][0]["ssid"].as_str()?;
                let mut matching=self.families.iter().filter(|f|f.ssid_2g==ssid||f.ssid_5g==ssid);
                let family=matching.next()?;
                matching.next().is_none().then_some(family.name.as_str())
            },
            "no_wifi" => self.wired_family.as_ref().and_then(|id|self.families.iter().find(|f|&f.id==id)).map(|f|f.name.as_str()),
            _ => None,
        }
    }
    pub fn get(&self,id:&str)->Result<&Family> { self.families.iter().find(|f|f.id==id).context("wifi_family_not_found") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn bands_names_and_host_resolution() {
        let mut lib:Library=serde_json::from_value(json!({"revision":0,"families":[
            {"id":"home","ssid_2g":"Home-2","ssid_5g":"Home-5"},
            {"id":"same","ssid_2g":"Both","ssid_5g":"Both","password_2g":"","password_5g":""}],
            "devices":{},"wired_family":"home"})).unwrap();
        lib.validate().unwrap();assert_eq!(lib.families[0].name,"Home-5");
        for ssid in ["Home-2","Home-5"] {
            assert_eq!(lib.current_name(&json!({"state":"connected","networks":[{"ssid":ssid}]})),Some("Home-5"));
        }
        assert_eq!(lib.current_name(&json!({"state":"no_wifi"})),Some("Home-5"));
        for state in ["permission_denied","ambiguous","unavailable"] {assert!(lib.current_name(&json!({"state":state})).is_none());}
        assert!(lib.current_name(&json!({"state":"connected","networks":[{"ssid":"Unknown"}]})).is_none());
        assert_eq!(lib.families[0].network(model::FIRMWARE_TARGET_NOTE4).unwrap().host_ssid,"Home-5");
        lib.families[1].ssid_2g="Home-2".into();assert!(lib.validate().is_err());
    }
}
