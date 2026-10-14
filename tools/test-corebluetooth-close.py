"""Replay production btleplug close/state handlers after remote removal.

Only CoreBluetooth objects/map entries are stubs; handlers and reply futures
are extracted/imported from the patched dependency. --out must already exist.
"""
import argparse
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--out', type=Path, required=True)
args = parser.parse_args()
assert args.out.is_dir(), 'pre-create output directory according to AGENTS.md'
backend = root / 'bridge/vendor/btleplug/src/corebluetooth'
source = (backend / 'internal.rs').read_text()

def method(name):
    marker = ('    pub fn ' if name == 'confirm_disconnect' else '    fn ') + name + '('
    start = source.index(marker)
    end = source.index('{', start) + 1
    depth = 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[start:end]

prefix = r'''
#![allow(dead_code, non_snake_case)]
macro_rules! trace { ($($args:tt)*) => {} }
use std::{collections::{HashMap,VecDeque}, future::Future, pin::Pin, sync::Arc,
    task::{Context, Poll, Wake, Waker}, cell::Cell};
type Uuid = u128;
#[derive(Debug, Clone, PartialEq)] enum CBPeripheralState { Connected, Disconnected }
#[derive(Debug, Clone, PartialEq)] enum CoreBluetoothReply { Ok, Err(String), State(CBPeripheralState) }
type CoreBluetoothReplyStateShared = BtlePlugFutureStateShared<CoreBluetoothReply>;
struct Peripheral;
impl Peripheral { unsafe fn state(&self) -> CBPeripheralState { CBPeripheralState::Connected } }
#[derive(Default)] struct Manager { cancels: Cell<usize> }
impl Manager { unsafe fn cancelPeripheralConnection(&self, _: &Peripheral) {
    self.cancels.set(self.cancels.get() + 1);
}
unsafe fn connectPeripheral_options(&self,_:&Peripheral,_:Option<()>) {}
}
struct CharacteristicInternal { read_future_state:VecDeque<CoreBluetoothReplyStateShared>,write_future_state:VecDeque<CoreBluetoothReplyStateShared>,subscribe_future_state:VecDeque<CoreBluetoothReplyStateShared>,unsubscribe_future_state:VecDeque<CoreBluetoothReplyStateShared> }
struct Service { characteristics:HashMap<u8,CharacteristicInternal> }
struct Entry { peripheral: Peripheral, connected_future_state:Option<CoreBluetoothReplyStateShared>,disconnected_future_state: Option<CoreBluetoothReplyStateShared>,services:HashMap<u8,Service> }
#[derive(Default)] struct Central { peripherals: HashMap<Uuid, Entry>, manager: Manager }
impl Central {
'''
checks = r'''
struct Noop; impl Wake for Noop { fn wake(self: Arc<Self>) {} }
fn poll(f: &mut BtlePlugFuture<CoreBluetoothReply>) -> Poll<CoreBluetoothReply> {
    Pin::new(f).poll(&mut Context::from_waker(&Waker::from(Arc::new(Noop))))
}
#[test] fn remotely_removed_peripheral_closes_without_another_native_cancel() {
    let mut c = Central::default();
    c.peripherals.insert(7, Entry { peripheral: Peripheral, connected_future_state:None,disconnected_future_state: None,services:HashMap::new() });
    c.peripherals.remove(&7); // native disconnected callback ran before queued close
    for _ in 0..3 {
        let mut f = BtlePlugFuture::default();
        assert_eq!(poll(&mut f), Poll::Pending);
        c.disconnect_peripheral(7, f.get_state_clone());
        assert_eq!(poll(&mut f), Poll::Ready(CoreBluetoothReply::Ok));
    }
    assert_eq!(c.manager.cancels.get(), 0);
}
#[test] fn missing_peripheral_state_read_completes_as_disconnected() {
    let mut c = Central::default(); let mut f = BtlePlugFuture::default();
    c.is_connected(7, f.get_state_clone());
    assert_eq!(poll(&mut f), Poll::Ready(CoreBluetoothReply::State(CBPeripheralState::Disconnected)));
}
#[test] fn live_peripheral_still_waits_for_native_confirmation() {
    let mut c = Central::default();
    c.peripherals.insert(7, Entry { peripheral: Peripheral, connected_future_state:None,disconnected_future_state: None,services:HashMap::new() });
    let mut f = BtlePlugFuture::default(); c.disconnect_peripheral(7, f.get_state_clone());
    assert_eq!(c.manager.cancels.get(), 1);
    assert_eq!(poll(&mut f), Poll::Pending); // never turn an uncertain close into success
    let mut p = c.peripherals.remove(&7).unwrap();
    p.disconnected_future_state.take().unwrap().lock().unwrap().set_reply(CoreBluetoothReply::Ok);
    assert_eq!(poll(&mut f), Poll::Ready(CoreBluetoothReply::Ok));
}
#[test] fn existing_peripheral_state_is_read_from_native_object() {
    let mut c = Central::default();
    c.peripherals.insert(7, Entry { peripheral: Peripheral, connected_future_state:None,disconnected_future_state: None,services:HashMap::new() });
    let mut f = BtlePlugFuture::default(); c.is_connected(7, f.get_state_clone());
    assert_eq!(poll(&mut f), Poll::Ready(CoreBluetoothReply::State(CBPeripheralState::Connected)));
}
#[test] fn terminal_disconnect_finishes_discovery_connect_and_repeated_callback_is_safe() {
    let mut connect = BtlePlugFuture::default(); let mut close = BtlePlugFuture::default();
    let mut p = Entry {peripheral:Peripheral,connected_future_state:Some(connect.get_state_clone()),disconnected_future_state:Some(close.get_state_clone()),services:HashMap::new()};
    p.confirm_disconnect();p.confirm_disconnect();
    assert_eq!(poll(&mut connect),Poll::Ready(CoreBluetoothReply::Err("Device disconnected".into())));
    assert_eq!(poll(&mut close),Poll::Ready(CoreBluetoothReply::Ok));
}
#[test] fn missing_peripheral_connect_is_a_definite_error() {
    let mut c = Central::default();let mut f = BtlePlugFuture::default();
    c.connect_peripheral(7,f.get_state_clone());
    assert_eq!(poll(&mut f),Poll::Ready(CoreBluetoothReply::Err("Device disconnected".into())));
}
#[test] fn repeated_connection_failure_with_no_pending_request_is_safe() {
    let mut c = Central::default();let mut f = BtlePlugFuture::default();
    c.peripherals.insert(7,Entry{peripheral:Peripheral,connected_future_state:Some(f.get_state_clone()),disconnected_future_state:None,services:HashMap::new()});
    c.on_peripheral_connection_failed(7,Some("failed".into()));
    c.on_peripheral_connection_failed(7,Some("late callback".into()));
    assert_eq!(poll(&mut f),Poll::Ready(CoreBluetoothReply::Err("failed".into())));
}
'''
future = f'#[path = "{backend.parent / "connection_lifecycle.rs"}"] mod connection_lifecycle;\nuse connection_lifecycle::future::*;\n'
path = args.out / 'corebluetooth-close-test.rs'
path.write_text(prefix + method('connect_peripheral') + method('disconnect_peripheral') + method('is_connected') + method('on_peripheral_connection_failed') + '\n}\nimpl Entry {\n' + method('confirm_disconnect') + '\n}\n' + checks + future)
binary = args.out / 'corebluetooth-close-test'
subprocess.run(['rustc', '--edition=2021', '--test', str(path), '-o', str(binary)], check=True)
subprocess.run([str(binary.resolve())], check=True)
