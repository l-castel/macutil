# macutil

Keyboard-driven terminal utilities for macOS: manage **WiFi**, **Bluetooth** and
**Tailscale** from the terminal, using native system frameworks (CoreWLAN and
IOBluetooth) rather than wrapping flaky command-line tools.

| Command | What it does |
|---|---|
| `macutil` | Menu that opens the tools below |
| `macutil-wifi` | Scan, connect (including hidden networks), disconnect, power |
| `macutil-bt` | Paired and nearby devices, connect, disconnect, pair, forget |
| `macutil-ts` | Peers, exit nodes (listed by city), ping, up/down, copy IP |

Requires macOS 14 (Sonoma) or newer. Apple silicon is the tested platform.

## Install

### Homebrew cask (recommended)

Installs `Macutil.app` and puts the four commands on your `PATH`. This is the
full-featured install, including the Location helper that lets macOS reveal WiFi
network names (see [Permissions](#permissions)).

```sh
brew install --cask l-castel/tap/macutil
```

### Homebrew formula

Builds the command-line tools from source with Homebrew's Rust toolchain. It does not
include the Location helper, so WiFi network names will be blank.

```sh
brew install l-castel/tap/macutil
```

The formula and the cask conflict; install one or the other.

### Install script

Builds from a checkout, installs `Macutil.app` into `~/Applications` and links the
commands into `~/.local/bin`.

```sh
git clone https://github.com/l-castel/macutil.git
cd macutil
./install.sh
```

Remove it again with `./install.sh --uninstall`. Override locations with `APPS_DIR`
and `BIN_DIR`.

### Build from source

You need macOS and a recent stable [Rust toolchain](https://rustup.rs).

```sh
git clone https://github.com/l-castel/macutil.git
cd macutil
cargo build --release
```

The binaries land in `target/release/` (`macutil`, `macutil-wifi`, `macutil-bt`,
`macutil-ts`). Keep them in the same directory: the menu launches its siblings.
To build the app bundle with the Location helper instead, run `scripts/bundle.sh`,
which produces `dist/Macutil.app`.

## Usage

Run `macutil` for the menu, or any tool directly. Movement is `↑`/`↓` or `j`/`k`, and
`q` quits.

**macutil-wifi**: `⏎` connect · `s` scan · `d` disconnect · `h` hidden network · `o` power

**macutil-bt**: `⏎` connect, disconnect or pair · `tab` paired/nearby · `s` scan ·
`f` forget (press twice) · `r` refresh

**macutil-ts**: `tab` peers/exit nodes · `u` up/down · `e` set or clear exit node ·
`p` ping · `y` copy IP · `/` filter · `r` refresh

Exit nodes are labelled by city and numbered within it, for example `Los Angeles, CA 2`.
The filter matches city, country name, two-letter country code and hostname.

macutil-ts needs the `tailscale` command line tool, either from the Tailscale app or
Homebrew.

## Permissions

- **Bluetooth**: macOS may ask your terminal for Bluetooth access the first time.
- **Location**: macOS hides WiFi network names from any process without Location access,
  and command-line tools cannot be granted it. The app bundle includes a small helper
  (`MacutilLocation.app`) that holds this permission. Run `macutil-wifi`, press `s`, and
  answer the prompt once. Without the helper, scans work but names are blank.

## Theming

Colours are read from `~/.config/macutil/theme.toml` (or the path in `$MACUTIL_THEME`)
and reloaded live when the file changes. All keys are optional `#rrggbb` values:

```toml
fg = "#e4e2e3"
dim = "#919092"
accent = "#bfc7d5"
on_accent = "#29313c"
good = "#9ece6a"
warn = "#e0af68"
bad = "#f7768e"
# bg = "#131314"   # unset keeps your terminal's background
```

### matugen

`./install.sh` copies a [matugen](https://github.com/InioX/matugen) template to
`~/.config/macutil/` and registers it in `~/.config/matugen/config.toml` if that file
exists. Re-run matugen on your wallpaper and any open macutil window recolours. To set
it up by hand, add this to your matugen config:

```toml
[templates.macutil]
input_path = '~/.config/macutil/matugen-template.toml'
output_path = '~/.config/macutil/theme.toml'
```

The template lives at `contrib/matugen/macutil-theme.toml`.

## Uninstall

- Cask: `brew uninstall --cask macutil` (add `--zap` to remove config)
- Formula: `brew uninstall macutil`
- Install script: `./install.sh --uninstall`

## License

MIT. See [LICENSE](LICENSE).
