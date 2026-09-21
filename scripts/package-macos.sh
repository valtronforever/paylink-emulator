#!/usr/bin/env sh
set -eu
# Run from repository root after cargo build -p paylink-gui (or --release).
mode="${1:-debug}"
case "$mode" in debug|release) ;; *) echo 'Expected debug or release' >&2; exit 2;; esac
bundle="target/$mode/PayLink Emulator.app"
mkdir -p "$bundle/Contents/MacOS"
cp "target/$mode/paylink-gui" "$bundle/Contents/MacOS/paylink-gui"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.valtronforever.paylink-emulator</string>
<key>CFBundleName</key><string>PayLink Emulator</string>
<key>CFBundleDisplayName</key><string>PayLink Emulator</string>
<key>CFBundleExecutable</key><string>paylink-gui</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
printf '%s\n' "$bundle"
