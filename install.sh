#!/bin/bash
# Installs macutil: builds Macutil.app into /Applications and links the TUIs into ~/.local/bin.
#   ./install.sh             install or update
#   ./install.sh --uninstall remove what install created
# Override locations with APPS_DIR and BIN_DIR (MATUGEN_CONFIG picks the matugen config).
set -euo pipefail
cd "$(dirname "$0")"

APPS_DIR="${APPS_DIR:-/Applications}"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
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

# matugen (optional): asks before touching the matugen config; remembers the answer.
MACUTIL_BIN="$APP/Contents/MacOS/macutil"
if [ -t 0 ]; then "$MACUTIL_BIN" setup-matugen; fi

echo
echo "installed $APP"
echo "linked ${BINS[*]} into $BIN_DIR"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "note: $BIN_DIR is not on your PATH" ;;
esac
cat <<MSG

next steps:
  1. run: macutil-wifi   (press s to scan and answer the Location prompt)
  2. theme: edit ~/.config/macutil/theme.toml, or let matugen write it (macutil setup-matugen)
MSG
