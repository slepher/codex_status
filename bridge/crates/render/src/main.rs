use std::path::PathBuf;

use bridge_render::{
    bits_to_png_size, canvas_size, compile, compiled_deserialize, compiled_serialize,
    png_to_bits_size, render_bits, render_compiled_bits_size, validate, Env,
};

struct Args {
    template: PathBuf,
    usage: Option<PathBuf>,
    out: Option<PathBuf>,
    diff: Option<PathBuf>,
    diff_out: Option<PathBuf>,
    compare_compiled: bool,
    regions: bool,
    channel: String,
    ip: String,
    sync: String,
    battery: i32,
    state: String,
    offline_mins: i32,
    mode: String,
}

fn parse_args() -> Args {
    let mut args = Args {
        template: PathBuf::new(),
        usage: None,
        out: None,
        diff: None,
        diff_out: None,
        compare_compiled: false,
        regions: false,
        channel: "WIFI".to_string(),
        ip: "192.168.1.50".to_string(),
        sync: "23:59".to_string(),
        battery: 78,
        state: "BLE OFF".to_string(),
        offline_mins: -1,
        mode: "light".to_string(),
    };
    let mut iter = std::env::args().skip(1);
    while let Some(flag) = iter.next() {
        let mut value = || iter.next().expect("missing value");
        match flag.as_str() {
            "--template" => args.template = PathBuf::from(value()),
            "--usage" => args.usage = Some(PathBuf::from(value())),
            "--out" => args.out = Some(PathBuf::from(value())),
            "--diff" => args.diff = Some(PathBuf::from(value())),
            "--diff-out" => args.diff_out = Some(PathBuf::from(value())),
            "--compare-compiled" => args.compare_compiled = true,
            "--regions" => args.regions = true,
            "--channel" => args.channel = value(),
            "--ip" => args.ip = value(),
            "--sync" => args.sync = value(),
            "--battery" => args.battery = value().parse().expect("battery number"),
            "--state" => args.state = value(),
            "--offline-mins" => args.offline_mins = value().parse().expect("offline minutes"),
            "--mode" => args.mode = value(),
            other => panic!("unknown flag {other}"),
        }
    }
    args
}

fn bit_diff(a: &[u8], b: &[u8], width: u32, height: u32) -> anyhow::Result<usize> {
    let row_bytes = ((width + 7) / 8) as usize;
    anyhow::ensure!(a.len() == b.len(), "bitmaps differ in size");
    let mut diff = 0usize;
    for y in 0..height as usize {
        for x in 0..width as usize {
            let idx = y * row_bytes + x / 8;
            let mask = 0x80u8 >> (x % 8);
            if (a[idx] ^ b[idx]) & mask != 0 {
                diff += 1;
            }
        }
    }
    Ok(diff)
}

fn main() -> anyhow::Result<()> {
    let args = parse_args();
    let template = std::fs::read_to_string(&args.template)?;
    let usage = match &args.usage {
        Some(path) => std::fs::read_to_string(path)?,
        None => String::new(),
    };
    if let Err(err) = validate(&template) {
        anyhow::bail!("template invalid: {err}");
    }
    let env = Env {
        channel: &args.channel,
        ip: &args.ip,
        sync_hhmm: &args.sync,
        battery: args.battery,
        state: &args.state,
        offline_mins: args.offline_mins,
        mode: &args.mode,
    };
    let bits = render_bits(&template, &usage, &env)?;
    let (width, height) =
        canvas_size(&template).ok_or_else(|| anyhow::anyhow!("unsupported canvas"))?;
    let row_bytes = ((width + 7) / 8) as usize;
    anyhow::ensure!(bits.len() == row_bytes * height as usize, "render size");
    if let Some(out) = &args.out {
        std::fs::write(out, bits_to_png_size(&bits, width, height)?)?;
        println!("wrote {} ({}x{})", out.display(), width, height);
    }
    if args.regions {
        // Region-derivation evidence for this canvas, JSON and compiled path.
        let n = bridge_render::rgn_build(&template)
            .map_err(|e| anyhow::anyhow!("region derivation: {e}"))?;
        println!("regions (json): {n}");
        println!("{}", bridge_render::rgn_dump());
        compile(&template).map_err(|e| anyhow::anyhow!("compile: {e}"))?;
        let blob = compiled_serialize()?;
        let n = bridge_render::rgn_build_compiled(&blob, width, height)
            .map_err(|e| anyhow::anyhow!("compiled region derivation: {e}"))?;
        println!("regions (compiled): {n}");
        println!("{}", bridge_render::rgn_dump());
    }
    if args.compare_compiled {
        // Host parity + CompiledTemplate round trip for the template's own canvas.
        compile(&template).map_err(|e| anyhow::anyhow!("compile: {e}"))?;
        let compiled = render_compiled_bits_size(&usage, &env, width, height)?;
        let json_vs_compiled = bit_diff(&bits, &compiled, width, height)?;
        let blob = compiled_serialize()?;
        compiled_deserialize(&blob).map_err(|e| anyhow::anyhow!("deserialize: {e}"))?;
        let reloaded = render_compiled_bits_size(&usage, &env, width, height)?;
        let round_trip = bit_diff(&compiled, &reloaded, width, height)?;
        println!("compiled record: {} bytes", blob.len());
        println!("json vs compiled diff pixels: {json_vs_compiled}");
        println!("compiled serialize/deserialize round-trip diff pixels: {round_trip}");
        if json_vs_compiled != 0 || round_trip != 0 {
            println!("FAIL: {}x{} shared-engine parity", width, height);
            std::process::exit(4);
        }
        println!(
            "OK: {}x{} json/compiled parity and compiled round trip",
            width, height
        );
    }
    if let Some(reference) = &args.diff {
        let reference_png = std::fs::read(reference)?;
        let reference_bits = png_to_bits_size(&reference_png, width, height)?;
        let mut diff = 0usize;
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let idx = y * row_bytes + x / 8;
                let mask = 0x80u8 >> (x % 8);
                if (bits[idx] ^ reference_bits[idx]) & mask != 0 {
                    diff += 1;
                    x0 = x0.min(x as u32);
                    y0 = y0.min(y as u32);
                    x1 = x1.max(x as u32);
                    y1 = y1.max(y as u32);
                }
            }
        }
        println!("diff pixels: {diff}");
        if let Some(path) = &args.diff_out {
            let mut data = vec![0u8; (width * height * 3) as usize];
            for y in 0..height as usize {
                for x in 0..width as usize {
                    let idx = y * row_bytes + x / 8;
                    let mask = 0x80u8 >> (x % 8);
                    let differs = (bits[idx] ^ reference_bits[idx]) & mask != 0;
                    let white = reference_bits[idx] & mask != 0;
                    let px = if differs {
                        [255u8, 0, 0]
                    } else if white {
                        [255, 255, 255]
                    } else {
                        [170, 170, 170]
                    };
                    let base = (y * width as usize + x) * 3;
                    data[base..base + 3].copy_from_slice(&px);
                }
            }
            let mut out = Vec::new();
            {
                let mut encoder = png::Encoder::new(&mut out, width, height);
                encoder.set_color(png::ColorType::Rgb);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder.write_header()?;
                writer.write_image_data(&data)?;
            }
            std::fs::write(path, out)?;
            println!("wrote diff image {}", path.display());
        }
        if diff != 0 {
            println!("diff bbox: x {x0}..{x1}, y {y0}..{y1}");
            std::process::exit(3);
        }
    }
    Ok(())
}
