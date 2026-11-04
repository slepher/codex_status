//! Durable original-operation fences. One application worker owns each receipt;
//! a persistence error stops mutation until the file is reloaded and queried.
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs::{File,OpenOptions},io::{Read,Write},path::Path};
use crate::ap01_app_set::Manifest;
const MAX_RECEIPT:usize=32768;

fn hex<const N:usize>(text:&str)->Result<[u8;N]> {
    ensure!(text.len()==2*N && text.bytes().all(|b|b.is_ascii_hexdigit()),"app_receipt_hex");
    let mut out=[0;N];for (i,b) in out.iter_mut().enumerate(){*b=u8::from_str_radix(&text[2*i..2*i+2],16)?;}Ok(out)
}
fn number(value:&Value,key:&str)->Result<u64> {value[key].as_u64().with_context(||format!("app_receipt_number: {key}"))}
fn digest_string(digest:&[u8])->String {digest.iter().map(|b|format!("{b:02x}")).collect()}

#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Phase {Receiving,Ready,SwitchPending,TrialStarted,RollbackPending,RollbackStarted,Succeeded,FailedRolledBack,RecoveryRequired,Cancelled}
impl Phase {pub fn terminal(self)->bool {matches!(self,Self::Succeeded|Self::FailedRolledBack|Self::RecoveryRequired|Self::Cancelled)}}
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {pub phase:Phase,pub sequence:u64,pub boot_id:u64,pub changed_mask:u32,pub ready_mask:u32,pub received:[u32;8]}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub format:u32,pub endpoint:String,pub device_mac:String,pub operation_id:String,
    pub original_boot:u64,pub expected_manifest:String,pub expected_version:u64,
    manifest_base64:String,pub progress:Option<Progress>,
}
impl Receipt {
    pub fn prepare(endpoint:&str,mac:&str,operation:&str,bytes:&[u8],status:&Value)->Result<Self> {
        let manifest=Manifest::parse(bytes)?;manifest.check_capabilities(status)?;
        let phase=status["phase"].as_str().context("app_receipt_phase")?;
        ensure!(matches!(phase,"idle"|"succeeded"|"failed_rolled_back"|"cancelled") && (status["cleanup_pending"].as_bool()==Some(false)||(phase=="idle"&&status["cleanup_pending"].is_null())) && status["runtime_reporting"]==true,"app_set_not_ready");
        let (digest,version)=if phase=="idle" {(String::from_utf8(vec![b'0';64])?,0)} else {
            let (d,v)=if phase=="succeeded" {("candidate_manifest","candidate_version")}else{("old_manifest","old_version")};
            (status[d].as_str().context("app_receipt_expected_manifest")?.to_owned(),number(status,v)?)
        };
        let out=Self{format:3,endpoint:endpoint.to_owned(),device_mac:mac.to_ascii_uppercase(),operation_id:operation.to_ascii_lowercase(),
            original_boot:number(status,"boot_id")?,expected_manifest:digest,expected_version:version,manifest_base64:STANDARD.encode(bytes),progress:None};
        out.validate()?;ensure!(status["device_mac"].as_str()==Some(out.device_mac.as_str()),"app_receipt_device_mismatch");Ok(out)
    }
    pub fn manifest(&self)->Result<Manifest> {
        ensure!(self.manifest_base64.len()<=4780,"app_receipt_manifest_bound");Manifest::parse(&STANDARD.decode(&self.manifest_base64)?)
    }
    fn validate(&self)->Result<()> {
        ensure!(self.format==3 && self.original_boot>0,"app_receipt_format_boot");
        let address:std::net::SocketAddr=self.endpoint.parse()?;ensure!(address.port()>0,"app_receipt_endpoint");
        hex::<6>(&self.device_mac)?;ensure!(self.device_mac==self.device_mac.to_ascii_uppercase(),"app_receipt_mac_canonical");
        ensure!(hex::<16>(&self.operation_id)?!=[0;16] && self.operation_id==self.operation_id.to_ascii_lowercase(),"app_receipt_operation");
        let old=hex::<32>(&self.expected_manifest)?;
        ensure!((self.expected_version==0)==(old==[0;32]) && self.expected_manifest==self.expected_manifest.to_ascii_lowercase(),"app_receipt_expected_fence");
        let manifest=self.manifest()?;ensure!(manifest.sequence>self.expected_version,"app_receipt_version_fence");
        if let Some(progress)=&self.progress {Self::validate_progress(&manifest,progress)?;}Ok(())
    }
    fn validate_progress(manifest:&Manifest,p:&Progress)->Result<()> {
        ensure!(p.sequence>0 && p.boot_id>0 && p.changed_mask>>manifest.apps.len()==0 && p.ready_mask&!p.changed_mask==0,"app_receipt_progress");
        for (i,n) in p.received.iter().enumerate(){let max=if p.changed_mask&(1<<i)!=0 {manifest.apps[i].package_bytes}else{0};ensure!(*n<=max,"app_receipt_offset");}Ok(())
    }
    fn bytes(&self)->Result<Vec<u8>> {self.validate()?;let bytes=serde_json::to_vec(self)?;ensure!(bytes.len()<=MAX_RECEIPT,"app_receipt_size");Ok(bytes)}
    /// Must finish before the first mutation. Existing paths are never replaced.
    pub fn create(&self,path:&Path)->Result<()> {
        let bytes=self.bytes()?;let mut options=OpenOptions::new();options.write(true).create_new(true);
        #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
        let mut file=options.open(path)?;file.write_all(&bytes)?;file.sync_all()?;drop(file);
        #[cfg(unix)] if let Some(parent)=path.parent(){File::open(parent)?.sync_all()?;}Ok(())
    }
    pub fn load_local(path:&Path,mac:&str)->Result<Self> {
        let mut bytes=Vec::new();File::open(path)?.take((MAX_RECEIPT+1) as u64).read_to_end(&mut bytes)?;ensure!(bytes.len()<=MAX_RECEIPT,"app_receipt_size");
        let out:Self=serde_json::from_slice(&bytes)?;out.validate()?;
        ensure!(out.device_mac==mac.to_ascii_uppercase(),"app_receipt_target_changed");Ok(out)
    }
    pub fn load(path:&Path,endpoint:&str,mac:&str)->Result<Self> {
        let out=Self::load_local(path,mac)?;
        ensure!(out.endpoint==endpoint,"app_receipt_target_changed");Ok(out)
    }
    /// Caller must freshly confirm terminal cleanup (and running identity on
    /// success). Keep an exclusive, synced copy before releasing the active slot.
    pub fn archive(&self,path:&Path)->Result<std::path::PathBuf> {
        ensure!(self.progress.as_ref().is_some_and(|p|matches!(p.phase,Phase::Succeeded|Phase::FailedRolledBack|Phase::Cancelled)),"app_receipt_not_terminal");
        self.archive_copy(path,"completed")
    }
    /// Only after a fresh authenticated query says the unacknowledged operation
    /// does not exist. This records a local abandonment, not a device Cancelled.
    pub fn archive_unstarted(&self,path:&Path)->Result<std::path::PathBuf> {
        ensure!(self.progress.is_none(),"app_receipt_already_acknowledged");
        self.archive_copy(path,"abandoned")
    }
    fn archive_copy(&self,path:&Path,suffix:&str)->Result<std::path::PathBuf> {
        let current=Self::load(path,&self.endpoint,&self.device_mac)?;
        ensure!(current.bytes()?==self.bytes()?,"app_receipt_file_changed");
        let name=path.file_name().and_then(|n|n.to_str()).context("app_receipt_filename")?;
        let archived=path.with_file_name(format!("{name}.{}.{suffix}",self.operation_id));
        if archived.exists(){
            ensure!(Self::load(&archived,&self.endpoint,&self.device_mac)?.bytes()?==self.bytes()?,"app_receipt_archive_conflict");
            File::open(&archived)?.sync_all()?;
            #[cfg(unix)] if let Some(parent)=archived.parent(){File::open(parent)?.sync_all()?;}
        }else{self.create(&archived)?;}
        ensure!(Self::load(path,&self.endpoint,&self.device_mac)?.bytes()?==self.bytes()?,"app_receipt_file_changed");
        std::fs::remove_file(path)?;
        #[cfg(unix)] if let Some(parent)=path.parent(){File::open(parent)?.sync_all()?;}
        Ok(archived)
    }
    /// A never-acknowledged receipt can follow a verified device even before
    /// begin exists. Preserve every operation/Manifest/boot fence verbatim.
    pub fn relocate_unstarted(&mut self,path:&Path,endpoint:&str,status:&Value)->Result<()> {
        ensure!(self.progress.is_none(),"app_receipt_already_acknowledged");
        ensure!(status["protocol"]==3 && status["device_mac"].as_str()==Some(self.device_mac.as_str()) && status["boot_id"].as_u64().is_some_and(|n|n>0),"app_receipt_identity");
        ensure!(Self::load(path,&self.endpoint,&self.device_mac)?.bytes()?==self.bytes()?,"app_receipt_file_changed");
        let mut next=self.clone();next.endpoint=endpoint.to_owned();
        crate::platform::store::atomic_write(path,&next.bytes()?)?;*self=next;Ok(())
    }
    pub fn begin_body(&self)->Result<Vec<u8>> {
        self.validate()?;let manifest=STANDARD.decode(&self.manifest_base64)?;let mut body=vec![0;64];body[..8].copy_from_slice(b"ASB3\x01\x00\x40\x00");
        body[8..40].copy_from_slice(&hex::<32>(&self.expected_manifest)?);body[40..48].copy_from_slice(&self.expected_version.to_le_bytes());body[48..52].copy_from_slice(&(manifest.len() as u32).to_le_bytes());body.extend_from_slice(&manifest);Ok(body)
    }
    /// Persist only typed progress, never arbitrary response fields/credentials.
    pub fn acknowledge(&mut self,path:&Path,value:&Value)->Result<()> {
        self.acknowledge_at(path,&self.endpoint.clone(),value)
    }
    /// Persist a newly discovered address only with an authenticated response
    /// matching the original MAC, transaction, Manifests and monotonic progress.
    pub fn acknowledge_at(&mut self,path:&Path,endpoint:&str,value:&Value)->Result<()> {
        let manifest=self.manifest()?;
        ensure!(value["protocol"]==3 && value["device_mac"].as_str()==Some(self.device_mac.as_str()) && value["operation_id"].as_str()==Some(self.operation_id.as_str()),"app_receipt_identity");
        ensure!(value["candidate_manifest"].as_str()==Some(digest_string(&manifest.digest).as_str()) && value["candidate_version"].as_u64()==Some(manifest.sequence) && value["old_manifest"].as_str()==Some(self.expected_manifest.as_str()) && value["old_version"].as_u64()==Some(self.expected_version),"app_receipt_manifest_mismatch");
        let p=Progress{phase:serde_json::from_value(value["phase"].clone())?,sequence:number(value,"sequence")?,boot_id:number(value,"boot_id")?,changed_mask:number(value,"changed_mask")?.try_into()?,ready_mask:number(value,"ready_mask")?.try_into()?,received:serde_json::from_value(value["received"].clone())?};
        Self::validate_progress(&manifest,&p)?;
        if let Some(old)=&self.progress {
            ensure!(p.changed_mask==old.changed_mask && p.sequence>=old.sequence && p.ready_mask&old.ready_mask==old.ready_mask && p.received.iter().zip(old.received).all(|(n,prior)|*n>=prior),"app_receipt_progress_regressed");
            ensure!(!old.phase.terminal() || p.phase==old.phase,"app_receipt_terminal_changed");
            ensure!(p.phase as u8>=old.phase as u8,"app_receipt_phase_regressed");
            if p.sequence==old.sequence {ensure!(p.phase==old.phase && p.ready_mask==old.ready_mask && p.received==old.received,"app_receipt_sequence_reused");}
        }
        let stored=Self::load(path,&self.endpoint,&self.device_mac)?;
        ensure!(stored.bytes()?==self.bytes()?,"app_receipt_file_changed");
        let mut next=self.clone();next.endpoint=endpoint.to_owned();next.progress=Some(p);crate::platform::store::atomic_write(path,&next.bytes()?)?;*self=next;Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;
    fn manifest_bytes()->Vec<u8> {
        let mut b=vec![0u8;64];b[..4].copy_from_slice(b"AMF3");b[4]=1;b[6]=64;b[8]=64;b[14]=160;b[16]=1;
        let hash=sha2::Sha256::digest(&b);b[32..64].copy_from_slice(&hash);b
    }
    fn status()->Value {serde_json::json!({"protocol":3,"device_mac":"AABBCCDDEEFF","phase":"idle","boot_id":11,"runtime_reporting":true,"app_max":8,
        "limits":{"package_bytes":1000,"image_bytes":1000,"heap_bytes":1000,"stack_bytes":1000,"tasks":4,"pages":4,"capabilities":127,"ram_bytes":4096,"resident_bytes":0}})}
    fn prepare()->Receipt {Receipt::prepare("127.0.0.1:18080","aabbccddeeff","01000000000000000000000000000000",&manifest_bytes(),&status()).unwrap()}
    fn reply(r:&Receipt,phase:&str,seq:u64)->Value {serde_json::json!({"protocol":3,"device_mac":r.device_mac,"operation_id":r.operation_id,
        "candidate_manifest":digest_string(&r.manifest().unwrap().digest),"candidate_version":1,"old_manifest":r.expected_manifest,"old_version":0,
        "phase":phase,"sequence":seq,"boot_id":12,"changed_mask":0,"ready_mask":0,"received":[0,0,0,0,0,0,0,0]})}
    #[test]
    fn durable_identity_and_monotonic_receipt() {
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("receipt.json");let mut r=prepare();r.create(&path).unwrap();assert!(r.create(&path).is_err());
        assert!(Receipt::load(&path,"127.0.0.1:18081",&r.device_mac).is_err());
        assert!(Receipt::load(&path,&r.endpoint,"000000000000").is_err());
        let body=r.begin_body().unwrap();assert_eq!(&body[..8],b"ASB3\x01\x00\x40\x00");assert_eq!(&body[64..],manifest_bytes());
        let mut value=reply(&r,"receiving",1);value["token"]="must-not-persist".into();r.acknowledge(&path,&value).unwrap();
        assert!(!String::from_utf8(std::fs::read(&path).unwrap()).unwrap().contains("must-not-persist"));
        let stable=std::fs::read(&path).unwrap();let mut bad=value.clone();bad["operation_id"]="02000000000000000000000000000000".into();assert!(r.acknowledge(&path,&bad).is_err());
        assert_eq!(std::fs::read(&path).unwrap(),stable);
        let value=reply(&r,"succeeded",2);r.acknowledge(&path,&value).unwrap();
        let value=reply(&r,"cancelled",3);assert!(r.acknowledge(&path,&value).is_err());
        let loaded=Receipt::load(&path,&r.endpoint,&r.device_mac).unwrap();assert_eq!(loaded.progress,r.progress);assert_eq!(loaded.operation_id,r.operation_id);
        let value=reply(&r,"receiving",1);assert!(r.acknowledge(&path,&value).is_err());
        std::fs::write(&path,b"{broken").unwrap();assert!(Receipt::load(&path,&r.endpoint,&r.device_mac).is_err());
    }
    #[test]
    fn persistence_failure_and_invalid_input_never_advance() {
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("receipt.json");let mut r=prepare();r.create(&path).unwrap();
        std::fs::create_dir(dir.path().join("receipt.json.tmp")).unwrap();
        let value=reply(&r,"receiving",1);assert!(r.acknowledge(&path,&value).is_err());assert!(r.progress.is_none());
        assert!(Receipt::load(&path,&r.endpoint,&r.device_mac).unwrap().progress.is_none());
        let mut s=status();s["cleanup_pending"]=true.into();assert!(Receipt::prepare(&r.endpoint,&r.device_mac,&r.operation_id,&manifest_bytes(),&s).is_err());
        let mut value=serde_json::to_value(&r).unwrap();value["menu_code"]="01234567".into();std::fs::write(&path,serde_json::to_vec(&value).unwrap()).unwrap();assert!(Receipt::load(&path,&r.endpoint,&r.device_mac).is_err());
        std::fs::write(&path,vec![b' ';MAX_RECEIPT+1]).unwrap();assert!(Receipt::load(&path,&r.endpoint,&r.device_mac).is_err());
    }
    #[test]
    #[ignore="requires AP01_MANIFEST real collection artifact"]
    fn real_progress_cannot_regress_or_reuse_sequence() {
        let bytes=std::fs::read(std::env::var("AP01_MANIFEST").unwrap()).unwrap();let m=Manifest::parse(&bytes).unwrap();
        let mut s=status();s["limits"]=serde_json::to_value(&m.resources).unwrap();
        s["limits"]["ram_bytes"]=(m.resources.image_bytes+m.resources.heap_bytes+m.resources.stack_bytes+4096).into();s["limits"]["resident_bytes"]=4096.into();
        let mut r=Receipt::prepare("127.0.0.1:18080","AABBCCDDEEFF","01000000000000000000000000000000",&bytes,&s).unwrap();
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("receipt.json");r.create(&path).unwrap();
        let mut v=reply(&r,"receiving",1);v["candidate_version"]=m.sequence.into();v["changed_mask"]=3.into();r.acknowledge(&path,&v).unwrap();
        v["sequence"]=2.into();v["received"][0]=1.into();r.acknowledge(&path,&v).unwrap();let stable=std::fs::read(&path).unwrap();
        v["received"][0]=2.into();assert!(r.acknowledge(&path,&v).is_err());
        v["sequence"]=3.into();v["received"][0]=0.into();assert!(r.acknowledge(&path,&v).is_err());
        v["received"][0]=1.into();v["received"][7]=1.into();assert!(r.acknowledge(&path,&v).is_err());
        assert_eq!(std::fs::read(&path).unwrap(),stable);
        let mut other=r.clone();other.operation_id="02000000000000000000000000000000".to_owned();std::fs::write(&path,other.bytes().unwrap()).unwrap();
        v["received"][7]=0.into();assert!(r.acknowledge(&path,&v).is_err());
    }

    #[test]
    fn archive_preserves_source_on_conflict_and_recovers_after_copy() {
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("active.json");let mut r=prepare();r.create(&path).unwrap();
        assert!(r.archive(&path).is_err());assert!(path.exists());
        r.acknowledge(&path,&reply(&r,"cancelled",2)).unwrap();
        let archive=dir.path().join(format!("active.json.{}.completed",r.operation_id));
        std::fs::write(&archive,b"conflicting archive").unwrap();
        assert!(r.archive(&path).is_err());assert!(path.exists());
        assert_eq!(std::fs::read(&archive).unwrap(),b"conflicting archive");std::fs::remove_file(&archive).unwrap();
        // Crash after durable copy but before deleting active: resume idempotently.
        r.create(&archive).unwrap();assert_eq!(r.archive(&path).unwrap(),archive);assert!(!path.exists());
        let saved=Receipt::load_local(&archive,&r.device_mac).unwrap();assert_eq!(saved.operation_id,r.operation_id);
        assert_eq!(saved.progress.unwrap().phase,Phase::Cancelled);
    }

    #[test]
    fn rediscovery_storage_failure_keeps_original_address() {
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("receipt.json");let mut r=prepare();r.create(&path).unwrap();
        let bytes=std::fs::read(&path).unwrap();let endpoint="127.0.0.1:18081";
        let mut wrong=status();wrong["device_mac"]="000000000000".into();
        assert!(r.relocate_unstarted(&path,endpoint,&wrong).is_err());assert_eq!(std::fs::read(&path).unwrap(),bytes);
        std::fs::create_dir(dir.path().join("receipt.json.tmp")).unwrap();
        assert!(r.relocate_unstarted(&path,endpoint,&status()).is_err());
        assert!(r.acknowledge_at(&path,endpoint,&reply(&r,"receiving",1)).is_err());
        assert_eq!(r.endpoint,"127.0.0.1:18080");assert!(r.progress.is_none());assert_eq!(std::fs::read(&path).unwrap(),bytes);
    }

}
