//! Opt-in integration against an isolated AP01 socket + real NuttX UI port.
//! The harness supplies the physical X11 knob events; this test cannot approve
//! its own binding by sending a network command.
use bridge_core::{ap01,device_client as client,platform::model::{DeviceDataContract,RENDER_TARGET_AP01},
    datasource::{assemble_fields,codex_snapshot},coordinator::data_fields_crc};
use serde_json::{json,Value};
use std::{fs,path::{Path,PathBuf},net::Ipv4Addr,sync::atomic::AtomicBool,time::{Duration,Instant}};
const TOKEN:&str="0123456789abcdef0123456789abcdef"; // Isolated test credential only.
const BRIDGE:&str="ap01-integration";
const TIMEOUT:Duration=Duration::from_secs(5);
fn stage(root:&Path,name:&str,value:&Value) {
    bridge_core::platform::store::write_json(&root.join("client-stage.json"),
        &json!({"stage":name,"value":value})).unwrap();
}
fn gate(root:&Path,name:&str) {
    let deadline=Instant::now()+Duration::from_secs(45);
    while Instant::now()<deadline {
        if fs::read_to_string(root.join("client-continue.txt")).ok().as_deref()==Some(name){return;}
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("UI harness did not confirm {name}");
}
fn envelope(remain:u32,five:Option<u32>,rc:bool)->Value {
    let mut windows=vec![json!({"windowMins":10080,"usedPercent":100-remain,"resetsAt":1792749600u64})];
    if let Some(value)=five{windows.push(json!({"windowMins":300,"usedPercent":100-value,"resetsAt":1792308600u64}));}
    let mut usage=json!({"account":{"plan":"pro"},"bridge":{"label":"username"},
        "buckets":[{"id":"codex","windows":windows}]});
    if rc{usage["resetCredits"]=json!({"availableCount":2});}usage
}
fn data(endpoint:&str,mac:&str,status:&Value,seq:u64,usage:&Value)->Value {
    let contract:DeviceDataContract=serde_json::from_value(status["data_contract"].clone()).unwrap();
    let values=assemble_fields(&codex_snapshot("codex",usage,3600),&contract.requirements);
    let fields:Vec<Value>=contract.requirements.iter().filter(|r|!r.local).map(|r| {
        let f=&values[&r.field];json!({"i":r.index,"k":r.field,"v":f.value,"q":f.quality})
    }).collect();
    let body=json!({"bridge_id":BRIDGE,"active_context_id":status["active_context_id"],"seq":seq,
        "crc":format!("{:08x}",data_fields_crc(&fields)),"fields":fields});
    http(client::data(endpoint,TOKEN,mac,&body,TIMEOUT)).unwrap()
}

#[test]
#[ignore="requires tools/test-ap01-platform-sim.py and isolated socket/UI processes"]
fn discover_pair_publish_and_sync_without_power_plan() {
    let root=PathBuf::from(std::env::var("AP01_LIVE_EVIDENCE").expect("AP01_LIVE_EVIDENCE"));
    let udp=std::env::var("AP01_LIVE_UDP").unwrap_or_else(|_|"127.0.0.1:18767".into()).parse().unwrap();
    let nonce="fedcba98765432100123456789abcdef";
    let discovery=ap01::probe(&[(Ipv4Addr::LOCALHOST,udp)],nonce,None,Duration::from_millis(500),&AtomicBool::new(false)).unwrap();
    assert_eq!(discovery.candidates.len(),1);let candidate=&discovery.candidates[0];
    assert_eq!(candidate.device_mac,"02000000A001");
    let maintenance=&candidate.maintenance_endpoint;let endpoint=&candidate.endpoint;assert_ne!(maintenance,endpoint);let mac=&candidate.device_mac;
    let (code,info)=http(client::pairing_request(maintenance,"GET","/api/pairing/info","",None,TIMEOUT)).unwrap();
    assert_eq!(code,200);ap01::validate_info(candidate,&info).unwrap();
    let request=format!("pair-{}",candidate.boot_id);
    let body=json!({"device_mac":mac,"boot_id":info["boot_id"],"pairing_revision":info["pairing_revision"],
        "request_id":request,"bridge_id":BRIDGE,"name":"AP01 test computer","display_code":"4827","binding_token":TOKEN});
    let (code,reply)=http(client::pairing_request(maintenance,"POST","/api/pairing","",Some(&body),TIMEOUT)).unwrap();
    assert_eq!(code,202,"{reply}");assert_eq!(reply["result"],"pending_display");
    assert!(http(client::status(endpoint,TOKEN,TIMEOUT)).is_err(),"candidate token must not authenticate before the knob confirmation");
    stage(&root,"pending-display",&json!({}));gate(&root,"pending-display");
    let deadline=Instant::now()+Duration::from_secs(15);
    loop {
        let (_,reply)=http(client::pairing_request(maintenance,"GET",&format!("/api/pairing/{request}"),TOKEN,None,TIMEOUT)).unwrap();
        if reply["state"]=="awaiting_confirmation"{break;}
        assert!(Instant::now()<deadline,"UI never confirmed the actual popup: {reply}");std::thread::sleep(Duration::from_millis(50));
    }
    stage(&root,"awaiting_confirmation",&json!({"mac":mac,"code":"4827"}));
    gate(&root,"stale-release");
    let (_,reply)=http(client::pairing_request(maintenance,"GET",&format!("/api/pairing/{request}"),TOKEN,None,TIMEOUT)).unwrap();
    assert_eq!(reply["state"],"awaiting_confirmation","old held button must not approve");
    stage(&root,"fresh-confirmation",&json!({}));
    let deadline=Instant::now()+Duration::from_secs(20);
    loop {
        let (_,reply)=http(client::pairing_request(maintenance,"GET",&format!("/api/pairing/{request}"),TOKEN,None,TIMEOUT)).unwrap();
        if reply["state"]=="approved"{break;}
        assert!(Instant::now()<deadline,"physical confirmation not persisted: {reply}");std::thread::sleep(Duration::from_millis(50));
    }
    let before=http(client::status(endpoint,TOKEN,TIMEOUT)).unwrap();assert_eq!(before["configured"],false);
    assert_eq!(before["fw_target"],"cuktech-ap01-0041");
    assert_eq!(before["power_mode"],"always_on");assert_eq!(before["power_plan"],false);
    let (code,claim)=http(client::claim(endpoint,TOKEN,TOKEN,&json!({"device_mac":mac,"bridge_id":BRIDGE,
        "request_id":"claim-test","name":"AP01 test computer","host":"127.0.0.1","port":18765,"lease_s":300}),TIMEOUT)).unwrap();
    assert_eq!(code,200,"{claim}");
    let mut source:Value=serde_json::from_str(include_str!("../../../../tools/test-bridge/templates/codex-status-ap01.json")).unwrap();
    let compiled=bridge_core::compile::compile(&source,RENDER_TARGET_AP01).unwrap();
    let mut second=source.clone();second["id"]=json!("codex-status-b");
    let second_compiled=bridge_core::compile::compile(&second,RENDER_TARGET_AP01).unwrap();
    let fonts_path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../artifacts/ap01-template-integration");
    let mut fonts=Vec::new();
    for name in ["labels","digits","medium"] {
        let bytes=fs::read(fonts_path.join(format!("ap01-{name}.csfn"))).unwrap();
        let info=bridge_core::platform::fonts::FontLibrary::validate(&bytes).unwrap();
        fonts.push(json!({"name":info.name,"font_id":info.id,
            "data":base64::Engine::encode(&base64::engine::general_purpose::STANDARD,bytes)}));
    }
    let bundle=json!({"bridge_id":BRIDGE,"job_id":"ap01-live-1","device_mac":mac,
        "firmware_target":"cuktech-ap01-0041","render_target":RENDER_TARGET_AP01,"compiler_abi":2,
        "profile":{"template_ids":["codex-status-a","codex-status-b"],"initial_active_id":"codex-status-a"},
        "templates":[{"key":{"template_id":"codex-status-a","render_target":RENDER_TARGET_AP01},"source":source,"compiled":compiled},
            {"key":{"template_id":"codex-status-b","render_target":RENDER_TARGET_AP01},"source":second,"compiled":second_compiled}],
        "resources":[],"fonts":fonts});
    let installed=http(client::install_bundle(endpoint,TOKEN,mac,BRIDGE,&bridge_core::template::canonical_bytes(&bundle),1024,TIMEOUT)).unwrap();
    assert_eq!(installed["result"],"applied","{installed}");
    let status=http(client::status(endpoint,TOKEN,TIMEOUT)).unwrap();
    assert_eq!(status["active_template_id"],"codex-status-a");assert!(!status["data_contract"].is_null());
    assert_eq!(http(client::plan(endpoint,TOKEN,mac,&json!({"bridge_id":BRIDGE,"mode":"light","plan_id":1}),TIMEOUT)).unwrap()["error"],"power_plan_unsupported");
    let original_boot=status["boot_id"].clone();
    for (seq,name,remain,five,rc) in [(1,"weekly",98,None,true),(2,"both",72,Some(86),true),
        (3,"no-rc",72,Some(86),false),(4,"zero",0,Some(0),true)] {
        let ack=data(endpoint,mac,&status,seq,&envelope(remain,five,rc));assert_eq!(ack["result"],"applied","{ack}");
        let actual=http(client::status(endpoint,TOKEN,TIMEOUT)).unwrap();assert_eq!(actual["applied_seq"],seq);
        stage(&root,name,&actual);gate(&root,name);
        let displayed=http(client::status(endpoint,TOKEN,TIMEOUT)).unwrap();
        assert_eq!(displayed["frame_generation"],displayed["displayed_generation"],"real UI must acknowledge the frame");
        let duplicate=data(endpoint,mac,&status,seq,&envelope(remain,five,rc));
        assert_eq!(duplicate["display_state"],"unchanged");
    }
    stage(&root,"rotate-next",&json!({}));gate(&root,"rotate-next");
    let second_status=http(client::status(endpoint,TOKEN,TIMEOUT)).unwrap();
    assert_eq!(second_status["active_template_id"],"codex-status-b");
    assert_ne!(second_status["active_context_id"],status["active_context_id"]);
    assert_eq!(data(endpoint,mac,&second_status,1,&envelope(98,None,true))["result"],"applied");
    stage(&root,"second",&http(client::status(endpoint,TOKEN,TIMEOUT)).unwrap());gate(&root,"second");
    // An occupied device never shows a different Bridge's popup.
    let (_,info)=http(client::pairing_request(maintenance,"GET","/api/pairing/info","",None,TIMEOUT)).unwrap();
    let mut other=body.clone();other["bridge_id"]=json!("other-computer");other["request_id"]=json!("other-pair");
    other["binding_token"]=json!("fedcba98765432100123456789abcdef");other["pairing_revision"]=info["pairing_revision"].clone();
    let (code,reply)=http(client::pairing_request(maintenance,"POST","/api/pairing","",Some(&other),TIMEOUT)).unwrap();
    assert_eq!(code,409);assert_eq!(reply["error"],"occupied");
    stage(&root,"restart",&json!({"boot_id":original_boot}));gate(&root,"restart");
    let recovered=http(client::status(endpoint,TOKEN,TIMEOUT)).unwrap();
    assert_ne!(recovered["boot_id"],original_boot);assert_eq!(recovered["configured"],true);
    assert_eq!(recovered["active_template_id"],"codex-status-b");
    let (code,_)=http(client::claim(endpoint,TOKEN,TOKEN,&json!({"device_mac":mac,"bridge_id":BRIDGE,
        "request_id":"claim-reboot","lease_s":300}),TIMEOUT)).unwrap();assert_eq!(code,200);
    assert_eq!(data(endpoint,mac,&recovered,1,&envelope(98,None,true))["result"],"applied");
    stage(&root,"restored",&http(client::status(endpoint,TOKEN,TIMEOUT)).unwrap());gate(&root,"restored");
    let (code,health)=http(client::pairing_request(maintenance,"GET","/health",TOKEN,None,TIMEOUT)).unwrap();
    assert_eq!(code,200);assert_eq!(health["service"],"ap01-maintenance");
    assert_eq!(http(client::pairing_request(maintenance,"GET","/health","",None,TIMEOUT)).unwrap().0,401);
    // No Codex data/status surface is served from the maintenance listener.
    assert!(http(client::status(maintenance,TOKEN,TIMEOUT)).is_err());
    stage(&root,"business-stop",&json!({}));gate(&root,"business-stop");
    assert!(http(client::status(endpoint,TOKEN,Duration::from_millis(500))).is_err());
    let (code,health)=http(client::pairing_request(maintenance,"GET","/health",TOKEN,None,TIMEOUT)).unwrap();
    assert_eq!(code,200);assert_eq!(health["business_online"],false);
    let (code,info)=http(client::pairing_request(maintenance,"GET","/api/pairing/info","",None,TIMEOUT)).unwrap();
    assert_eq!(code,200);assert_eq!(info["mac"].as_str(),Some(mac.as_str()));
    // Keep the source variable alive to make its lifetime distinct from the
    // installed bundle; editing a draft does not mutate the installed profile.
    source["version"]=json!(2);
    stage(&root,"complete",&json!({"result":"passed","discovery":true,"physical_pairing":true,
        "full_bundle_fonts":true,"independent_maintenance_http":true,"local_profile_activation":true,"rc_and_missing_5h":true,"persistent_reconnect":true,"power_plan":"unsupported"}));
}

// This synchronous process/socket harness joins the async client before returning.
// A scoped runtime also works inside its existing BLE runtime fixtures.
fn http<F: std::future::Future + Send>(future: F) -> F::Output where F::Output: Send {
    std::thread::scope(|scope| scope.spawn(|| tokio::runtime::Builder::new_current_thread()
        .enable_all().build().unwrap().block_on(future)).join().unwrap())
}
