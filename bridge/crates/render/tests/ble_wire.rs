use bridge_render::simulator_ble_wire as wire;
use serde_json::{json, Value};

fn envelope() -> Value {
    json!({"device_mac":"AA:BB:CC:DD:EE:FF","request_id":"\u{1}".repeat(64),
        "wake_generation":u32::MAX,"wake_seq":u32::MAX})
}
#[test]
fn production_ble_status_preserves_all_maximum_byte_identifiers() {
    let mut input = envelope();
    input["session_nonce"] = json!("f".repeat(32));
    input["rendezvous_v"] = json!(1);
    input["automatic_contact"] = json!(false);
    input["network_error"] = json!("bridge_http_unreachable");
    input["active_context_id"] = json!("\u{1}".repeat(32));
    input["active_template_id"] = json!("\\".repeat(16));
    input["committed_job_id"] = json!("é".repeat(8));
    input["applied_seq"] = json!(u64::MAX); input["commit_seq"] = json!(u32::MAX);
    input["power"] = json!({"mode":"light","plan_id":u64::MAX,"remaining_s":u32::MAX,"provisional_remaining_s":u32::MAX});
    input["sync"] = json!({"v":1,"enabled":true,"rounds":255,"retry_skip":255,"due":true,"pending":true});
    let raw = wire("status", &input).unwrap();
    println!("max status bytes={}", raw.len());
    assert!(raw.len() <= 512);
    let doc: Value = serde_json::from_slice(&raw).unwrap();
    use base64::Engine;
    let decode = |v: &Value| base64::engine::general_purpose::STANDARD.decode(v.as_str().unwrap()).unwrap();
    assert_eq!(decode(&doc["rid"]), vec![1;64]);
    assert_eq!(decode(&doc["s"][1]), vec![1;32]);
    assert_eq!(decode(&doc["s"][2]), vec![b'\\';16]);
    assert_eq!(decode(&doc["s"][3]), "é".repeat(8).as_bytes());
    assert_eq!(doc["s"][4], u64::MAX);
    assert!(doc.get("data_contract").is_none());
    assert_eq!(doc["rendezvous_v"], 1);
    assert_eq!(doc["ac"], false);
    assert_eq!(doc["ne"], 2);
    assert_eq!(doc["s"].as_array().unwrap().len(), 16);
    // Ordinary Bridge status uses an r + 16-hex request ID. Test its complete
    // compact frame too; the adapter separately rejects oversized custom IDs.
    input["request_id"] = json!("rffffffffffffffff");
    let compact_payload=wire("status", &input).unwrap();
    assert!(compact_payload.len()+48<=512);
    println!("compact status with maximum fields={} bytes", compact_payload.len()+48);
}
#[test]
fn production_info_and_all_ack_classes_are_bounded() {
    let info = wire("info", &json!({"mac":"AA:BB:CC:DD:EE:FF","ip":"255.255.255.255",
        "fw":"\u{1}".repeat(64),"wake_generation":u32::MAX,"wake_seq":u32::MAX,
        "pairingWindow":true,"peerBonded":true,"peerEncrypted":true,"bundle_configured":true,
        "wifi_setup_protocol":255,"wifi_management":2,"wifi_connection":1,"rf_remaining_ms":u32::MAX-1})).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&info).unwrap()["wc"],1);
    println!("max INFO bytes={}", info.len());
    assert!(info.len() <= 512);
    for op in ["data","plan","sync_config","sync_open","network_start","command"] {
        for result in ["applied","rejected"] {
            let mut input = envelope();
            input["op"] = json!(op); input["result"] = json!(result);
            input["display_state"] = json!("unchanged"); input["retention"] = json!("flash");
            input["data_seq"] = json!(u64::MAX); input["plan_id"] = json!(u64::MAX);
            input["active_context_id"] = json!("\u{1}".repeat(32));
            input["accepted_remaining_s"] = json!(u32::MAX);
            if result == "rejected" { input["error"] = json!("request_conflict"); }
            input["accepted_remaining_s"] = json!(u32::MAX - 1);
            let raw = wire("ack", &input).unwrap();
            println!("max ACK {op}/{result} bytes={}", raw.len());
            let doc: Value = serde_json::from_slice(&raw).unwrap();
            assert_eq!(doc["result"], result, "legal ACK must fit without fallback");
            assert_eq!(doc["data_seq"], u64::MAX);
        }
    }
    for category in ["auth","endpoint"] {
        for ok in [true,false] {
            let mut doc = json!({"ack":category,"ok":ok});
            if category == "endpoint" {
                doc["request_id"] = json!("\u{1}".repeat(64)); doc["device_mac"] = json!("AA:BB:CC:DD:EE:FF");
                doc["bridge_id"] = json!("\u{1}".repeat(64));
                if !ok { doc["error"] = json!("storage_unknown"); doc["stage"] = json!("persist"); }
                let reply: Value = serde_json::from_slice(&wire(category, &doc).unwrap()).unwrap();
                assert_eq!(reply["ble_wire"],1);
                use base64::Engine;
                for field in ["rid","pid"] {
                    assert_eq!(base64::engine::general_purpose::STANDARD.decode(reply[field].as_str().unwrap()).unwrap(),vec![1;64]);
                }
                println!("max endpoint {ok} bytes={}",wire(category,&doc).unwrap().len());
                assert_eq!(reply["result"], if ok {"applied"} else {"unknown"});
            }
            if category == "auth" && ok { doc["token"] = json!("f".repeat(32)); }
            assert!(wire(category, &doc).unwrap().len() <= 512);
        }
    }
}
#[test]
fn final_attribute_boundary_is_511_512_and_513_encoded_bytes() {
    for size in [511,512,513] {
        let body = json!({"padding":"x".repeat(size - 14)});
        assert_eq!(serde_json::to_vec(&body).unwrap().len(), size);
        let result = wire("attribute", &body);
        assert_eq!(result.is_ok(), size <= 512);
        if let Ok(raw) = result { assert_eq!(raw.len(), size); }
    }
}
#[test]
fn invalid_identity_is_not_echoed_and_oversize_is_unknown_after_execution() {
    for id in [String::new(), "r".repeat(65), "r\0suffix".to_owned()] {
        let mut doc = envelope(); doc["request_id"] = json!(id);
        assert!(wire("reply", &doc).is_err());
    }
    let mut doc = envelope(); doc["result"] = json!("applied"); doc["payload"] = json!("x".repeat(635));
    let raw = wire("reply", &doc).unwrap();
    let reply: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(reply["result"], "unknown"); assert_eq!(reply["error"], "reply_too_large");
    doc.as_object_mut().unwrap().remove("payload");
    doc["records"] = json!([{"seq":1,"detail":"x".repeat(400)},{"seq":2,"detail":"x".repeat(400)}]);
    let reply: Value = serde_json::from_slice(&wire("reply", &doc).unwrap()).unwrap();
    assert_eq!(reply["more"], true); assert!(reply["records"].as_array().unwrap().len() < 2);
}
