#!/usr/bin/env bash
# Generate platform icons from the SVG source.
#
# Requires: ImageMagick (convert/magick), icnsutils (png2icns)
# Output:   windows/app.ico, macos/goosemusic.icns, icons/logo.svg (copy)
set -euo pipefail

SVG="icons/app-icon.svg"
WIN_DIR="windows"
MAC_DIR="macos"

HAS_IMAGEMAGICK=false
if command -v convert >/dev/null 2>&1 || command -v magick >/dev/null 2>&1; then
	HAS_IMAGEMAGICK=true
fi

if $HAS_IMAGEMAGICK; then
	# Prefer 'magick' (ImageMagick 7); fall back to 'convert' (ImageMagick 6).
	# On Windows 'convert' is an NTFS utility, so try magick first.
	convert() { command magick "$@" 2>/dev/null || command convert "$@"; }
fi

mkdir -p "$WIN_DIR" "$MAC_DIR" "icons"

# ── Windows .ico ──────────────────────────────────────────────
if $HAS_IMAGEMAGICK; then
	echo "Generating Windows .ico ..."
	TMP=$(mktemp -d)
	trap 'rm -rf "$TMP"' EXIT

	for size in 16 32 48 64 128 256; do
		convert -background none "$SVG" -resize "${size}x${size}" -gravity center -extent "${size}x${size}" "$TMP/$size.png"
	done

	convert "$TMP/16.png" "$TMP/32.png" "$TMP/48.png" \
		"$TMP/64.png" "$TMP/128.png" "$TMP/256.png" \
		"$WIN_DIR/app.ico"
	echo "  -> $WIN_DIR/app.ico"
else
	echo "Skipping Windows .ico (ImageMagick not found)"
fi

# ── macOS .icns ──────────────────────────────────────────────
if $HAS_IMAGEMAGICK && command -v png2icns >/dev/null 2>&1; then
	echo "Generating macOS .icns ..."

	# Icon name must match CFBundleIconFile without the .icns extension
	ICON_NAME="goosemusic"
	# Required sizes for a full-resolution .icns
	for size in 16 32 128 256 512; do
		convert -background none "$SVG" -resize "${size}x${size}" -gravity center -extent "${size}x${size}" \
			"$MAC_DIR/${ICON_NAME}${size}x${size}.png"
	done

	# 512@2x (1024px) for Retina
	convert -background none "$SVG" -resize 1024x1024 -gravity center -extent 1024x1024 \
		"$MAC_DIR/${ICON_NAME}512x512@2x.png"

	png2icns "$MAC_DIR/$ICON_NAME.icns" \
		"$MAC_DIR/${ICON_NAME}16x16.png" \
		"$MAC_DIR/${ICON_NAME}32x32.png" \
		"$MAC_DIR/${ICON_NAME}128x128.png" \
		"$MAC_DIR/${ICON_NAME}256x256.png" \
		"$MAC_DIR/${ICON_NAME}512x512.png" \
		"$MAC_DIR/${ICON_NAME}512x512@2x.png"

	# Clean up intermediate PNGs
	rm -f "$MAC_DIR"/${ICON_NAME}*.png
	echo "  -> $MAC_DIR/$ICON_NAME.icns"
else
	echo "Skipping macOS .icns (png2icns not found — install icnsutils)"
fi

# ── Linux SVG (copy to icons dir) ────────────────────────────
# The SVG is already in icons/logo.svg — nothing to do here.
echo "  -> icons/logo.svg (already present)"

echo "Done. Generated: .ico, .icns, .svg"
