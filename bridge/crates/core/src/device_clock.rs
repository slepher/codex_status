//! Explicit per-device experiment clocks. Real devices use the host clock.
//! Runtime deadlines should use `monotonic_ms`; persisted timestamps retain
//! their existing Unix epoch meaning through `wall_secs`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use serde_json::{json, Value};

#[derive(Clone)]
struct View {
    anchor: Instant,
    monotonic_ms: u64,
    wall_ms: i64,
    rate_ppm: u64,
}

impl View {
    fn at(&self, now: Instant) -> (u64, i64) {
        let delta = now.duration_since(self.anchor).as_millis()
            .saturating_mul(u128::from(self.rate_ppm)) / 1_000_000;
        let delta = delta.min(u128::from(u64::MAX)) as u64;
        (self.monotonic_ms.saturating_add(delta),
            self.wall_ms.saturating_add(delta.min(i64::MAX as u64) as i64))
    }
}

static VIEWS: OnceLock<Mutex<HashMap<String, View>>> = OnceLock::new();

fn views() -> &'static Mutex<HashMap<String, View>> {
    VIEWS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn key(mac: &str) -> String {
    crate::platform::model::DeviceIdentity::normalized_mac(mac)
        .unwrap_or_else(|| mac.to_uppercase())
}

pub fn wall_secs(mac: &str) -> u64 {
    let view = views().lock().unwrap().get(&key(mac)).cloned();
    view.map_or_else(crate::now_secs, |view| view.at(Instant::now()).1.max(0) as u64 / 1000)
}

pub fn monotonic_ms(mac: &str) -> Option<u64> {
    views().lock().unwrap().get(&key(mac)).map(|view| view.at(Instant::now()).0)
}

pub fn configure(mac: &str, monotonic_ms: u64, wall_ms: i64, rate_ppm: u64) -> Result<Value> {
    let normalized = crate::platform::model::DeviceIdentity::normalized_mac(mac)
        .ok_or_else(|| anyhow::anyhow!("invalid device MAC"))?;
    let first = u8::from_str_radix(&normalized[..2], 16)?;
    if first & 0x03 != 0x02 { bail!("experiment clock requires a local unicast MAC"); }
    if rate_ppm > 1_000_000_000 { bail!("clock rate exceeds limit"); }
    let mut views = views().lock().unwrap();
    views.insert(normalized.clone(), View {
        anchor: Instant::now(), monotonic_ms, wall_ms, rate_ppm,
    });
    Ok(snapshot_locked(&views, &normalized))
}

pub fn change_rate(mac: &str, rate_ppm: u64) -> Result<Value> {
    if rate_ppm > 1_000_000_000 { bail!("clock rate exceeds limit"); }
    let mut views = views().lock().unwrap();
    let view = views.get_mut(&key(mac)).ok_or_else(|| anyhow::anyhow!("no experiment clock"))?;
    let (monotonic_ms, wall_ms) = view.at(Instant::now());
    *view = View {anchor:Instant::now(),monotonic_ms,wall_ms,rate_ppm};
    Ok(snapshot_locked(&views, mac))
}

pub fn step(mac: &str, delta_ms: u64) -> Result<Value> {
    if delta_ms > 86_400_000 { bail!("clock step exceeds one day"); }
    let mut views = views().lock().unwrap();
    let view = views.get_mut(&key(mac)).ok_or_else(|| anyhow::anyhow!("no experiment clock"))?;
    if view.rate_ppm != 0 { bail!("clock must be paused before step"); }
    view.monotonic_ms = view.monotonic_ms.saturating_add(delta_ms);
    view.wall_ms = view.wall_ms.saturating_add(delta_ms.min(i64::MAX as u64) as i64);
    Ok(snapshot_locked(&views, mac))
}

pub fn set_wall(mac: &str, wall_ms: i64) -> Result<Value> {
    let mut views = views().lock().unwrap();
    let view = views.get_mut(&key(mac)).ok_or_else(|| anyhow::anyhow!("no experiment clock"))?;
    let (monotonic_ms, _) = view.at(Instant::now());
    *view = View {anchor:Instant::now(),monotonic_ms,wall_ms,rate_ppm:view.rate_ppm};
    Ok(snapshot_locked(&views, mac))
}

pub fn snapshot(mac: &str) -> Option<Value> {
    let views = views().lock().unwrap();
    views.contains_key(&key(mac)).then(|| snapshot_locked(&views, mac))
}

fn snapshot_locked(views: &HashMap<String, View>, mac: &str) -> Value {
    let key = key(mac);
    let view = &views[&key];
    let (monotonic_ms, wall_ms) = view.at(Instant::now());
    json!({"mac":key,"monotonic_ms":monotonic_ms,
        "wall_ms":wall_ms,"rate_ppm":view.rate_ppm})
}

pub fn clear(mac: &str) {
    views().lock().unwrap().remove(&key(mac));
}

fn host_ms() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis().min(u128::from(u64::MAX)) as u64)
}

pub fn export() -> Result<Value> {
    let saved_host_ms = host_ms()?;
    let views = views().lock().unwrap();
    let items: Vec<Value> = views.iter().map(|(mac, view)| {
        let (monotonic_ms, wall_ms) = view.at(Instant::now());
        json!({"mac":mac,"monotonic_ms":monotonic_ms,"wall_ms":wall_ms,
            "rate_ppm":view.rate_ppm})
    }).collect();
    Ok(json!({"schema":1,"saved_host_ms":saved_host_ms,"views":items}))
}

pub fn restore(document: &Value) -> Result<()> {
    if document["schema"] != 1 { bail!("unsupported bridge experiment clock state"); }
    let saved_host_ms = document["saved_host_ms"].as_u64()
        .ok_or_else(|| anyhow::anyhow!("missing saved host time"))?;
    let elapsed = host_ms()?.saturating_sub(saved_host_ms);
    let items = document["views"].as_array()
        .ok_or_else(|| anyhow::anyhow!("missing clock views"))?;
    let mut restored = HashMap::new();
    for item in items {
        let mac = item["mac"].as_str().ok_or_else(|| anyhow::anyhow!("missing MAC"))?;
        let base_monotonic = item["monotonic_ms"].as_u64()
            .ok_or_else(|| anyhow::anyhow!("missing monotonic time"))?;
        let base_wall = item["wall_ms"].as_i64()
            .ok_or_else(|| anyhow::anyhow!("missing wall time"))?;
        let rate = item["rate_ppm"].as_u64()
            .ok_or_else(|| anyhow::anyhow!("missing rate"))?;
        let normalized = crate::platform::model::DeviceIdentity::normalized_mac(mac)
            .ok_or_else(|| anyhow::anyhow!("invalid MAC"))?;
        let first = u8::from_str_radix(&normalized[..2], 16)?;
        if first & 0x03 != 0x02 || rate > 1_000_000_000 {
            bail!("invalid experiment clock entry");
        }
        let delta = u128::from(elapsed).saturating_mul(u128::from(rate)) / 1_000_000;
        let delta = delta.min(u128::from(u64::MAX)) as u64;
        if restored.insert(normalized, View {
            anchor:Instant::now(),
            monotonic_ms:base_monotonic.saturating_add(delta),
            wall_ms:base_wall.saturating_add(delta.min(i64::MAX as u64) as i64),
            rate_ppm:rate,
        }).is_some() { bail!("duplicate experiment clock MAC"); }
    }
    *views().lock().unwrap() = restored;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_views_and_wall_jump_do_not_move_monotonic() {
        let a = "0200000000C1";
        let b = "0200000000C2";
        configure(a, 1000, 2_000_000, 0).unwrap();
        configure(b, 9000, 3_000_000, 0).unwrap();
        assert_eq!(wall_secs(a), 2000);
        assert_eq!(wall_secs(b), 3000);
        configure("02:00:00:00:00:C1", 1000, 2_000_000, 0).unwrap();
        assert_eq!(wall_secs(a), 2000);
        step(a, 500).unwrap();
        assert_eq!(monotonic_ms(a), Some(1500));
        assert_eq!(monotonic_ms(b), Some(9000));
        set_wall(a, 9_000_000).unwrap();
        assert_eq!(monotonic_ms(a), Some(1500));
        assert_eq!(wall_secs(a), 9000);
        let saved = export().unwrap();
        clear(a);
        restore(&saved).unwrap();
        assert_eq!(monotonic_ms(a), Some(1500));
        clear(a);
        clear(b);
    }
}
