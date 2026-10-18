//! OS interface addresses and existing neighbors; never guess a /24 or sweep it.
use bridge_core::ap01::LanInterface;
use std::net::Ipv4Addr;

#[cfg(windows)]
pub fn interfaces() -> Result<Vec<LanInterface>, String> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{GetAdaptersInfo, IP_ADAPTER_INFO};
    let mut size=0u32;
    unsafe { GetAdaptersInfo(std::ptr::null_mut(), &mut size); }
    if size==0 || size>1024*1024 { return Err("lan_interface_unavailable".into()); }
    // Native structures contain pointers, so their storage must be aligned.
    let mut memory=vec![0usize;(size as usize).div_ceil(std::mem::size_of::<usize>())];
    let pointer=memory.as_mut_ptr().cast::<IP_ADAPTER_INFO>();
    if unsafe {GetAdaptersInfo(pointer,&mut size)}!=0 { return Err("lan_interface_unavailable".into()); }
    let mut result=Vec::new();let mut adapter=pointer;
    unsafe {
        while !adapter.is_null() {
            let mut row: *const windows_sys::Win32::NetworkManagement::IpHelper::IP_ADDR_STRING=&(*adapter).IpAddressList;
            while !row.is_null() {
                let ip=std::ffi::CStr::from_ptr((*row).IpAddress.String.as_ptr()).to_str().ok();
                let mask=std::ffi::CStr::from_ptr((*row).IpMask.String.as_ptr()).to_str().ok();
                if let (Some(ip),Some(mask))=(ip,mask) { add(&mut result,ip.parse().ok(),mask.parse().ok()); }
                row=(*row).Next;
            }
            adapter=(*adapter).Next;
        }
    }
    Ok(result)
}

#[cfg(unix)]
pub fn interfaces() -> Result<Vec<LanInterface>, String> {
    let mut first=std::ptr::null_mut();
    if unsafe {libc::getifaddrs(&mut first)}!=0 {return Err("lan_interface_unavailable".into());}
    let mut result=Vec::new();let mut row=first;
    unsafe {
        while !row.is_null() {
            let iface=&*row;
            if iface.ifa_flags & libc::IFF_UP as u32 !=0 && !iface.ifa_addr.is_null() &&
                !iface.ifa_netmask.is_null() && (*iface.ifa_addr).sa_family as i32==libc::AF_INET {
                let ip=&*iface.ifa_addr.cast::<libc::sockaddr_in>();
                let mask=&*iface.ifa_netmask.cast::<libc::sockaddr_in>();
                add(&mut result,Some(Ipv4Addr::from(ip.sin_addr.s_addr.to_ne_bytes())),
                    Some(Ipv4Addr::from(mask.sin_addr.s_addr.to_ne_bytes())));
            }
            row=iface.ifa_next;
        }
        libc::freeifaddrs(first);
    }
    Ok(result)
}

fn add(result:&mut Vec<LanInterface>,ip:Option<Ipv4Addr>,mask:Option<Ipv4Addr>) {
    if let (Some(address),Some(mask))=(ip,mask) {
        let mask=u32::from(mask);let prefix_len=mask.leading_ones() as u8;
        if prefix_len>0 && prefix_len<=30 && mask==u32::MAX << (32-prefix_len) &&
            !address.is_loopback() && !address.is_unspecified() && !address.is_link_local() {
            let value=LanInterface{address,prefix_len};
            if !result.contains(&value){result.push(value);}
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn preserves_real_masks_and_rejects_noncontiguous_masks() {
        let mut result=Vec::new();
        super::add(&mut result,"192.168.3.2".parse().ok(),"255.255.254.0".parse().ok());
        super::add(&mut result,"192.168.3.2".parse().ok(),"255.0.255.0".parse().ok());
        assert_eq!(result.len(),1);assert_eq!(result[0].prefix_len,23);
    }
}
