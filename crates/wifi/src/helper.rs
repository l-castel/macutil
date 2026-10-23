//! macOS hides SSIDs from processes without Location access, and a CLI run from a terminal
//! can't be granted it. The same binary, packaged as `MacutilLocation.app` (see
//! `scripts/bundle.sh`), runs in helper mode: it asks for Location, scans, writes JSON.

use std::path::PathBuf;
use std::process::Command;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use objc2_core_location::CLLocationManager;
use objc2_foundation::{NSDate, NSRunLoop};
use serde::{Deserialize, Serialize};

use crate::backend::{link, scan, Link, Network};

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub authorized: bool,
    pub link: Link,
    pub networks: Vec<Network>,
}

/// Entry point for `--helper <outfile>`, run inside the helper bundle.
pub fn run(out: &str) -> anyhow::Result<()> {
    let mtm = MainThreadMarker::new().ok_or_else(|| anyhow::anyhow!("not on main thread"))?;
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let authorized = unsafe {
        let lm = CLLocationManager::new();
        lm.requestWhenInUseAuthorization();
        let mut ok = false;
        // Up to ~90s so the user can answer the system prompt.
        for _ in 0..900 {
            NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.1));
            // 3 = authorized always, 4 = authorized when in use; 2 = denied, 1 = restricted.
            let status: i32 = objc2::msg_send![&*lm, authorizationStatus];
            if status >= 3 {
                ok = true;
                break;
            }
            if status == 1 || status == 2 {
                break;
            }
        }
        ok
    };
    let networks = scan().unwrap_or_default();
    let snap = Snapshot { authorized, link: link(), networks };
    std::fs::write(out, serde_json::to_vec(&snap)?)?;
    Ok(())
}

fn helper_app() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    [
        exe.parent()?.join("../Helpers/MacutilLocation.app"),
        exe.parent()?.join("MacutilLocation.app"),
        home.as_ref()?.join("Applications/Macutil.app/Contents/Helpers/MacutilLocation.app"),
        PathBuf::from("/Applications/Macutil.app/Contents/Helpers/MacutilLocation.app"),
    ]
    .into_iter()
    .find(|p| p.exists())
}

/// Runs the helper app and returns its snapshot, or `None` if it isn't installed or failed.
pub fn snapshot() -> Option<Snapshot> {
    let app = helper_app()?;
    let out = std::env::temp_dir().join(format!("macutil-wifi-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let ok = Command::new("open")
        .args(["-W", "-g", "-j", "-n", "-a"])
        .arg(&app)
        .args(["--args", "--helper"])
        .arg(&out)
        .status()
        .ok()?
        .success();
    let data = std::fs::read(&out).ok();
    let _ = std::fs::remove_file(&out);
    if !ok {
        return None;
    }
    serde_json::from_slice(&data?).ok()
}
