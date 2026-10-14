//! Shared request completion logic for native and simulated peripherals.
// The future implementation is the same one used by CoreBluetooth.
#[path = "corebluetooth/future.rs"]
pub mod future;
use future::BtlePlugFutureStateShared;

/// A terminal callback must finish a connect still waiting for discovery.
pub fn complete<R>(pending: &mut Option<BtlePlugFutureStateShared<R>>, result: R) {
    if let Some(reply) = pending.take() { reply.lock().unwrap().set_reply(result); }
}

/// A stale handle is a definite failure, not an unbounded native request.
pub fn connect<P, R>(peripheral: Option<&mut P>, reply: BtlePlugFutureStateShared<R>,
    missing: R, start: impl FnOnce(&mut P, BtlePlugFutureStateShared<R>)) {
    if let Some(peripheral) = peripheral { start(peripheral, reply); }
    else { reply.lock().unwrap().set_reply(missing); }
}

/// Present peripherals need a native callback; an absent peripheral is already closed.
pub fn disconnect<P, R>(peripheral: Option<&mut P>, reply: BtlePlugFutureStateShared<R>,
    closed: R, start: impl FnOnce(&mut P, BtlePlugFutureStateShared<R>)) {
    if let Some(peripheral) = peripheral {
        start(peripheral, reply);
    } else {
        reply.lock().unwrap().set_reply(closed);
    }
}

/// A removed peripheral must answer state queries too.
pub fn state<P, R>(peripheral: Option<&P>, reply: BtlePlugFutureStateShared<R>,
    disconnected: R, read: impl FnOnce(&P) -> R) {
    reply.lock().unwrap().set_reply(peripheral.map(read).unwrap_or(disconnected));
}
