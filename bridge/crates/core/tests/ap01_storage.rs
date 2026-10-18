//! Opt-in measured Bundle sizing through the same compiler/codec/C++ store.
use bridge_core::{compile,template::canonical_bytes,platform::model::{FIRMWARE_TARGET_AP01,RENDER_TARGET_AP01}};
use bridge_render::SimulatorBundle;
use serde_json::{json,Value};
use std::{fs,path::{Path,PathBuf}};
const RESERVE:usize=49152;
const CACHE_RECORD:usize=11354; // ID17 + length4 + CTP1(11333), verified against installed files.
fn install(sim:&mut SimulatorBundle,wire:&[u8],request:&str,context:&str)->Value {
    let command=json!({"bridge_id":"storage-test","request_id":request,"session_nonce":"storage-nonce",
        "length":wire.len(),"content_crc":format!("{:08x}",crc32fast::hash(wire))});
    let begin=sim.begin(&command.to_string(),"storage-nonce",0).unwrap();
    if begin["result"]!="applied" {return begin;}
    for (index,chunk) in wire.chunks(1024).enumerate(){
        let ack=sim.chunk(request,"storage-nonce",&(index*1024).to_string(),chunk,1).unwrap();
        assert_eq!(ack["result"],"applied","{ack}");
    }
    sim.commit(&command.to_string(),"storage-nonce",2,context).unwrap()
}
fn stored(root:&Path)->usize {
    fs::read_dir(root).unwrap().map(|entry|entry.unwrap().metadata().unwrap().len() as usize).sum()
}

#[test]
#[ignore="requires AP01_STORAGE_OUT in a precreated isolated evidence directory"]
fn measure_resource_partition_first_install_update_and_full_rejection() {
    let out=PathBuf::from(std::env::var("AP01_STORAGE_OUT").expect("AP01_STORAGE_OUT"));
    assert!(out.is_dir());
    let mut fonts=Vec::new();
    for name in ["labels","digits","medium"] {
        let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../../artifacts/ap01-template-integration/ap01-{name}.csfn"));
        let bytes=fs::read(path).unwrap();let info=bridge_core::platform::fonts::FontLibrary::validate(&bytes).unwrap();
        fonts.push(json!({"name":info.name,"font_id":info.id,"data":base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,bytes)}));
    }
    let source:Value=serde_json::from_str(include_str!("../../../../tools/test-bridge/templates/codex-status-ap01.json")).unwrap();
    let batch=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let mut measurements=Vec::new();
    for count in [1,2,4,8] {
        let mut templates=Vec::new();let mut ids=Vec::new();
        for i in 0..count {
            let id=format!("codex-status-{}",(b'a'+i as u8) as char);let mut source=source.clone();source["id"]=json!(id);
            let compiled=compile::compile(&source,RENDER_TARGET_AP01).unwrap();ids.push(id.clone());
            templates.push(json!({"key":{"template_id":id,"render_target":RENDER_TARGET_AP01},"source":source,"compiled":compiled}));
        }
        let mut bundle=json!({"bridge_id":"storage-test","job_id":format!("storage-{count}"),"device_mac":"02000000A001",
            "firmware_target":FIRMWARE_TARGET_AP01,"render_target":RENDER_TARGET_AP01,"compiler_abi":2,
            "profile":{"template_ids":ids,"initial_active_id":ids[0]},"templates":templates,"resources":[],"fonts":fonts});
        // Reuse the exact compiled fixture for native ROM upper-bound checks.
        fs::write(out.join(format!("bundle-{count}.json")),canonical_bytes(&bundle)).unwrap();
        for compressed in [false,true] {
            let mode=if compressed{"csz1"}else{"raw"};let dir=out.join(format!("{count}-{mode}-{batch}"));
            fs::create_dir(&dir).unwrap(); // Caller runs elevated per AGENTS.
            let raw=canonical_bytes(&bundle);
            let wire=if compressed {bridge_render::bundle_codec::encode(&raw).unwrap()}else{raw.clone()};
            let mut sim=SimulatorBundle::new(&dir,FIRMWARE_TARGET_AP01,1,"cold").unwrap();
            sim.set_storage_capacity(0x140000,8192).unwrap(); // Explicit empty-FS metadata assumption.
            let maximum_empty=sim.status().unwrap()["max_bundle_bytes"].as_u64().unwrap();
            let first=install(&mut sim,&wire,"first-install","00000000000000000000000000000001");
            assert_eq!(first["result"],"applied","{count} {mode}: {first}");
            let content=dir.join("littlefs-2f62756e646c652f636f6e74656e742e62696e");
            let content_size=content.metadata().unwrap().len() as usize;
            assert_eq!(content_size,wire.len()+count*CACHE_RECORD);
            let bytes_after_first=stored(&dir);let maximum_update=sim.status().unwrap()["max_bundle_bytes"].as_u64().unwrap();
            bundle["job_id"]=json!(format!("update-{count}"));
            let next_raw=canonical_bytes(&bundle);
            let next_wire=if compressed {bridge_render::bundle_codec::encode(&next_raw).unwrap()}else{next_raw};
            let updated=install(&mut sim,&next_wire,"next-install","00000000000000000000000000000002");
            assert_eq!(updated["result"],"applied","update {count} {mode}: {updated}");
            let after_update=stored(&dir);
            measurements.push(json!({"templates":count,"encoding":mode,"raw_payload_bytes":raw.len(),"wire_bytes":wire.len(),
                "compiled_cache_bytes":count*CACHE_RECORD,"installed_content_bytes":content_size,"files_after_install_bytes":bytes_after_first,
                "files_after_update_bytes":after_update,"max_empty_transfer_bytes":maximum_empty,"max_update_transfer_bytes":maximum_update,
                "first_install_admission_required_free_bytes":wire.len()+RESERVE+8*CACHE_RECORD,
                "update_admission_required_partition_bytes":8192+bytes_after_first+next_wire.len()+RESERVE+8*CACHE_RECORD,
                "first_install":"applied","second_install":"applied"}));
            // Full device: real admission refuses before replacing the old Bundle.
            sim.set_storage_capacity(0x140000,(0x140000-after_update-1) as u32).unwrap();
            let before=fs::read(&content).unwrap();
            let rejected=install(&mut sim,&next_wire,"full-storage","00000000000000000000000000000003");
            assert_eq!(rejected["result"],"rejected");assert_eq!(fs::read(content).unwrap(),before);
            drop(sim);
            bundle["job_id"]=json!(format!("storage-{count}"));
        }
    }
    let report=json!({"scope":"logical host filesystem model; physical free space and LittleFS block amplification are unmeasured",
        "firmware_target":FIRMWARE_TARGET_AP01,"partition_bytes":0x140000,"assumed_fs_metadata_and_other_files_bytes":8192,
        "store_reserve_bytes":RESERVE,"worst_cache_admission_bytes":8*CACHE_RECORD,"measurements":measurements,
        "physical_resource_free_bytes":Value::Null,"gif_page_removed":false});
    fs::write(out.join("bundle-storage.json"),serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("{}",json!({"passed":true,"cases":measurements.len(),"report":out.join("bundle-storage.json")}));
}
