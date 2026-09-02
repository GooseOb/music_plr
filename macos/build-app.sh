#!/usr/bin/env bash
# build-app.sh — wrap the release binary into a macOS .app bundle.
#
# Usage:  macos/build-app.sh <binary> <icns> [output-dir]
# Example: macos/build-app.sh target/release/goosemusic macos/goosemusic.icns dist
set -euo pipefail

BINARY="${1:?usage: build-app.sh <binary> <icns> [output-dir]}"
ICNS="${2:?usage: build-app.sh <binary> <icns> [output-dir]}"
OUT="${3:-dist}"

APP="Goosemusic.app"
BUNDLE="$OUT/$APP"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
[ -z "$VERSION" ] && { echo "error: could not read version from Cargo.toml" >&2; exit 1; }

mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"

sed "s/VERSION/$VERSION/g" macos/Info.plist > "$BUNDLE/Contents/Info.plist"
cp "$ICNS" "$BUNDLE/Contents/Resources/goosemusic.icns"
cp "$BINARY" "$BUNDLE/Contents/Macos/goosemusic"
chmod +x "$BUNDLE/Contents/Macos/goosemusic"

echo "Built $APP  ($VERSION)"
du -sh "$BUNDLE"
