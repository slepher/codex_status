//! Offline collection artifact validation. Target admission and execution remain
//! firmware responsibilities; selected package bytes are never modified here.
use anyhow::{ensure, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn u16le(b:&[u8],at:usize)->u16 {u16::from_le_bytes(b[at..at+2].try_into().unwrap())}
fn u32le(b:&[u8],at:usize)->u32 {u32::from_le_bytes(b[at..at+4].try_into().unwrap())}
fn u64le(b:&[u8],at:usize)->u64 {u64::from_le_bytes(b[at..at+8].try_into().unwrap())}
fn text(b:&[u8])->Result<String> {
    let n=b.iter().position(|v|*v==0).unwrap_or(b.len());
    ensure!(n>0 && n<32 && b[n..].iter().all(|v|*v==0) && b[..n].iter().all(|v|v.is_ascii_alphanumeric()||b"._+-".contains(v)),"app_set_text");
    Ok(String::from_utf8(b[..n].to_vec())?)
}
fn digest(b:&[u8],at:usize)->[u8;32] {let mut hash=Sha256::new();hash.update(&b[..at]);hash.update([0u8;32]);hash.update(&b[at+32..]);hash.finalize().into()}

#[derive(Debug,Clone,Serialize)]
pub struct App {
    pub id:String,pub version:String,pub digest:[u8;32],pub package_bytes:u32,pub order:u16,
    #[serde(skip)] raw:[u8;160],
}
#[derive(Debug,Default,Clone,Serialize)]
pub struct Resources {pub package_bytes:u64,pub image_bytes:u64,pub heap_bytes:u64,pub stack_bytes:u64,pub tasks:u64,pub pages:u64,pub capabilities:u64}
#[derive(Debug,Clone,Serialize)]
pub struct Manifest {pub sequence:u64,pub digest:[u8;32],pub apps:Vec<App>,pub resources:Resources}

impl Manifest {
    pub fn parse(b:&[u8])->Result<Self> {
        ensure!((64..=3584).contains(&b.len()) && &b[..4]==b"AMF3","app_set_header");
        let count=u16le(b,12) as usize;let deps=u16le(b,24) as usize;
        ensure!(u16le(b,4)==1 && u16le(b,6)==64 && u32le(b,8) as usize==b.len() && count<=8 && deps<=56 && u16le(b,14)==160 && u64le(b,16)>0 && b[26..32]==[0;6] && b.len()==64+160*count+40*deps,"app_set_bounds");
        let expected=digest(b,32);ensure!(b[32..64]==expected,"app_set_digest");
        let mut out=Self{sequence:u64le(b,16),digest:expected,apps:Vec::with_capacity(count),resources:Resources::default()};
        let mut orders=0u32;
        for i in 0..count {
            let raw:&[u8;160]=b[64+160*i..64+160*(i+1)].try_into().unwrap();
            let id=text(&raw[..32])?;let version=text(&raw[32..64])?;let order=u16le(raw,136);
            ensure!(id!="." && id!=".." && out.apps.last().is_none_or(|a|a.id<id) && raw[64..96]!=[0;32],"app_set_identity");
            ensure!((order as usize)<count && orders&(1<<order)==0,"app_set_order");orders|=1<<order;
            let package=u32le(raw,96);let image=u32le(raw,100);let heap=u32le(raw,104);let stack=u32le(raw,108);
            let tasks=u16le(raw,138);let pages=u16le(raw,140);let tcp=u16le(raw,132);let udp=u16le(raw,134);
            ensure!(u16le(raw,142)==0 && u32le(raw,144)==3 && raw[148..]==[0;12],"app_set_reserved");
            ensure!(package>=256 && image>0 && stack>=256 && stack%16==0 && tasks<=4 && pages<=32 && u32::from(tasks)*256<=stack && u32le(raw,112)<=u32le(raw,120) && u32le(raw,120)<=u32le(raw,116) && tcp!=18080 && udp!=18080,"app_set_resources");
            for prior in &out.apps {ensure!((tcp==0||tcp!=u16le(&prior.raw,132)) && (udp==0||udp!=u16le(&prior.raw,134)),"app_set_port_conflict");}
            let r=&mut out.resources;r.package_bytes+=u64::from(package);r.image_bytes+=(u64::from(image)+15)&!15;r.heap_bytes+=u64::from(heap);r.stack_bytes+=u64::from(stack);r.tasks+=u64::from(tasks);r.pages+=u64::from(pages);r.capabilities|=u64le(raw,124);
            out.apps.push(App{id,version,digest:raw[64..96].try_into().unwrap(),package_bytes:package,order,raw:*raw});
        }
        let mut previous=None;
        for i in 0..deps {
            let raw=&b[64+160*count+40*i..64+160*count+40*(i+1)];let source=u16le(raw,0) as usize;let target=u16le(raw,2) as usize;
            ensure!(source<count && target<count && source!=target && u32le(raw,4)==0 && previous.is_none_or(|p|(source,target)>p),"app_set_dependency");
            ensure!(out.apps[target].order<out.apps[source].order && raw[8..]==out.apps[target].digest,"app_set_dependency_identity");previous=Some((source,target));
        }
        Ok(out)
    }

    pub fn check_capabilities(&self,value:&serde_json::Value)->Result<()> {
        ensure!(value["protocol"].as_u64()==Some(3),"app_set_protocol");
        let number=|object:&serde_json::Value,key:&str|object[key].as_u64().ok_or_else(||anyhow::anyhow!("app_set_capability_missing_or_invalid: {key}"));
        ensure!(self.apps.len() as u64<=number(value,"app_max")?,"app_set_app_budget");
        let limits=&value["limits"];let r=&self.resources;
        for (key,total) in [("package_bytes",r.package_bytes),("image_bytes",r.image_bytes),("heap_bytes",r.heap_bytes),("stack_bytes",r.stack_bytes),("tasks",r.tasks),("pages",r.pages)] {
            ensure!(total<=number(limits,key)?,"app_set_budget: {key}");
        }
        ensure!(r.capabilities&!number(limits,"capabilities")?==0,"app_set_capabilities");
        let ram=number(limits,"ram_bytes")?;let resident=number(limits,"resident_bytes")?;
        ensure!(resident<=ram && r.image_bytes+r.heap_bytes+r.stack_bytes<=ram-resident,"app_set_ram_budget");
        Ok(())
    }

    pub fn package_index(&self,b:&[u8])->Result<usize> {
        ensure!(b.len()>=256 && b.len()<=u32::MAX as usize && &b[..4]==b"APX2","app_package_header");
        ensure!(u32le(b,4)==2 && u32le(b,8)==256 && u32le(b,12) as usize==b.len() && u32le(b,16)==1 && u32le(b,20)==1 && u32le(b,24)==3 && u32le(b,28)==0 && b[224..256]==[0;32],"app_package_target");
        let id=text(&b[128..160])?;let index=self.apps.iter().position(|a|a.id==id).ok_or_else(||anyhow::anyhow!("app_package_not_selected"))?;
        let app=&self.apps[index];let raw=&app.raw;
        ensure!(b[128..224]==raw[..96] && b.len()==app.package_bytes as usize && digest(b,192)==app.digest,"app_package_identity");
        let image=u64::from(u32le(b,44));let text=u64::from(u32le(b,52));let ro=u64::from(u32le(b,56));let ro_bytes=u64::from(u32le(b,60));let data=u64::from(u32le(b,64));let data_bytes=u64::from(u32le(b,68));let bss=u64::from(u32le(b,72));let fixes=u64::from(u32le(b,112));
        ensure!(u32le(b,40)==256 && u32le(b,48)==0 && u32le(b,76)==16 && text>0 && text<=ro && ro%16==0 && ro+ro_bytes<=data && data%16==0 && data+data_bytes==image && image%4==0 && image+bss<=u32::MAX as u64 && 256+image==u64::from(u32le(b,108)) && 256+image+8*fixes==b.len() as u64 && fixes<=image/4,"app_package_layout");
        ensure!(image+bss+u64::from(u32le(b,80))+u64::from(u32le(b,84))<=u32::MAX as u64 && image+bss==u64::from(u32le(raw,100)) && b[80..88]==raw[104..112] && b[116..128]==raw[112..124] && b[32..40]==raw[124..132],"app_package_resources");
        ensure!(&b[100..108]==[0;8],"app_package_abi_entries");
        for at in [88,92,96] {let entry=u64::from(u32le(b,at));ensure!(entry%2==0 && entry+2<=text,"app_package_entry");}
        ensure!(b[256+text as usize..256+ro as usize].iter().all(|v|*v==0) && b[256+(ro+ro_bytes) as usize..256+data as usize].iter().all(|v|*v==0),"app_package_padding");
        let mut previous=None;
        for i in 0..fixes as usize {
            let at=256+image as usize+8*i;let dest=u64::from(u32le(b,at));let target=u64::from(u32le(b,at+4));
            ensure!(dest%4==0 && ((dest>=ro && dest+4<=ro+ro_bytes)||(dest>=data && dest+4<=image)) && previous.is_none_or(|p|dest>p),"app_package_fixup_destination");
            ensure!(target<text || (target>=ro && target<ro+ro_bytes) || (target>=data && target<image+bss),"app_package_fixup_target");
            ensure!(u64::from(u32le(b,256+dest as usize))==target,"app_package_fixup_word");previous=Some(dest);
        }
        Ok(index)
    }

    pub fn changed_packages<'a>(&self,provided:&'a [Vec<u8>],mask:u32)->Result<Vec<(usize,&'a [u8])>> {
        ensure!(mask>>self.apps.len()==0,"app_set_changed_mask");let mut packages=BTreeMap::new();
        for b in provided {let index=self.package_index(b)?;ensure!(packages.insert(index,b.as_slice()).is_none(),"app_package_duplicate");}
        (0..self.apps.len()).filter(|i|mask&(1<<i)!=0).map(|i|Ok((i,*packages.get(&i).ok_or_else(||anyhow::anyhow!("app_package_missing"))?))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_collection_and_truncation() {
        let mut b=vec![0u8;64];b[..4].copy_from_slice(b"AMF3");b[4]=1;b[6]=64;b[8]=64;b[14]=160;b[16]=1;
        let hash=digest(&b,32);b[32..64].copy_from_slice(&hash);let manifest=Manifest::parse(&b).unwrap();
        assert!(manifest.apps.is_empty());assert!(manifest.changed_packages(&[],0).unwrap().is_empty());assert!(manifest.changed_packages(&[],1).is_err());
        for i in 0..64 {assert!(Manifest::parse(&b[..i]).is_err());}
        b[26]=1;let hash=digest(&b,32);b[32..64].copy_from_slice(&hash);assert!(Manifest::parse(&b).is_err());
    }
    #[test]
    #[ignore="requires AP01_MANIFEST, AP01_PACKAGE and AP01_PEER real build artifacts"]
    fn real_collection_artifacts() {
        let read=|name|std::fs::read(std::env::var(name).unwrap()).unwrap();
        let bytes=read("AP01_MANIFEST");let m=Manifest::parse(&bytes).unwrap();let packages=vec![read("AP01_PACKAGE"),read("AP01_PEER")];
        assert_eq!(m.apps.len(),2);assert_eq!(m.changed_packages(&packages,3).unwrap().len(),2);
        assert_eq!(m.changed_packages(&packages[..1],1).unwrap().len(),1);assert!(m.changed_packages(&packages[..1],3).is_err());
        assert!(m.changed_packages(&[packages[0].clone(),packages[0].clone()],1).is_err());
        let mut limits=serde_json::to_value(&m.resources).unwrap();
        let ram=m.resources.image_bytes+m.resources.heap_bytes+m.resources.stack_bytes+4096;
        limits["ram_bytes"]=ram.into();limits["resident_bytes"]=4096.into();
        let caps=serde_json::json!({"protocol":3,"app_max":2,"limits":limits});m.check_capabilities(&caps).unwrap();
        for key in ["package_bytes","image_bytes","heap_bytes","stack_bytes","tasks","pages","capabilities","ram_bytes","resident_bytes"] {
            let mut bad=caps.clone();bad["limits"].as_object_mut().unwrap().remove(key);assert!(m.check_capabilities(&bad).is_err());
            if key!="resident_bytes" {let mut bad=caps.clone();bad["limits"][key]=0.into();assert!(m.check_capabilities(&bad).is_err());}
        }
        let mut bad=caps.clone();bad["limits"]["ram_bytes"]=(ram-1).into();assert!(m.check_capabilities(&bad).is_err());
        let mut bad=caps.clone();bad["limits"]["resident_bytes"]=(ram+1).into();assert!(m.check_capabilities(&bad).is_err());
        for n in 0..bytes.len() {assert!(Manifest::parse(&bytes[..n]).is_err());}
        for p in &packages {for n in 0..256 {assert!(m.package_index(&p[..n]).is_err());}let mut bad=p.clone();*bad.last_mut().unwrap()^=1;assert!(m.package_index(&bad).is_err());}
        let reject_rehashed=|mut bad:Vec<u8>,reason:&str| {
            let hash=digest(&bad,192);bad[192..224].copy_from_slice(&hash);
            let mut altered=m.clone();altered.apps[0].digest=hash;altered.apps[0].raw[64..96].copy_from_slice(&hash);
            assert!(altered.package_index(&bad).unwrap_err().to_string().contains(reason));
        };
        let mut bad=packages[0].clone();bad[88]|=1;reject_rehashed(bad,"app_package_entry");
        let mut bad=packages[0].clone();bad[224]=1;reject_rehashed(bad,"app_package_target");
        let mut bad=packages[0].clone();assert!(u32le(&bad,112)>0);
        let record=u32le(&bad,108) as usize;let destination=u32le(&bad,record) as usize;
        bad[256+destination]^=1;reject_rehashed(bad,"app_package_fixup_word");
        for at in [206,212] {let mut bad=bytes.clone();bad[at]^=1;let hash=digest(&bad,32);bad[32..64].copy_from_slice(&hash);assert!(Manifest::parse(&bad).is_err());}
    }
}
