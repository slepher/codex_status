use bridge_render as _;
extern "C" {
    fn codex_plan_store_check() -> i32;
    fn codex_boot_capture_check() -> i32;
}
#[test]
fn production_plan_persistence_faults_and_reboots() {
    assert_eq!(unsafe { codex_plan_store_check() }, 0, "C++ assertion line");
}
#[test]
fn production_boot_capture_and_manual_icon_invalidation() {
    assert_eq!(unsafe { codex_boot_capture_check() }, 0, "C++ assertion line");
}

#[test]
fn plan_network_contract_matches_rom_and_replay_is_immutable() {
    use serde_json::json;
    let network=json!({"v":1,"bridge_addr":"192.168.1.2","bridge_port":8765,
        "target":{"network_id":"home","open":true,"metadata_key":"a".repeat(64)}});
    bridge_core::template::validate_plan_activation(&network).unwrap();
    let frozen=bridge_render::simulator_plan_network(42,None,&network).unwrap();
    assert_eq!(bridge_render::simulator_plan_network(42,Some((42,&frozen)),&network).unwrap(),frozen);
    let mut changed=network.clone();changed["bridge_port"]=json!(8766);
    assert!(bridge_render::simulator_plan_network(42,Some((42,&frozen)),&changed).unwrap_err().to_string().contains("plan_network_conflict"));
    for (pointer,value) in [("/v",json!(2)),("/bridge_addr",json!("127.0.0.1")),
        ("/bridge_addr",json!("192.168.001.2")),("/bridge_port",json!(0)),("/bridge_port",json!(65536)),
        ("/target/network_id",json!("x".repeat(33))),("/target/open",json!("true")),
        ("/target/metadata_key",json!("A".repeat(64))),("/target/metadata_key",json!("a".repeat(63)))] {
        let mut invalid=network.clone();*invalid.pointer_mut(pointer).unwrap()=value;
        assert!(bridge_core::template::validate_plan_activation(&invalid).is_err(),"{pointer}");
        assert!(bridge_render::simulator_plan_network(43,None,&invalid).is_err(),"{pointer}");
    }
    let mut maximum=network.clone();maximum["target"]["network_id"]=json!("x".repeat(32));
    maximum["target"]["credential_key"]=json!("f".repeat(64));
    bridge_core::template::validate_plan_activation(&maximum).unwrap();
    assert!(bridge_render::simulator_plan_network(43,None,&maximum).unwrap().to_string().len()<=384);
}
