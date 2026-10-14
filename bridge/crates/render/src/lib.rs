//! Host build of the firmware template engine (same sources, same fonts, same
//! Paint raster), used for pixel-accurate template previews.

use std::ffi::CString;
mod status_pages;
pub mod bundle_codec;
pub use status_pages::SimulatorStatusPages;
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
    Validate,
    Compile,
    RenderCompiled,
    Serialize,
    Deserialize,
    ReqList,
    Artifact,
}

struct Job {
    op: Op,
    template: String,
    usage: String,
    channel: String,
    ip: String,
    sync_hhmm: String,
    battery: i32,
    wifi_state: String,
    wifi_blink: i32,
    ble_manual_active: bool,
    offline_mins: i32,
    mode: String,
    blob: Vec<u8>,
    fonts: Option<Vec<Vec<u8>>>,
    width: u32,
    height: u32,
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
        if let Some(fonts) = &job.fonts {
            unsafe { codex_font_clear_assets() };
            if !fonts.iter().all(|font| unsafe { codex_font_bind_asset(font.as_ptr(), font.len() as c_int) == 1 }) {
                unsafe { codex_font_clear_assets() };
                let _ = job.reply.send((-1, Vec::new(), "font asset cannot bind to template font slot".to_string()));
                continue;
            }
        }
        // Large enough for the serialized compiled record; render ops truncate.
        let mut out = vec![0u8; 64 * 1024];
        let mut message = String::new();
        let rc = run_job(&job, &mut out, &mut message);
        if job.fonts.is_some() { unsafe { codex_font_clear_assets() } }
        unsafe { codex_set_canvas(WIDTH as c_int, HEIGHT as c_int) };
        if matches!(job.op, Op::RenderJson | Op::RenderCompiled) {
            out.truncate(((job.width + 7) / 8 * job.height) as usize);
        } else if rc > 0 && (rc as usize) <= out.len() {
            out.truncate(rc as usize);
        }
        let _ = job.reply.send((rc, out, message));
    }
}

fn run_job(job: &Job, out: &mut [u8], message: &mut String) -> i32 {
    unsafe { codex_set_canvas(job.width as c_int, job.height as c_int) };
    match job.op {
        Op::Validate => {
            let tmpl = match CString::new(job.template.as_str()) {
                Ok(value) => value,
                Err(_) => return -3,
            };
            let mut err = vec![0i8; 128];
            let ok = unsafe { codex_validate(tmpl.as_ptr(), err.as_mut_ptr(), err.len() as c_int) };
            if ok != 1 { *message = cstr(&err); }
            return ok;
        }
        Op::Artifact => {
            let source = match CString::new(job.template.as_str()) {
                Ok(value) => value,
                Err(_) => return -3,
            };
            let mut meta = vec![0i8; 16384];
            let result = unsafe { codex_ct_artifact(
                if job.template.is_empty() { std::ptr::null() } else { source.as_ptr() },
                job.blob.as_ptr(), job.blob.len() as c_int,
                out.as_mut_ptr(), out.len() as c_int, meta.as_mut_ptr(), meta.len() as c_int) };
            *message = cstr(&meta);
            return result;
        }
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
    let wifi_state = match CString::new(job.wifi_state.as_str()) {
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
                wifi_state.as_ptr(),
                job.wifi_blink,
                i32::from(job.ble_manual_active),
                job.offline_mins,
                mode.as_ptr(),
                out.as_mut_ptr(),
                (((job.width + 7) / 8) * job.height) as c_int,
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
            wifi_state.as_ptr(),
            job.wifi_blink,
            i32::from(job.ble_manual_active),
            job.offline_mins,
            mode.as_ptr(),
            out.as_mut_ptr(),
            (((job.width + 7) / 8) * job.height) as c_int,
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
    let (width, height) = canvas_size(template).unwrap_or((WIDTH, HEIGHT));
    Job {
        op: Op::RenderJson,
        template: template.to_string(),
        usage: String::new(),
        channel: env.channel.to_string(),
        ip: env.ip.to_string(),
        sync_hhmm: env.sync_hhmm.to_string(),
        battery: env.battery,
        wifi_state: env.wifi_state.to_string(),
        wifi_blink: env.wifi_blink,
        ble_manual_active: env.ble_manual_active,
        offline_mins: env.offline_mins,
        mode: env.mode.to_string(),
        blob: Vec::new(),
        fonts: None,
        width,
        height,
        reply: std::sync::mpsc::channel().0,
    }
}

pub fn canvas_size(template: &str) -> Option<(u32, u32)> {
    let source: serde_json::Value = serde_json::from_str(template).ok()?;
    let size = (
        source.get("canvas")?.get("w")?.as_u64()?,
        source.get("canvas")?.get("h")?.as_u64()?,
    );
    match size {
        (200, 200) | (400, 300) => Some((size.0 as u32, size.1 as u32)),
        _ => None,
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

/// Shared C++ wire compiler/validator. Returns binary CTP1 bytes and metadata
/// obtained from that same record in one serialized engine operation.
pub fn compiled_artifact(source: Option<&str>, blob: &[u8]) -> anyhow::Result<(Vec<u8>, String)> {
    compiled_artifact_with_canvas(source, blob, WIDTH, HEIGHT)
}

pub fn compiled_artifact_with_canvas(
    source: Option<&str>,
    blob: &[u8],
    width: u32,
    height: u32,
) -> anyhow::Result<(Vec<u8>, String)> {
    let source = source.unwrap_or("");
    let mut job = base_job(source, &Env::default());
    job.width = width;
    job.height = height;
    job.blob = blob.to_vec();
    let (rc, out, metadata) = run_engine(Op::Artifact, source, job)?;
    if rc <= 0 {
        anyhow::bail!("compiled artifact: {metadata}");
    }
    Ok((out, metadata))
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
    render_compiled_bits_size(usage, env, WIDTH, HEIGHT)
}

/// Canvas-aware compiled render. The engine keeps one global compiled record,
/// so the caller must pass the canvas that record was compiled for; otherwise
/// ops fall outside the 200x200 default frame buffer.
pub fn render_compiled_bits_size(
    usage: &str,
    env: &Env<'_>,
    width: u32,
    height: u32,
) -> anyhow::Result<Vec<u8>> {
    let mut job = base_job("", env);
    job.width = width;
    job.height = height;
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
    #[cfg(test)]
    fn codex_rv_window_until(now:u64, light:u64, sync:u64)->u64;
    fn codex_rv_new() -> *mut std::ffi::c_void;
    fn codex_rv_free(p:*mut std::ffi::c_void);
    fn codex_rv_begin(p:*mut std::ffi::c_void,now:u64,advertise_ms:u32);
    fn codex_rv_connected(p:*mut std::ffi::c_void,now:u64);
    fn codex_rv_plan(p:*mut std::ffi::c_void);
    fn codex_rv_await_network(p:*mut std::ffi::c_void,now:u64,legal_until:u64);
    fn codex_rv_admit_http(p:*mut std::ffi::c_void,now:u64,legal_until:u64);
    fn codex_rv_http_ready(p:*mut std::ffi::c_void);
    fn codex_rv_phase(p:*mut std::ffi::c_void)->i32;
    fn codex_rv_deadline(p:*mut std::ffi::c_void)->u64;
    fn codex_rv_tick(p:*mut std::ffi::c_void,now:u64)->i32;
    fn codex_sim_sync_append_wake(p: *mut std::ffi::c_void, json: *const c_char) -> c_int;
    fn codex_sim_sync_history(p: *mut std::ffi::c_void, since: u32, limit: u32, out: *mut c_char, cap: c_int) -> c_int;
    fn codex_sim_sync_log_context(p: *mut std::ffi::c_void, utc: u64, connected: c_int) -> c_int;
    fn codex_sim_sync_archive(p: *mut std::ffi::c_void, utc: u64, hour: u64, offset: u32, serial: u64, out: *mut c_char, cap: c_int) -> c_int;

    fn codex_sim_wifi_legacy(json: *const c_char);
    fn codex_wifi_convert_storage(input:*mut u8,size:c_int,output:*mut u8,capacity:c_int)->c_int;
    fn codex_sim_wifi_open(dir: *const c_char) -> c_int;
    fn codex_sim_wifi_control(input:*const c_char,principal:*const c_char,name_space:*const c_char,generation:u32,
        now:u64,until:u64,physical:u64,claimed:c_int,session_valid:c_int,out:*mut c_char,cap:c_int)->c_int;
    fn codex_sim_wifi_read_page(input:*const c_char,peer:*const c_char,generation:u32,text:*const c_char,out:*mut c_char,cap:c_int)->c_int;
    fn codex_sim_wifi_poll(now: u64, peer: *const c_char, generation: u32, encrypted: c_int, outcome: c_int, start_allowed: c_int) -> c_int;
    fn codex_sim_wifi_storage_failure(fail: c_int);
    fn codex_sim_wifi_constrain(now:u64,until:u64,owner_allowed:c_int);
    fn codex_sim_wifi_storage_status(out:*mut c_char,cap:c_int)->c_int;
    fn codex_sim_wifi_observe(out:*mut c_char,cap:c_int)->c_int;

    #[cfg(test)]
    fn codex_sync_core_check() -> c_int;
    #[cfg(test)]
    fn codex_sync_store_check() -> c_int;
    fn codex_sim_sync_new(data_dir: *const c_char, mac: *const c_char,
        target: *const c_char, seed: u64, boot_id: u64,
        wake_cause: *const c_char) -> *mut std::ffi::c_void;
    fn codex_sim_sync_free(p: *mut std::ffi::c_void);
    fn codex_sim_sync_set_owner(p: *mut std::ffi::c_void,
        owner: *const c_char, enabled: c_int) -> c_int;
    fn codex_sim_sync_round(p: *mut std::ffi::c_void, wake_seq: u32,
        uptime_ms: u32) -> c_int;
    fn codex_sim_sync_reason(p: *mut std::ffi::c_void, bit: c_int,
        uptime_ms: u32) -> c_int;
    fn codex_sim_sync_append_text(p: *mut std::ffi::c_void,
        text: *const c_char, uptime_ms: u32) -> c_int;
    fn codex_sim_sync_corrupt(p: *mut std::ffi::c_void,
        header: c_int, seq: u64) -> c_int;
    fn codex_sim_sync_fail(p: *mut std::ffi::c_void, uptime_ms: u32) -> c_int;
    fn codex_sim_sync_consume_skip(p: *mut std::ffi::c_void) -> c_int;
    fn codex_sim_sync_status(p: *mut std::ffi::c_void,
        out: *mut c_char, cap: c_int) -> c_int;
    fn codex_sim_sync_command(p: *mut std::ffi::c_void,
        operation: *const c_char, message: *const c_char,
        snapshot: *const c_char, out: *mut c_char, cap: c_int) -> c_int;
    fn codex_sim_bundle_new(data_dir: *const c_char, target: *const c_char,
        boot_id: u64, wake_cause: *const c_char) -> *mut std::ffi::c_void;
    fn codex_sim_bundle_free(p: *mut std::ffi::c_void);
    fn codex_sim_bundle_set_wall(p: *mut std::ffi::c_void, epoch_secs: i64) -> c_int;
    fn codex_sim_bundle_display(p: *mut std::ffi::c_void, deep: c_int, wifi_up: c_int, contact: c_int) -> c_int;
    fn codex_sim_bundle_status(p: *mut std::ffi::c_void, out: *mut c_char, cap: c_int) -> c_int;
    fn codex_sim_bundle_frame(p: *mut std::ffi::c_void, out: *mut u8, cap: c_int) -> c_int;
    fn codex_sim_store_budget(p: *mut std::ffi::c_void, budget: i64) -> c_int;
    fn codex_sim_store_crash_after_sync(p: *mut std::ffi::c_void,
        kind: *const c_char, count: c_int) -> c_int;
    fn codex_sim_display_fail_next(p: *mut std::ffi::c_void) -> c_int;
    fn codex_sim_button_next(p: *mut std::ffi::c_void, now_ms: u64,
        context: *const c_char, out: *mut c_char, cap: c_int) -> c_int;
    fn codex_sim_bundle_begin(p: *mut std::ffi::c_void, message: *const c_char,
        nonce: *const c_char, now_ms: u64, out: *mut c_char, cap: c_int) -> c_int;
    fn codex_sim_bundle_chunk(p: *mut std::ffi::c_void, request: *const c_char,
        nonce: *const c_char, offset: *const c_char, body: *const u8, len: c_int,
        now_ms: u64, out: *mut c_char, cap: c_int) -> c_int;
    fn codex_sim_bundle_commit(p: *mut std::ffi::c_void, message: *const c_char,
        nonce: *const c_char, now_ms: u64, context: *const c_char,
        out: *mut c_char, cap: c_int) -> c_int;
    fn codex_sim_data(p: *mut std::ffi::c_void, message: *const c_char,
        out: *mut c_char, cap: c_int) -> c_int;
    fn codex_sim_activate(p: *mut std::ffi::c_void, message: *const c_char,
        now_ms: u64, context: *const c_char, out: *mut c_char, cap: c_int) -> c_int;
    fn codex_set_canvas(w: c_int, h: c_int);
    fn codex_ct_artifact(source: *const c_char, blob: *const u8, len: c_int,
        out: *mut u8, cap: c_int, metadata: *mut c_char, meta_cap: c_int) -> c_int;
    fn codex_render(
        tmpl: *const c_char,
        usage: *const c_char,
        channel: *const c_char,
        ip: *const c_char,
        sync_hhmm: *const c_char,
        battery: c_int,
        wifi_state: *const c_char,
        wifi_blink: c_int,
        ble_manual: c_int,
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
        wifi_state: *const c_char,
        wifi_blink: c_int,
        ble_manual: c_int,
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
    fn codex_set_panel(w: c_int, h: c_int);
    fn codex_font_asset_check(bytes: *const u8, len: c_int, out: *mut c_char,
        cap: c_int) -> c_int;
    fn codex_font_bind_asset(bytes: *const u8, len: c_int) -> c_int;
    fn codex_font_clear_assets();
    fn codex_display_state_after_render(writes_before: u32, writes_after: u32,
        busy_before: u32, busy_after: u32) -> c_int;
    fn codex_font_store_reset();
    fn codex_font_store_budget(budget: i64);
    fn codex_font_store_begin(profile: *const c_char) -> c_int;
    fn codex_font_store_write(bytes: *const u8, len: c_int, out: *mut c_char,
        cap: c_int) -> c_int;
    fn codex_font_store_inventory(out: *mut c_char, cap: c_int) -> c_int;
    fn codex_font_store_load(id: *const c_char, out: *mut c_char, cap: c_int) -> c_int;
    fn codex_font_store_prune(ids: *const c_char, out: *mut c_char, cap: c_int) -> c_int;
    fn codex_font_store_clear() -> c_int;
    fn codex_font_store_usage(out: *mut c_char, cap: c_int) -> c_int;
    fn codex_font_store_put_raw(id: *const c_char, bytes: *const u8, len: c_int) -> c_int;
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
    fn codex_ble_wire(kind: *const c_char, input: *const c_char, out: *mut c_char, cap: c_int) -> c_int;
    fn codex_compact_frame_golden(out: *mut u8, cap: c_int) -> c_int;
    fn codex_v2_status_snapshot(input: *const c_char, out: *mut c_char, cap: c_int) -> c_int;
    fn codex_v2_plan_new() -> *mut std::ffi::c_void;
    fn codex_v2_plan_free(p: *mut std::ffi::c_void);
    fn codex_v2_plan_decide(
        p: *mut std::ffi::c_void,
        message: *const c_char,
        now_ms: u64,
        provisional: c_int,
        out: *mut c_char,
        cap: c_int,
    ) -> c_int;
    fn codex_v2_power_sleep_decide(p: *mut std::ffi::c_void, configured: c_int,
        light_mode: c_int, plugged: c_int, deep_on_usb: c_int, manual_hold: c_int,
        provisional: c_int, boot_ms: u64, safety_deadline_ms: u64, now_ms: u64) -> c_int;
    fn codex_v2_battery_power_off(plugged: c_int, battery_pct: c_int) -> c_int;
    fn codex_v2_command_parse(message: *const c_char, out: *mut c_char, cap: c_int) -> c_int;
    fn codex_v2_command_check(
        message: *const c_char,
        current_mac: *const c_char,
        nonce: *const c_char,
        out: *mut c_char,
        cap: c_int,
    ) -> c_int;
    fn codex_v2_build_ack(
        op: *const c_char,
        result: *const c_char,
        display: *const c_char,
        retention: *const c_char,
        error: *const c_char,
        seq: i64,
        plan_id: u64,
        context: *const c_char,
        accepted_remaining_s: u32,
        fw_target: *const c_char,
        out: *mut c_char,
        cap: c_int,
    ) -> c_int;
    fn codex_v2_claim_decide(
        message: *const c_char,
        have_owner: c_int,
        current_json: *const c_char,
        out: *mut c_char,
        cap: c_int,
    ) -> c_int;
}

#[cfg(test)]
#[test]
fn shared_sync_core_integrity() {
    assert_eq!(unsafe { codex_sync_core_check() }, 0);
    assert_eq!(unsafe { codex_sync_store_check() }, 0);
}

/// Host instance of the production diagnostic ring and frozen Flash store.
/// The host LittleFS shim is process global, so use one instance per process.
pub struct SimulatorSync { state: *mut std::ffi::c_void }
unsafe impl Send for SimulatorSync {}

impl SimulatorSync {
    pub fn new(data_dir: &std::path::Path, mac: &str, target: &str,
               seed: u64, boot_id: u64, wake_cause: &str) -> anyhow::Result<Self> {
        let dir = CString::new(data_dir.to_str().ok_or_else(||
            anyhow::anyhow!("simulator data directory is not UTF-8"))?)?;
        let mac = CString::new(mac)?;
        let target = CString::new(target)?;
        let wake = CString::new(wake_cause)?;
        let state = unsafe { codex_sim_sync_new(dir.as_ptr(), mac.as_ptr(),
            target.as_ptr(), seed, boot_id, wake.as_ptr()) };
        anyhow::ensure!(!state.is_null(), "simulator sync store unavailable");
        Ok(Self { state })
    }

    pub fn set_owner(&mut self, owner: &str, enabled: bool) -> anyhow::Result<()> {
        let owner = CString::new(owner)?;
        anyhow::ensure!(unsafe { codex_sim_sync_set_owner(self.state,
            owner.as_ptr(), enabled as c_int) } == 1, "sync owner persistence failed");
        Ok(())
    }

    pub fn deep_round(&mut self, wake_seq: u32, uptime_ms: u32) -> anyhow::Result<()> {
        anyhow::ensure!(unsafe { codex_sim_sync_round(self.state, wake_seq, uptime_ms) } == 1,
            "sync RTC persistence failed");
        Ok(())
    }

    pub fn add_reason(&mut self, bit: i32, uptime_ms: u32) -> anyhow::Result<()> {
        anyhow::ensure!(unsafe { codex_sim_sync_reason(self.state, bit, uptime_ms) } == 1,
            "sync RTC persistence failed");
        Ok(())
    }

    pub fn append_text(&mut self, text: &str, uptime_ms: u32) -> anyhow::Result<()> {
        let text = CString::new(text)?;
        anyhow::ensure!(unsafe { codex_sim_sync_append_text(self.state,
            text.as_ptr(), uptime_ms) } == 1, "sync diagnostic append failed");
        Ok(())
    }

    pub fn log_context(&mut self, utc: u64, connected: bool) -> anyhow::Result<()> {
        anyhow::ensure!(unsafe { codex_sim_sync_log_context(self.state,utc,connected as c_int) }==1,"log context failed");Ok(())
    }
    pub fn archive(&mut self, utc:u64, hour:u64, offset:u32, serial:u64) -> anyhow::Result<serde_json::Value> {
        let mut out=vec![0i8;8192];let rc=unsafe {codex_sim_sync_archive(self.state,utc,hour,offset,serial,out.as_mut_ptr(),out.len() as c_int)};
        simulator_ffi_json(rc,&out,"offline archive")
    }

    pub fn corrupt(&mut self, header: bool, seq: u64) -> anyhow::Result<()> {
        anyhow::ensure!(unsafe { codex_sim_sync_corrupt(self.state,
            header as c_int, seq) } == 1, "sync diagnostic corruption point unavailable");
        Ok(())
    }

    pub fn fail(&mut self, uptime_ms: u32) -> anyhow::Result<()> {
        anyhow::ensure!(unsafe { codex_sim_sync_fail(self.state, uptime_ms) } == 1,
            "sync failure could not be persisted");
        Ok(())
    }

    pub fn consume_skip(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!(unsafe { codex_sim_sync_consume_skip(self.state) } == 1,
            "sync skip could not be persisted");
        Ok(())
    }

    pub fn status(&self) -> anyhow::Result<serde_json::Value> {
        let mut out = vec![0i8; 8192];
        let rc = unsafe { codex_sim_sync_status(self.state, out.as_mut_ptr(), out.len() as c_int) };
        simulator_ffi_json(rc, &out, "sync status")
    }

    pub fn append_wake(&mut self, value: &serde_json::Value) -> anyhow::Result<()> {
        let text=CString::new(value.to_string())?;
        anyhow::ensure!(unsafe{codex_sim_sync_append_wake(self.state,text.as_ptr())}==1,"wake_fixture_failed");Ok(())
    }
    pub fn history(&self, since:u32, limit:u32) -> anyhow::Result<serde_json::Value> {
        let mut out=vec![0i8;16384];let rc=unsafe{codex_sim_sync_history(self.state,since,limit,out.as_mut_ptr(),out.len() as c_int)};
        simulator_ffi_json(rc,&out,"history")
    }
    pub fn command(&mut self, op: &str, message: &str,
                   snapshot: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let op = CString::new(op)?;
        let message = CString::new(message)?;
        let snapshot = CString::new(snapshot.to_string())?;
        let mut out = vec![0i8; 8192];
        let rc = unsafe { codex_sim_sync_command(self.state, op.as_ptr(), message.as_ptr(),
            snapshot.as_ptr(), out.as_mut_ptr(), out.len() as c_int) };
        simulator_ffi_json(rc, &out, "sync command")
    }
}

impl Drop for SimulatorSync {
    fn drop(&mut self) { unsafe { codex_sim_sync_free(self.state) } }
}

/// Executes the firmware Bundle decisions and A/B store in the simulator's
/// single C++ device context. One instance is allowed per process.
pub struct SimulatorBundle {
    state: *mut std::ffi::c_void,
}

unsafe impl Send for SimulatorBundle {}

impl SimulatorBundle {
    pub fn note_bridge_contact(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!(unsafe { codex_sim_bundle_display(self.state, -1, -1, 1) } == 1,
            "simulator contact unavailable");
        Ok(())
    }
    pub fn display_state(&mut self, deep: bool, wifi_up: bool, contact: bool) -> anyhow::Result<()> {
        anyhow::ensure!(unsafe { codex_sim_bundle_display(self.state, deep as c_int, wifi_up as c_int, contact as c_int) } == 1,
            "simulator display state unavailable");
        Ok(())
    }
    pub fn set_wall_secs(&mut self, epoch_secs: i64) -> anyhow::Result<()> {
        anyhow::ensure!(unsafe { codex_sim_bundle_set_wall(self.state, epoch_secs) } == 1,
            "simulator wall clock unavailable");
        Ok(())
    }

    pub fn new(data_dir: &std::path::Path, target: &str, boot_id: u64,
        wake_cause: &str) -> anyhow::Result<Self> {
        let data_dir = CString::new(data_dir.to_str().ok_or_else(||
            anyhow::anyhow!("simulator data directory is not UTF-8"))?)?;
        let target = CString::new(target)?;
        let wake_cause = CString::new(wake_cause)?;
        let state = unsafe { codex_sim_bundle_new(data_dir.as_ptr(), target.as_ptr(),
            boot_id, wake_cause.as_ptr()) };
        anyhow::ensure!(!state.is_null(), "simulator Bundle storage invalid or unavailable");
        Ok(Self { state })
    }

    fn result(&self, operation: &str, call: impl FnOnce(*mut c_char, c_int) -> c_int)
        -> anyhow::Result<serde_json::Value> {
        let mut out = vec![0i8; 8192];
        let rc = call(out.as_mut_ptr(), out.len() as c_int);
        simulator_ffi_json(rc, &out, operation)
    }

    pub fn status(&self) -> anyhow::Result<serde_json::Value> {
        self.result("bundle status", |out, cap| unsafe {
            codex_sim_bundle_status(self.state, out, cap)
        })
    }

    pub fn frame_bits(&self) -> anyhow::Result<Vec<u8>> {
        let mut bits = vec![0u8; 15_000];
        let len = unsafe { codex_sim_bundle_frame(self.state, bits.as_mut_ptr(), bits.len() as c_int) };
        anyhow::ensure!(len >= 0 && (len as usize) <= bits.len(), "simulator frame read failed");
        bits.truncate(len as usize);
        Ok(bits)
    }

    pub fn set_write_budget(&mut self, budget: i64) -> anyhow::Result<()> {
        anyhow::ensure!(unsafe { codex_sim_store_budget(self.state, budget) } == 1,
            "invalid simulator write budget");
        Ok(())
    }

    pub fn crash_after_sync(&mut self, kind: &str, count: i32) -> anyhow::Result<()> {
        let kind = CString::new(kind)?;
        anyhow::ensure!(unsafe { codex_sim_store_crash_after_sync(
            self.state, kind.as_ptr(), count) } == 1,
            "invalid simulator crash boundary");
        Ok(())
    }

    pub fn fail_next_display(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!(unsafe { codex_sim_display_fail_next(self.state) } == 1,
            "cannot arm simulator display failure");
        Ok(())
    }

    pub fn button_next(&mut self, now_ms: u64, context: &str)
        -> anyhow::Result<serde_json::Value> {
        let context = CString::new(context)?;
        self.result("button next", |out, cap| unsafe {
            codex_sim_button_next(self.state, now_ms, context.as_ptr(), out, cap)
        })
    }

    pub fn begin(&mut self, message: &str, nonce: &str, now_ms: u64)
        -> anyhow::Result<serde_json::Value> {
        let message = CString::new(message)?;
        let nonce = CString::new(nonce)?;
        self.result("bundle begin", |out, cap| unsafe {
            codex_sim_bundle_begin(self.state, message.as_ptr(), nonce.as_ptr(), now_ms, out, cap)
        })
    }

    pub fn chunk(&mut self, request: &str, nonce: &str, offset: &str, body: &[u8],
                 now_ms: u64) -> anyhow::Result<serde_json::Value> {
        let request = CString::new(request)?;
        let nonce = CString::new(nonce)?;
        let offset = CString::new(offset)?;
        anyhow::ensure!(body.len() <= i32::MAX as usize, "bundle chunk too large");
        self.result("bundle chunk", |out, cap| unsafe {
            codex_sim_bundle_chunk(self.state, request.as_ptr(), nonce.as_ptr(),
                offset.as_ptr(), body.as_ptr(), body.len() as c_int, now_ms, out, cap)
        })
    }

    pub fn commit(&mut self, message: &str, nonce: &str, now_ms: u64, context: &str)
        -> anyhow::Result<serde_json::Value> {
        let message = CString::new(message)?;
        let nonce = CString::new(nonce)?;
        let context = CString::new(context)?;
        self.result("bundle commit", |out, cap| unsafe {
            codex_sim_bundle_commit(self.state, message.as_ptr(), nonce.as_ptr(),
                now_ms, context.as_ptr(), out, cap)
        })
    }

    pub fn data(&mut self, message: &str) -> anyhow::Result<serde_json::Value> {
        let message = CString::new(message)?;
        self.result("data", |out, cap| unsafe {
            codex_sim_data(self.state, message.as_ptr(), out, cap)
        })
    }

    pub fn activate(&mut self, message: &str, now_ms: u64, context: &str)
        -> anyhow::Result<serde_json::Value> {
        let message = CString::new(message)?;
        let context = CString::new(context)?;
        self.result("activate", |out, cap| unsafe {
            codex_sim_activate(self.state, message.as_ptr(), now_ms, context.as_ptr(), out, cap)
        })
    }
}

impl Drop for SimulatorBundle {
    fn drop(&mut self) { unsafe { codex_sim_bundle_free(self.state) }; }
}

/// Cross-language fixed frame vector from the firmware encoder and decoder.
pub fn simulator_compact_frame_golden() -> anyhow::Result<[u8; 50]> {
    let mut out = [0; 50];
    let rc = unsafe { codex_compact_frame_golden(out.as_mut_ptr(), out.len() as c_int) };
    anyhow::ensure!(rc == 50, "firmware compact frame vector failed");
    Ok(out)
}

/// Build a BLE attribute using the SAME firmware encoder. Return its exact
/// serialized bytes; callers must not parse/serialize before storing them.
pub fn simulator_ble_wire(kind: &str, input: &serde_json::Value) -> anyhow::Result<Vec<u8>> {
    let kind = CString::new(kind)?;
    let input = CString::new(serde_json::to_string(input)?)?;
    let mut out = vec![0u8; 513];
    let rc = unsafe { codex_ble_wire(kind.as_ptr(), input.as_ptr(), out.as_mut_ptr().cast(), out.len() as c_int) };
    anyhow::ensure!(rc > 0 && rc <= 512, "BLE wire builder rejected frame ({rc})");
    out.truncate(rc as usize);
    Ok(out)
}

/// Build the simulator's `/api/status` payload with the firmware's C++ builder.
/// This only creates a status snapshot; it does not simulate command side effects.
pub fn simulator_status_snapshot(input: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let input = serde_json::to_string(input)?;
    anyhow::ensure!(input.len() <= 64 * 1024, "simulator status input too large");
    let input =
        CString::new(input).map_err(|_| anyhow::anyhow!("simulator status input contains NUL"))?;
    let mut out = vec![0i8; 8192];
    let rc =
        unsafe { codex_v2_status_snapshot(input.as_ptr(), out.as_mut_ptr(), out.len() as c_int) };
    anyhow::ensure!(
        rc > 0 && (rc as usize) < out.len(),
        "simulator status builder failed ({rc})"
    );
    let result: serde_json::Value = serde_json::from_str(&cstr(&out))?;
    result
        .get("status")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("simulator status result missing"))
}

/// Run the firmware's shared claim argument preparation and action decision.
pub fn simulator_claim_decision(
    message: &serde_json::Value,
    have_owner: bool,
    current: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let message = CString::new(serde_json::to_string(message)?)?;
    let current = CString::new(serde_json::to_string(current)?)?;
    let mut out = vec![0i8; 8192];
    let rc = unsafe {
        codex_v2_claim_decide(
            message.as_ptr(),
            c_int::from(have_owner),
            current.as_ptr(),
            out.as_mut_ptr(),
            out.len() as c_int,
        )
    };
    anyhow::ensure!(
        rc > 0 && (rc as usize) < out.len(),
        "simulator claim decision failed ({rc})"
    );
    Ok(serde_json::from_str(&cstr(&out))?)
}

fn simulator_ffi_json(rc: c_int, out: &[i8], operation: &str) -> anyhow::Result<serde_json::Value> {
    anyhow::ensure!(
        rc > 0 && (rc as usize) < out.len(),
        "simulator {operation} failed ({rc})"
    );
    Ok(serde_json::from_str(&cstr(out))?)
}

pub fn simulator_command_parse(message: &str) -> anyhow::Result<serde_json::Value> {
    let message = CString::new(message)?;
    let mut out = vec![0i8; 8192];
    let rc = unsafe {
        codex_v2_command_parse(message.as_ptr(), out.as_mut_ptr(), out.len() as c_int)
    };
    simulator_ffi_json(rc, &out, "command parse")
}

pub fn simulator_command_check(
    message: &str,
    mac: &str,
    nonce: &str,
) -> anyhow::Result<serde_json::Value> {
    let message = CString::new(message)?;
    let mac = CString::new(mac)?;
    let nonce = CString::new(nonce)?;
    let mut out = vec![0i8; 8192];
    let rc = unsafe {
        codex_v2_command_check(
            message.as_ptr(), mac.as_ptr(), nonce.as_ptr(), out.as_mut_ptr(), out.len() as c_int,
        )
    };
    simulator_ffi_json(rc, &out, "command session check")
}

/// The firmware's pure BLE-to-HTTP rendezvous state machine. The caller
/// supplies its own monotonic device clock and owns the simulated radio.
pub struct SimulatorRendezvous { state:*mut std::ffi::c_void }
unsafe impl Send for SimulatorRendezvous {}
impl SimulatorRendezvous {
    pub fn new(now:u64,advertise_ms:u32)->anyhow::Result<Self> {
        let state=unsafe{codex_rv_new()};
        anyhow::ensure!(!state.is_null(),"rendezvous allocation failed");
        unsafe{codex_rv_begin(state,now,advertise_ms)};
        Ok(Self{state})
    }
    pub fn begin(&mut self,now:u64,advertise_ms:u32){unsafe{codex_rv_begin(self.state,now,advertise_ms)}}
    pub fn connected(&mut self,now:u64){unsafe{codex_rv_connected(self.state,now)}}
    pub fn plan_received(&mut self){unsafe{codex_rv_plan(self.state)}}
    pub fn await_network(&mut self,now:u64,legal_until:u64){
        unsafe{codex_rv_await_network(self.state,now,legal_until)}
    }
    pub fn admit_http(&mut self,now:u64,legal_until:u64){
        unsafe{codex_rv_admit_http(self.state,now,legal_until)}
    }
    pub fn http_ready(&mut self){unsafe{codex_rv_http_ready(self.state)}}
    pub fn phase(&self)->i32{unsafe{codex_rv_phase(self.state)}}
    pub fn deadline(&self)->u64{unsafe{codex_rv_deadline(self.state)}}
    pub fn tick(&mut self,now:u64)->bool{unsafe{codex_rv_tick(self.state,now)==1}}
}
impl Drop for SimulatorRendezvous {
    fn drop(&mut self){unsafe{codex_rv_free(self.state)}}
}

#[test]
fn simulator_rendezvous_uses_firmware_network_deadline_without_renewal() {
    unsafe {
        assert_eq!(codex_rv_window_until(1000, 3000, 81000), 81000);
        assert_eq!(codex_rv_window_until(3000, 3000, 81000), 81000);
        assert_eq!(codex_rv_window_until(81000, 3000, 81000), 0);
    }
    let mut rv=SimulatorRendezvous::new(1_000,4_000).unwrap();
    rv.connected(1_100);
    rv.plan_received();
    rv.await_network(1_200,50_000);
    let deadline=rv.deadline();
    assert!(deadline<=50_000 && deadline>1_200);
    rv.admit_http(1_300,50_000);
    assert_eq!(rv.phase(),2);
    rv.admit_http(1_400,80_000);
    assert_eq!(rv.deadline(),deadline,"repeat may not extend the original window");
    rv.http_ready();
    assert_eq!(rv.phase(),3);
    assert!(!rv.tick(deadline-1));
    assert!(rv.tick(deadline));
    assert_eq!(rv.phase(),5);
}

pub struct SimulatorPlan {
    state: *mut std::ffi::c_void,
    accepted: Option<(u64, String, u64, u32)>,
}

unsafe impl Send for SimulatorPlan {}

impl SimulatorPlan {
    pub fn new() -> anyhow::Result<Self> {
        let state = unsafe { codex_v2_plan_new() };
        anyhow::ensure!(!state.is_null(), "simulator plan allocation failed");
        Ok(Self {
            state,
            accepted: None,
        })
    }

    pub fn decide(&mut self, message: &str, now_ms: u64) -> anyhow::Result<serde_json::Value> {
        let message_c = CString::new(message)?;
        let parsed: serde_json::Value = serde_json::from_str(message)?;
        let mut out = vec![0i8; 8192];
        let rc = unsafe {
            codex_v2_plan_decide(
                self.state,
                message_c.as_ptr(),
                now_ms,
                0,
                out.as_mut_ptr(),
                out.len() as c_int,
            )
        };
        let decision = simulator_ffi_json(rc, &out, "plan decision")?;
        if decision["accepted"] == true {
            let id = decision["accepted_id"].as_u64().unwrap_or(0);
            if self.accepted.as_ref().is_none_or(|(old_id, ..)| *old_id != id) {
                self.accepted = Some((
                    id,
                    parsed["mode"].as_str().unwrap_or("sleep").to_owned(),
                    now_ms,
                    decision["state_granted_s"].as_u64().unwrap_or(0) as u32,
                ));
            }
        }
        Ok(decision)
    }

    pub fn status_fields(&self) -> (bool, &str, u64, u32, u64) {
        match &self.accepted {
            Some((id, mode, accepted_at_ms, granted_s)) => {
                (true, mode, *id, *granted_s, *accepted_at_ms)
            }
            None => (false, "sleep", 0, 0, 0),
        }
    }

    pub fn sleep_reason(&self, configured: bool, light_mode: bool, plugged: bool,
        deep_on_usb: bool, manual_hold: bool, provisional: bool,
        boot_ms: u64, safety_deadline_ms: u64, now_ms: u64) -> u8 {
        unsafe { codex_v2_power_sleep_decide(self.state, configured.into(), light_mode.into(),
            plugged.into(), deep_on_usb.into(), manual_hold.into(), provisional.into(),
            boot_ms, safety_deadline_ms, now_ms) as u8 }
    }
}

impl Drop for SimulatorPlan {
    fn drop(&mut self) {
        unsafe { codex_v2_plan_free(self.state) };
    }
}

pub fn battery_power_off(plugged: bool, battery_pct: u8) -> bool {
    unsafe { codex_v2_battery_power_off(plugged.into(), battery_pct.into()) != 0 }
}

pub fn simulator_plan_ack(
    op: &str,
    result: &str,
    display: &str,
    error: Option<&str>,
    plan_id: u64,
    granted_s: Option<u32>,
) -> anyhow::Result<serde_json::Value> {
    let op = CString::new(op)?;
    let result = CString::new(result)?;
    let display = CString::new(display)?;
    let retention = CString::new("ram")?;
    let error = error.map(CString::new).transpose()?;
    let target = CString::new("codex-status-154g")?;
    let mut out = vec![0i8; 8192];
    let rc = unsafe {
        codex_v2_build_ack(
            op.as_ptr(),
            result.as_ptr(),
            display.as_ptr(),
            retention.as_ptr(),
            error.as_ref().map_or(std::ptr::null(), |value| value.as_ptr()),
            -1,
            plan_id,
            std::ptr::null(),
            granted_s.unwrap_or(u32::MAX),
            target.as_ptr(),
            out.as_mut_ptr(),
            out.len() as c_int,
        )
    };
    simulator_ffi_json(rc, &out, "plan ACK")
}

/// Result of pushing one font container into the device font store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontStoreWrite {
    /// The container was validated and written (or replaced).
    Stored { id: String, name: String },
    /// The id was already installed: a no-op that transfers nothing.
    AlreadyPresent { id: String, name: String },
}

/// Reset the in-memory device filesystem and unbind the font store.
pub fn font_store_reset() {
    unsafe { codex_font_store_reset() };
}

/// Inject a torn write after exactly `budget` bytes (`-1` disables).
pub fn font_store_set_write_budget(budget: i64) {
    unsafe { codex_font_store_budget(budget) };
}

/// Bind the font store to a Profile directory (the device's `/fonts/<id>/`).
pub fn font_store_begin(profile: &str) -> bool {
    let c = match CString::new(profile) { Ok(v) => v, Err(_) => return false };
    unsafe { codex_font_store_begin(c.as_ptr()) == 1 }
}

pub fn font_store_write(bytes: &[u8]) -> Result<FontStoreWrite, String> {
    let mut out = vec![0i8; 192];
    let rc = unsafe {
        codex_font_store_write(bytes.as_ptr(), bytes.len() as c_int, out.as_mut_ptr(),
                               out.len() as c_int)
    };
    let message = cstr(&out);
    if rc < 0 {
        return Err(message);
    }
    let id = field(&message, "id=").unwrap_or_default();
    let name = field(&message, "name=").unwrap_or_default();
    Ok(if rc == 1 {
        FontStoreWrite::Stored { id, name }
    } else {
        FontStoreWrite::AlreadyPresent { id, name }
    })
}

/// `(count, corrupt_count, "id:name:bytes …")` for the bound Profile.
pub fn font_store_inventory() -> (usize, usize, String) {
    let mut out = vec![0i8; 512];
    let n = unsafe { codex_font_store_inventory(out.as_mut_ptr(), out.len() as c_int) };
    let message = cstr(&out);
    let bad = field(&message, "bad=")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    (n.max(0) as usize, bad, message)
}

/// Load one installed font: `(id, name, bytes, crc32)` of the stored container.
pub fn font_store_load(id: &str) -> Result<(String, String, u32, u32), String> {
    let c = CString::new(id).map_err(|e| e.to_string())?;
    let mut out = vec![0i8; 192];
    let rc = unsafe { codex_font_store_load(c.as_ptr(), out.as_mut_ptr(), out.len() as c_int) };
    let message = cstr(&out);
    if rc != 1 {
        return Err(message);
    }
    let get = |key: &str| field(&message, key).unwrap_or_default();
    Ok((
        get("id="),
        get("name="),
        get("bytes=").trim().parse().unwrap_or(0),
        u32::from_str_radix(get("crc=").trim(), 16).unwrap_or(0),
    ))
}

/// Keep only the listed font ids in the bound Profile: `(removed, left, bytes)`.
pub fn font_store_prune(keep_ids: &[&str]) -> (i32, usize, u32) {
    let c = CString::new(keep_ids.join(",")).unwrap_or_default();
    let mut out = vec![0i8; 128];
    let removed = unsafe { codex_font_store_prune(c.as_ptr(), out.as_mut_ptr(), out.len() as c_int) };
    let message = cstr(&out);
    (
        removed,
        field(&message, "left=").and_then(|v| v.trim().parse().ok()).unwrap_or(0),
        field(&message, "bytes=").and_then(|v| v.trim().parse().ok()).unwrap_or(0),
    )
}

pub fn font_store_clear() -> bool {
    unsafe { codex_font_store_clear() == 1 }
}

pub fn font_store_usage() -> (u32, usize, String) {
    let mut out = vec![0i8; 128];
    let count = unsafe { codex_font_store_usage(out.as_mut_ptr(), out.len() as c_int) };
    let message = cstr(&out);
    let bytes = field(&message, "bytes=").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
    let profile = field(&message, "profile=").unwrap_or_default();
    (bytes, count.max(0) as usize, profile)
}

/// Write raw bytes at the store path for `id`, bypassing validation (models a
/// file that was damaged or mis-named outside the write path).
pub fn font_store_put_raw(id: &str, bytes: &[u8]) -> bool {
    let c = match CString::new(id) { Ok(v) => v, Err(_) => return false };
    unsafe { codex_font_store_put_raw(c.as_ptr(), bytes.as_ptr(), bytes.len() as c_int) == 1 }
}

fn field(haystack: &str, key: &str) -> Option<String> {
    let start = haystack.find(key)? + key.len();
    let rest = &haystack[start..];
    let end = rest.find(|c: char| c == ' ' || c == '\n').unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// Validate a CSFN v1 font asset container with the device's own parser.
/// On success returns the descriptor line the device would report; on rejection
/// returns the device's rejection reason.
pub fn font_asset_check(bytes: &[u8]) -> Result<String, String> {
    let mut out = vec![0i8; 256];
    let rc = unsafe {
        codex_font_asset_check(bytes.as_ptr(), bytes.len() as c_int,
                               out.as_mut_ptr(), out.len() as c_int)
    };
    let message = cstr(&out);
    match rc {
        1 => Ok(message),
        0 => Err(message),
        _ => Err("font asset check failed".to_string()),
    }
}

/// The caller must keep `bytes` alive until `font_clear_assets` is called.
/// Like compiled-template operations, font bindings share global engine state.
pub fn font_bind_asset(bytes: &[u8]) -> bool {
    unsafe { codex_font_bind_asset(bytes.as_ptr(), bytes.len() as c_int) == 1 }
}

pub fn font_clear_assets() { unsafe { codex_font_clear_assets() } }

pub fn display_state_after_render(writes_before: u32, writes_after: u32,
                                  busy_before: u32, busy_after: u32) -> i32 {
    unsafe { codex_display_state_after_render(writes_before, writes_after, busy_before, busy_after) }
}

/// Derive the semantic refresh regions for a template (display-safety layer).
/// Callers must serialize all `rgn_*` calls (the policy state is global, like
/// the firmware's). The panel geometry is taken from the template's own canvas,
/// so a 400x300 template is not derived against the 200x200 default.
pub fn rgn_build(template: &str) -> Result<usize, String> {
    let (width, height) = canvas_size(template).unwrap_or((WIDTH, HEIGHT));
    rgn_build_size(template, width, height)
}

/// Canvas-explicit region derivation (callers that already resolved the canvas).
pub fn rgn_build_size(template: &str, width: u32, height: u32) -> Result<usize, String> {
    set_panel(width, height);
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

/// Declare the panel geometry the region policy should use. The policy keeps
/// one global panel size, so it must be set for the template's canvas before
/// deriving regions (200x200 and 400x300 share this code).
pub fn set_panel(width: u32, height: u32) {
    unsafe { codex_set_panel(width as c_int, height as c_int) };
}

/// Derive refresh regions from the compiled record (activation path).
pub fn rgn_build_compiled(blob: &[u8], width: u32, height: u32) -> Result<usize, String> {
    set_panel(width, height);
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
    /// Local Wi-Fi lifecycle metadata, independent of BLE/contact state.
    pub wifi_state: &'a str,
    /// 0/1 only while connecting; -1 means absent.
    pub wifi_blink: i32,
    pub ble_manual_active: bool,
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
            wifi_state: "idle",
            wifi_blink: -1,
            ble_manual_active: false,
            offline_mins: -1,
            mode: "light",
        }
    }
}

/// 1-bit raster in the device framebuffer layout (bit set = white, clear = ink).
pub fn render_bits(template: &str, usage: &str, env: &Env<'_>) -> anyhow::Result<Vec<u8>> {
    render_bits_inner(template, usage, env, None)
}

/// Render one preview with Profile assets on the serialized engine worker.
pub fn render_bits_with_fonts(template: &str, usage: &str, env: &Env<'_>, fonts: Vec<Vec<u8>>) -> anyhow::Result<Vec<u8>> {
    render_bits_inner(template, usage, env, Some(fonts))
}

fn render_bits_inner(template: &str, usage: &str, env: &Env<'_>, fonts: Option<Vec<Vec<u8>>>) -> anyhow::Result<Vec<u8>> {
    let mut job = base_job(template, env);
    job.usage = usage.to_string();
    job.fonts = fonts;
    let (rc, out, message) = run_engine(Op::RenderJson, template, job)?;
    match rc {
        1 => Ok(out),
        0 => anyhow::bail!("template rejected by the firmware engine {message}"),
        other => anyhow::bail!("render failed (rc={other})"),
    }
}

pub fn validate(template: &str) -> Result<(), String> {
    let job = base_job(template, &Env::default());
    let (rc, _, message) = run_engine(Op::Validate, template, job).map_err(|e| e.to_string())?;
    if rc == 1 {
        Ok(())
    } else {
        Err(message)
    }
}

/// 8-bit grayscale PNG of the raster (white = 255, ink = 0).
pub fn bits_to_png(bits: &[u8]) -> anyhow::Result<Vec<u8>> {
    bits_to_png_size(bits, WIDTH, HEIGHT)
}

pub fn bits_to_png_size(bits: &[u8], width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    let row_bytes = ((width + 7) / 8) as usize;
    anyhow::ensure!(
        bits.len() == row_bytes * height as usize,
        "framebuffer size"
    );
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        let mut data = vec![0u8; (width * height) as usize];
        for y in 0..height as usize {
            for x in 0..width as usize {
                let byte = bits[y * row_bytes + x / 8];
                let white = byte & (0x80 >> (x % 8)) != 0;
                data[y * width as usize + x] = if white { 255 } else { 0 };
            }
        }
        writer.write_image_data(&data)?;
    }
    Ok(out)
}

/// Decode a reference PNG (grayscale or RGB, 8-bit) to the same bit layout.
pub fn png_to_bits(png_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    png_to_bits_size(png_bytes, WIDTH, HEIGHT)
}

/// Canvas-aware reference decode: the PNG must match the requested canvas.
pub fn png_to_bits_size(png_bytes: &[u8], width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    anyhow::ensure!(
        (info.width, info.height) == (width, height),
        "reference png is {}x{}, template canvas is {}x{}",
        info.width,
        info.height,
        width,
        height
    );
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1usize,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => anyhow::bail!("unsupported reference png color type: {other:?}"),
    };
    let row_bytes = ((width + 7) / 8) as usize;
    let mut bits = vec![0u8; row_bytes * height as usize];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let idx = (y * width as usize + x) * channels;
            let luma = buf[idx];
            if luma >= 128 {
                bits[y * row_bytes + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    Ok(bits)
}

/// One Wi-Fi instance per Fake process, matching the production singleton.
/// Serialize every call through SimState.wifi; no OS network is touched.
/// Offline conversion of WFS3/4/5 or verified Note4 legacy ABI records. No simulator startup or
/// transaction recovery runs; unknown receipts and generations stay unchanged.
pub fn convert_wifi_storage(input:&[u8])->anyhow::Result<Vec<u8>> {
    anyhow::ensure!((33..=8192).contains(&input.len()),"wifi_store_invalid");
    let mut source=input.to_vec();let mut output=vec![0u8;8192];
    let length=unsafe {codex_wifi_convert_storage(source.as_mut_ptr(),source.len() as c_int,output.as_mut_ptr(),output.len() as c_int)};
    source.fill(0);
    if length<0 {output.fill(0);anyhow::bail!("wifi_store_invalid_or_unsupported");}
    output.truncate(length as usize);Ok(output)
}

pub struct SimulatorWifi { dir: std::path::PathBuf }
impl SimulatorWifi {
    pub fn new(dir: &std::path::Path) -> anyhow::Result<Self> {
        let dir = CString::new(dir.to_str().ok_or_else(|| anyhow::anyhow!("wifi_directory"))?)?;
        anyhow::ensure!(unsafe { codex_sim_wifi_open(dir.as_ptr()) } == 1, "wifi_store_corrupt");
        let sim = Self { dir: std::path::PathBuf::from(dir.to_str()?) };
        let legacy=std::fs::read_to_string(sim.dir.join("wifi-legacy.json")).unwrap_or("[]".into());
        let legacy=CString::new(legacy)?;unsafe { codex_sim_wifi_legacy(legacy.as_ptr()) };Ok(sim)
    }
    pub fn legacy(&mut self, value: &serde_json::Value) -> anyhow::Result<()> {
        let list=value.as_array().ok_or_else(||anyhow::anyhow!("invalid_legacy"))?;
        anyhow::ensure!(list.len()<=3,"legacy_capacity");
        for n in list {
            anyhow::ensure!(n.as_object().is_some_and(|o|o.len()==2)&&n["network_id"].as_str().is_some_and(|s|!s.is_empty()&&s.len()<=32&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_'))&&
                n["ssid"].as_str().is_some_and(|s|!s.is_empty()&&s.len()<=32&&!s.contains('\0')),"invalid_legacy");
        }
        let bytes=serde_json::to_vec(value)?;
        let path=self.dir.join("wifi-legacy.json");let temp=self.dir.join("wifi-legacy.json.tmp");
        std::fs::write(&temp,&bytes)?;std::fs::rename(&temp,&path)?;
        let text=CString::new(bytes)?;unsafe {codex_sim_wifi_legacy(text.as_ptr())};Ok(())
    }
    pub fn control(&mut self,input:&serde_json::Value,principal:&str,namespace:&str,generation:u32,now:u64,until:u64,physical:u64,claimed:bool,session_valid:bool)->anyhow::Result<serde_json::Value> {
        let input=CString::new(serde_json::to_string(input)?)?;let principal=CString::new(principal)?;let namespace=CString::new(namespace)?;let mut out=vec![0i8;16384];
        let rc=unsafe{codex_sim_wifi_control(input.as_ptr(),principal.as_ptr(),namespace.as_ptr(),generation,now,until,physical,claimed as c_int,session_valid as c_int,out.as_mut_ptr(),out.len() as c_int)};
        simulator_ffi_json(rc,&out,"wifi_control")
    }
    pub fn read_page(&mut self,input:&serde_json::Value,peer:&str,generation:u32,text:&str)->anyhow::Result<serde_json::Value> {
        let input=CString::new(serde_json::to_string(input)?)?;let peer=CString::new(peer)?;let text=CString::new(text)?;let mut out=vec![0i8;16384];
        let rc=unsafe{codex_sim_wifi_read_page(input.as_ptr(),peer.as_ptr(),generation,text.as_ptr(),out.as_mut_ptr(),out.len() as c_int)};simulator_ffi_json(rc,&out,"wifi_status_page")
    }
    pub fn poll(&mut self, now: u64, peer: &str, generation: u32, encrypted: bool, outcome: i32) -> anyhow::Result<bool> {
        self.poll_with_start_allowed(now,peer,generation,encrypted,outcome,true)
    }
    pub fn poll_with_start_allowed(&mut self, now: u64, peer: &str, generation: u32, encrypted: bool, outcome: i32, start_allowed: bool) -> anyhow::Result<bool> {
        let peer = CString::new(peer)?;
        Ok(unsafe { codex_sim_wifi_poll(now, peer.as_ptr(), generation, encrypted as c_int, outcome,start_allowed as c_int) } == 1)
    }
    pub fn storage_status(&mut self)->anyhow::Result<serde_json::Value> {
        let mut out=vec![0i8;2048];let rc=unsafe{codex_sim_wifi_storage_status(out.as_mut_ptr(),out.len() as c_int)};
        simulator_ffi_json(rc,&out,"wifi_storage_status")
    }
    pub fn storage_failure(&mut self, fail: bool) { unsafe { codex_sim_wifi_storage_failure(fail as c_int) }; }
    pub fn constrain(&mut self,now:u64,until:u64,owner_allowed:bool) {
        unsafe{codex_sim_wifi_constrain(now,until,owner_allowed as c_int)};
    }
    pub fn observe(&self)->anyhow::Result<serde_json::Value> {
        let mut out=vec![0i8;2048];let rc=unsafe{codex_sim_wifi_observe(out.as_mut_ptr(),out.len() as c_int)};
        simulator_ffi_json(rc,&out,"wifi_observe")
    }
}

#[test]
fn rendezvous_plan_replay_ack_reports_remaining_without_renewal() {
    let mut plan = SimulatorPlan::new().unwrap();
    let body = r#"{"plan_id":1,"mode":"light","light_duration_s":120,"rendezvous_period_s":60}"#;
    for (time, remaining) in [(0,120),(60_000,60),(121_000,0)] {
        let ack = plan.decide(body, time).unwrap();
        assert_eq!(ack["granted_s"], remaining, "replay ACK must report time left");
        assert_eq!(ack["deadline_ms"], 120_000);
    }
}
#[cfg(windows)]
mod windows_wifi_hal {
    use sha2::{Digest, Sha256};

    // Rust's Windows rename replaces an existing file atomically (MoveFileExW).
    #[no_mangle]
    unsafe extern "C" fn codex_wifi_replace(from: *const std::ffi::c_char, to: *const std::ffi::c_char) -> i32 {
        let from = unsafe { std::ffi::CStr::from_ptr(from) }.to_str();
        let to = unsafe { std::ffi::CStr::from_ptr(to) }.to_str();
        match (from, to) {
            (Ok(from), Ok(to)) if std::fs::rename(from, to).is_ok() => 0,
            _ => -1,
        }
    }

    unsafe fn copy_output(out: *mut u8, capacity: usize, bytes: &[u8]) -> usize {
        if bytes.len() > capacity { return 0; }
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len()); }
        bytes.len()
    }

    #[no_mangle]
    unsafe extern "C" fn codex_wifi_deflate(out: *mut u8, capacity: usize, input: *const u8, size: usize) -> usize {
        let input = unsafe { std::slice::from_raw_parts(input, size) };
        let bytes = miniz_oxide::deflate::compress_to_vec_zlib(input, 6);
        unsafe { copy_output(out, capacity, &bytes) }
    }

    #[no_mangle]
    unsafe extern "C" fn codex_wifi_inflate(out: *mut u8, capacity: usize, input: *const u8, size: usize) -> usize {
        let input = unsafe { std::slice::from_raw_parts(input, size) };
        match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(input, capacity) {
            Ok(bytes) => unsafe { copy_output(out, capacity, &bytes) },
            Err(_) => 0,
        }
    }

    #[no_mangle]
    unsafe extern "C" fn codex_wifi_sha256(input: *const u8, size: usize, out: *mut u8) {
        let input = unsafe { std::slice::from_raw_parts(input, size) };
        let hash = Sha256::digest(input);
        unsafe { std::ptr::copy_nonoverlapping(hash.as_ptr(), out, hash.len()); }
    }

    #[test]
    fn windows_wifi_codec_checks_limits_corruption_and_hash() {
        let input = b"wifi legacy storage round trip";
        let mut packed = [0u8; 128];
        let size = unsafe { codex_wifi_deflate(packed.as_mut_ptr(), packed.len(), input.as_ptr(), input.len()) };
        assert!(size > 0);
        let mut decoded = [0u8; 128];
        let n = unsafe { codex_wifi_inflate(decoded.as_mut_ptr(), decoded.len(), packed.as_ptr(), size) };
        assert_eq!(&decoded[..n], input);
        assert_eq!(unsafe { codex_wifi_inflate(decoded.as_mut_ptr(), 1, packed.as_ptr(), size) }, 0);
        assert_eq!(unsafe { codex_wifi_deflate(packed.as_mut_ptr(), 1, input.as_ptr(), input.len()) }, 0);
        packed[0] ^= 0xff;
        assert_eq!(unsafe { codex_wifi_inflate(decoded.as_mut_ptr(), decoded.len(), packed.as_ptr(), size) }, 0);
        let mut hash = [0u8; 32];
        unsafe { codex_wifi_sha256(b"abc".as_ptr(), 3, hash.as_mut_ptr()); }
        assert_eq!(hash, [0xba,0x78,0x16,0xbf,0x8f,0x01,0xcf,0xea,0x41,0x41,0x40,0xde,0x5d,0xae,0x22,0x23,
            0xb0,0x03,0x61,0xa3,0x96,0x17,0x7a,0x9c,0xb4,0x10,0xff,0x61,0xf2,0x00,0x15,0xad]);
    }
}

#[test]
fn ble_claim_projection_preserves_capability_and_bounded_error_reply() {
    let info=simulator_ble_wire("info",&serde_json::json!({"mac":"7C:4F:AD:B9:34:08","ip":"255.255.255.255",
        "fw":"x".repeat(64),"wifi_management":3,"wifi_connection":1,"ble_claim":1,
        "rf_remaining_ms":u32::MAX-1,"wake_generation":u32::MAX,"wake_seq":u32::MAX})).unwrap();
    assert!(info.len()<=512);
    let info:serde_json::Value=serde_json::from_slice(&info).unwrap();
    assert_eq!(info["ble_claim"],1);
    for error in ["occupied","storage","unauthorized"] {
        let bytes=simulator_ble_wire("reply",&serde_json::json!({"op":"claim","request_id":"a".repeat(64),
            "device_mac":"7C:4F:AD:B9:34:08","result":"rejected","error":error,"stage":"claim",
            "owner_id":"o".repeat(128),"expires_in_s":300})).unwrap();
        let reply:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
        assert!(bytes.len()<=512);assert_eq!(reply["error"],error);assert_eq!(reply["result"],"rejected");
    }
}

#[cfg(test)]
static WIFI_PERMISSION_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn owner_snapshot_grants_shared_configuration_permissions() {
    let _guard=WIFI_PERMISSION_TEST_LOCK.lock().unwrap();
    let dir=std::env::temp_dir().join(format!("codex-wifi-projection-{}",std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut sim=SimulatorWifi::new(&dir).unwrap();
    for (i,namespace) in ["legacy","foreign"].iter().enumerate() {
        let ack=sim.control(&serde_json::json!({"op":"apply","bridge_id":"test","request_id":format!("write{i}"),"device_mac":"AABBCCDDEEFF","expected_revision":i,
            "changes":[{"network_id":format!("slot{i}"),"host_ssid":"Host","ssid":"Device","credential":"replace","password":"test-password"}]}),"test",namespace,1,1000,u64::MAX,0,true,true).unwrap();
        assert_eq!(ack["outcome"],"committed");
    }
    let mut offset=0;let mut id=String::new();let mut full=String::new();
    loop {
        let r=sim.control(&serde_json::json!({"op":"snapshot","bridge_id":"test","request_id":"read","device_mac":"AABBCCDDEEFF","offset":offset,"snapshot_id":id}),"test","own",1,1001,u64::MAX,0,true,true).unwrap();
        if offset==0 {
            let denied=sim.control(&serde_json::json!({"op":"snapshot","bridge_id":"test","request_id":"expired-owner","device_mac":"AABBCCDDEEFF","offset":r["next_offset"],"snapshot_id":r["snapshot_id"]}),"test","own",1,1001,u64::MAX,0,false,true).unwrap();
            assert_eq!(denied["error"],"claim_required");
        }
        assert_eq!(r["ok"],true);full.push_str(r["chunk"].as_str().unwrap());id=r["snapshot_id"].as_str().unwrap().into();offset=r["next_offset"].as_u64().unwrap();
        if r["complete"]==true {break;}
    }
    let snapshot:serde_json::Value=serde_json::from_str(&full).unwrap();
    let rows=snapshot["networks"].as_array().unwrap();
    let legacy=rows.iter().find(|r|r["bond"]=="legacy").unwrap();
    assert_eq!(legacy["writable"],true);assert_eq!(legacy["network"]["password"],"test-password");
    let foreign=rows.iter().find(|r|r["bond"]=="foreign").unwrap();assert_eq!(foreign["network"]["password"],"test-password");assert_eq!(foreign["writable"],true);
    let mut offset=0;let mut id=String::new();let mut full=String::new();
    loop {
        let r=sim.control(&serde_json::json!({"op":"snapshot","bridge_id":"test","request_id":"unclaimed-read","device_mac":"AABBCCDDEEFF","offset":offset,"snapshot_id":id}),"test","own",1,1002,u64::MAX,0,false,true).unwrap();
        full.push_str(r["chunk"].as_str().unwrap());id=r["snapshot_id"].as_str().unwrap().into();offset=r["next_offset"].as_u64().unwrap();if r["complete"]==true {break;}
    }
    let hidden:serde_json::Value=serde_json::from_str(&full).unwrap();assert!(hidden["networks"].as_array().unwrap().iter().all(|r|r["network"]["password"].is_null()));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shared_wifi_cross_bridge_writes_keep_credentials_and_receipt_isolation() {
    let _guard=WIFI_PERMISSION_TEST_LOCK.lock().unwrap();
    let dir=std::env::temp_dir().join(format!("codex-wifi-shared-{}",std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut sim=SimulatorWifi::new(&dir).unwrap();
    let command=|op:&str,id:&str,revision:u64,changes:serde_json::Value| serde_json::json!({
        "op":op,"bridge_id":"windows","request_id":id,"device_mac":"AABBCCDDEEFF","expected_revision":revision,"changes":changes});
    let seed=serde_json::json!({"op":"apply","bridge_id":"mac","request_id":"seed","device_mac":"AABBCCDDEEFF","expected_revision":0,
        "changes":[{"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"replace","password":"fixture-password"},
                   {"network_id":"backup","host_ssid":"Other","ssid":"Backup","credential":"open"}]});
    assert_eq!(sim.control(&seed,"mac","mac-bond",1,1000,u64::MAX,0,true,true).unwrap()["outcome"],"committed");
    let update=command("apply","shared-update",1,serde_json::json!([{"network_id":"home","host_ssid":"Host","ssid":"Device","label":"Updated from Windows","credential":"keep"}]));
    assert_eq!(sim.control(&update,"windows","windows-bond",1,1001,u64::MAX,0,false,true).unwrap()["error"],"claim_required");
    assert_eq!(sim.control(&update,"windows","windows-bond",1,1001,u64::MAX,0,true,false).unwrap()["error"],"session_expired");
    assert_eq!(sim.control(&update,"windows","windows-bond",1,1001,u64::MAX,0,true,true).unwrap()["outcome"],"committed");
    // Replaying the same transaction does not repeat the write.
    assert_eq!(sim.control(&update,"windows","windows-bond",1,1002,u64::MAX,0,true,true).unwrap()["revision"],2);
    let query=serde_json::json!({"op":"result","bridge_id":"mac","request_id":"query","query_id":"shared-update","device_mac":"AABBCCDDEEFF"});
    assert_eq!(sim.control(&query,"mac","mac-bond",1,1002,u64::MAX,0,true,true).unwrap()["outcome"],"unknown");
    let matched=serde_json::json!({"op":"match","bridge_id":"windows","request_id":"match","network_id":"home","open":false,"metadata_key":"0".repeat(64),"device_mac":"AABBCCDDEEFF"});
    let found=sim.control(&matched,"windows","windows-bond",1,1002,u64::MAX,0,true,true).unwrap();assert_eq!(found["ok"],true);assert_eq!(found["version"],2);
    let stale=command("apply","stale",1,serde_json::json!([{"network_id":"backup","deleted":true}]));
    assert_eq!(sim.control(&stale,"windows","windows-bond",1,1002,u64::MAX,0,true,true).unwrap()["error"],"revision_conflict");
    let activate=serde_json::json!({"op":"activate","bridge_id":"windows","request_id":"activate-shared","network_id":"home","version":2,"device_mac":"AABBCCDDEEFF"});
    assert_eq!(sim.control(&activate,"windows","windows-bond",1,1003,u64::MAX,0,true,true).unwrap()["ok"],true);
    sim.poll(1004,"windows-bond",1,true,0).unwrap();
    sim.poll(1005,"windows-bond",1,true,1).unwrap();
    let mut offset=0;let mut id=String::new();let mut full=String::new();
    loop {
        let r=sim.control(&serde_json::json!({"op":"snapshot","bridge_id":"windows","request_id":"read","device_mac":"AABBCCDDEEFF","offset":offset,"snapshot_id":id}),"windows","windows-bond",1,1005,u64::MAX,0,true,true).unwrap();
        assert_eq!(r["ok"],true);full.push_str(r["chunk"].as_str().unwrap());id=r["snapshot_id"].as_str().unwrap().into();offset=r["next_offset"].as_u64().unwrap();if r["complete"]==true {break;}
    }
    let snapshot:serde_json::Value=serde_json::from_str(&full).unwrap();
    let home=snapshot["networks"].as_array().unwrap().iter().find(|r|r["network"]["network_id"]=="home").unwrap();
    assert_eq!(home["bond"],"mac-bond");assert_eq!(home["writable"],true);assert_eq!(home["network"]["password"],"fixture-password");
    assert_eq!(snapshot["active"]["network_id"],"home");
    let delete=command("apply","delete-shared",snapshot["revision"].as_u64().unwrap(),serde_json::json!([{"network_id":"backup","deleted":true}]));
    assert_eq!(sim.control(&delete,"windows","windows-bond",1,1006,u64::MAX,0,true,true).unwrap()["outcome"],"committed");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shared_wifi_global_capacity_keeps_bounded_storage() {
    let _guard=WIFI_PERMISSION_TEST_LOCK.lock().unwrap();
    let dir=std::env::temp_dir().join(format!("codex-wifi-capacity-{}",std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();let mut sim=SimulatorWifi::new(&dir).unwrap();
    println!("empty {}",sim.storage_status().unwrap());
    let mut count=0;
    for (batch,size) in [1,3,4,4].into_iter().enumerate() {
        let changes:Vec<_>=(count..count+size).map(|i|serde_json::json!({"network_id":format!("network{i}"),"host_ssid":format!("Host{i}"),"ssid":format!("Device{i}"),"credential":"open"})).collect();
        let request=serde_json::json!({"op":"apply","bridge_id":"test","request_id":format!("batch{batch}"),"device_mac":"AABBCCDDEEFF","expected_revision":batch,"changes":changes});
        assert_eq!(sim.control(&request,"test","same-bond",1,1000,u64::MAX,0,true,true).unwrap()["outcome"],"committed");
        count+=size;println!("records={count} {}",sim.storage_status().unwrap());
    }
    let excess=serde_json::json!({"op":"apply","bridge_id":"test","request_id":"excess","device_mac":"AABBCCDDEEFF","expected_revision":4,"changes":[{"network_id":"excess","host_ssid":"Excess","ssid":"Excess","credential":"open"}]});
    assert_eq!(sim.control(&excess,"test","same-bond",1,1000,u64::MAX,0,true,true).unwrap()["error"],"capacity");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shared_wifi_old_duplicate_ids_are_preserved_and_explicitly_rejected() {
    let _guard=WIFI_PERMISSION_TEST_LOCK.lock().unwrap();
    let dir=std::env::temp_dir().join(format!("codex-wifi-duplicate-{}",std::process::id()));
    // Frozen WFS4 bytes from the retired management2 writer: two open networks
    // named "same" in namespaces "mac"/"windows", revision 2, two save receipts.
    // Keep recovery coverage without compiling a retired command adapter.
    use base64::Engine;
    let fixture=base64::engine::general_purpose::STANDARD.decode(
        "V0ZTNAQAAgLxAAAAf3iPKgIAAAAAAAAAAAAAAAAAAAAAHwABAAAAAAAAAANtYWMEc2FtZQRIb3N0BkRldmljZQAAIwACAAAAAAAAAAd3aW5kb3dzBHNhbWUESG9zdAZEZXZpY2UAAEEAAQAAAAAAAAA49Q8W5RKj62BZvSuTtZT31ZHiTZxssxCL2EUD2SACwgNvbGQFc2VlZDADbWFjAAAAAAAAAAAAAABFAAIAAAAAAAAAOPUPFuUSo+tgWb0rk7WU99WR4k2cbLMQi9hFA9kgAsIDb2xkBXNlZWQxB3dpbmRvd3MAAAAAAAAAAAAAAA==",
    ).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("wifi-schemes.bin"),&fixture).unwrap();
    let mut sim=SimulatorWifi::new(&dir).unwrap();
    assert_eq!(std::fs::read(dir.join("wifi-schemes.bin")).unwrap(),fixture);
    let request=serde_json::json!({"op":"apply","bridge_id":"new","request_id":"ambiguous","device_mac":"AABBCCDDEEFF","expected_revision":2,"changes":[{"network_id":"same","deleted":true}]});
    for _ in 0..2 {
        sim=SimulatorWifi::new(&dir).unwrap();
        let r=sim.control(&request,"new","new",1,1000,30000,0,true,true).unwrap();assert_eq!(r["error"],"ambiguous_network_id");assert_eq!(sim.storage_status().unwrap()["revision"],2);
    }
    let activate=serde_json::json!({"op":"activate","bridge_id":"new","request_id":"ambiguous-activate","network_id":"same","version":1,"device_mac":"AABBCCDDEEFF"});
    assert_eq!(sim.control(&activate,"new","new",1,1000,30000,0,true,true).unwrap()["error"],"ambiguous_network_id");
    assert_eq!(sim.storage_status().unwrap()["networks"],2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn physical_ble_hold_finishes_wifi_activation_without_starting_deferred_rf() {
    let _guard=WIFI_PERMISSION_TEST_LOCK.lock().unwrap();
    let dir=std::env::temp_dir().join(format!("codex-wifi-hold-{}",std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();let mut sim=SimulatorWifi::new(&dir).unwrap();
    let seed=serde_json::json!({"op":"apply","bridge_id":"test","request_id":"seed","device_mac":"AABBCCDDEEFF","expected_revision":0,
        "changes":[{"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"}]});
    assert_eq!(sim.control(&seed,"test","bond",1,1000,300000,300000,true,true).unwrap()["outcome"],"committed");
    let activation=|id:&str|serde_json::json!({"op":"activate","bridge_id":"test","request_id":id,"device_mac":"AABBCCDDEEFF","network_id":"home","version":1});
    assert_eq!(sim.control(&activation("hold"),"test","bond",1,1100,300000,300000,true,true).unwrap()["ok"],true);
    // Physical hold keeps BLE active; association has already started. The
    // ROM must still poll completion, commit the receipt and preserve Wi-Fi.
    assert!(!sim.poll_with_start_allowed(1200,"bond",1,true,1,false).unwrap());
    let query=serde_json::json!({"op":"result","bridge_id":"test","request_id":"query","query_id":"hold","device_mac":"AABBCCDDEEFF"});
    assert_eq!(sim.control(&query,"test","bond",1,1300,300000,300000,true,true).unwrap()["outcome"],"committed");
    assert_eq!(sim.observe().unwrap()["connected"],true);
    let begins=sim.observe().unwrap()["begins"].clone();
    assert_eq!(sim.control(&activation("healthy"),"test","bond",1,1350,300000,300000,true,true).unwrap()["ok"],true);
    assert_eq!(sim.observe().unwrap()["connected"],true,"explicit activation must preserve the matching healthy link");
    assert!(!sim.poll_with_start_allowed(1360,"bond",1,true,0,false).unwrap());
    assert_eq!(sim.observe().unwrap()["begins"],begins,"no repeated association for the same connected network");
    // Ordinary BLE defers a NEW association, then permits it after release.
    assert_eq!(sim.control(&activation("deferred"),"test","bond",1,1400,300000,0,true,true).unwrap()["ok"],true);
    assert!(sim.poll_with_start_allowed(1500,"bond",1,true,0,false).unwrap());
    assert_eq!(sim.control(&serde_json::json!({"op":"result","bridge_id":"test","request_id":"query2","query_id":"deferred","device_mac":"AABBCCDDEEFF"}),"test","bond",1,1600,300000,0,true,true).unwrap()["outcome"],"accepted");
    assert!(!sim.poll_with_start_allowed(1700,"bond",1,false,1,true).unwrap());
    std::fs::remove_dir_all(dir).unwrap();
}
