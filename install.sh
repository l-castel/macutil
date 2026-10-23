#!/bin/bash
# Installs macutil: builds Macutil.app into ~/Applications and links the TUIs into ~/.local/bin.
#   ./install.sh             install or update
#   ./install.sh --uninstall remove what install created
# Override locations with APPS_DIR, BIN_DIR, CONFIG_DIR and MATUGEN_CONFIG.
set -euo pipefail
cd "$(dirname "$0")"

APPS_DIR="${APPS_DIR:-$HOME/Applications}"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
CONFIG_DIR="${CONFIG_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/macutil}"
MATUGEN_CONFIG="${MATUGEN_CONFIG:-${XDG_CONFIG_HOME:-$HOME/.config}/matugen/config.toml}"
APP="$APPS_DIR/Macutil.app"
BINS=(macutil macutil-wifi macutil-bt macutil-ts)

if [ "${1:-}" = "--uninstall" ]; then
  for b in "${BINS[@]}"; do
    [ -L "$BIN_DIR/$b" ] && rm "$BIN_DIR/$b"
  done
  rm -rf "$APP"
  echo "removed $APP and links in $BIN_DIR"
  exit 0
fi

[ "$(uname)" = "Darwin" ] || { echo "macutil only supports macOS" >&2; exit 1; }
command -v cargo >/dev/null || { echo "cargo not found; install Rust from https://rustup.rs" >&2; exit 1; }

scripts/bundle.sh

mkdir -p "$APPS_DIR" "$BIN_DIR"
rm -rf "$APP"
cp -R dist/Macutil.app "$APP"
for b in "${BINS[@]}"; do
  ln -sf "$APP/Contents/MacOS/$b" "$BIN_DIR/$b"
done

# matugen: install the template, and register it in the matugen config if not already there.
mkdir -p "$CONFIG_DIR"
cp contrib/matugen/macutil-theme.toml "$CONFIG_DIR/matugen-template.toml"
MATUGEN_NOTE="matugen config not found at $MATUGEN_CONFIG; add contrib/matugen/config-snippet.toml yourself"
if [ -f "$MATUGEN_CONFIG" ]; then
  if grep -q '^\[templates\.macutil\]' "$MATUGEN_CONFIG"; then
    MATUGEN_NOTE="matugen already configured for macutil"
  else
    cat >> "$MATUGEN_CONFIG" <<TOML

[templates.macutil]
input_path = '$CONFIG_DIR/matugen-template.toml'
output_path = '$CONFIG_DIR/theme.toml'
TOML
    MATUGEN_NOTE="added [templates.macutil] to $MATUGEN_CONFIG"
  fi
fi

echo
echo "installed $APP"
echo "$MATUGEN_NOTE"
echo "linked ${BINS[*]} into $BIN_DIR"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "note: $BIN_DIR is not on your PATH" ;;
esac
cat <<MSG

next steps:
  1. run: macutil-wifi   (press s to scan and answer the Location prompt)
  2. regenerate your theme (e.g. re-run matugen on your wallpaper); the TUIs pick up changes live
MSG
