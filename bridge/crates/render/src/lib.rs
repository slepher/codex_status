//! Host build of the firmware template engine (same sources, same fonts, same
//! Paint raster), used for pixel-accurate template previews.

use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::sync::mpsc::{sync_channel, Receiver, Sender, SyncSender};
use std::sync::OnceLock;

/// The firmware engine draws into a single global Paint buffer, so all render
/// jobs go through one dedicated worker thread (a serialized engine queue)
/// instead of racing on a shared lock from many caller threads.
/// Engine operations are serialized on one worker because the firmware engine
/// has one global Paint buffer and one global compiled template.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    RenderJson,
    Compile,
    RenderCompiled,
    Serialize,
    Deserialize,
    ReqList,
}

struct Job {
    op: Op,
    template: String,
    usage: String,
    channel: String,
    ip: String,
    sync_hhmm: String,
    battery: i32,
    state: String,
    offline_mins: i32,
    mode: String,
    blob: Vec<u8>,
    reply: Sender<(i32, Vec<u8>, String)>,
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
        // Large enough for the serialized compiled record; render ops truncate.
        let mut out = vec![0u8; 64 * 1024];
        let mut message = String::new();
        let rc = run_job(&job, &mut out, &mut message);
        if matches!(job.op, Op::RenderJson | Op::RenderCompiled) {
            out.truncate(BUF_LEN);
        } else if rc > 0 && (rc as usize) <= out.len() {
            out.truncate(rc as usize);
        }
        let _ = job.reply.send((rc, out, message));
    }
}

fn run_job(job: &Job, out: &mut [u8], message: &mut String) -> i32 {
    match job.op {
        Op::Compile => {
            let tmpl = match CString::new(job.template.as_str()) {
                Ok(value) => value,
                Err(_) => return -3,
            };
            let mut err = vec![0i8; 128];
            let ok = unsafe { codex_compile(tmpl.as_ptr(), err.as_mut_ptr(), err.len() as c_int) };
            if ok != 1 {
                *message = cstr(&err);
            }
            return ok;
        }
        Op::Serialize => {
            let mut blob = vec![0u8; 64 * 1024];
            let n = unsafe { codex_ct_serialize(blob.as_mut_ptr(), blob.len() as c_int) };
            if n <= 0 {
                return -1;
            }
            blob.truncate(n as usize);
            out[..blob.len()].copy_from_slice(&blob);
            return n;
        }
        Op::Deserialize => {
            let mut err = vec![0i8; 128];
            let ok = unsafe {
                codex_ct_deserialize(
                    job.blob.as_ptr(),
                    job.blob.len() as c_int,
                    err.as_mut_ptr(),
                    err.len() as c_int,
                )
            };
            if ok != 1 {
                *message = cstr(&err);
            }
            return ok;
        }
        Op::ReqList => {
            let count = unsafe { codex_ct_req_count() };
            if count < 0 {
                return -1;
            }
            let mut text = String::new();
            for i in 0..count {
                let mut buf = vec![0i8; 96];
                let ok = unsafe { codex_ct_req_path(i, buf.as_mut_ptr(), buf.len() as c_int) };
                if ok != 1 {
                    return -1;
                }
                if i > 0 {
                    text.push('\n');
                }
                text.push_str(&cstr(&buf));
            }
            let bytes = text.into_bytes();
            out[..bytes.len()].copy_from_slice(&bytes);
            return bytes.len() as i32;
        }
        Op::RenderCompiled | Op::RenderJson => {}
    }
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
    let state = match CString::new(job.state.as_str()) {
        Ok(value) => value,
        Err(_) => return -3,
    };
    let mode = match CString::new(job.mode.as_str()) {
        Ok(value) => value,
        Err(_) => return -3,
    };
    if job.op == Op::RenderCompiled {
        return unsafe {
            codex_render_compiled(
                usage.as_ptr(),
                channel.as_ptr(),
                ip.as_ptr(),
                sync.as_ptr(),
                job.battery,
                state.as_ptr(),
                job.offline_mins,
                mode.as_ptr(),
                out.as_mut_ptr(),
                BUF_LEN as c_int,
            )
        };
    }
    let tmpl = match CString::new(job.template.as_str()) {
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
            state.as_ptr(),
            job.offline_mins,
            mode.as_ptr(),
            out.as_mut_ptr(),
            BUF_LEN as c_int,
        )
    }
}

fn cstr(buf: &[i8]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}

fn run_engine(op: Op, template: &str, job: Job) -> anyhow::Result<(i32, Vec<u8>, String)> {
    let (reply_tx, reply_rx) = std::sync::mpsc::channel();
    let mut job = job;
    job.op = op;
    job.template = template.to_string();
    job.reply = reply_tx;
    engine()
        .send(job)
        .map_err(|_| anyhow::anyhow!("render queue stopped"))?;
    reply_rx
        .recv()
        .map_err(|_| anyhow::anyhow!("render worker dropped the job"))
}

fn base_job(template: &str, env: &Env<'_>) -> Job {
    Job {
        op: Op::RenderJson,
        template: template.to_string(),
        usage: String::new(),
        channel: env.channel.to_string(),
        ip: env.ip.to_string(),
        sync_hhmm: env.sync_hhmm.to_string(),
        battery: env.battery,
        state: env.state.to_string(),
        offline_mins: env.offline_mins,
        mode: env.mode.to_string(),
        blob: Vec::new(),
        reply: std::sync::mpsc::channel().0,
    }
}

/// Compile a template into the engine's active compiled record (one parse).
pub fn compile(template: &str) -> Result<(), String> {
    let job = base_job(template, &Env::default());
    let (rc, _, message) = run_engine(Op::Compile, template, job).map_err(|e| e.to_string())?;
    if rc == 1 {
        Ok(())
    } else {
        Err(message)
    }
}

/// Requirements (field paths, index order) of the active compiled template.
pub fn compiled_requirements() -> Result<Vec<String>, String> {
    let job = base_job("", &Env::default());
    let (rc, out, message) = run_engine(Op::ReqList, "", job).map_err(|e| e.to_string())?;
    if rc < 0 {
        return Err(message);
    }
    let text = String::from_utf8_lossy(&out[..rc as usize]).to_string();
    Ok(text.split('\n').map(|s| s.to_string()).collect())
}

pub fn compiled_source_crc() -> u32 {
    unsafe { codex_ct_source_crc() as u32 }
}

/// Render using the compiled template (no template JSON parsing).
pub fn render_compiled_bits(usage: &str, env: &Env<'_>) -> anyhow::Result<Vec<u8>> {
    let mut job = base_job("", env);
    job.usage = usage.to_string();
    let (rc, out, message) = run_engine(Op::RenderCompiled, "", job)?;
    match rc {
        1 => Ok(out),
        0 => anyhow::bail!("compiled template rejected the render"),
        other => anyhow::bail!("compiled render failed (rc={other}) {message}"),
    }
}

/// Serialize the active compiled template (fixed layout, ABI checked).
pub fn compiled_serialize() -> anyhow::Result<Vec<u8>> {
    let job = base_job("", &Env::default());
    let (rc, out, message) = run_engine(Op::Serialize, "", job)?;
    if rc <= 0 {
        anyhow::bail!("compiled serialize failed {message}");
    }
    Ok(out[..rc as usize].to_vec())
}

/// Load a compiled template from its fixed layout (validates ABI/CRC/bounds).
pub fn compiled_deserialize(blob: &[u8]) -> Result<(), String> {
    let mut job = base_job("", &Env::default());
    job.blob = blob.to_vec();
    let (rc, _, message) = run_engine(Op::Deserialize, "", job).map_err(|e| e.to_string())?;
    if rc == 1 {
        Ok(())
    } else {
        Err(message)
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
        state: *const c_char,
        offline_mins: c_int,
        mode: *const c_char,
        out: *mut u8,
        out_len: c_int,
    ) -> c_int;
    fn codex_validate(tmpl: *const c_char, err: *mut c_char, err_len: c_int) -> c_int;
    fn codex_compile(tmpl: *const c_char, err: *mut c_char, err_len: c_int) -> c_int;
    fn codex_render_compiled(
        usage: *const c_char,
        channel: *const c_char,
        ip: *const c_char,
        sync_hhmm: *const c_char,
        battery: c_int,
        state: *const c_char,
        offline_mins: c_int,
        mode: *const c_char,
        out: *mut u8,
        out_len: c_int,
    ) -> c_int;
    fn codex_ct_req_count() -> c_int;
    fn codex_ct_req_path(i: c_int, out: *mut c_char, cap: c_int) -> c_int;
    fn codex_ct_source_crc() -> c_int;
    fn codex_ct_serialize(out: *mut u8, cap: c_int) -> c_int;
    fn codex_ct_deserialize(blob: *const u8, len: c_int, err: *mut c_char, err_len: c_int) -> c_int;
    fn codex_rgn_build(tmpl: *const c_char) -> c_int;
    fn codex_rgn_build_ct(blob: *const u8, len: c_int, err: *mut c_char, errcap: c_int) -> c_int;
    fn codex_rgn_decide(
        old_fb: *const u8,
        new_fb: *const u8,
        trusted: c_int,
        force_full: c_int,
        clean: c_int,
        out: *mut c_char,
        out_len: c_int,
    ) -> c_int;
    fn codex_rgn_on_partial() -> c_int;
    fn codex_rgn_on_full() -> c_int;
    fn codex_rgn_dump(out: *mut c_char, out_len: c_int) -> c_int;
}

/// Derive the semantic refresh regions for a template (display-safety layer).
/// Callers must serialize all `rgn_*` calls (the policy state is global, like
/// the firmware's).
pub fn rgn_build(template: &str) -> Result<usize, String> {
    let tmpl = CString::new(template).map_err(|e| e.to_string())?;
    let n = unsafe { codex_rgn_build(tmpl.as_ptr()) };
    if n < 1 {
        return Err("region derivation failed".to_string());
    }
    Ok(n as usize)
}

/// Policy decision for a frame pair. Returns the action (0 none, 1 partial,
/// 2 full) and the JSON detail from the C++ side.
pub fn rgn_decide(
    old_fb: &[u8],
    new_fb: &[u8],
    trusted: bool,
    force_full: bool,
    clean: bool,
) -> Result<(i32, String), String> {
    if old_fb.len() != BUF_LEN || new_fb.len() != BUF_LEN {
        return Err(format!("framebuffer must be {BUF_LEN} bytes"));
    }
    let mut detail = vec![0i8; 192];
    let action = unsafe {
        codex_rgn_decide(
            old_fb.as_ptr(),
            new_fb.as_ptr(),
            trusted as c_int,
            force_full as c_int,
            clean as c_int,
            detail.as_mut_ptr(),
            detail.len() as c_int,
        )
    };
    let bytes: Vec<u8> = detail
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    Ok((action, String::from_utf8_lossy(&bytes).to_string()))
}

pub fn rgn_on_partial() {
    unsafe { codex_rgn_on_partial() };
}

pub fn rgn_on_full() {
    unsafe { codex_rgn_on_full() };
}

/// Derive refresh regions from the compiled record (activation path).
pub fn rgn_build_compiled(blob: &[u8]) -> Result<usize, String> {
    let mut err = vec![0i8; 128];
    let n = unsafe {
        codex_rgn_build_ct(
            blob.as_ptr(),
            blob.len() as c_int,
            err.as_mut_ptr(),
            err.len() as c_int,
        )
    };
    if n < 0 {
        return Err(cstr(&err));
    }
    Ok(n as usize)
}

pub fn rgn_dump() -> String {
    let mut out = vec![0i8; 2048];
    unsafe { codex_rgn_dump(out.as_mut_ptr(), out.len() as c_int) };
    let bytes: Vec<u8> = out
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}

pub struct Env<'a> {
    pub channel: &'a str,
    pub ip: &'a str,
    pub sync_hhmm: &'a str,
    pub battery: i32,
    /// Device state word: `AP` / `BLE ON` / `BLE OFF` / `WIFI OFF` / `DEEP`.
    pub state: &'a str,
    /// Minutes since the last successful sync; negative = unknown.
    pub offline_mins: i32,
    /// Device mode word: `deep` / `light` (bind `device.mode`, v0.14).
    pub mode: &'a str,
}

impl Default for Env<'_> {
    fn default() -> Self {
        Self {
            channel: "WIFI",
            ip: "0.0.0.0",
            sync_hhmm: "--:--",
            battery: -1,
            state: "BLE OFF",
            offline_mins: -1,
            mode: "light",
        }
    }
}

/// 1-bit raster in the device framebuffer layout (bit set = white, clear = ink).
pub fn render_bits(template: &str, usage: &str, env: &Env<'_>) -> anyhow::Result<Vec<u8>> {
    let mut job = base_job(template, env);
    job.usage = usage.to_string();
    let (rc, out, message) = run_engine(Op::RenderJson, template, job)?;
    match rc {
        1 => Ok(out),
        0 => anyhow::bail!("template rejected by the firmware engine {message}"),
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
