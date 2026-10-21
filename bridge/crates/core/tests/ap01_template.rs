use bridge_core::{compile, template::validate_template};
use bridge_core::platform::model::RENDER_TARGET_AP01;
use bridge_render::{Env, frame_bytes};
use serde_json::{json, Value};

fn source() -> Value {
    serde_json::from_str(include_str!("../../../../tools/test-bridge/templates/codex-status-ap01.json")).unwrap()
}
fn usage(remaining: u32, five: Option<u32>) -> Value {
    // Paid plans expose 5h + weekly; free/Go plans expose only the monthly window.
    let mut windows = vec![json!({"windowMins":if five.is_some(){10080}else{43200},"usedPercent":100-remaining,"resetsAt":1792749600})];
    if let Some(five) = five { windows.push(json!({"windowMins":300,"usedPercent":100-five,"resetsAt":1792308600})); }
    let mut value=json!({"server_time":1792305600,"account":{"plan":"pro"},"bridge":{"label":"username"},"buckets":[{"id":"codex","windows":windows}]});
    value["resetCredits"]=json!({"availableCount":2});
    value
}
fn pixel(frame: &[u8], x: usize, y: usize) -> u8 {
    let byte = frame[y*160+x/2];
    if x%2==0 { byte>>4 } else { byte&15 }
}

#[test]
fn ap01_shared_compiler_layouts_and_boundaries() {
    let src = source();
    validate_template(&src).unwrap();
    let compiled = compile::compile(&src, RENDER_TARGET_AP01).unwrap();
    assert_eq!(compiled.template_id,src["id"].as_str().unwrap());
    assert_eq!((compiled.canvas_w, compiled.canvas_h), (320,240));
    assert_eq!(compile::decode(&compile::encode(&compiled)).unwrap(), compiled);
    let mut caps=bridge_core::platform::model::DeviceCapabilities::ssd1681_154g();
    caps.firmware_target=bridge_core::platform::model::FIRMWARE_TARGET_AP01.into();
    caps.render_target=RENDER_TARGET_AP01.into(); caps.width=320; caps.height=240;
    caps.pixel_format="4bpp".into(); caps.colors="ap01-palette-v2".into();
    caps.partial=false; caps.hardware_verified=false; caps.ble=false;
    caps.bundle_font_protocol=3;
    caps.validate().unwrap();
    let mut old_protocol=bridge_core::platform::model::DeviceCapabilities::ssd1681_154g();
    old_protocol.bundle_font_protocol=3;assert!(old_protocol.validate().is_err());
    let mut wrong=caps.clone(); wrong.pixel_format="1bpp".into(); assert!(wrong.validate().is_err());
    assert!(compiled.requirements.iter().all(|r| !r.field.contains("token")));
    if let Ok(dir)=std::env::var("AP01_PREVIEW_DIR") {
        let manifest=json!({"template_id":compiled.template_id,"render_target":compiled.render_target,
            "compiler_abi":compiled.compiler_abi,"source_crc":compiled.source_crc,
            "compiled_record_bytes":compiled.binary.len()/2,"op_count":compiled.op_count,
            "remote_fields":compiled.requirements.iter().filter(|r| !r.local).map(|r| &r.field).collect::<Vec<_>>(),
            "local_fields":compiled.local_dependencies,"palette_frame_bytes":frame_bytes(320,240)});
        std::fs::write(std::path::Path::new(&dir).join("template-manifest.json"),serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }
    let mut env = Env::default();
    env.sync_hhmm = "12:40";
    let fonts: Vec<Vec<u8>> = ["AP01_PREVIEW_FONT", "AP01_PREVIEW_DIGITS", "AP01_PREVIEW_MEDIUM", "AP01_PREVIEW_LATIN"]
        .into_iter().filter_map(|name| std::env::var(name).ok())
        .map(|path| std::fs::read(path).unwrap()).collect();
    let render = |input: &Value| {
        if fonts.is_empty() { bridge_render::render_bits(&src.to_string(), &input.to_string(), &env).unwrap() }
        else { bridge_render::render_bits_with_fonts(&src.to_string(), &input.to_string(), &env, fonts.clone()).unwrap() }
    };
    for (name, remain, five) in [("monthly-only",98,None),("weekly-only",98,None),("both-windows",72,Some(86)),("zero",0,None),("full",100,None),
                               ("both-full",100,Some(100)),("both-zero",0,Some(0)),("both-one-digit",9,Some(9)),("both-99",99,Some(99))] {
        let mut input = usage(remain,five);
        if name == "weekly-only" {
            input["account"]["plan"] = json!("prolite");
            input["buckets"][0]["windows"][0]["windowMins"] = json!(10080);
        }
        let data = input.to_string();
        if let Ok(dir)=std::env::var("AP01_PREVIEW_DIR") {
            std::fs::write(std::path::Path::new(&dir).join(format!("{name}.usage.json")), &data).unwrap();
        }
        let frame = render(&input);
        assert_eq!(frame.len(),frame_bytes(320,240));
        if !fonts.is_empty() {
            // Compare actual ink rather than line boxes: the quota number is
            // balanced between the heading and the percent below it.
            let cx: usize=if five.is_some(){78}else{160};
            let heading_bottom=(64..88).filter(|&y| ((cx-43)..(cx+42)).any(|x| pixel(&frame,x,y)==2)).max().unwrap();
            let number_rows: Vec<usize>=(90..149).filter(|&y| ((cx-60)..(cx+60)).any(|x| pixel(&frame,x,y)==1)).collect();
            let percent_top=(143..165).find(|&y| ((cx-10)..(cx+10)).any(|x| pixel(&frame,x,y)==1)).unwrap();
            let upper=number_rows[0] as i32-heading_bottom as i32-1;
            let lower=percent_top as i32-*number_rows.last().unwrap() as i32-1;
            assert!(upper>=0 && lower>=0 && (upper-lower).abs()<=3,
                    "{name}: quota ink must be vertically centered (upper={upper}, lower={lower})");
            // Compare against an unrestricted rendering of the same glyphs:
            // balanced spacing alone can still hide clipped top rows.
            let isolated=json!({"schema":1,"id":"quota-ink","version":1,"render_target":RENDER_TARGET_AP01,
                "canvas":{"w":320,"h":240},"elements":[
                    {"type":"rect","rect":[0,0,320,240],"fill":true,"color":"black"},
                    {"type":"text","text":remain.to_string(),"font":"ntreg64","x":50,"y":50,"color":"white","bg":"black"}]});
            let reference=bridge_render::render_bits_with_fonts(&isolated.to_string(),"{}",&env,fonts.clone()).unwrap();
            let full_ink=(0..240).map(|y| (0..320).filter(|&x| pixel(&reference,x,y)!=0).count()).sum::<usize>();
            let displayed_ink=(90..149).map(|y| ((cx-60)..(cx+60)).filter(|&x| pixel(&frame,x,y)!=0).count()).sum::<usize>();
            assert_eq!(displayed_ink,full_ink,"{name}: quota glyph pixels must not be clipped");
            if let Some(value)=five {
                let isolated=json!({"schema":1,"id":"five-hour-ink","version":1,"render_target":RENDER_TARGET_AP01,
                    "canvas":{"w":320,"h":240},"elements":[
                        {"type":"rect","rect":[0,0,320,240],"fill":true,"color":"black"},
                        {"type":"text","text":value.to_string(),"font":"nt30","x":50,"y":50,"color":"white","bg":"black"},
                        {"type":"text","text":"%","font":"nt16","x":250,"y":100,"color":"white","bg":"black"}]});
                let reference=bridge_render::render_bits_with_fonts(&isolated.to_string(),"{}",&env,fonts.clone()).unwrap();
                let full_ink=(0..240).map(|y| (0..320).filter(|&x| pixel(&reference,x,y)!=0).count()).sum::<usize>();
                let displayed_ink=(82..132).map(|y| (166..304).filter(|&x| pixel(&frame,x,y)!=0).count()).sum::<usize>();
                assert_eq!(displayed_ink,full_ink,"{name}: 5h glyph pixels and percent must not be clipped");
            }
        }
        assert!((43..166).any(|y| (99..222).any(|x| pixel(&frame,x,y)>=7)), "ring must contain coverage shades");
        if std::env::var("AP01_PREVIEW_FONT").is_ok() {
            assert!((214..240).any(|y| (16..196).any(|x| pixel(&frame,x,y)>=7)), "sync text must contain coverage shades");
        }
        // RC is independent of the 5h window, in both layouts.
        assert!((214..240).any(|y| (240..304).any(|x| pixel(&frame,x,y)==3)));
        let mut no_rc=input.clone();
        no_rc.as_object_mut().unwrap().remove("resetCredits");
        let hidden=render(&no_rc);
        assert!((214..240).all(|y| (240..304).all(|x| pixel(&hidden,x,y)==0)));
        if let Ok(dir) = std::env::var("AP01_PREVIEW_DIR") {
            std::fs::write(std::path::Path::new(&dir).join(format!("{name}.png")), bridge_render::bits_to_png_size(&frame,320,240).unwrap()).unwrap();
        }
        if five.is_none() {
            let filled_start=(48..55).any(|y| (160..173).any(|x| pixel(&frame,x,y)==2));
            assert_eq!(filled_start,remain>0,"quota arc starts only for nonzero remaining");
            // The former 5h area is clear; there is no label or placeholder.
            let unexpected = (52..100).flat_map(|y| (260..304).map(move |x| (x,y))).find(|&(x,y)| pixel(&frame,x,y)!=0);
            assert_eq!(unexpected,None,"unexpected pixel in hidden 5h block");
        }
        if name == "both-windows" {
            let mut missing_account=input.clone();
            missing_account.as_object_mut().unwrap().remove("bridge");
            let missing=render(&missing_account);
            assert!((10..36).all(|y| (76..239).all(|x| pixel(&missing,x,y)==0)));
            let mut long_account=input.clone();
            long_account["bridge"]["label"]=json!("a-very-long-account-name-that-must-not-cover-time");
            let long=render(&long_account);
            assert!((10..36).all(|y| (244..304).all(|x| pixel(&long,x,y)==pixel(&frame,x,y))),"account must not overwrite clock");
        }
        let png = bridge_render::bits_to_png_size(&frame,320,240).unwrap();
        if let Ok(dir) = std::env::var("AP01_PREVIEW_DIR") {
            std::fs::write(std::path::Path::new(&dir).join(format!("{name}.png")), &png).unwrap();
            std::fs::write(std::path::Path::new(&dir).join(format!("{name}.palette4")), &frame).unwrap();
        }
    }
    for patch in [json!({"type":"ring","x":160,"y":100,"radius":300,"thickness":4,"bind":"buckets[codex].weekly.remaining"}),
                  json!({"type":"ring","x":160,"y":100,"radius":40,"thickness":4,"bind":"bridge.label"}),
                  json!({"type":"rect","rect":[0,0,320,240],"color":"unknown"})] {
        let mut bad=src.clone(); bad["elements"]=json!([patch]);
        assert!(validate_template(&bad).is_err());
        assert!(bridge_render::validate(&bad.to_string()).is_err());
    }
    let mut old=src.clone(); old["canvas"]=json!({"w":200,"h":200});
    old["render_target"]=json!("epd-ssd1681-200x200-1bpp");
    assert!(validate_template(&old).is_err());
    assert!(bridge_render::validate(&old.to_string()).is_err());
}

// A tiny alpha4 Unicode container exercises the protocol without local fonts.
fn alpha_font() -> Vec<u8> {
    let mut b=vec![0u8;64];
    b[..4].copy_from_slice(b"CSFN");
    b[4..6].copy_from_slice(&3u16.to_le_bytes());
    b[6..8].copy_from_slice(&64u16.to_le_bytes());
    b[16..20].copy_from_slice(&4u32.to_le_bytes());
    b[20..24].copy_from_slice(&95u32.to_le_bytes());
    b[24]=4;b[25]=2;b[26]=12;b[27]=2;
    b[28..30].copy_from_slice(&64u16.to_le_bytes());
    b[34..36].copy_from_slice(&8u16.to_le_bytes());
    b[36..38].copy_from_slice(&4u16.to_le_bytes());
    b[38..40].copy_from_slice(&7u16.to_le_bytes());
    b[40..42].copy_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(b"ntthin18testunicode\0");
    for cp in 32u32..127 {b.extend_from_slice(&cp.to_le_bytes());}
    let off=b.len();b.resize(off+95*8,0);
    let g=off+33*8; b[g+2]=64;b[g+4]=3;b[g+5]=2;
    b.extend_from_slice(&[0x48,0xf0,0xf8,0x40]);
    let len=b.len() as u32;b[8..12].copy_from_slice(&len.to_le_bytes());
    let crc=crc32fast::hash(&b[64..]);b[12..16].copy_from_slice(&crc.to_le_bytes());
    b
}

#[test]
fn ap01_alpha4_masks_reject_malformed_and_epaper_targets() {
    use bridge_core::platform::fonts::FontLibrary;
    let font=alpha_font();
    let descriptor=FontLibrary::validate(&font).unwrap();
    assert_eq!(descriptor.pixel_format,"alpha4");
    assert!(descriptor.check_target("4bpp").is_ok());
    assert!(descriptor.check_target("1bpp").is_err());
    assert!(descriptor.check_target("2bpp").is_err());
    let src=json!({"schema":1,"id":"alpha-test","version":1,"render_target":RENDER_TARGET_AP01,
        "canvas":{"w":320,"h":240},"elements":[{"type":"text","text":"A","font":"ntthin18","x":10,"y":10,"color":"white","bg":"black"}]});
    use bridge_core::platform::model::{Bundle,BundleProfile,BundleFont,Template,TemplateKey,FIRMWARE_TARGET_AP01};
    let compiled=compile::compile(&src,RENDER_TARGET_AP01).unwrap();
    let template=Template{key:TemplateKey::new("alpha-test",RENDER_TARGET_AP01),source:src.clone(),source_crc:compiled.source_crc.clone(),compiled,saved_at:0};
    let bundle=Bundle{job_id:"alpha-test".into(),device_mac:"000000000001".into(),bridge_id:"test".into(),
        firmware_target:FIRMWARE_TARGET_AP01.into(),render_target:RENDER_TARGET_AP01.into(),compiler_abi:2,
        profile:BundleProfile{template_ids:vec!["alpha-test".into()],initial_active_id:"alpha-test".into(),bindings:vec![],full_sync_s:3600},
        templates:vec![template],resources:vec![],fonts:vec![BundleFont{name:descriptor.name.clone(),font_id:descriptor.id.clone(),
        data:base64::Engine::encode(&base64::engine::general_purpose::STANDARD,&font)}],total_len:0,crc:String::new()}.seal().unwrap();
    bundle.verify().unwrap();
    let frame=bridge_render::render_bits_with_fonts(&src.to_string(),"{}",&Env::default(),vec![font.clone()]).unwrap();
    assert_eq!(pixel(&frame,12,18),1);
    assert!(pixel(&frame,10,18)>=7);
    for (at,value) in [(4,2),(24,1),(25,3)] {
        let mut bad=font.clone();bad[at]=value;
        assert!(FontLibrary::validate(&bad).is_err());
        assert!(bridge_render::render_bits_with_fonts(&src.to_string(),"{}",&Env::default(),vec![bad]).is_err());
    }
    let mut bad=font.clone();bad.truncate(bad.len()-2);
    let len=bad.len() as u32;bad[8..12].copy_from_slice(&len.to_le_bytes());
    bad[16..20].copy_from_slice(&2u32.to_le_bytes());
    let crc=crc32fast::hash(&bad[64..]);bad[12..16].copy_from_slice(&crc.to_le_bytes());
    assert!(FontLibrary::validate(&bad).is_err());
    assert!(bridge_render::render_bits_with_fonts(&src.to_string(),"{}",&Env::default(),vec![bad]).is_err());
    let mut old=src;old["canvas"]=json!({"w":200,"h":200});
    old["render_target"]=json!("epd-ssd1681-200x200-1bpp");
    assert!(bridge_render::render_bits_with_fonts(&old.to_string(),"{}",&Env::default(),vec![font]).is_err());
}

#[test]
fn ap01_never_allocates_or_queues_a_power_plan() {
    use bridge_core::platform::{service::PlatformService,model::{DeviceCapabilities,DeviceIdentity,PlanMode}};
    let root=tempfile::tempdir().unwrap();let svc=PlatformService::open(root.path()).unwrap();
    let mac="001122334455";
    svc.device_upsert(DeviceIdentity::new(mac,"AP01").unwrap(),DeviceCapabilities::ap01()).unwrap();
    assert!(!svc.uses_power_plan(mac).unwrap());
    assert!(svc.plan_for_rendezvous(mac,100,"rendezvous",0).unwrap_err().to_string().contains("power_plan_unsupported"));
    assert!(svc.queue_explicit_light(mac,100).is_err());
    assert!(svc.explicit_plan(mac,PlanMode::Light,600,"manual").is_err());
    assert!(svc.explicit_plan(mac,PlanMode::Sleep,0,"manual").is_err());
}
