//! CoreWLAN backend. All calls happen on short-lived worker threads (CoreWLAN objects
//! are not `Send`, and scans/associations block for seconds), returning plain data.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::thread;

use objc2_core_wlan::{CWInterface, CWNetwork, CWSecurity, CWWiFiClient};
use objc2_foundation::NSString;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Network {
    pub ssid: String,
    pub rssi: i64,
    pub channel: i64,
    pub security: String,
    pub secured: bool,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Link {
    pub iface: String,
    pub powered: bool,
    pub ssid: Option<String>,
    pub bssid: Option<String>,
    pub rssi: i64,
    pub noise: i64,
    pub rate: f64,
    pub country: String,
}

pub enum Event {
    Link(u64, Link),
    Scan(Result<Vec<Network>, String>),
    Done(Result<String, String>),
}

fn iface() -> Option<objc2::rc::Retained<CWInterface>> {
    unsafe { CWWiFiClient::sharedWiFiClient().interface() }
}

fn security_name(n: &CWNetwork) -> &'static str {
    const ORDER: [(CWSecurity, &str); 8] = [
        (CWSecurity::WPA3Enterprise, "WPA3-Ent"),
        (CWSecurity::WPA3Personal, "WPA3"),
        (CWSecurity::WPA3Transition, "WPA3/2"),
        (CWSecurity::WPA2Enterprise, "WPA2-Ent"),
        (CWSecurity::WPA2Personal, "WPA2"),
        (CWSecurity::WPAPersonal, "WPA"),
        (CWSecurity::WEP, "WEP"),
        (CWSecurity::None, "open"),
    ];
    ORDER.iter().find(|(s, _)| unsafe { n.supportsSecurity(*s) }).map(|(_, l)| *l).unwrap_or("?")
}

/// SSID learned from the Location helper, used while macOS hides it from this process.
static HELPER_SSID: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

static SEQ: AtomicU64 = AtomicU64::new(0);
static REFRESHING: AtomicBool = AtomicBool::new(false);

fn link_event() -> Event {
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let mut l = link();
    // Associated (non-zero rate) but SSID hidden: use the name the helper reported.
    if l.ssid.is_none() && l.rate > 0.0 {
        l.ssid = HELPER_SSID.lock().ok().and_then(|g| g.clone());
    }
    Event::Link(seq, l)
}

pub fn link() -> Link {
    let Some(i) = iface() else { return Link::default() };
    unsafe {
        Link {
            iface: i.interfaceName().map(|s| s.to_string()).unwrap_or_default(),
            powered: i.powerOn(),
            ssid: i.ssid().map(|s| s.to_string()),
            bssid: i.bssid().map(|s| s.to_string()),
            rssi: i.rssiValue() as i64,
            noise: i.noiseMeasurement() as i64,
            rate: i.transmitRate(),
            country: i.countryCode().map(|s| s.to_string()).unwrap_or_default(),
        }
    }
}

pub fn scan() -> Result<Vec<Network>, String> {
    let i = iface().ok_or("no WiFi interface")?;
    let set = unsafe { i.scanForNetworksWithName_error(None) }.map_err(|e| e.localizedDescription().to_string())?;
    let mut out: Vec<Network> = set
        .iter()
        .map(|n| unsafe {
            let sec = security_name(&n);
            Network {
                ssid: n.ssid().map(|s| s.to_string()).unwrap_or_default(),
                rssi: n.rssiValue() as i64,
                channel: n.wlanChannel().map(|c| c.channelNumber() as i64).unwrap_or(0),
                security: sec.to_string(),
                secured: sec != "open",
            }
        })
        .collect();
    out.sort_by(|a, b| b.rssi.cmp(&a.rssi));
    Ok(out)
}

fn connect(ssid: &str, password: Option<&str>) -> Result<String, String> {
    let i = iface().ok_or("no WiFi interface")?;
    let name = NSString::from_str(ssid);
    // A directed scan also finds hidden networks.
    let set = unsafe { i.scanForNetworksWithName_error(Some(&name)) }.map_err(|e| e.localizedDescription().to_string())?;
    let net = set.iter().next().ok_or_else(|| format!("network '{ssid}' not found"))?;
    let pw = password.map(NSString::from_str);
    unsafe { i.associateToNetwork_password_error(&net, pw.as_deref()) }
        .map_err(|e| e.localizedDescription().to_string())?;
    Ok(format!("connected to {ssid}"))
}

/// Runs `job` off the UI thread and reports back over `tx`.
fn spawn(tx: &Sender<Event>, job: impl FnOnce(&Sender<Event>) + Send + 'static) {
    let tx = tx.clone();
    thread::spawn(move || job(&tx));
}

/// Periodic refresh; skipped while one is already running.
pub fn refresh_link(tx: &Sender<Event>) {
    if REFRESHING.swap(true, Ordering::SeqCst) {
        return;
    }
    spawn(tx, |tx| {
        let _ = tx.send(link_event());
        REFRESHING.store(false, Ordering::SeqCst);
    });
}

pub fn start_scan(tx: &Sender<Event>) {
    spawn(tx, |tx| {
        let mut result = scan();
        // Names come back blank without Location access; ask the helper app if installed.
        if result.as_ref().is_ok_and(|n| n.iter().any(|n| n.ssid.is_empty())) {
            if let Some(snap) = crate::helper::snapshot().filter(|s| s.authorized) {
                if let Ok(mut g) = HELPER_SSID.lock() {
                    *g = snap.link.ssid.clone();
                }
                result = Ok(snap.networks);
            }
        }
        let _ = tx.send(Event::Scan(result));
        let _ = tx.send(link_event());
    });
}

pub fn start_connect(tx: &Sender<Event>, ssid: String, password: Option<String>) {
    spawn(tx, move |tx| {
        let _ = tx.send(Event::Done(connect(&ssid, password.as_deref())));
        let _ = tx.send(link_event());
    });
}

pub fn start_disconnect(tx: &Sender<Event>) {
    spawn(tx, |tx| {
        if let Some(i) = iface() {
            unsafe { i.disassociate() };
        }
        let _ = tx.send(Event::Done(Ok("disconnected".into())));
        let _ = tx.send(link_event());
    });
}

pub fn start_power(tx: &Sender<Event>, on: bool) {
    spawn(tx, move |tx| {
        let r = match iface() {
            Some(i) => unsafe { i.setPower_error(on) }
                .map(|_| format!("wifi {}", if on { "on" } else { "off" }))
                .map_err(|e| e.localizedDescription().to_string()),
            None => Err("no WiFi interface".into()),
        };
        let _ = tx.send(Event::Done(r));
        let _ = tx.send(link_event());
    });
}
