use std::path::PathBuf;

use bridge_render::{bits_to_png, png_to_bits, render_bits, validate, Env};

struct Args {
    template: PathBuf,
    usage: Option<PathBuf>,
    out: Option<PathBuf>,
    diff: Option<PathBuf>,
    diff_out: Option<PathBuf>,
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
    if let Some(out) = &args.out {
        std::fs::write(out, bits_to_png(&bits)?)?;
        println!("wrote {} ({}x{})", out.display(), bridge_render::WIDTH, bridge_render::HEIGHT);
    }
    if let Some(reference) = &args.diff {
        let reference_png = std::fs::read(reference)?;
        let reference_bits = png_to_bits(&reference_png)?;
        let mut diff = 0usize;
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
        for y in 0..bridge_render::HEIGHT as usize {
            for x in 0..bridge_render::WIDTH as usize {
                let idx = y * bridge_render::ROW_BYTES + x / 8;
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
            let mut data = vec![0u8; (bridge_render::WIDTH * bridge_render::HEIGHT * 3) as usize];
            for y in 0..bridge_render::HEIGHT as usize {
                for x in 0..bridge_render::WIDTH as usize {
                    let idx = y * bridge_render::ROW_BYTES + x / 8;
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
                    let base = (y * bridge_render::WIDTH as usize + x) * 3;
                    data[base..base + 3].copy_from_slice(&px);
                }
            }
            let mut out = Vec::new();
            {
                let mut encoder =
                    png::Encoder::new(&mut out, bridge_render::WIDTH, bridge_render::HEIGHT);
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
