//! CoreAudio backend, called directly through the C property API (the handful of functions
//! needed don't justify a binding crate). Every call is a quick synchronous property read or
//! write on the HAL, so the UI calls these on its own thread.

use std::ffi::c_void;
use std::ptr;

type ObjectId = u32;
type OsStatus = i32;

#[repr(C)]
struct Address {
    selector: u32,
    scope: u32,
    element: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ValueRange {
    min: f64,
    max: f64,
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectHasProperty(id: ObjectId, addr: *const Address) -> u8;
    fn AudioObjectIsPropertySettable(id: ObjectId, addr: *const Address, out: *mut u8) -> OsStatus;
    fn AudioObjectGetPropertyDataSize(id: ObjectId, addr: *const Address, qsize: u32, qdata: *const c_void, size: *mut u32) -> OsStatus;
    fn AudioObjectGetPropertyData(id: ObjectId, addr: *const Address, qsize: u32, qdata: *const c_void, size: *mut u32, data: *mut c_void) -> OsStatus;
    fn AudioObjectSetPropertyData(id: ObjectId, addr: *const Address, qsize: u32, qdata: *const c_void, size: u32, data: *const c_void) -> OsStatus;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringGetLength(s: *const c_void) -> isize;
    fn CFStringGetCString(s: *const c_void, buf: *mut u8, len: isize, encoding: u32) -> u8;
    fn CFRelease(o: *const c_void);
}

const fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

const SYSTEM: ObjectId = 1;
const MAIN: u32 = 0;
const GLOBAL: u32 = fourcc(b"glob");
const INPUT: u32 = fourcc(b"inpt");
const OUTPUT: u32 = fourcc(b"outp");

const DEVICES: u32 = fourcc(b"dev#");
const DEFAULT_OUTPUT: u32 = fourcc(b"dOut");
const DEFAULT_INPUT: u32 = fourcc(b"dIn ");
const DEFAULT_SYSTEM: u32 = fourcc(b"sOut");
const NAME: u32 = fourcc(b"lnam");
const STREAMS: u32 = fourcc(b"stm#");
const TRANSPORT: u32 = fourcc(b"tran");
const HIDDEN: u32 = fourcc(b"hidn");
const VOLUME: u32 = fourcc(b"volm");
const VIRTUAL_VOLUME: u32 = fourcc(b"vmvc");
const MUTE: u32 = fourcc(b"mute");
const RATE: u32 = fourcc(b"nsrt");
const RATES: u32 = fourcc(b"nsr#");

const UTF8: u32 = 0x0800_0100;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Dir {
    Output,
    Input,
}

impl Dir {
    fn scope(self) -> u32 {
        match self {
            Dir::Output => OUTPUT,
            Dir::Input => INPUT,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Device {
    pub id: ObjectId,
    pub name: String,
    pub kind: &'static str,
    pub default: bool,
    /// Output only: the device alerts and sound effects play through.
    pub system: bool,
    /// 0.0–1.0, `None` when the device has no software volume (HDMI, many USB interfaces).
    pub volume: Option<f32>,
    pub muted: Option<bool>,
    pub rate: Option<f64>,
}

fn addr(selector: u32, scope: u32, element: u32) -> Address {
    Address { selector, scope, element }
}

fn has(id: ObjectId, a: &Address) -> bool {
    unsafe { AudioObjectHasProperty(id, a) != 0 }
}

fn settable(id: ObjectId, a: &Address) -> bool {
    let mut out = 0u8;
    has(id, a) && unsafe { AudioObjectIsPropertySettable(id, a, &mut out) } == 0 && out != 0
}

fn get<T: Copy + Default>(id: ObjectId, a: &Address) -> Option<T> {
    let mut v = T::default();
    let mut size = std::mem::size_of::<T>() as u32;
    let st = unsafe { AudioObjectGetPropertyData(id, a, 0, ptr::null(), &mut size, &mut v as *mut T as *mut c_void) };
    (st == 0).then_some(v)
}

fn get_vec<T: Copy + Default>(id: ObjectId, a: &Address) -> Vec<T> {
    let mut size = 0u32;
    if unsafe { AudioObjectGetPropertyDataSize(id, a, 0, ptr::null(), &mut size) } != 0 {
        return vec![];
    }
    let mut v = vec![T::default(); size as usize / std::mem::size_of::<T>()];
    let st = unsafe { AudioObjectGetPropertyData(id, a, 0, ptr::null(), &mut size, v.as_mut_ptr() as *mut c_void) };
    if st != 0 {
        return vec![];
    }
    v.truncate(size as usize / std::mem::size_of::<T>());
    v
}

fn set<T: Copy>(id: ObjectId, a: &Address, v: T) -> Result<(), String> {
    let st = unsafe { AudioObjectSetPropertyData(id, a, 0, ptr::null(), std::mem::size_of::<T>() as u32, &v as *const T as *const c_void) };
    if st == 0 { Ok(()) } else { Err(format!("CoreAudio error {st}")) }
}

fn string(id: ObjectId, a: &Address) -> Option<String> {
    let s: usize = get(id, a)?;
    if s == 0 {
        return None;
    }
    let s = s as *const c_void;
    unsafe {
        // UTF-8 needs at most 4 bytes per UTF-16 unit, plus the terminator.
        let cap = CFStringGetLength(s) * 4 + 1;
        let mut buf = vec![0u8; cap as usize];
        let ok = CFStringGetCString(s, buf.as_mut_ptr(), cap, UTF8) != 0;
        CFRelease(s);
        if !ok {
            return None;
        }
        buf.truncate(buf.iter().position(|&b| b == 0).unwrap_or(buf.len()));
        String::from_utf8(buf).ok()
    }
}

fn kind(transport: u32) -> &'static str {
    match &transport.to_be_bytes() {
        b"bltn" => "built-in",
        b"usb " => "usb",
        b"blue" | b"blea" => "bluetooth",
        b"hdmi" => "hdmi",
        b"dprt" => "displayport",
        b"airp" => "airplay",
        b"thun" => "thunderbolt",
        b"1394" => "firewire",
        b"pci " => "pci",
        b"virt" => "virtual",
        b"grup" | b"agrp" => "aggregate",
        b"ccwd" | b"ccwl" => "continuity",
        _ => "device",
    }
}

/// The elements that carry this device's volume: the main element if it has one, else each
/// channel. Apps like Sound settings drive the "virtual main" volume, which only some drivers expose.
fn volume_elements(id: ObjectId, scope: u32) -> Vec<Address> {
    let main = addr(VOLUME, scope, MAIN);
    if has(id, &main) {
        return vec![main];
    }
    (1..=2).map(|ch| addr(VOLUME, scope, ch)).filter(|a| has(id, a)).collect()
}

fn volume(id: ObjectId, scope: u32) -> Option<f32> {
    let virt = addr(VIRTUAL_VOLUME, scope, MAIN);
    if has(id, &virt) {
        return get(id, &virt);
    }
    let vals: Vec<f32> = volume_elements(id, scope).iter().filter_map(|a| get(id, a)).collect();
    (!vals.is_empty()).then(|| vals.iter().sum::<f32>() / vals.len() as f32)
}

fn mute_addr(id: ObjectId, scope: u32) -> Option<Address> {
    let a = addr(MUTE, scope, MAIN);
    has(id, &a).then_some(a)
}

/// All visible devices that have streams in `dir`, the default one marked.
pub fn devices(dir: Dir) -> Vec<Device> {
    let scope = dir.scope();
    let default: ObjectId = get(SYSTEM, &addr(if dir == Dir::Output { DEFAULT_OUTPUT } else { DEFAULT_INPUT }, GLOBAL, MAIN)).unwrap_or(0);
    let system: ObjectId = get(SYSTEM, &addr(DEFAULT_SYSTEM, GLOBAL, MAIN)).unwrap_or(0);
    get_vec::<ObjectId>(SYSTEM, &addr(DEVICES, GLOBAL, MAIN))
        .into_iter()
        .filter(|&id| !get_vec::<ObjectId>(id, &addr(STREAMS, scope, MAIN)).is_empty())
        .filter(|&id| get::<u32>(id, &addr(HIDDEN, GLOBAL, MAIN)).unwrap_or(0) == 0)
        .map(|id| Device {
            id,
            name: string(id, &addr(NAME, GLOBAL, MAIN)).unwrap_or_else(|| format!("device {id}")),
            kind: kind(get(id, &addr(TRANSPORT, GLOBAL, MAIN)).unwrap_or(0)),
            default: id == default,
            system: dir == Dir::Output && id == system,
            volume: volume(id, scope),
            muted: mute_addr(id, scope).and_then(|a| get::<u32>(id, &a)).map(|m| m != 0),
            rate: get(id, &addr(RATE, GLOBAL, MAIN)),
        })
        .collect()
}

pub fn set_default(dir: Dir, id: ObjectId) -> Result<(), String> {
    set(SYSTEM, &addr(if dir == Dir::Output { DEFAULT_OUTPUT } else { DEFAULT_INPUT }, GLOBAL, MAIN), id)
}

pub fn set_system(id: ObjectId) -> Result<(), String> {
    set(SYSTEM, &addr(DEFAULT_SYSTEM, GLOBAL, MAIN), id)
}

pub fn set_volume(dir: Dir, id: ObjectId, v: f32) -> Result<(), String> {
    let scope = dir.scope();
    let v = v.clamp(0.0, 1.0);
    let virt = addr(VIRTUAL_VOLUME, scope, MAIN);
    if settable(id, &virt) {
        return set(id, &virt, v);
    }
    let els: Vec<Address> = volume_elements(id, scope).into_iter().filter(|a| settable(id, a)).collect();
    if els.is_empty() {
        return Err("this device has no adjustable volume".into());
    }
    els.iter().try_for_each(|a| set(id, a, v))
}

pub fn toggle_mute(dir: Dir, id: ObjectId) -> Result<bool, String> {
    let scope = dir.scope();
    let a = mute_addr(id, scope).filter(|a| settable(id, a)).ok_or("this device can't be muted")?;
    let now = get::<u32>(id, &a).unwrap_or(0) == 0;
    set(id, &a, now as u32)?;
    Ok(now)
}

/// Sample rates the device supports. Ranges that span more than one value (some USB
/// interfaces report e.g. 44.1k–192k as one range) are expanded to the common rates in them.
pub fn rates(id: ObjectId) -> Vec<f64> {
    const COMMON: [f64; 8] = [8000.0, 16000.0, 22050.0, 44100.0, 48000.0, 88200.0, 96000.0, 192000.0];
    let mut out: Vec<f64> = get_vec::<ValueRange>(id, &addr(RATES, GLOBAL, MAIN))
        .into_iter()
        .flat_map(|r| {
            if r.min == r.max { vec![r.min] } else { COMMON.into_iter().filter(|c| (r.min..=r.max).contains(c)).collect() }
        })
        .collect();
    out.sort_by(f64::total_cmp);
    out.dedup();
    out
}

pub fn set_rate(id: ObjectId, rate: f64) -> Result<(), String> {
    set(id, &addr(RATE, GLOBAL, MAIN), rate)
}
