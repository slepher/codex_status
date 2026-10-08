use std::ffi::{CString,c_void,c_char};
use anyhow::{ensure,Result};
unsafe extern "C" {
    fn sim_status_create()->*mut c_void;
    fn sim_status_delete(p:*mut c_void);
    fn sim_status_page(p:*mut c_void,body:*const c_char,token:*const c_char,id:*const c_char,
        offset:usize,now:u64,out:*mut c_char,capacity:usize)->i32;
    #[cfg(test)] fn sim_network_display_test()->i32;
}
pub struct SimulatorStatusPages(*mut c_void);
unsafe impl Send for SimulatorStatusPages {}
impl SimulatorStatusPages {
    pub fn new()->Result<Self> {let p=unsafe{sim_status_create()};ensure!(!p.is_null(),"status allocation");Ok(Self(p))}
    pub fn page(&mut self,body:Option<&str>,token:&str,id:&str,offset:usize,now:u64)->Result<serde_json::Value> {
        let body=body.map(CString::new).transpose()?;let token=CString::new(token)?;let id=CString::new(id)?;
        let mut out=vec![0u8;4096];
        let size=unsafe{sim_status_page(self.0,body.as_ref().map_or(std::ptr::null(),|s|s.as_ptr()),token.as_ptr(),id.as_ptr(),offset,now,out.as_mut_ptr().cast(),out.len())};
        ensure!(size>=0,"status page exceeds frame");Ok(serde_json::from_slice(&out[..size as usize])?)
    }
}
impl Drop for SimulatorStatusPages {fn drop(&mut self){unsafe{sim_status_delete(self.0)}}}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn shared_status_snapshot_is_complete_bounded_and_principal_scoped() {
        let mut pager=SimulatorStatusPages::new().unwrap();let id="123456789abcdef0123456789abcdef0";
        let original=serde_json::json!({"device_mac":"AABBCCDDEEFF","contract":"长字段".repeat(900)}).to_string();
        let mut page=pager.page(Some(&original),"synthetic",id,0,10).unwrap();let mut bytes=Vec::new();
        assert_eq!(pager.page(None,"wrong",id,1800,11).unwrap()["error"],"status_read_expired");
        assert_eq!(pager.page(None,"synthetic","wrong",1800,11).unwrap()["error"],"status_read_expired");
        assert_eq!(pager.page(None,"synthetic",id,1,11).unwrap()["error"],"status_offset");
        loop {
            assert!(page.to_string().len()+48<=4096);let hex=page["chunk_hex"].as_str().unwrap();
            for i in (0..hex.len()).step_by(2){bytes.push(u8::from_str_radix(&hex[i..i+2],16).unwrap());}
            let next=page["next_offset"].as_u64().unwrap();if next==0{break;}
            page=pager.page(None,"synthetic",id,next as usize,12).unwrap();
        }
        assert_eq!(String::from_utf8(bytes).unwrap(),original);
        assert_eq!(pager.page(None,"synthetic",id,0,15010).unwrap()["error"],"status_read_expired");
        assert_eq!(pager.page(Some(&"x".repeat(16385)),"synthetic",id,0,16000).unwrap()["error"],"status_limit");
    }
    #[test] fn shared_network_glyph_updates_without_business_data_and_bounds_retry() {assert_eq!(unsafe{sim_network_display_test()},0);}
}
