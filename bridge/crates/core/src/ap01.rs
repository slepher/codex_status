//! AP01 LAN discovery and the existing screen-confirm binding bootstrap.
//! Discovery produces untrusted candidates. Registration requires the original
//! pairing transaction and an authenticated status read, never a UDP message.
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    sync::atomic::{AtomicBool, Ordering}, time::{Duration, Instant}};

pub const DISCOVERY_PORT: u16 = 8767;
pub const HTTP_PORT: u16 = 18081; // Codex Status/display business service.
pub const MAINTENANCE_HTTP_PORT:u16=18080;
pub const BINDING_METHOD: &str = "screen_confirm_v1";
const MAX_CANDIDATES: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanInterface { pub address: Ipv4Addr, pub prefix_len: u8 }
impl LanInterface {
    pub fn broadcast(&self) -> Result<Ipv4Addr> {
        ensure!(self.prefix_len > 0 && self.prefix_len <= 30, "discovery_interface_prefix");
        let mask = u32::MAX << (32-self.prefix_len);
        Ok(Ipv4Addr::from(u32::from(self.address) | !mask))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub device_mac: String,
    pub name: String,
    pub endpoint: String,
    pub maintenance_endpoint:String,
    pub boot_id: u64,
    pub binding_method: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiscoveryReport {
    pub candidates: Vec<Candidate>,
    pub errors: Vec<String>,
    pub partial: bool,
    pub cancelled: bool,
}

fn valid_nonce(nonce: &str) -> bool {
    nonce.len() == 32 && nonce.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn discovery_request(nonce: &str, target_mac: Option<&str>) -> Result<Vec<u8>> {
    ensure!(valid_nonce(nonce), "discovery_nonce");
    let mut value=json!({"magic":"codex-status","discovery_v":1,"kind":"discover","nonce":nonce});
    if let Some(mac)=target_mac {
        value["target_mac"]=json!(crate::platform::model::DeviceIdentity::normalized_mac(mac)
            .context("discovery_target_mac")?);
    }
    Ok(serde_json::to_vec(&value)?)
}

pub fn decode_reply(bytes: &[u8], source: SocketAddr, nonce: &str) -> Result<Candidate> {
    ensure!(bytes.len() <= 768, "discovery_reply_size");
    let value: Value=serde_json::from_slice(bytes).context("discovery_reply_json")?;
    ensure!(value["magic"]=="codex-status" && value["discovery_v"]==1 &&
        value["kind"]=="discovery_reply" && value["nonce"]==nonce, "discovery_reply_identity");
    ensure!(value["model"]=="njcuk.enstor.ap01" && value["binding_method"]==BINDING_METHOD,
        "discovery_reply_capability");
    let mac=crate::platform::model::DeviceIdentity::normalized_mac(
        value["mac"].as_str().context("discovery_reply_mac")?).context("discovery_reply_mac")?;
    let port=value["http_port"].as_u64().filter(|p| *p>0 && *p<=65535).context("discovery_reply_port")?;
    let maintenance=value["maintenance_port"].as_u64().filter(|p|*p>0&&*p<=65535&&*p!=port)
        .context("discovery_maintenance_port")?;
    let SocketAddr::V4(source)=source else { bail!("discovery_ipv4_required") };
    ensure!(!source.ip().is_unspecified() && !source.ip().is_multicast() && !source.ip().is_broadcast(),
        "discovery_reply_source");
    let name=value["name"].as_str().unwrap_or("AP01");
    ensure!(name.len()<=96 && !name.chars().any(char::is_control), "discovery_reply_name");
    Ok(Candidate { device_mac:mac, name:name.to_owned(),
        endpoint:SocketAddrV4::new(*source.ip(),port as u16).to_string(),
        maintenance_endpoint:SocketAddrV4::new(*source.ip(),maintenance as u16).to_string(),
        boot_id:value["boot_id"].as_u64().context("discovery_reply_boot")?,binding_method:BINDING_METHOD.into() })
}

pub fn decode_announce(bytes:&[u8],source:SocketAddr)->Result<Candidate> {
    ensure!(bytes.len()<=768,"discovery_reply_size");
    let mut value:Value=serde_json::from_slice(bytes)?;
    ensure!(value["kind"]=="announce" && value["discovery_v"]==1,"discovery_announce_kind");
    // Reuse capability/identity checks; an announce has no transaction nonce.
    let nonce="00000000000000000000000000000000";
    value["kind"]=json!("discovery_reply");value["nonce"]=json!(nonce);
    decode_reply(&serde_json::to_vec(&value)?,source,nonce)
}

/// One bounded search across all caller-selected LAN interfaces. Its outbound
/// socket is also the reply socket, including for named Bridge instances.
pub fn discover(interfaces: &[LanInterface], nonce: &str, target_mac: Option<&str>,
                budget: Duration, cancelled: &AtomicBool) -> Result<DiscoveryReport> {
    let mut routes=Vec::new();
    for interface in interfaces {
        if interface.address.is_loopback() || interface.address.is_unspecified() { continue; }
        routes.push((interface.address,SocketAddr::from((interface.broadcast()?,DISCOVERY_PORT))));
    }
    let mut report=probe(&routes,nonce,target_mac,budget,cancelled)?;
    report.candidates.retain(|candidate| {
        let address=candidate.endpoint.parse::<SocketAddrV4>().ok();
        address.is_some_and(|address|interfaces.iter().any(|iface| {
            let mask=u32::MAX << (32-iface.prefix_len);
            u32::from(*address.ip()) & mask == u32::from(iface.address) & mask
        }))
    });
    Ok(report)
}

/// Explicit routes also permit isolated loopback tests without broadcasting on
/// the user's LAN. The app derives routes only from operating-system interfaces.
pub fn probe(routes: &[(Ipv4Addr,SocketAddr)], nonce: &str, target_mac: Option<&str>,
             budget: Duration, cancelled: &AtomicBool) -> Result<DiscoveryReport> {
    let request=discovery_request(nonce,target_mac)?;
    let expected=target_mac.and_then(crate::platform::model::DeviceIdentity::normalized_mac);
    let deadline=Instant::now()+budget.min(Duration::from_secs(8));
    let mut report=DiscoveryReport::default();
    report.partial=routes.len()>16;
    let mut sockets=Vec::new();
    for (address,destination) in routes.iter().take(16) {
        match UdpSocket::bind((*address,0)).and_then(|socket| {
            socket.set_broadcast(true)?;socket.set_nonblocking(true)?;Ok(socket)
        }) {
            Ok(socket)=>sockets.push((socket,*destination,0usize,Instant::now())),
            Err(_)=>report.errors.push(format!("discovery_socket:{address}")),
        }
    }
    if sockets.is_empty() { report.errors.push("no_lan_interface".into()); return Ok(report); }
    let mut candidates=BTreeMap::new();
    let mut buffer=[0u8;769];
    while Instant::now()<deadline && !cancelled.load(Ordering::Relaxed) {
        for (socket,destination,sent,next) in &mut sockets {
            if *sent<3 && Instant::now()>=*next {
                if socket.send_to(&request,*destination).is_err() {
                    report.errors.push(format!("discovery_send:{}",destination.ip()));
                }
                *sent+=1;
                *next=Instant::now()+Duration::from_millis(if *sent==1 {400}else{800});
            }
            // Bound work per socket: a flood cannot extend the global deadline.
            for _ in 0..32 {
                let (size,from)=match socket.recv_from(&mut buffer) {
                    Ok(value)=>value,Err(_)=>break,
                };
                if let Ok(candidate)=decode_reply(&buffer[..size],from,nonce) {
                    if expected.as_ref().is_some_and(|mac| mac!=&candidate.device_mac) {continue;}
                    let key=(candidate.device_mac.clone(),candidate.endpoint.clone());
                    if candidates.len()<MAX_CANDIDATES || candidates.contains_key(&key) {
                        candidates.insert(key,candidate);
                    } else {report.partial=true;}
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    report.cancelled=cancelled.load(Ordering::Relaxed);
    report.candidates=candidates.into_values().collect();
    Ok(report)
}

pub fn validate_info(candidate: &Candidate, info: &Value) -> Result<()> {
    ensure!(crate::platform::model::DeviceIdentity::normalized_mac(
        info["mac"].as_str().unwrap_or(""))==Some(candidate.device_mac.clone()),"pairing_info_mac");
    ensure!(info["model"]=="njcuk.enstor.ap01" && info["binding_method"]==BINDING_METHOD,
        "pairing_info_capability");
    ensure!(info["boot_id"].as_u64()==Some(candidate.boot_id),"pairing_info_boot_changed");
    ensure!(info["pairing_revision"].as_u64().is_some(),"pairing_info_revision");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn respects_prefixes_and_uses_packet_source() {
        let interface=LanInterface{address:Ipv4Addr::new(192,168,5,10),prefix_len:23};
        assert_eq!(interface.broadcast().unwrap(),Ipv4Addr::new(192,168,5,255));
        let nonce="0123456789abcdef0123456789abcdef";
        let reply=json!({"magic":"codex-status","discovery_v":1,"kind":"discovery_reply","nonce":nonce,
            "mac":"00:11:22:33:44:55","model":"njcuk.enstor.ap01","binding_method":BINDING_METHOD,
            "boot_id":4,"http_port":18081,"maintenance_port":18080,"ip":"203.0.113.8"});
        let bytes=serde_json::to_vec(&reply).unwrap();
        let candidate=decode_reply(&bytes,"192.168.5.20:8767".parse().unwrap(),nonce).unwrap();
        assert_eq!(candidate.endpoint,"192.168.5.20:18081");
        assert_eq!(candidate.maintenance_endpoint,"192.168.5.20:18080");
        let mut bad=reply.clone();bad["maintenance_port"]=bad["http_port"].clone();
        assert!(decode_reply(&serde_json::to_vec(&bad).unwrap(),"192.168.5.20:8767".parse().unwrap(),nonce).is_err());
        assert!(decode_reply(&bytes,"192.168.5.20:8767".parse().unwrap(),"00000000000000000000000000000000").is_err());
        assert!(validate_info(&candidate,&json!({"mac":"001122334455","model":"njcuk.enstor.ap01",
            "binding_method":BINDING_METHOD,"boot_id":5,"pairing_revision":0})).is_err());
    }
    #[test]
    fn device_already_running_replies_to_a_later_bridge() {
        let server=UdpSocket::bind("127.0.0.1:0").unwrap();
        server.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let target=server.local_addr().unwrap();
        let responder=std::thread::spawn(move || {
            let mut buffer=[0u8;256];let (n,source)=server.recv_from(&mut buffer).unwrap();
            let request:Value=serde_json::from_slice(&buffer[..n]).unwrap();
            let reply=json!({"magic":"codex-status","discovery_v":1,"kind":"discovery_reply",
                "nonce":request["nonce"],"mac":"001122334455","model":"njcuk.enstor.ap01",
                "http_port":18081,"maintenance_port":18080,"boot_id":1,"binding_method":BINDING_METHOD});
            server.send_to(&serde_json::to_vec(&reply).unwrap(),source).unwrap();
        });
        let report=probe(&[(Ipv4Addr::LOCALHOST,target)],"0123456789abcdef0123456789abcdef",None,
            Duration::from_millis(100),&AtomicBool::new(false)).unwrap();
        responder.join().unwrap();assert_eq!(report.candidates.len(),1);
    }
}
