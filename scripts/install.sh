#!/usr/bin/env bash
# install.sh — one-command installer for Goosemusic.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/GooseOb/music_plr/master/scripts/install.sh | bash
#
# Installs to:
#   Linux:   ~/.local/bin/goosemusic  + desktop integration files
#   macOS:   /Applications/Goosemusic.app
#   Windows: not supported (use the .zip from GitHub Releases)
set -euo pipefail

REPO="GooseOb/music_plr"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

red() { printf '\033[1;31m%s\033[0m\n' "$*" >&2; }
green() { printf '\033[1;32m%s\033[0m\n' "$*" >&2; }

info() { green "  $*"; }
err() {
	red "error: $*" >&2
	exit 1
}

need() {
	command -v "$1" >/dev/null 2>&1 || err "$1 is required but not found"
}

# ── Platform detection ────────────────────────────────────────

detect_rust_target() {
	local os arch

	case "$(uname -s)" in
	Linux*) os="unknown-linux-gnu" ;;
	Darwin*) os="apple-darwin" ;;
	MINGW* | MSYS* | CYGWIN*)
		err "Windows is not supported by this script. Download the .zip from:
  https://github.com/$REPO/releases/latest"
		;;
	*) err "unsupported OS: $(uname -s)" ;;
	esac

	case "$(uname -m)" in
	x86_64 | amd64) arch="x86_64" ;;
	aarch64 | arm64) arch="aarch64" ;;
	*) err "unsupported architecture: $(uname -m)" ;;
	esac

	echo "${arch}-${os}"
}

# ── Download latest release asset ─────────────────────────────

download_release() {
	local target="$1" asset_name archive

	case "$target" in
	*-unknown-linux-gnu) asset_name="goosemusic-${target}.tar.gz" ;;
	*-apple-darwin) asset_name="Goosemusic-${target}.tar.gz" ;;
	*) err "unsupported target: $target" ;;
	esac

	need curl
	need tar

	local tmpdir
	tmpdir="$(mktemp -d)"

	info "Fetching latest release..."
	archive="$tmpdir/$asset_name"

	curl -fL "https://github.com/$REPO/releases/latest/download/$asset_name" \
		-o "$archive" ||
		err "download failed — check https://github.com/$REPO/releases"

	info "Extracting..."
	tar -xzf "$archive" -C "$tmpdir"

	echo "$tmpdir"
}

# ── Install ───────────────────────────────────────────────────

install_linux() {
	local tmpdir="$1"

	mkdir -p "$INSTALL_DIR"
	cp "$tmpdir/goosemusic" "$INSTALL_DIR/"
	chmod +x "$INSTALL_DIR/goosemusic"
	info "Binary  → $INSTALL_DIR/goosemusic"

	if [ -f "$tmpdir/goosemusic.desktop" ]; then
		local share="${XDG_DATA_HOME:-$HOME/.local/share}"
		mkdir -p "$share/applications" "$share/icons"
		cp "$tmpdir/goosemusic.desktop" "$share/applications/"
		[ -f "$tmpdir/icons/logo.svg" ] && cp "$tmpdir/icons/logo.svg" "$share/icons/goosemusic.svg"
		info "Desktop → $share/applications/goosemusic.desktop"
		info "Icon    → $share/icons/goosemusic.svg"
	fi
}

install_macos() {
	local tmpdir="$1"

	if [ -d "$tmpdir/Goosemusic.app" ]; then
		rm -rf /Applications/Goosemusic.app
		cp -R "$tmpdir/Goosemusic.app" /Applications/
		info "App     → /Applications/Goosemusic.app"
	else
		err "Goosemusic.app not found in the release archive"
	fi
}

# ── Main ──────────────────────────────────────────────────────

main() {
	echo ""
	green "Goosemusic installer"
	echo ""

	local target
	target="$(detect_rust_target)"
	info "Target: $target"

	local tmpdir
	tmpdir="$(download_release "$target")"

	case "$target" in
	*-unknown-linux-gnu) install_linux "$tmpdir" ;;
	*-apple-darwin) install_macos "$tmpdir" ;;
	esac

	echo ""
	green "Done!"
	case "$target" in
	*-unknown-linux-gnu)
		echo ""
		if echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
			info "Run: goosemusic"
		else
			info "Run: $INSTALL_DIR/goosemusic"
			info "Or add to PATH: export PATH=\"$INSTALL_DIR:\$PATH\""
		fi
		;;
	*-apple-darwin)
		info "Open Goosemusic from Applications or Spotlight"
		;;
	esac
	rm -rf "$tmpdir"
	echo ""
}

main
