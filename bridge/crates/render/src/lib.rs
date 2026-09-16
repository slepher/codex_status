//! Host build of the firmware template engine (same sources, same fonts, same
//! Paint raster), used for pixel-accurate template previews.

use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::sync::mpsc::{sync_channel, Receiver, Sender, SyncSender};
use std::sync::OnceLock;

/// The firmware engine draws into a single global Paint buffer, so all render
/// jobs go through one dedicated worker thread (a serialized engine queue)
/// instead of racing on a shared lock from many caller threads.
struct Job {
    template: String,
    usage: String,
    channel: String,
    ip: String,
    sync_hhmm: String,
    battery: i32,
    reply: Sender<(i32, Vec<u8>)>,
}

static ENGINE_TX: OnceLock<SyncSender<Job>> = OnceLock::new();

fn engine() -> &'static SyncSender<Job> {
    ENGINE_TX.get_or_init(|| {
        let (tx, rx) = sync_channel::<Job>(64);
        std::thread::Builder::new()
            .name("bridge-render".to_string())
            .spawn(move || engine_loop(rx))
            .expect("spawn render worker");
        tx
    })
}

fn engine_loop(rx: Receiver<Job>) {
    for job in rx {
        let mut out = vec![0u8; BUF_LEN];
        let rc = render_job(&job, &mut out);
        let _ = job.reply.send((rc, out));
    }
}

fn render_job(job: &Job, out: &mut [u8]) -> i32 {
    let tmpl = match CString::new(job.template.as_str()) {
        Ok(value) => value,
        Err(_) => return -3,
    };
    let usage = match CString::new(job.usage.as_str()) {
        Ok(value) => value,
        Err(_) => return -3,
    };
    let channel = match CString::new(job.channel.as_str()) {
        Ok(value) => value,
        Err(_) => return -3,
    };
    let ip = match CString::new(job.ip.as_str()) {
        Ok(value) => value,
        Err(_) => return -3,
    };
    let sync = match CString::new(job.sync_hhmm.as_str()) {
        Ok(value) => value,
        Err(_) => return -3,
    };
    unsafe {
        codex_render(
            tmpl.as_ptr(),
            usage.as_ptr(),
            channel.as_ptr(),
            ip.as_ptr(),
            sync.as_ptr(),
            job.battery,
            out.as_mut_ptr(),
            BUF_LEN as c_int,
        )
    }
}

pub const WIDTH: u32 = 200;
pub const HEIGHT: u32 = 200;
/// Preview fallback for values the template cannot obtain (the device reports
/// the real battery itself); keeps previews looking like a live screen.
pub const DEFAULT_BATTERY: i32 = 75;
pub const ROW_BYTES: usize = (WIDTH as usize + 7) / 8;
pub const BUF_LEN: usize = ROW_BYTES * HEIGHT as usize;

extern "C" {
    fn codex_render(
        tmpl: *const c_char,
        usage: *const c_char,
        channel: *const c_char,
        ip: *const c_char,
        sync_hhmm: *const c_char,
        battery: c_int,
        out: *mut u8,
        out_len: c_int,
    ) -> c_int;
    fn codex_validate(tmpl: *const c_char, err: *mut c_char, err_len: c_int) -> c_int;
}

pub struct Env<'a> {
    pub channel: &'a str,
    pub ip: &'a str,
    pub sync_hhmm: &'a str,
    pub battery: i32,
}

impl Default for Env<'_> {
    fn default() -> Self {
        Self {
            channel: "WIFI",
            ip: "0.0.0.0",
            sync_hhmm: "--:--",
            battery: -1,
        }
    }
}

/// 1-bit raster in the device framebuffer layout (bit set = white, clear = ink).
pub fn render_bits(template: &str, usage: &str, env: &Env<'_>) -> anyhow::Result<Vec<u8>> {
    let (reply_tx, reply_rx) = std::sync::mpsc::channel();
    let job = Job {
        template: template.to_string(),
        usage: usage.to_string(),
        channel: env.channel.to_string(),
        ip: env.ip.to_string(),
        sync_hhmm: env.sync_hhmm.to_string(),
        battery: env.battery,
        reply: reply_tx,
    };
    engine()
        .send(job)
        .map_err(|_| anyhow::anyhow!("render queue stopped"))?;
    let (rc, out) = reply_rx
        .recv()
        .map_err(|_| anyhow::anyhow!("render worker dropped the job"))?;
    match rc {
        1 => Ok(out),
        0 => anyhow::bail!("template rejected by the firmware engine"),
        other => anyhow::bail!("render failed (rc={other})"),
    }
}

pub fn validate(template: &str) -> Result<(), String> {
    let tmpl = CString::new(template).map_err(|e| e.to_string())?;
    let mut err = vec![0i8; 128];
    let ok = unsafe { codex_validate(tmpl.as_ptr(), err.as_mut_ptr(), err.len() as c_int) };
    if ok == 1 {
        return Ok(());
    }
    let bytes: Vec<u8> = err
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    Err(String::from_utf8_lossy(&bytes).to_string())
}

/// 8-bit grayscale PNG of the raster (white = 255, ink = 0).
pub fn bits_to_png(bits: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        let mut data = vec![0u8; (WIDTH * HEIGHT) as usize];
        for y in 0..HEIGHT as usize {
            for x in 0..WIDTH as usize {
                let byte = bits[y * ROW_BYTES + x / 8];
                let white = byte & (0x80 >> (x % 8)) != 0;
                data[y * WIDTH as usize + x] = if white { 255 } else { 0 };
            }
        }
        writer.write_image_data(&data)?;
    }
    Ok(out)
}

/// Decode a reference PNG (grayscale or RGB, 8-bit) to the same bit layout.
pub fn png_to_bits(png_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1usize,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => anyhow::bail!("unsupported reference png color type: {other:?}"),
    };
    let mut bits = vec![0u8; BUF_LEN];
    for y in 0..HEIGHT as usize {
        for x in 0..WIDTH as usize {
            let idx = (y * WIDTH as usize + x) * channels;
            let luma = buf[idx];
            if luma >= 128 {
                bits[y * ROW_BYTES + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    Ok(bits)
}
