//! Offline device-blob conversion. Never opens a device or live Bridge data.
use std::{fs::{self, OpenOptions}, io::{Read, Seek, SeekFrom, Write}, path::Path};
use anyhow::{Context, Result};

fn convert_file(input:&Path,output:&Path)->Result<(usize,usize)> {
    // Bound the read even if the input changes after metadata inspection.
    let mut source=Vec::new();
    fs::File::open(input)?.take(8193).read_to_end(&mut source)?;
    let converted=bridge_render::convert_wifi_storage(&source)?;
    anyhow::ensure!(bridge_render::convert_wifi_storage(&converted)?==converted,"conversion_readback_mismatch");
    let mut temporary=output.as_os_str().to_owned();temporary.push(".pending");
    let temporary=std::path::PathBuf::from(temporary);
    let mut options=OpenOptions::new();options.read(true).write(true).create_new(true);
    #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
    // Existing output or staging files are never replaced, including input aliases.
    anyhow::ensure!(!output.try_exists()?,"output_exists");
    let mut file=options.open(&temporary).context("create_conversion_staging")?;
    let result=(|| {
        file.write_all(&converted)?;file.sync_all()?;file.seek(SeekFrom::Start(0))?;
        let mut check=Vec::new();(&mut file).take(8193).read_to_end(&mut check)?;
        anyhow::ensure!(check==converted,"file_readback_mismatch");
        // Same-directory hard-link publication is atomic and refuses a racing
        // existing destination; unlike rename it never overwrites another file.
        fs::hard_link(&temporary,output).context("publish_conversion_no_overwrite")?;
        Ok((source.len(),converted.len()))
    })();
    drop(file);
    // Only this invocation's create_new staging file is removed.
    let cleanup=fs::remove_file(&temporary);
    source.fill(0);
    match result {Err(error)=>Err(error),Ok(sizes)=>{cleanup.context("remove_conversion_staging")?;Ok(sizes)}}
}

fn main()->Result<()> {
    let args:Vec<_>=std::env::args_os().skip(1).collect();
    if args.len()==1 && args[0]=="--help" {
        println!("wifi-store-convert INPUT_BLOB NEW_OUTPUT_BLOB\nAccepts WFS3/WFS4/WFS5 and verified Note4 v1/v2/WFZ2 layouts; emits a historical WFS5 archive without boot recovery; NOT a deployable device store.\nUse private existing directories. Does not overwrite or deploy. Other legacy ABIs are unsupported.");
        return Ok(());
    }
    anyhow::ensure!(args.len()==2,"usage: wifi-store-convert INPUT_BLOB NEW_OUTPUT_BLOB");
    let (before,after)=convert_file(Path::new(&args[0]),Path::new(&args[1]))?;
    println!("Validated historical WFS5 archive (not deployable): {before} -> {after} bytes; input unchanged.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field(out:&mut Vec<u8>,value:&[u8]) {out.push(value.len() as u8);out.extend_from_slice(value);}
    fn seal(out:&mut [u8]) {
        let size=out.len() as u32;out[8..12].copy_from_slice(&size.to_le_bytes());out[12..16].fill(0);
        let crc=crc32fast::hash(out);out[12..16].copy_from_slice(&crc.to_le_bytes());
    }
    fn fixture(version:u8,with_operation:bool)->Vec<u8> {
        let mut out=format!("WFS{version}").into_bytes();out.extend_from_slice(&[version,u8::from(with_operation),1,1]);out.extend_from_slice(&[0;8]);
        out.extend_from_slice(&7u64.to_le_bytes());out.extend_from_slice(&7u64.to_le_bytes());
        if version==5 {out.extend_from_slice(&0u64.to_le_bytes());}out.push(1);
        field(&mut out,b"original-namespace");field(&mut out,b"home");
        let mut network=7u64.to_le_bytes().to_vec();
        for text in [b"original-namespace".as_slice(),b"home",b"Host",b"Device",b"fixture-password",b"Desk"] {field(&mut network,text);}
        out.extend_from_slice(&(network.len() as u16).to_le_bytes());out.extend_from_slice(&network);
        let mut receipt=7u64.to_le_bytes().to_vec();receipt.extend_from_slice(&[0x53;32]);
        let request=if with_operation {b"w3_0000000000000009".as_slice()}else{b"original-request"};
        for text in [b"principal".as_slice(),request,b"original-namespace",b"pending"] {field(&mut receipt,text);}
        if version>=4 {
            receipt.extend_from_slice(&if with_operation {6u64}else{0}.to_le_bytes());
            field(&mut receipt,if with_operation {b"activate"}else{b""});field(&mut receipt,if with_operation {b"home"}else{b""});
        }
        if version==5 {receipt.extend_from_slice(&0u64.to_le_bytes());}
        out.extend_from_slice(&(receipt.len() as u16).to_le_bytes());out.extend_from_slice(&receipt);
        if with_operation {
            let mut cursor=12u64.to_le_bytes().to_vec();field(&mut cursor,b"principal");field(&mut cursor,b"original-namespace");
            out.extend_from_slice(&(cursor.len() as u16).to_le_bytes());out.extend_from_slice(&cursor);
        }
        seal(&mut out);out
    }
    #[test]
    fn conversion_preserves_unknown_identity_fingerprint_and_operation() {
        for version in [3,4,5] {
            let original=fixture(version,version>=4);
            let current=bridge_render::convert_wifi_storage(&original).unwrap();
            assert_eq!(current,fixture(5,version>=4));
            assert_eq!(bridge_render::convert_wifi_storage(&current).unwrap(),current);
            for size in 0..original.len() {assert!(bridge_render::convert_wifi_storage(&original[..size]).is_err());}
            for offset in 0..original.len() {
                let mut corrupt=original.clone();corrupt[offset]^=0x80;assert!(bridge_render::convert_wifi_storage(&corrupt).is_err());
            }
        }
        let mut unknown=fixture(5,true);unknown[4]=6;seal(&mut unknown);
        assert!(bridge_render::convert_wifi_storage(&unknown).is_err());
        assert!(bridge_render::convert_wifi_storage(&vec![0;8193]).is_err());
        let mut executing=fixture(5,true);
        let network_at=41+1+b"original-namespace".len()+1+b"home".len();
        let receipt_at=network_at+2+u16::from_le_bytes(executing[network_at..network_at+2].try_into().unwrap()) as usize;
        let receipt_end=receipt_at+2+u16::from_le_bytes(executing[receipt_at..receipt_at+2].try_into().unwrap()) as usize;
        executing[32..40].copy_from_slice(&42u64.to_le_bytes());
        executing[receipt_end-8..receipt_end].copy_from_slice(&42u64.to_le_bytes());seal(&mut executing);
        assert_eq!(bridge_render::convert_wifi_storage(&executing).unwrap(),executing,"offline conversion must not run boot interruption recovery");
        executing[receipt_end+2..receipt_end+10].copy_from_slice(&8u64.to_le_bytes());seal(&mut executing);
        assert!(bridge_render::convert_wifi_storage(&executing).is_err(),"cursor must fence the stored request sequence");
    }
    #[test]
    fn note4_legacy_layout_and_compression_preserve_records_without_guessing() {
        fn fixed(out:&mut [u8],offset:usize,value:&[u8]) {out[offset..offset+value.len()].copy_from_slice(value);}
        fn crc(out:&mut [u8],start:usize,end:usize) {
            let value=crc32fast::hash(&out[start..end]);out[end..end+4].copy_from_slice(&value.to_le_bytes());
        }
        for v2 in [false,true] {
            // Byte fixtures use DWARF-verified Xtensa offsets, not host structs.
            let mut raw=vec![0u8;if v2 {5248}else{2296}];let base=if v2 {4}else{0};
            raw[base..base+4].copy_from_slice(&1u32.to_le_bytes());
            for (offset,value) in [(0,b"original-namespace".as_slice()),(22,b"home"),(55,b"Host"),(88,b"Device"),(121,b"fixture-password")] {
                fixed(&mut raw,base+4+offset,value);
            }
            fixed(&mut raw,base+2236,b"original-namespace");fixed(&mut raw,base+2258,b"home");crc(&mut raw,base,base+2292);
            if v2 {
                raw[..4].copy_from_slice(&2u32.to_le_bytes());raw.copy_within(8..194,2300);
                for offset in [2488,2496,2504,3384+224] {raw[offset..offset+8].copy_from_slice(&7u64.to_le_bytes());}
                fixed(&mut raw,2600,b"Desk");fixed(&mut raw,3384,b"principal");fixed(&mut raw,3384+65,b"original-request");
                fixed(&mut raw,3384+130,b"original-namespace");fixed(&mut raw,3384+184,b"pending");raw[3384+152..3384+184].fill(0x53);crc(&mut raw,0,5240);
            }
            let converted=bridge_render::convert_wifi_storage(&raw).unwrap();
            assert_eq!(bridge_render::convert_wifi_storage(&converted).unwrap(),converted);
            if v2 {assert_eq!(converted,fixture(5,false));} else {
                assert_eq!(&converted[..8],b"WFS5\x05\x00\x01\x00");assert_eq!(&converted[16..40],&[0;24]);
                assert_eq!(converted[40],1);assert!(converted.windows(16).any(|w|w==b"fixture-password"));
            }
            for size in 0..raw.len() {assert!(bridge_render::convert_wifi_storage(&raw[..size]).is_err());}
            for offset in 0..if v2 {5244}else{2296} {
                let mut damaged=raw.clone();damaged[offset]^=0x80;assert!(bridge_render::convert_wifi_storage(&damaged).is_err());
            }
            if v2 {
                let mut packed=b"WFZ2".to_vec();packed.extend(miniz_oxide::deflate::compress_to_vec_zlib(&raw,6));
                assert_eq!(bridge_render::convert_wifi_storage(&packed).unwrap(),converted);
                packed.truncate(packed.len()-1);assert!(bridge_render::convert_wifi_storage(&packed).is_err());
                let mut missing_cursor=raw.clone();missing_cursor[3384+65..3384+130].fill(0);
                fixed(&mut missing_cursor,3384+65,b"w3_0000000000000009");crc(&mut missing_cursor,0,5240);
                assert!(bridge_render::convert_wifi_storage(&missing_cursor).is_err());
                let mut oversized=b"WFZ2".to_vec();oversized.extend(miniz_oxide::deflate::compress_to_vec_zlib(&vec![0;5249],6));
                assert!(bridge_render::convert_wifi_storage(&oversized).is_err());
            }
        }
    }
    #[test]
    fn runtime_reads_deployed_wfs4_without_rewriting_and_rejects_archive_formats() {
        let root=std::env::temp_dir().join(format!("wifi-current-runtime-{}",std::process::id()));
        fs::create_dir(&root).unwrap();let path=root.join("wifi-schemes.bin");
        for original in [vec![],fixture(3,false),fixture(4,true),fixture(5,true)] {
            fs::write(&path,&original).unwrap();
            if original.get(4)==Some(&4) {
                let mut sim=bridge_render::SimulatorWifi::new(&root).unwrap();
                let status=sim.storage_status().unwrap();
                assert_eq!(status["format"],"WFS4");assert_eq!(status["receipts"],1);
                assert_eq!(status["transaction_format"],"embedded");assert_eq!(status["cursors"],1);
            } else { assert!(bridge_render::SimulatorWifi::new(&root).is_err()); }
            assert_eq!(fs::read(&path).unwrap(),original);
        }
        fs::remove_file(path).unwrap();fs::remove_dir(root).unwrap();
    }

    #[test]
    fn file_conversion_preserves_input_and_refuses_existing_outputs() {
        let root=std::env::temp_dir().join(format!("wifi-convert-{}",std::process::id()));
        fs::create_dir(&root).unwrap();
        let input=root.join("input");let output=root.join("output");let staging=root.join("output.pending");
        let original=fixture(4,true);fs::write(&input,&original).unwrap();
        assert!(convert_file(&input,&input).is_err());assert_eq!(fs::read(&input).unwrap(),original);
        fs::write(&staging,b"keep").unwrap();assert!(convert_file(&input,&output).is_err());assert_eq!(fs::read(&staging).unwrap(),b"keep");
        fs::remove_file(&staging).unwrap();convert_file(&input,&output).unwrap();
        let converted=fs::read(&output).unwrap();assert_eq!(converted,fixture(5,true));assert!(!staging.exists());
        assert!(convert_file(&input,&output).is_err());assert_eq!(fs::read(&output).unwrap(),converted);assert_eq!(fs::read(&input).unwrap(),original);
        fs::remove_file(input).unwrap();fs::remove_file(output).unwrap();fs::remove_dir(root).unwrap();
    }
}
