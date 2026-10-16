#!/usr/bin/env python3
"""Replay production discovery-error branches and completion handler; no radio access."""
import pathlib
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
NATIVE = ROOT / "bridge/vendor/btleplug/src/corebluetooth"


def block(source, marker):
    start = source.index(marker)
    brace = source.index("{", start + len(marker))
    depth = 1
    end = brace + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def main():
    delegate = (NATIVE / "central_delegate.rs").read_text()
    internal = (NATIVE / "internal.rs").read_text()
    callbacks = []
    for name, stage in [
        ("delegate_peripheral_diddiscoverservices", "services"),
        ("delegate_peripheral_diddiscovercharacteristicsforservice_error", "characteristics"),
        ("delegate_peripheral_diddiscoverdescriptorsforcharacteristic_error", "descriptors"),
    ]:
        method = block(delegate, "fn " + name + "(")
        failure = block(method, "if error.is_some()")
        callbacks.append("fn " + stage + "(&self, peripheral: &Peripheral, error: Option<()>) {" + failure + "}")
    route = block(internal, "CentralDelegateEvent::DiscoveryFailed { peripheral_uuid, stage } =>")
    failure_handler = block(internal, "fn on_peripheral_connection_failed(")
    lifecycle = ROOT / "bridge/vendor/btleplug/src/connection_lifecycle.rs"
    source = r'''
#![allow(dead_code,unused_unsafe)]
macro_rules! trace {($($args:tt)*)=>{}}
macro_rules! warn {($($args:tt)*)=>{}}
use std::{collections::HashMap,cell::RefCell,future::Future,pin::Pin,sync::Arc,task::{Context,Poll,Wake,Waker}};
type Uuid=u128;
#[derive(Debug,PartialEq)] enum CoreBluetoothReply { Err(String) }
struct Peripheral; impl Peripheral { fn identifier(&self)->&Uuid { &7 } }
fn nsuuid_to_uuid(id:&Uuid)->Uuid { *id }
enum CentralDelegateEvent { DiscoveryFailed { peripheral_uuid:Uuid,stage:&'static str } }
#[derive(Default)] struct Delegate { events:RefCell<Vec<CentralDelegateEvent>> }
impl Delegate {
 fn send_event(&self,e:CentralDelegateEvent) {self.events.borrow_mut().push(e);}
 CALLBACKS
}
struct Pending { connected_future_state:Option<connection_lifecycle::future::BtlePlugFutureStateShared<CoreBluetoothReply>> }
struct Central { peripherals:HashMap<Uuid,Pending> }
impl Central {
 HANDLER
 fn dispatch(&mut self,e:CentralDelegateEvent) {match e {ROUTE}}
}
struct Noop;impl Wake for Noop {fn wake(self:Arc<Self>) {}}
fn poll(f:&mut connection_lifecycle::future::BtlePlugFuture<CoreBluetoothReply>)->Poll<CoreBluetoothReply> {
 Pin::new(f).poll(&mut Context::from_waker(&Waker::from(Arc::new(Noop))))
}
fn main() {
 for stage in ["services","characteristics","descriptors"] {
  let delegate=Delegate::default();
  let mut future=connection_lifecycle::future::BtlePlugFuture::default();
  let mut central=Central{peripherals:HashMap::from([(7,Pending{connected_future_state:Some(future.get_state_clone())})])};
  assert_eq!(poll(&mut future),Poll::Pending);
  for error in [None,Some(())] {
   match stage {"services"=>delegate.services(&Peripheral,error),"characteristics"=>delegate.characteristics(&Peripheral,error),_=>delegate.descriptors(&Peripheral,error)}
  }
  assert_eq!(delegate.events.borrow().len(),1);
  for e in delegate.events.into_inner() {central.dispatch(e);}
  assert_eq!(poll(&mut future),Poll::Ready(CoreBluetoothReply::Err(format!("ble_discovery_failed:{stage}"))));
  central.dispatch(CentralDelegateEvent::DiscoveryFailed{peripheral_uuid:7,stage});
  println!("PASS native callback -> discovery error -> original connect future: {stage}");
 }
}
'''
    source = source.replace("CALLBACKS", "\n".join(callbacks)).replace("HANDLER", failure_handler).replace("ROUTE", route)
    source += f'\n#[path = "{lifecycle}"] mod connection_lifecycle;\n'
    # Caller runs elevated when required by repository directory-creation rules.
    with tempfile.TemporaryDirectory(prefix="bridge-discovery-") as temporary:
        path = pathlib.Path(temporary)
        (path / "replay.rs").write_text(source)
        subprocess.run(["rustc", "--edition=2021", str(path / "replay.rs"), "-o", str(path / "replay")], check=True)
        subprocess.run([str(path / "replay")], check=True)


if __name__ == "__main__":
    main()
