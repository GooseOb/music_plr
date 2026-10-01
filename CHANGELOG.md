# Changelog

All notable user-facing changes to goosemusic are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This file is the source of truth for GitHub release notes: the `Release`
workflow extracts the section matching the tag and prepends it to the release
body. `generate_release_notes` stays on, so GitHub still appends the
auto-generated compare link below it.

## [1.3.9] - 2026-10-01

### Added

- Like button in playbar and track rows.
- "Save to playlist" button in browse view.

### Changed

- Radio view now matches the browse view layout.
- Adding and jumping to a playlist now use the same dialog.

## [1.3.8] - 2026-09-30

### Changed

- Python dependencies are now installed into a venv instead of globally.
- Streaming is quality-aware.

## [1.3.7] - 2026-09-27

### Fixed

- Clearing non-main list selection on navigation.
- Selection ordering; Ctrl+C / Ctrl+V now respect the hovered list.

[1.3.9]: https://github.com/GooseOb/music_plr/compare/v1.3.8...v1.3.9
[1.3.8]: https://github.com/GooseOb/music_plr/compare/v1.3.7...v1.3.8
[1.3.7]: https://github.com/GooseOb/music_plr/compare/v1.3.6...v1.3.7
