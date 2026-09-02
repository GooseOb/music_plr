#!/usr/bin/env bash
# build-resources.sh — compile Windows .ico → .obj for linking into the binary.
#
# Usage:  bash windows/build-resources.sh
# Output: prints the absolute path to the compiled .obj (for RUSTFLAGS)
# Requires: rc.exe, cvtres.exe (MSVC — available on windows-latest runners)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RC="$SCRIPT_DIR/app.rc"
ICO="$SCRIPT_DIR/app.ico"

[ -f "$ICO" ] || { echo "error: $ICO not found — run scripts/generate-icons.sh first" >&2; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

rc.exe /fo "$TMP/app.res" "$RC" >/dev/null
cvtres.exe /nologo /machine:x64 /out:"$TMP/app.obj" "$TMP/app.res" >/dev/null

# Copy to a stable location outside tmpdir (CI may clean it up)
OUT="$SCRIPT_DIR/app.obj"
cp "$TMP/app.obj" "$OUT"

echo "$(pwd -W)/$OUT"
