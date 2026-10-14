//! Logical native peripheral proxy. HTTP only carries simulated device events.
use btleplug::connection_lifecycle::{self, future::{BtlePlugFuture, BtlePlugFutureStateShared}};
use serde_json::Value;

#[derive(Default)]
struct Peripheral { closing: Option<BtlePlugFutureStateShared<()>> }

/// Native connect completes only after discovery. A device disconnect callback
/// before discovery must finish the pending request, using the production sink.
pub(super) async fn discover(url:&str,token:&str,generation:u64) -> anyhow::Result<()> {
    let future=BtlePlugFuture::default();let mut pending=None;
    connection_lifecycle::connect(Some(&mut pending),future.get_state_clone(),Err("Device disconnected"),
        |pending,reply| *pending=Some(reply));
    let device:Value=reqwest::Client::new().post(format!("{url}/sim/ble/discover"))
        .bearer_auth(token).header("x-sim-ble-generation",generation.to_string())
        .send().await?.error_for_status()?.json().await?;
    let connected=device["connected"]==true && device["generation"].as_u64()==Some(generation);
    connection_lifecycle::complete(&mut pending,if connected{Ok(())}else{Err("Device disconnected")});
    future.await.map_err(|error|anyhow::anyhow!(error))
}

pub(super) async fn disconnect(url: &str, token: &str, generation: u64) -> btleplug::Result<()> {
    async fn run(url: &str, token: &str, generation: u64) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let device: Value = client.get(format!("{url}/sim/ble/device"))
            .bearer_auth(token).send().await?.error_for_status()?.json().await?;
        let current = device["generation"].as_u64().ok_or_else(|| anyhow::anyhow!("missing generation"))?;
        let connected = device["connected"].as_bool().ok_or_else(|| anyhow::anyhow!("missing connected state"))?;
        let callback = device["close_callback"].as_bool().ok_or_else(|| anyhow::anyhow!("missing callback policy"))?;
        // Processing the device's disconnect event removes this generation from
        // the central table. A later generation belongs to a different handle.
        let mut peripheral = (current == generation && connected).then(Peripheral::default);
        let state = BtlePlugFuture::default();
        connection_lifecycle::state(peripheral.as_ref(), state.get_state_clone(), false, |_| true);
        let _connected = state.await;
        let closed = BtlePlugFuture::default();
        connection_lifecycle::disconnect(peripheral.as_mut(), closed.get_state_clone(), (),
            |p, reply| p.closing = Some(reply));
        if let Some(p) = &mut peripheral {
            client.post(format!("{url}/sim/ble/close"))
                .header("x-sim-ble-generation", generation.to_string())
                .bearer_auth(token).send().await?.error_for_status()?;
            // An HTTP response proves carrier delivery, not a native callback.
            if callback { p.closing.take().unwrap().lock().unwrap().set_reply(()); }
        }
        closed.await;
        Ok(())
    }
    run(url, token, generation).await.map_err(|e| btleplug::Error::RuntimeError(e.to_string()))
}
