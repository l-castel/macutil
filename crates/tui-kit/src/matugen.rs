//! Optional matugen integration. macutil never needs matugen: without it the theme comes
//! from `theme.toml` (hand-edited) or the built-in Tokyo Night defaults. When matugen is
//! installed, macutil asks once (first run, or `macutil setup-matugen`) whether it may
//! register its template in the matugen config. The answer is stored in
//! `~/.config/macutil/matugen-consent`, so it is never asked again.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const TEMPLATE: &str = include_str!("../../../contrib/matugen/macutil-theme.toml");
const SECTION: &str = "[templates.macutil]";

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
        .join("macutil")
}

fn consent_file() -> PathBuf {
    config_dir().join("matugen-consent")
}

/// The matugen binary, if installed (GUI launches and launchers often have a minimal PATH).
pub fn binary() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    dirs.push(home().join(".cargo/bin"));
    dirs.into_iter().map(|d| d.join("matugen")).find(|p| p.is_file())
}

/// The config matugen actually reads: `$MATUGEN_CONFIG`, else the macOS default
/// (`~/Library/Application Support/com.InioX.matugen/config.toml`) if it exists, else
/// `~/.config/matugen/config.toml` if it exists (setups that pass `-c`), else the default.
pub fn config_path() -> PathBuf {
    if let Some(p) = std::env::var_os("MATUGEN_CONFIG") {
        return p.into();
    }
    let default = home().join("Library/Application Support/com.InioX.matugen/config.toml");
    let xdg = home().join(".config/matugen/config.toml");
    if !default.exists() && xdg.exists() {
        xdg
    } else {
        default
    }
}

/// Called before the TUI starts: asks once if matugen is installed and nobody has answered yet.
pub fn first_run() {
    if consent_file().exists() || binary().is_none() || !std::io::stdin().is_terminal() {
        return;
    }
    setup(None);
}

/// `macutil setup-matugen [--yes|--no]`: asks (unless answered by the flag), records the
/// answer and, on yes, registers the template and renders the current theme once.
pub fn setup(answer: Option<bool>) {
    let Some(bin) = binary() else {
        println!("matugen is not installed; macutil uses theme.toml or its built-in theme.");
        return;
    };
    let cfg = config_path();
    let yes = answer.unwrap_or_else(|| {
        print!(
            "matugen is installed. Allow macutil to add its template to {} so its colours follow your matugen theme? [y/N] ",
            cfg.display()
        );
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        matches!(line.trim(), "y" | "Y" | "yes")
    });
    let _ = std::fs::create_dir_all(config_dir());
    let _ = std::fs::write(consent_file(), if yes { "yes\n" } else { "no\n" });
    if !yes {
        println!("Left the matugen config untouched. Run `macutil setup-matugen` to change your mind.");
        return;
    }
    match register(&cfg) {
        Ok(true) => println!("Added {SECTION} to {}", cfg.display()),
        Ok(false) => println!("{SECTION} is already in {}", cfg.display()),
        Err(e) => return eprintln!("could not update {}: {e}", cfg.display()),
    }
    render_now(&bin);
}

fn template_path() -> PathBuf {
    config_dir().join("matugen-template.toml")
}

fn block() -> String {
    format!(
        "\n# Added by macutil (macutil setup-matugen)\n{SECTION}\ninput_path = '{}'\noutput_path = '{}'\n",
        template_path().display(),
        config_dir().join("theme.toml").display()
    )
}

/// Writes the template (keeping any user edits) and appends the block if missing.
/// `Ok(false)` = already registered.
fn register(cfg: &Path) -> std::io::Result<bool> {
    if !template_path().exists() {
        std::fs::write(template_path(), TEMPLATE)?;
    }
    let current = std::fs::read_to_string(cfg).unwrap_or_default();
    if current.lines().any(|l| l.trim() == SECTION) {
        return Ok(false);
    }
    if let Some(dir) = cfg.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let head = if current.trim().is_empty() { "[config]\n" } else { "" };
    std::fs::OpenOptions::new().create(true).append(true).open(cfg)?.write_all(format!("{head}{}", block()).as_bytes())?;
    Ok(true)
}

/// matugen keeps no "current theme", so derive it from the current wallpaper. Only macutil's
/// template is rendered (a throwaway config), so no other app's output or hooks are touched.
fn render_now(bin: &Path) {
    let wallpaper = osascript("tell application \"System Events\" to get picture of current desktop");
    if wallpaper.is_empty() || !Path::new(&wallpaper).is_file() {
        return println!("Theme will update on your next matugen run.");
    }
    let dark = osascript("tell application \"System Events\" to tell appearance preferences to get dark mode") == "true";
    let tmp = std::env::temp_dir().join(format!("macutil-matugen-{}.toml", std::process::id()));
    if std::fs::write(&tmp, format!("[config]\n{}", block())).is_err() {
        return;
    }
    let ok = Command::new(bin)
        .args(["-c"])
        .arg(&tmp)
        .args(["image", &wallpaper, "-m", if dark { "dark" } else { "light" }, "--source-color-index", "0", "-q"])
        .stdin(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = std::fs::remove_file(&tmp);
    println!("{}", if ok { "Rendered the current theme from your wallpaper." } else { "Theme will update on your next matugen run." });
}

fn osascript(script: &str) -> String {
    Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}
