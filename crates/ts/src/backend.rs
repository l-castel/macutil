//! Thin wrapper over the `tailscale` CLI (`status --json`, `up/down`, `set`, `ping`).

use std::process::Command;

use anyhow::{anyhow, Result};
use serde::Deserialize;

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct RawStatus {
    backend_state: String,
    #[serde(default)]
    auth_url: String,
    #[serde(default)]
    current_tailnet: Option<Tailnet>,
    #[serde(rename = "Self")]
    me: Option<RawPeer>,
    #[serde(default)]
    peer: std::collections::HashMap<String, RawPeer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Tailnet {
    name: String,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase")]
struct Location {
    #[serde(default)]
    country: String,
    #[serde(default)]
    country_code: String,
    #[serde(default)]
    city: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct RawPeer {
    #[serde(default)]
    location: Option<Location>,
    #[serde(default)]
    host_name: String,
    #[serde(default, rename = "DNSName")]
    dns_name: String,
    #[serde(default, rename = "TailscaleIPs")]
    ips: Vec<String>,
    #[serde(default)]
    online: bool,
    #[serde(default, rename = "OS")]
    os: String,
    #[serde(default)]
    exit_node: bool,
    #[serde(default)]
    exit_node_option: bool,
    #[serde(default)]
    relay: String,
    #[serde(default)]
    cur_addr: String,
}

#[derive(Clone)]
pub struct Peer {
    pub name: String,
    /// What the UI shows: "City 2" for located exit nodes, otherwise the hostname.
    pub label: String,
    pub country: String,
    pub country_code: String,
    pub city: String,
    /// Position among exit nodes in the same city (1-based); 0 when unlocated.
    pub number: usize,
    pub dns: String,
    pub ip: String,
    pub online: bool,
    pub os: String,
    pub is_exit_active: bool,
    pub is_exit_option: bool,
    pub path: String,
}

pub struct Status {
    pub state: String,
    pub tailnet: String,
    pub auth_url: String,
    pub me: Option<Peer>,
    pub peers: Vec<Peer>,
}

impl Status {
    pub fn running(&self) -> bool {
        self.state == "Running"
    }
}

fn convert(p: RawPeer) -> Peer {
    let path = if !p.cur_addr.is_empty() {
        "direct".to_string()
    } else if !p.relay.is_empty() {
        format!("relay {}", p.relay)
    } else {
        String::new()
    };
    let loc = p.location.unwrap_or_default();
    Peer {
        label: p.host_name.clone(),
        country: loc.country,
        country_code: loc.country_code,
        city: loc.city,
        number: 0,
        dns: p.dns_name.trim_end_matches('.').to_string(),
        ip: p.ips.into_iter().next().unwrap_or_default(),
        name: p.host_name,
        online: p.online,
        os: p.os,
        is_exit_active: p.exit_node,
        is_exit_option: p.exit_node_option,
        path,
    }
}

fn sort_key(p: &Peer) -> (String, String, usize, String) {
    (p.country.to_lowercase(), p.city.to_lowercase(), p.number, p.name.to_lowercase())
}

/// Names located exit nodes after their city, numbering them when a city has several
/// ("Los Angeles, CA 1", "Los Angeles, CA 2", …) in hostname order.
fn label_by_city(peers: &mut [Peer]) {
    use std::collections::HashMap;
    let mut groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (i, p) in peers.iter().enumerate().filter(|(_, p)| !p.city.is_empty()) {
        groups.entry((p.country.clone(), p.city.clone())).or_default().push(i);
    }
    for idxs in groups.values_mut() {
        idxs.sort_by_key(|&i| peers[i].name.clone());
        let many = idxs.len() > 1;
        for (n, &i) in idxs.iter().enumerate() {
            let p = &mut peers[i];
            p.number = n + 1;
            p.label = if many { format!("{} {}", p.city, n + 1) } else { p.city.clone() };
        }
    }
}

/// Finds the CLI even when launched from the app bundle, which gets a minimal PATH.
fn bin() -> &'static std::path::Path {
    use std::path::PathBuf;
    static BIN: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    BIN.get_or_init(|| {
        let path = std::env::var("PATH").unwrap_or_default();
        let known = [
            "/usr/local/bin",
            "/opt/homebrew/bin",
            "/Applications/Tailscale.app/Contents/MacOS",
        ];
        let found = path
            .split(':')
            .chain(known)
            .map(|d| PathBuf::from(d).join("tailscale"))
            .chain([PathBuf::from("/Applications/Tailscale.app/Contents/MacOS/Tailscale")])
            .find(|p| p.is_file());
        found
            .unwrap_or_else(|| PathBuf::from("tailscale"))
    })
}

fn tailscale(args: &[&str]) -> Result<String> {
    let out = Command::new(bin()).args(args).output()?;
    if !out.status.success() {
        return Err(anyhow!(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn status() -> Result<Status> {
    // `status --json` exits non-zero on some states (e.g. stopped), so parse stdout regardless.
    let out = Command::new(bin()).args(["status", "--json"]).output()?;
    let raw: RawStatus = serde_json::from_slice(&out.stdout)
        .map_err(|_| anyhow!("tailscale: {}", String::from_utf8_lossy(&out.stderr).trim()))?;
    let mut peers: Vec<Peer> = raw.peer.into_values().map(convert).collect();
    label_by_city(&mut peers);
    peers.sort_by(|a, b| {
        b.online
            .cmp(&a.online)
            .then(a.city.is_empty().cmp(&b.city.is_empty()).reverse())
            .then_with(|| sort_key(a).cmp(&sort_key(b)))
    });
    Ok(Status {
        state: raw.backend_state,
        tailnet: raw.current_tailnet.map(|t| t.name).unwrap_or_default(),
        auth_url: raw.auth_url,
        me: raw.me.map(convert),
        peers,
    })
}

pub fn up() -> Result<()> {
    tailscale(&["up"]).map(|_| ())
}

pub fn down() -> Result<()> {
    tailscale(&["down"]).map(|_| ())
}

pub fn set_exit_node(dns_or_ip: &str) -> Result<()> {
    tailscale(&["set", &format!("--exit-node={dns_or_ip}")]).map(|_| ())
}

pub fn ping(target: &str) -> Result<String> {
    let out = tailscale(&["ping", "-c", "3", "--timeout", "3s", target])?;
    Ok(out.lines().last().unwrap_or("").to_string())
}

pub fn copy(text: &str) -> Result<()> {
    use std::io::Write;
    let mut c = Command::new("pbcopy").stdin(std::process::Stdio::piped()).spawn()?;
    c.stdin.take().ok_or_else(|| anyhow!("pbcopy stdin"))?.write_all(text.as_bytes())?;
    c.wait()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status() {
        let json = r#"{"BackendState":"Running","CurrentTailnet":{"Name":"t"},
          "Self":{"HostName":"me","TailscaleIPs":["100.1.1.1"],"Online":true,"OS":"macOS"},
          "Peer":{"k":{"HostName":"exit1","DNSName":"exit1.ts.net.","TailscaleIPs":["100.2.2.2"],
          "Online":true,"ExitNodeOption":true,"CurAddr":"1.2.3.4:5"}}}"#;
        let raw: RawStatus = serde_json::from_str(json).unwrap();
        let p = convert(raw.peer.into_values().next().unwrap());
        assert_eq!((p.dns.as_str(), p.ip.as_str(), p.path.as_str()), ("exit1.ts.net", "100.2.2.2", "direct"));
        assert!(p.is_exit_option && !p.is_exit_active);
    }

    fn located(name: &str, country: &str, city: &str) -> Peer {
        let mut p = convert(RawPeer { host_name: name.into(), ..Default::default() });
        p.country = country.into();
        p.city = city.into();
        p
    }

    #[test]
    fn numbers_exit_nodes_per_city() {
        let mut peers = vec![
            located("us-lax-wg-402", "USA", "Los Angeles, CA"),
            located("us-lax-wg-401", "USA", "Los Angeles, CA"),
            located("jp-tyo-wg-001", "Japan", "Tokyo"),
        ];
        label_by_city(&mut peers);
        assert_eq!(peers[0].label, "Los Angeles, CA 2");
        assert_eq!(peers[1].label, "Los Angeles, CA 1");
        assert_eq!(peers[2].label, "Tokyo");
    }
}
