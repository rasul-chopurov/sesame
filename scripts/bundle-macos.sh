#!/usr/bin/env bash
# Packs a built `sesame` binary into Sesame.app, a .zip (for the updater) and a .dmg.
#   scripts/bundle-macos.sh <path/to/sesame> <arch: arm64|x64> [out-dir]
set -euo pipefail

BIN=$1
ARCH=$2
OUT=${3:-dist}
VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="sesame-app"))')
APP="$OUT/Sesame.app"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/sesame"
cp assets/macos/Sesame.icns "$APP/Contents/Resources/Sesame.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Sesame</string>
  <key>CFBundleDisplayName</key><string>Sesame</string>
  <key>CFBundleIdentifier</key><string>dev.sesame.Sesame</string>
  <key>CFBundleExecutable</key><string>sesame</string>
  <key>CFBundleIconFile</key><string>Sesame</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION%%-*}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
</dict>
</plist>
PLIST
# Ad-hoc signature so Apple Silicon runs it; replaced by Developer ID once notarized.
codesign --force --deep --sign - "$APP"

ZIP="$OUT/Sesame-$VERSION-macos-$ARCH.zip"
DMG="$OUT/Sesame-$VERSION-macos-$ARCH.dmg"
rm -f "$ZIP" "$DMG"
ditto -c -k --keepParent "$APP" "$ZIP"
STAGE=$(mktemp -d)
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "Sesame $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
echo "$ZIP"
echo "$DMG"
