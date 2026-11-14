#!/usr/bin/env bash
# bump_version.sh — bump the project version in Cargo.toml and Cargo.lock and
# cut a new dated release section in CHANGELOG.md from the entries currently
# under [Unreleased], which is left in place and emptied for the next cycle.
# The matching compare-link references at the bottom are added/updated too,
# including [Unreleased] (which points at everything after the new tag).
#
# Usage:
#   ./bump_version.sh patch|minor|major
#   ./bump_version.sh v1.6.0        # or 1.6.0 / 1.6.0-rc.1
#
# Committing and pushing are left to you: .github/workflows/tag.yml creates the
# tag when the new Cargo.toml version lands on master.
set -euo pipefail

REPO_URL="https://github.com/GooseOb/music_plr"

cd "$(dirname "${BASH_SOURCE[0]}")"

die() {
	printf 'error: %s\n' "$*" >&2
	exit 1
}

usage() {
	sed -n '2,13p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

[ $# -eq 1 ] || {
	usage
	exit 1
}

case "$1" in
-h | --help)
	usage
	exit 0
	;;
esac

[ -f Cargo.toml ] || die "Cargo.toml not found (run from the repository root)"
[ -f Cargo.lock ] || die "Cargo.lock not found"
[ -f CHANGELOG.md ] || die "CHANGELOG.md not found"

current=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
[ -n "$current" ] || die "could not read the version from Cargo.toml"

core=${current%%[-+]*}
IFS=. read -r cur_major cur_minor cur_patch <<<"$core"
[[ $cur_major =~ ^[0-9]+$ && $cur_minor =~ ^[0-9]+$ && $cur_patch =~ ^[0-9]+$ ]] ||
	die "Cargo.toml version '$current' is not X.Y.Z"
cur_major=$((10#$cur_major))
cur_minor=$((10#$cur_minor))
cur_patch=$((10#$cur_patch))

case "$1" in
patch)
	new="$cur_major.$cur_minor.$((cur_patch + 1))"
	;;
minor)
	new="$cur_major.$((cur_minor + 1)).0"
	;;
major)
	new="$((cur_major + 1)).0.0"
	;;
*)
	new=${1#v}
	[[ $new =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] ||
		die "invalid version '$1' (expected patch, minor, major or vX.Y.Z)"
	;;
esac

[ "$new" != "$current" ] || die "version is already $current"

grep -q "^## \[$new\]" CHANGELOG.md && die "CHANGELOG.md already has a [$new] section"

today=$(date +%Y-%m-%d)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

awk -v v="$new" '
	!done && /^version = "/ { sub(/"[^"]*"/, "\"" v "\""); done = 1 }
	{ print }
	END { if (!done) exit 3 }
' Cargo.toml >"$work/Cargo.toml" || die "could not update the version in Cargo.toml"

awk -v v="$new" '
	/^\[\[package\]\]$/ { in_pkg = 1; target = 0 }
	in_pkg && /^name = "goosemusic"$/ { target = 1; print; next }
	target && /^version = "/ { sub(/"[^"]*"/, "\"" v "\""); target = 0; done = 1 }
	{ print }
	END { if (!done) exit 3 }
' Cargo.lock >"$work/Cargo.lock" || die "could not update goosemusic in Cargo.lock"

awk -v heading="## [$new] - $today" '
	BEGIN { prev_blank = 0; pending = 0 }
	!done && /^## \[Unreleased\]([[:space:]]|$)/ {
		print "## [Unreleased]"
		print ""
		print heading
		done = 1
		pending = 1
		next
	}
	!done && /^## \[[0-9]/ {
		if (!prev_blank) print ""
		print "## [Unreleased]"
		print ""
		print heading
		print ""
		done = 1
	}
	pending { if ($0 != "") print ""; pending = 0 }
	{ print; prev_blank = ($0 == "") }
	END { if (!done) exit 3 }
' CHANGELOG.md >"$work/CHANGELOG.section" ||
	die "CHANGELOG.md has no [Unreleased] or release heading to cut the new section from"

awk -v v="$new" \
	-v compare="$REPO_URL/compare/v$current...v$new" \
	-v unreleased="$REPO_URL/compare/v$new...HEAD" '
	BEGIN {
		ref = "[" v "]: " compare
		uref = "[Unreleased]: " unreleased
		inserted = 0
	}
	index($0, "[" v "]:") == 1 { next }
	index($0, "[Unreleased]:") == 1 { next }
	/^\[[^]]+\]:[[:space:]]*https?:\/\// {
		if (!inserted) { print uref; print ref; inserted = 1 }
		print
		next
	}
	{ print }
	END { if (!inserted) { print ""; print uref; print ref } }
' "$work/CHANGELOG.section" >"$work/CHANGELOG.md" ||
	die "could not update the changelog references"

mv "$work/Cargo.toml" Cargo.toml
mv "$work/Cargo.lock" Cargo.lock
mv "$work/CHANGELOG.md" CHANGELOG.md

printf 'Bumped %s -> %s\n' "$current" "$new"
printf '  Cargo.toml\n'
printf '  Cargo.lock\n'
printf '  CHANGELOG.md  (## [%s] - %s; [Unreleased] left empty)\n' "$new" "$today"
printf '\nReview the changelog, then commit and push; the Auto-tag workflow creates v%s.\n' "$new"
