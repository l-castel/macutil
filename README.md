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

Every option below installs the same tools. They differ in one thing: whether they include
the **Location helper**, a small app that lets `macutil-wifi` show network names (macOS hides
them otherwise; see [Permissions](#permissions)).

| Method | Location helper | WiFi network names |
|---|---|---|
| Homebrew cask | included | shown |
| Install script | included | shown |
| Build the app bundle (`scripts/bundle.sh`) | included | shown |
| Homebrew formula | **not included** | **blank** |
| Plain `cargo build` | **not included** | **blank** |

If you use WiFi, pick one of the first three.

### Homebrew cask (recommended)

Installs `Macutil.app` and puts the four commands on your `PATH`. The cask comes from a
personal tap, so it needs no Apple notarization.

```sh
brew install --cask l-castel/tap/macutil
```

### Homebrew formula (no Location helper)

Builds only the command-line tools from source with Homebrew's Rust toolchain. WiFi
network names will be blank. Use the cask if you need them.

```sh
brew install l-castel/tap/macutil
```

The formula and the cask install the same commands, so install one or the other, not both.

### Install script

Builds from a checkout, installs `Macutil.app` (with the Location helper) into
`~/Applications` and links the commands into `~/.local/bin`.

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
This gives you the tools **without** the Location helper, so WiFi network names are
blank. To get the helper, run `scripts/bundle.sh` instead. It builds `dist/Macutil.app`,
which you can copy to `~/Applications`.

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
  answer the prompt once. Without the helper (formula or plain `cargo build`), scans still
  work but network names are blank.

## Theming

macutil works on its own: with no theme file it uses a built-in Tokyo Night palette.
To change it, edit `~/.config/macutil/theme.toml` (or the path in `$MACUTIL_THEME`).
Changes apply live. All keys are optional `#rrggbb` values:

```toml
fg = "#c0caf5"
dim = "#565f89"
accent = "#7aa2f7"
on_accent = "#000000"
good = "#9ece6a"
warn = "#e0af68"
bad = "#f7768e"
# bg = "#1a1b26"   # unset keeps your terminal's background
```

### matugen (optional)

If [matugen](https://github.com/InioX/matugen) is installed, macutil can follow your
matugen theme. It asks first, once:

```
Allow macutil to add its template to <matugen config> so its colours follow your matugen theme? [y/N]
```

You'll see this during `./install.sh`, or on the first launch of any macutil tool if you
installed with Homebrew (Homebrew installs can't prompt or write to your home directory).
Your answer is saved in `~/.config/macutil/matugen-consent`. To change it, delete that
file, or run `macutil setup-matugen` (add `--yes` or `--no` to skip the question).

If you answer yes, macutil:

1. writes its template to `~/.config/macutil/matugen-template.toml`. An existing file is
   kept, so you can edit it to pick other Material You roles.
2. appends this block to your matugen config, unless it's already there:
   ```toml
   [templates.macutil]
   input_path = '~/.config/macutil/matugen-template.toml'
   output_path = '~/.config/macutil/theme.toml'
   ```
3. renders the theme once from your current wallpaper, so it matches straight away.
   Only macutil's template is rendered, so other apps and matugen hooks aren't run.

From then on, every matugen run rewrites `theme.toml`, and macutil picks it up.

No macOS permission is involved. The matugen config is a regular file in your home
directory. The config used is `$MATUGEN_CONFIG` if set. Otherwise it's matugen's macOS
default, `~/Library/Application Support/com.InioX.matugen/config.toml`, or
`~/.config/matugen/config.toml` if only that one exists (for setups that call
`matugen -c`). To undo, remove the `[templates.macutil]` block.

## Uninstall

- Cask: `brew uninstall --cask macutil` (add `--zap` to remove config)
- Formula: `brew uninstall macutil`
- Install script: `./install.sh --uninstall`

## License

MIT. See [LICENSE](LICENSE).
