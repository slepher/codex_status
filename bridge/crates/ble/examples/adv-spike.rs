//! Windows BLE advertisement publisher/watcher spike (Plan C task-5 §7.2 /
//! task-6 §1). Standalone harness: it never touches the production bridge state
//! machine and does not require the device.
//!
//! Measures, per round:
//!   - `Start() -> Status=Started` latency (publisher is best-effort, the OS
//!     may answer `Waiting` first);
//!   - whether the local watcher can receive our own beacon (loopback) and the
//!     first-packet latency from `Start()`;
//!   - whether third-party advertisements keep arriving while publishing;
//!   - `Stop() -> first advertisement received` recovery latency;
//!   - observed Manufacturer Specific Data payload length for the candidate
//!     24-byte application payload.
//!
//! Run (raw output to a file for artifacts):
//!   cargo run -p bridge-ble --example adv-spike -- --seconds 20 --rounds 3

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use btleplug::api::{Central, CentralEvent, Manager as _, ScanFilter};
use btleplug::platform::Manager;
use clap::Parser;
use futures::StreamExt;
use serde::Serialize;
use windows::core::Ref;
use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementPublisher, BluetoothLEAdvertisementPublisherStatusChangedEventArgs,
    BluetoothLEManufacturerData,
};
use windows::Foundation::TypedEventHandler;
use windows::Storage::Streams::DataWriter;

/// Bluetooth SIG reserved value for tests; production must use its own ID.
const TEST_COMPANY_ID: u16 = 0xFFFF;
const STATUS_STARTED: i32 = 2;

#[derive(Parser, Debug)]
#[command(
    name = "adv-spike",
    about = "Windows BLE publisher + watcher concurrency spike"
)]
struct Args {
    /// Company ID of the Manufacturer Specific Data (0xFFFF = reserved test value).
    #[arg(long, default_value_t = TEST_COMPANY_ID)]
    company: u16,
    /// Application payload bytes (candidate protocol is 24).
    #[arg(long, default_value_t = 24)]
    payload: usize,
    /// Seconds the publisher stays started in each round.
    #[arg(long, default_value_t = 20)]
    seconds: u64,
    /// Rounds: start -> hold -> stop -> recovery.
    #[arg(long, default_value_t = 3)]
    rounds: usize,
}

#[derive(Default)]
struct Record {
    own: Vec<(Instant, Vec<u8>)>,
    any: Vec<Instant>,
    status: Vec<(i32, Instant)>,
}

#[derive(Serialize)]
struct RoundReport {
    round: usize,
    started_ms: Option<f64>,
    own_first_ms: Option<f64>,
    own_count: usize,
    own_len: Option<usize>,
    any_before_stop: usize,
    stop_to_any_adv_ms: Option<f64>,
    statuses: Vec<i32>,
}

fn ms(from: Instant, to: Instant) -> f64 {
    to.saturating_duration_since(from).as_secs_f64() * 1000.0
}

fn wait_status(state: &Arc<Mutex<Record>>, want: i32, timeout: Duration) -> Option<Instant> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some((_, at)) = state
            .lock()
            .unwrap()
            .status
            .iter()
            .find(|(status, _)| *status == want)
        {
            return Some(*at);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_any_after(
    state: &Arc<Mutex<Record>>,
    after: Instant,
    timeout: Duration,
) -> Option<Instant> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(at) = state.lock().unwrap().any.iter().find(|at| **at > after) {
            return Some(*at);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let payload: Vec<u8> = (0..args.payload).map(|i| 0x40 + (i as u8 & 0x0F)).collect();

    let manager = Manager::new().await.context("bluetooth manager")?;
    let adapter = manager
        .adapters()
        .await
        .context("bluetooth adapters")?
        .into_iter()
        .next()
        .context("no bluetooth adapter")?;
    adapter
        .start_scan(ScanFilter::default())
        .await
        .context("start watcher scan")?;
    let mut events = adapter.events().await.context("watcher events")?;

    let state: Arc<Mutex<Record>> = Arc::new(Mutex::new(Record::default()));
    let watcher_state = state.clone();
    let company = args.company;
    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            let now = Instant::now();
            let mut rec = watcher_state.lock().unwrap();
            match event {
                CentralEvent::ManufacturerDataAdvertisement {
                    manufacturer_data, ..
                } => {
                    rec.any.push(now);
                    if let Some(bytes) = manufacturer_data.get(&company) {
                        rec.own.push((now, bytes.clone()));
                    }
                }
                CentralEvent::DeviceDiscovered(_) | CentralEvent::DeviceUpdated(_) => {
                    rec.any.push(now)
                }
                _ => {}
            }
        }
    });

    let mut reports: Vec<RoundReport> = Vec::new();
    for round in 0..args.rounds {
        let publisher = BluetoothLEAdvertisementPublisher::new().context("publisher")?;
        {
            let adv = publisher.Advertisement().context("advertisement")?;
            let md = BluetoothLEManufacturerData::new().context("manufacturer data")?;
            md.SetCompanyId(args.company)?;
            let writer = DataWriter::new().context("data writer")?;
            writer.WriteBytes(&payload)?;
            md.SetData(&writer.DetachBuffer()?)?;
            adv.ManufacturerData()
                .context("advertisement manufacturer data")?
                .Append(&md)?;
        }
        {
            let status_state = state.clone();
            let handler = TypedEventHandler::new(
                move |_sender: Ref<BluetoothLEAdvertisementPublisher>,
                      args: Ref<BluetoothLEAdvertisementPublisherStatusChangedEventArgs>| {
                    if let Ok(args) = args.ok() {
                        if let Ok(status) = args.Status() {
                            status_state
                                .lock()
                                .unwrap()
                                .status
                                .push((status.0, Instant::now()));
                        }
                    }
                    Ok(())
                },
            );
            publisher
                .StatusChanged(&handler)
                .context("status handler")?;
        }

        {
            let mut rec = state.lock().unwrap();
            rec.own.clear();
            rec.any.clear();
            rec.status.clear();
        }

        let start = Instant::now();
        publisher.Start().context("publisher start")?;
        let started = wait_status(&state, STATUS_STARTED, Duration::from_secs(2));
        tokio::time::sleep(Duration::from_secs(args.seconds)).await;
        publisher.Stop().context("publisher stop")?;
        let stop = Instant::now();
        let recovered = wait_any_after(&state, stop, Duration::from_secs(5));

        let (own_first, own_count, own_len, any_before_stop, statuses) = {
            let rec = state.lock().unwrap();
            let own_first = rec.own.first().map(|(at, _)| *at);
            let own_len = rec.own.first().map(|(_, bytes)| bytes.len());
            (
                own_first,
                rec.own.len(),
                own_len,
                rec.any.iter().filter(|at| **at < stop).count(),
                rec.status.iter().map(|(status, _)| *status).collect(),
            )
        };
        let report = RoundReport {
            round,
            started_ms: started.map(|at| ms(start, at)),
            own_first_ms: own_first.map(|at| ms(start, at)),
            own_count,
            own_len,
            any_before_stop,
            stop_to_any_adv_ms: recovered.map(|at| ms(stop, at)),
            statuses,
        };
        println!("{}", serde_json::to_string(&report)?);
        reports.push(report);

        if round + 1 < args.rounds {
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }

    let pct = |values: &mut Vec<f64>, p: f64| -> Option<f64> {
        if values.is_empty() {
            return None;
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((values.len() - 1) as f64 * p).round() as usize;
        Some(values[idx])
    };
    let mut started: Vec<f64> = reports.iter().filter_map(|r| r.started_ms).collect();
    let mut recovery: Vec<f64> = reports
        .iter()
        .filter_map(|r| r.stop_to_any_adv_ms)
        .collect();
    let mut own: Vec<f64> = reports.iter().filter_map(|r| r.own_first_ms).collect();
    println!(
        "{}",
        serde_json::json!({
            "summary": {
                "rounds": reports.len(),
                "started_p50_ms": pct(&mut started, 0.5),
                "started_p95_ms": pct(&mut started, 0.95),
                "own_loopback_rounds": own.len(),
                "own_first_p50_ms": pct(&mut own, 0.5),
                "stop_to_any_adv_p50_ms": pct(&mut recovery, 0.5),
                "stop_to_any_adv_p95_ms": pct(&mut recovery, 0.95),
            }
        })
    );

    let _ = adapter.stop_scan().await;
    Ok(())
}
