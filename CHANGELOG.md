# Changelog

All notable user-facing changes to goosemusic are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This file is the source of truth for GitHub release notes: the `Release`
workflow extracts the section matching the tag and prepends it to the release
body. `generate_release_notes` stays on, so GitHub still appends the
auto-generated compare link below it.

## [1.3.20] - 2026-10-30

### Fixed

Track row padding

## [1.3.19] - 2026-10-29

### Fixed

#### Localization

- "{n} plays" on tracks is count-aware (e.g. "1 play" instead of "1 plays").
- Destination in notification text e.g. "Queue: added 5 tracks" instead of "Added 5 tracks to Queue" for languages where case is not nominative.
- The local provider is translated ("Local files" in English) instead of a hardcoded "Local" in toasts, menus, the Edit popup and the default-provider picker.
- Failures from search, downloads, artist pages, resolve and Edit Track say what went wrong in your language — network trouble, rate limiting, a sign-in wall, missing content, a missing yt-dlp/Python install, or an unreadable response — instead of showing the provider's raw English message. Unrecognized failures still show that message.
- Arabic reads right to left: the sidebar moves to the right, every row mirrors (icons lead on the right), text and text inputs align right, chips and labels start from the right edge, list content still clears the scrollbar on the right, page actions hug the block's outer edge, and back/forward, skip and chevron arrows point along the reading direction. Context menus mirror with them — labels sit beside their icon at the reading edge and a submenu opens to the left.

### Changed

- In-list search, the playlist jumper, search history and the AI model filter ignore accents, case and script: "bjork" finds "Björk", "zolc" finds "Żółć", "strasse" finds "Straße", and Latin input finds Cyrillic titles ("kino", "korolev", "kyiv") while Cyrillic input finds Latin ones.

## [1.3.18] - 2026-10-29

### Fixed

- "Translate with AI" works with non-synced lyrics.
- Jump playlist dialog closes when Escape is pressed.
- "Add to queue" on empty queue does not crash the app anymore.
- Track list no longer looks empty in some cases.

## [1.3.17] - 2026-10-28

### Fixed

- No longer uses 100% CPU on ALSA worker issues.
- A little performance and size improvement due to removing unused iced features.

## [1.3.16] - 2026-10-19

### Added

- On artist page, section list items are now draggable (into library or playlists).
- Changing the source track in edit track dialog via "Manual search" button.
- Context menu for albums, artists, and playlists.
- "Open in browser" context menu item for tracks, albums, artists, and playlists.

### Changed

- Adjusted position of navigation buttons and queue panel tab buttons to be aligned with search bar.

## [1.3.15] - 2026-10-16

### Fixed

Context menu position in scrolled lists is now correct.

## [1.3.14] - 2026-10-15

### Changed

- Playback progress bar is now more smooth.
- Reduced amount of rerenders, potentially reducing CPU usage.

## [1.3.13] - 2026-10-11

### Fixed

Plain lyrics are now displayed correctly when synced lyrics are not available.

## [1.3.12] - 2026-10-10

### Fixed

- Clicking artist on track rows in queue panel now redirects to the artist page.

## [1.3.11] - 2026-10-08

### Added

- Trashbin for tracks to be autoskipped: Separate tab in navigation with trashed tracklist and items in context menu to add/remove tracks from the trashbin.

### Fixed

- Right-clicking a track in the Queue or Recently Played panel no longer crashes the app.

## [1.3.10] - 2026-10-06

### Fixed

- "Go to artist" on a regular youtube channel now opens the artist page with channel data instead of showing errors.
- "Most popular songs" on YouTube artist pages now shows play counts and durations instead of hiding them.

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
