//! Host port of the ROM's screen-confirm core, with atomic local persistence.
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::{ffi::{c_char,c_int,c_void,CStr,CString},fs,io::Write,path::{Path,PathBuf}};

extern "C" {
    fn codex_ap01_binding_new(boot:u64,bytes:*const u8,len:usize,
        save:extern "C" fn(*mut c_void,*const u8,usize)->c_int,user:*mut c_void)->*mut c_void;
    fn codex_ap01_binding_free(state:*mut c_void);
    fn codex_ap01_pairing_cancel_ui(state:*mut c_void,now:u64);
    fn codex_ap01_pairing_http(state:*mut c_void,mac:*const c_char,method:*const c_char,path:*const c_char,
        token:*const c_char,body:*const c_char,now:u64,out:*mut c_char,cap:usize)->c_int;
    fn codex_ap01_pairing_ui(state:*mut c_void,now:u64,out:*mut c_char,cap:usize)->c_int;
    fn codex_ap01_pairing_displayed(state:*mut c_void,request:*const c_char,revision:u32,event:u64)->c_int;
    fn codex_ap01_pairing_button(state:*mut c_void,pressed:c_int,event:u64,now:u64)->*const c_char;
    fn codex_ap01_principal(state:*mut c_void,token:*const c_char)->*const c_char;
    fn codex_ap01_claim(state:*mut c_void,bridge:*const c_char,token:*const c_char,now:u64)->*const c_char;
    fn codex_ap01_binding_resolve(state:*mut c_void,bytes:*const u8,len:usize)->c_int;
    fn codex_ap01_owner(state:*mut c_void,now:u64,out:*mut c_char,cap:usize)->c_int;
}

struct Storage {path:PathBuf}
extern "C" fn save(user:*mut c_void,bytes:*const u8,len:usize)->c_int {
    // No panic or Rust reference may escape through the native callback.
    std::panic::catch_unwind(|| {
        if user.is_null()||bytes.is_null()||len>4096{return 0;}
        let storage=unsafe{&*(user as *const Storage)};
        let bytes=unsafe{std::slice::from_raw_parts(bytes,len)};
        let temporary=storage.path.with_extension("pending");
        let result=(||->std::io::Result<()> {
            let mut file=fs::File::create(&temporary)?;file.write_all(bytes)?;file.sync_all()
        })();
        if result.is_err(){return 0;}
        if fs::rename(&temporary,&storage.path).is_err(){return -1;}
        match fs::read(&storage.path) {Ok(readback) if readback==bytes=>1,_=>-1}
    }).unwrap_or(-1)
}

/// One instance per simulated device; native calls must be serialized by owner.
pub struct Pairing {
    state:*mut c_void,
    storage:Box<Storage>,
    mac:CString,
}
unsafe impl Send for Pairing {}
impl Pairing {
    pub fn open(path:&Path,mac:&str,boot:u64)->Result<Self> {
        let mac=CString::new(mac)?;
        let bytes=match fs::metadata(path) {
            Ok(meta)=>{ensure!(meta.len()<=4096,"binding_store_too_large");fs::read(path)?},
            Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Vec::new(),
            Err(e)=>return Err(e.into()),
        };
        let mut storage=Box::new(Storage{path:path.to_owned()});
        let state=unsafe{codex_ap01_binding_new(boot,bytes.as_ptr(),bytes.len(),save,
            (&mut *storage as *mut Storage).cast())};
        ensure!(!state.is_null(),"binding_store_invalid_or_unavailable");
        Ok(Self{state,storage,mac})
    }
    pub fn http(&mut self,method:&str,path:&str,token:&str,body:&Value,now:u64)->Result<(u16,Value)> {
        // A lost post-rename readback can be reconciled only by the original
        // persisted request, without replacing its request ID or credential.
        if let Ok(meta)=fs::metadata(&self.storage.path) {
            if meta.len()<=4096 {
                if let Ok(bytes)=fs::read(&self.storage.path) {
                    unsafe{codex_ap01_binding_resolve(self.state,bytes.as_ptr(),bytes.len())};
                }
            }
        }
        let method=CString::new(method)?;let path=CString::new(path)?;
        let token=CString::new(token)?;let body=CString::new(body.to_string())?;
        let mut output=vec![0i8;8192];
        let status=unsafe{codex_ap01_pairing_http(self.state,self.mac.as_ptr(),method.as_ptr(),path.as_ptr(),
            token.as_ptr(),body.as_ptr(),now,output.as_mut_ptr(),output.len())};
        ensure!((100..600).contains(&status),"pairing_response_unavailable");
        Ok((status as u16,json(&output)?))
    }
    pub fn ui(&mut self,now:u64)->Result<Value> {
        let mut out=[0i8;1024];
        ensure!(unsafe{codex_ap01_pairing_ui(self.state,now,out.as_mut_ptr(),out.len())}==200,"pairing_ui");
        json(&out)
    }
    pub fn displayed(&mut self,request:&str,revision:u32,event:u64)->Result<bool> {
        Ok(unsafe{codex_ap01_pairing_displayed(self.state,CString::new(request)?.as_ptr(),revision,event)}==1)
    }
    pub fn button(&mut self,pressed:bool,event:u64,now:u64)->String {
        owned(unsafe{codex_ap01_pairing_button(self.state,pressed as c_int,event,now)})
            .unwrap_or_else(||"pairing_unavailable".into())
    }
    pub fn principal(&self,token:&str)->Option<String> {
        let token=CString::new(token).ok()?;
        owned(unsafe{codex_ap01_principal(self.state,token.as_ptr())})
    }
    pub fn claim(&mut self,bridge:&str,token:&str,now:u64)->Result<String> {
        Ok(owned(unsafe{codex_ap01_claim(self.state,CString::new(bridge)?.as_ptr(),CString::new(token)?.as_ptr(),now)})
            .context("claim_response")?)
    }
    pub fn cancel_from_ui(&mut self) { unsafe{codex_ap01_pairing_cancel_ui(self.state,0)} }
    pub fn owner(&self,now:u64)->Value {
        let mut out=[0i8;65];let remaining=unsafe{codex_ap01_owner(self.state,now,out.as_mut_ptr(),out.len())};
        if remaining<=0 {Value::Null} else {serde_json::json!({"id":owned(out.as_ptr()),"lease_s":300,"expires_in_s":remaining})}
    }
}
impl Drop for Pairing {fn drop(&mut self){unsafe{codex_ap01_binding_free(self.state)}}}
fn owned(value:*const c_char)->Option<String> {
    if value.is_null(){None}else{Some(unsafe{CStr::from_ptr(value)}.to_string_lossy().into_owned())}
}
fn json(bytes:&[i8])->Result<Value> {
    Ok(serde_json::from_slice(unsafe{CStr::from_ptr(bytes.as_ptr())}.to_bytes())?)
}
