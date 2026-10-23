//! IOBluetooth backend. Calls run on short-lived worker threads and return plain data;
//! discovery spins the thread's run loop because IOBluetooth delivers results through it.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread;

use objc2::rc::Retained;
use objc2_io_bluetooth::BluetoothPINCode;

use crate::pairing::{PairDelegate, Req};
use objc2_foundation::{NSArray, NSDate, NSObjectProtocol, NSRunLoop, NSString};
use objc2_io_bluetooth::{IOBluetoothDevice, IOBluetoothDeviceInquiry, IOBluetoothDevicePair, IOBluetoothHostController};

#[derive(Clone, Debug)]
pub struct Device {
    pub name: String,
    pub address: String,
    pub connected: bool,
    pub paired: bool,
    pub rssi: Option<i8>,
    pub kind: &'static str,
}

pub enum PromptKind {
    Pin,
    Confirm(u32),
}

pub enum Reply {
    Pin(String),
    Confirm(bool),
    Cancel,
}

pub enum Event {
    Devices { seq: u64, devices: Vec<Device>, powered: bool },
    Discovered(Vec<Device>),
    DiscoveryDone,
    PairPrompt { device: String, kind: PromptKind, reply: Sender<Reply> },
    Done(Result<String, String>),
}

fn kind(major: u32) -> &'static str {
    match major {
        1 => "computer",
        2 => "phone",
        3 => "network",
        4 => "audio",
        5 => "input",
        6 => "imaging",
        7 => "wearable",
        8 => "toy",
        9 => "health",
        _ => "device",
    }
}

fn convert(d: &IOBluetoothDevice) -> Device {
    unsafe {
        let rssi = d.rawRSSI();
        let address = d.addressString().map(|s| s.to_string()).unwrap_or_default();
        Device {
            name: d.nameOrAddress().map(|s| s.to_string()).unwrap_or_else(|| address.clone()),
            address,
            connected: d.isConnected(),
            paired: d.isPaired(),
            rssi: (rssi != 127 && rssi != 0).then_some(rssi),
            kind: kind(d.deviceClassMajor() as u32),
        }
    }
}

fn devices_of(arr: Option<Retained<NSArray>>) -> Vec<Device> {
    arr.map(|a| {
        a.iter().filter_map(|o| o.downcast::<IOBluetoothDevice>().ok()).map(|d| convert(&d)).collect()
    })
    .unwrap_or_default()
}

fn find(address: &str) -> Result<Retained<IOBluetoothDevice>, String> {
    unsafe { IOBluetoothDevice::deviceWithAddressString(Some(&NSString::from_str(address))) }
        .ok_or_else(|| format!("no device {address}"))
}

fn powered() -> bool {
    // The generated binding declares an unsigned return, but the method returns `int`
    // and objc2 panics on the mismatch, so send the message by hand.
    unsafe {
        IOBluetoothHostController::defaultController().is_some_and(|c| {
            let state: i32 = objc2::msg_send![&*c, powerState];
            state == 1
        })
    }
}

fn spawn(tx: &Sender<Event>, job: impl FnOnce(&Sender<Event>) + Send + 'static) {
    let tx = tx.clone();
    thread::spawn(move || {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(&tx))).is_err();
        if panicked {
            REFRESHING.store(false, Ordering::SeqCst);
            let _ = tx.send(Event::Done(Err("Bluetooth call failed (see macutil-bt --probe)".into())));
            let _ = tx.send(Event::DiscoveryDone);
        }
    });
}

static SEQ: AtomicU64 = AtomicU64::new(0);
static REFRESHING: AtomicBool = AtomicBool::new(false);

fn snapshot() -> Event {
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let mut devices = devices_of(unsafe { IOBluetoothDevice::pairedDevices() });
    devices.sort_by(|a, b| b.connected.cmp(&a.connected).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Event::Devices { seq, devices, powered: powered() }
}

/// Periodic refresh; skipped while one is already running.
pub fn refresh(tx: &Sender<Event>) {
    if REFRESHING.swap(true, Ordering::SeqCst) {
        return;
    }
    spawn(tx, |tx| {
        let _ = tx.send(snapshot());
        REFRESHING.store(false, Ordering::SeqCst);
    });
}

fn io(code: i32, ok: String, what: &str) -> Result<String, String> {
    if code == 0 { Ok(ok) } else { Err(format!("{what} failed (IOReturn {code:#x})")) }
}

pub fn connect(tx: &Sender<Event>, address: String) {
    spawn(tx, move |tx| {
        let r = find(&address).and_then(|d| unsafe {
            let name = d.nameOrAddress().map(|s| s.to_string()).unwrap_or_default();
            io(d.openConnection(), format!("connected to {name}"), "connect")
        });
        let _ = tx.send(Event::Done(r));
        let _ = tx.send(snapshot());
    });
}

pub fn disconnect(tx: &Sender<Event>, address: String) {
    spawn(tx, move |tx| {
        let r = find(&address).and_then(|d| unsafe {
            let name = d.nameOrAddress().map(|s| s.to_string()).unwrap_or_default();
            io(d.closeConnection(), format!("disconnected {name}"), "disconnect")
        });
        let _ = tx.send(Event::Done(r));
        let _ = tx.send(snapshot());
    });
}

/// Pairs with a device. PIN and confirmation requests are forwarded to the UI, which
/// answers over `reply`; the run loop is spun here so delegate callbacks are delivered.
pub fn pair(tx: &Sender<Event>, address: String) {
    spawn(tx, move |tx| {
        let r = find(&address).and_then(|d| unsafe {
            let name = d.nameOrAddress().map(|s| s.to_string()).unwrap_or_default();
            let (req_tx, req_rx) = mpsc::channel();
            let (reply_tx, reply_rx) = mpsc::channel::<Reply>();
            let delegate = PairDelegate::new(req_tx);
            let p = IOBluetoothDevicePair::pairWithDevice(Some(&d)).ok_or("could not create pairing session")?;
            p.setDelegate(Some(&delegate));
            let code = p.start();
            if code != 0 {
                return Err(format!("pairing failed to start (IOReturn {code:#x})"));
            }
            // ~2 minutes overall, enough to read a code and answer.
            for _ in 0..1200 {
                spin(0.1);
                while let Ok(req) = req_rx.try_recv() {
                    match req {
                        Req::Finished(0) => return Ok(format!("paired {name}")),
                        Req::Finished(c) => return Err(format!("pairing failed (status {c:#x})")),
                        Req::Pin => {
                            let _ = tx.send(Event::PairPrompt { device: name.clone(), kind: PromptKind::Pin, reply: reply_tx.clone() });
                        }
                        Req::Confirm(v) => {
                            let _ = tx.send(Event::PairPrompt { device: name.clone(), kind: PromptKind::Confirm(v), reply: reply_tx.clone() });
                        }
                    }
                }
                while let Ok(reply) = reply_rx.try_recv() {
                    match reply {
                        Reply::Confirm(yes) => p.replyUserConfirmation(yes),
                        Reply::Pin(text) => {
                            let mut pin = BluetoothPINCode { data: [0; 16] };
                            let bytes = text.as_bytes();
                            let n = bytes.len().min(16);
                            pin.data[..n].copy_from_slice(&bytes[..n]);
                            p.replyPINCode_PINCode(n as _, &mut pin);
                        }
                        Reply::Cancel => {
                            p.stop();
                            return Err("pairing cancelled".to_string());
                        }
                    }
                }
                if d.isPaired() {
                    return Ok(format!("paired {name}"));
                }
            }
            p.stop();
            Err("pairing timed out".to_string())
        });
        let _ = tx.send(Event::Done(r));
        let _ = tx.send(snapshot());
    });
}

/// Removes a paired device. IOBluetooth has no public API for this; `-remove` is the
/// selector System Settings itself uses.
pub fn forget(tx: &Sender<Event>, address: String) {
    spawn(tx, move |tx| {
        let r = find(&address).and_then(|d| unsafe {
            // Private selector: an unrecognised one would raise an uncatchable ObjC exception.
            let sel = objc2::sel!(remove);
            if !d.respondsToSelector(sel) {
                return Err("this macOS version can't forget devices from here".to_string());
            }
            let _: () = objc2::msg_send![&*d, remove];
            Ok("removed device".to_string())
        });
        let _ = tx.send(Event::Done(r));
        let _ = tx.send(snapshot());
    });
}

fn spin(secs: f64) {
    NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(secs));
}

/// Inquiry for nearby devices (~`secs` seconds), streaming results as they appear.
pub fn discover(tx: &Sender<Event>, secs: u8) {
    spawn(tx, move |tx| unsafe {
        let Some(inq) = IOBluetoothDeviceInquiry::inquiryWithDelegate(None) else {
            let _ = tx.send(Event::Done(Err("inquiry unavailable".into())));
            let _ = tx.send(Event::DiscoveryDone);
            return;
        };
        inq.setInquiryLength(secs);
        inq.setUpdateNewDeviceNames(true);
        inq.clearFoundDevices();
        if inq.start() != 0 {
            let _ = tx.send(Event::Done(Err("could not start inquiry (is Bluetooth on?)".into())));
            let _ = tx.send(Event::DiscoveryDone);
            return;
        }
        let mut seen = HashSet::new();
        for _ in 0..(secs as u32 * 2 + 4) {
            spin(0.5);
            let found = devices_of(inq.foundDevices());
            seen.extend(found.iter().map(|d| d.address.clone()));
            let _ = tx.send(Event::Discovered(found));
        }
        inq.stop();
        let _ = tx.send(Event::DiscoveryDone);
        let _ = tx.send(Event::Done(Ok(format!("found {} device(s)", seen.len()))));
    });
}
