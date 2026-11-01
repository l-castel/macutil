#!/bin/bash
# Builds MacutilLocation.app, the helper that holds the Location permission macOS requires
# before it reveals WiFi network names (see crates/wifi/src/helper.rs). macutil-wifi looks
# for it in ../libexec next to its own directory, then beside itself.
#   scripts/make-helper.sh <macutil-wifi binary> <dir>    creates <dir>/MacutilLocation.app
set -euo pipefail
[ $# -eq 2 ] || { echo "usage: $0 <macutil-wifi binary> <dir>" >&2; exit 1; }
HELPER=$2/MacutilLocation.app

rm -rf "$HELPER"
mkdir -p "$HELPER/Contents/MacOS"
cp "$1" "$HELPER/Contents/MacOS/macutil-wifi"
cat > "$HELPER/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>dev.macutil.location</string>
  <key>CFBundleName</key><string>MacutilLocation</string>
  <key>CFBundleExecutable</key><string>macutil-wifi</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSUIElement</key><true/>
  <key>NSLocationUsageDescription</key><string>macOS only reveals WiFi network names to apps with Location access.</string>
  <key>NSLocationWhenInUseUsageDescription</key><string>macOS only reveals WiFi network names to apps with Location access.</string>
</dict></plist>
PLIST

# Ad-hoc signing gives the bundle a stable identity for the privacy database.
codesign --force --sign - "$HELPER"
echo "built $HELPER"
