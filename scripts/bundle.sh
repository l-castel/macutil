#!/bin/bash
# Builds dist/Macutil.app: a launcher that opens the macutil menu in your terminal, plus a
# nested MacutilLocation.app helper that holds the Location permission macOS requires
# before it will reveal WiFi network names.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
APP=dist/Macutil.app
HELPER=$APP/Contents/Helpers/MacutilLocation.app
rm -rf dist && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$HELPER/Contents/MacOS"
cp packaging/Macutil.icns "$APP/Contents/Resources/Macutil.icns"

for b in macutil macutil-wifi macutil-bt macutil-ts; do
  cp "target/release/$b" "$APP/Contents/MacOS/$b"
done
cp target/release/macutil-wifi "$HELPER/Contents/MacOS/macutil-wifi"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>dev.macutil.app</string>
  <key>CFBundleName</key><string>Macutil</string>
  <key>CFBundleExecutable</key><string>macutil-launcher</string>
  <key>CFBundleIconFile</key><string>Macutil</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>LSUIElement</key><true/>
  <key>NSBluetoothAlwaysUsageDescription</key><string>Macutil manages your Bluetooth devices.</string>
</dict></plist>
PLIST

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

# Opens the menu in the first terminal found (override with MACUTIL_TERMINAL=/path/to/App.app).
cat > "$APP/Contents/MacOS/macutil-launcher" <<'LAUNCH'
#!/bin/bash
BIN="$(cd "$(dirname "$0")" && pwd)/macutil"
for t in "${MACUTIL_TERMINAL:-}" /Applications/Ghostty.app /Applications/kitty.app /Applications/Alacritty.app /Applications/WezTerm.app; do
  [ -n "$t" ] && [ -d "$t" ] || continue
  case "$t" in
    *WezTerm*) exec open -na "$t" --args start -- "$BIN" ;;
    *kitty*)   exec open -na "$t" --args "$BIN" ;;
    *)         exec open -na "$t" --args -e "$BIN" ;;
  esac
done
exec osascript -e "tell application \"Terminal\" to do script \"exec '$BIN'\"" -e 'tell application "Terminal" to activate'
LAUNCH
chmod +x "$APP/Contents/MacOS/macutil-launcher"

# Ad-hoc signing gives each bundle a stable identity for the privacy database.
codesign --force --sign - "$HELPER"
for b in macutil macutil-launcher macutil-wifi macutil-bt macutil-ts; do codesign --force --sign - "$APP/Contents/MacOS/$b"; done
codesign --force --sign - "$APP"
echo "built $APP"
echo "install: cp -R $APP /Applications/   (then run /Applications/Macutil.app/Contents/MacOS/macutil-wifi, press s, and answer the Location prompt)"
