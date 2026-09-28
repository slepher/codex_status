//! RF-only Note4 bridge_first recovery experiment. No production state or keys.
//! `a`: continuous PC AVAILABLE, `c`: periodic AVAILABLE, `b`: wait for device
//! CHALLENGE. All modes answer CHALLENGE with a bounded OFFER advertisement.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use btleplug::api::{Central, CentralEvent, Manager as _, ScanFilter};
use btleplug::platform::Manager;
use clap::Parser;
use futures::StreamExt;
use windows::core::Ref;
use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementPublisher, BluetoothLEAdvertisementPublisherStatusChangedEventArgs,
    BluetoothLEManufacturerData,
};
use windows::Foundation::TypedEventHandler;
use windows::Storage::Streams::DataWriter;

const COMPANY: u16 = 0xFFFF; // Bluetooth SIG reserved test value; never production.

#[derive(Parser)]
struct Args {
    #[arg(long, value_parser = ["a", "b", "c"])]
    strategy: String,
    #[arg(long)]
    run_id: u32,
    #[arg(long, default_value_t = 40)]
    seconds: u64,
    #[arg(long, default_value_t = 10_000)]
    period_ms: u64,
    #[arg(long, default_value_t = 2_000)]
    on_ms: u64,
}

fn frame(kind: u8, run: u32) -> Vec<u8> {
    let mut bytes = vec![0u8; 24];
    bytes[0] = 0xE0;
    bytes[1] = kind;
    bytes[2..6].copy_from_slice(&run.to_be_bytes());
    bytes
}

fn challenge_matches(bytes: &[u8], run: u32) -> bool {
    bytes.len() >= 6 && bytes[0] == 0xE0 && bytes[1] == 0xB1 &&
        bytes[2..6] == run.to_be_bytes()
}

fn report(origin: Instant, event: &str, detail: serde_json::Value) {
    println!(
        "{}",
        serde_json::json!({
            "t_ms": origin.elapsed().as_millis(),
            "event": event,
            "detail": detail,
            "rf_only": true
        })
    );
}

fn publisher(kind: u8, run: u32, origin: Instant) -> Result<BluetoothLEAdvertisementPublisher> {
    let publisher = BluetoothLEAdvertisementPublisher::new().context("publisher")?;
    let adv = publisher.Advertisement().context("advertisement")?;
    let manufacturer = BluetoothLEManufacturerData::new().context("manufacturer data")?;
    manufacturer.SetCompanyId(COMPANY)?;
    let writer = DataWriter::new().context("data writer")?;
    writer.WriteBytes(&frame(kind, run))?;
    manufacturer.SetData(&writer.DetachBuffer()?)?;
    adv.ManufacturerData()?.Append(&manufacturer)?;
    let status_events = Arc::new(Mutex::new(Vec::<(i32, u128)>::new()));
    let status_events_cb = status_events.clone();
    publisher.StatusChanged(&TypedEventHandler::new(
        move |_sender: Ref<BluetoothLEAdvertisementPublisher>,
              args: Ref<BluetoothLEAdvertisementPublisherStatusChangedEventArgs>| {
            if let Ok(args) = args.ok() {
                if let Ok(status) = args.Status() {
                    status_events_cb.lock().unwrap().push((status.0, origin.elapsed().as_millis()));
                }
            }
            Ok(())
        },
    ))?;
    // The publisher owns the callback; a short poll flushes status events in main.
    STATUS_EVENTS.lock().unwrap().push(status_events);
    Ok(publisher)
}

// Only one process-wide experiment is run at once. Keep event buffers alive
// until their publishers have stopped; status events are read without logging
// any device identity or payload.
static STATUS_EVENTS: Mutex<Vec<Arc<Mutex<Vec<(i32, u128)>>>>> = Mutex::new(Vec::new());

fn flush_status(origin: Instant) {
    let buffers = STATUS_EVENTS.lock().unwrap();
    for buffer in buffers.iter() {
        let mut events = buffer.lock().unwrap();
        for (status, at) in events.drain(..) {
            report(origin, "publisher_status", serde_json::json!({"status":status,"at_ms":at}));
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    anyhow::ensure!(args.run_id > 0 && args.seconds <= 600, "run/seconds out of range");
    anyhow::ensure!(args.on_ms > 0 && args.on_ms < args.period_ms, "invalid pulse");
    let origin = Instant::now();
    let manager = Manager::new().await.context("bluetooth manager")?;
    let adapter = manager.adapters().await?.into_iter().next().context("no adapter")?;
    adapter.start_scan(ScanFilter::default()).await.context("watcher scan")?;
    let mut events = adapter.events().await.context("watcher events")?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()/* challenge */>();
    let run = args.run_id;
    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            if let CentralEvent::ManufacturerDataAdvertisement { manufacturer_data, .. } = event {
                if let Some(bytes) = manufacturer_data.get(&COMPANY) {
                    if challenge_matches(bytes, run) { let _ = tx.send(()); }
                }
            }
        }
    });
    report(origin, "start", serde_json::json!({"strategy":args.strategy,"run_id":run}));

    let mut active: Option<BluetoothLEAdvertisementPublisher> = None;
    let mut active_kind = 0u8;
    let mut offer_until = origin;
    let mut last_challenge = false;
    while origin.elapsed() < Duration::from_secs(args.seconds) {
        let now = Instant::now();
        if !last_challenge && rx.try_recv().is_ok() {
            last_challenge = true;
            offer_until = now + Duration::from_millis(2500);
            report(origin, "challenge", serde_json::json!({"run_id":run}));
        }
        let wanted = if now < offer_until {
            0xB2
        } else if args.strategy == "a" ||
                  (args.strategy == "c" && (origin.elapsed().as_millis() as u64 % args.period_ms) < args.on_ms) {
            0xA0
        } else { 0 };
        if wanted != active_kind {
            if let Some(old) = active.take() {
                old.Stop()?;
                report(origin, "tx_stop", serde_json::json!({"kind":active_kind}));
            }
            active_kind = 0;
            if wanted != 0 {
                let next = publisher(wanted, run, origin)?;
                next.Start()?;
                active_kind = wanted;
                active = Some(next);
                report(origin, "tx_start", serde_json::json!({"kind":wanted}));
            }
        }
        flush_status(origin);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if let Some(old) = active { old.Stop()?; }
    adapter.stop_scan().await.context("stop watcher")?;
    flush_status(origin);
    report(origin, "end", serde_json::json!({"challenge_seen":last_challenge}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_is_bound_to_run() {
        assert!(challenge_matches(&frame(0xB1, 42), 42));
        assert!(!challenge_matches(&frame(0xB1, 42), 43));
        assert!(!challenge_matches(&frame(0xA0, 42), 42));
    }
}
