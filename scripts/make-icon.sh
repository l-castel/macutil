#!/bin/bash
# Regenerates packaging/Macutil.icns and docs/icon.png from scripts/make-icon.swift.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p target docs packaging
swiftc -O scripts/make-icon.swift -o target/make-icon
rm -rf target/Macutil.iconset && target/make-icon target/Macutil.iconset docs/icon.png
iconutil -c icns target/Macutil.iconset -o packaging/Macutil.icns
