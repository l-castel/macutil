#!/bin/bash
# Installs macutil from a checkout: the commands into $PREFIX/bin and the Location helper
# (MacutilLocation.app) into $PREFIX/libexec. PREFIX defaults to ~/.local.
#   ./install.sh             install or update
#   ./install.sh --uninstall remove what install created
# MATUGEN_CONFIG picks the matugen config.
set -euo pipefail
cd "$(dirname "$0")"

PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
HELPER="$PREFIX/libexec/MacutilLocation.app"
BINS=(macutil macutil-wifi macutil-bt macutil-ts macutil-audio)

if [ "${1:-}" = "--uninstall" ]; then
  for b in "${BINS[@]}"; do rm -f "$BIN_DIR/$b"; done
  rm -rf "$HELPER"
  echo "removed ${BINS[*]} from $BIN_DIR and $HELPER"
  exit 0
fi

[ "$(uname)" = "Darwin" ] || { echo "macutil only supports macOS" >&2; exit 1; }
command -v cargo >/dev/null || { echo "cargo not found; install Rust from https://rustup.rs" >&2; exit 1; }

cargo build --release
mkdir -p "$BIN_DIR" "$PREFIX/libexec"
for b in "${BINS[@]}"; do
  # rm first: older installs left symlinks here, and cp would write through them.
  rm -f "$BIN_DIR/$b"
  cp "target/release/$b" "$BIN_DIR/$b"
done
scripts/make-helper.sh target/release/macutil-wifi "$PREFIX/libexec"

# matugen (optional): asks before touching the matugen config; remembers the answer.
if [ -t 0 ]; then "$BIN_DIR/macutil" setup-matugen; fi

echo
echo "installed ${BINS[*]} into $BIN_DIR"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "note: $BIN_DIR is not on your PATH" ;;
esac
[ -d /Applications/Macutil.app ] && echo "note: /Applications/Macutil.app is from an older install and can be deleted"
cat <<MSG

next steps:
  1. run: macutil-wifi   (press s to scan and answer the Location prompt)
  2. theme: edit ~/.config/macutil/theme.toml, or let matugen write it (macutil setup-matugen)
MSG
