# GooseOb's Music Player

A YouTube/SoundCloud search music player with local playback, downloads, and MPRIS, built with [iced](https://iced.rs).

## Features

- **Search** — YouTube Music (Songs / Videos / Artists / Albums / Playlists) via ytmusicapi with yt-dlp fallback, plus SoundCloud search. Drill
  into artists, albums, and playlists.
- **Streaming & caching** — Stream audio via yt-dlp with fully native decoding, cached to disk for instant replay.
- **Downloads** — Download tracks to MP3 via yt-dlp, with a Downloads view and on-row indicators.
- **Local music & playlists** — Add local files (MP3, FLAC, WAV, OGG, M4A, AAC, OPUS, WMA) and create, rename, delete, and organize playlists.
- **Library** — Save albums, artists, and playlists; browse them from the sidebar.
- **Artist pages** — Header with stats plus Most popular, Albums, Playlists, and Fans-also-like sections, each with its own provider picker.
- **Radio** — Song radio and artist radio from search results.
- **Queue** — Queue panel with Up Next and Recently Played tabs.
- **Drag & drop** — Drag tracks between views, into the queue, onto playlists (reorder or turn a card into a local playlist), and into the
  Library.
- **Lyrics** — Free, no-key LRCLib lyrics with synced lines that seek on click; cached per track.
- **MPRIS** — Full D-Bus interface for media keys.
- **More** — Search history, volume normalization, navigation history, session restore, right-click context menu, dark theme.
- **Localization** — 12 languages: English, Polski, Español, Português (Brasil), 简体中文, العربية, Беларуская, Français, Deutsch, 日本語, Русский, हिन्दी.

## Supported languages

| Language                       | Code    |
| ------------------------------ | ------- |
| English                        | `en`    |
| Polski (Polish)                | `pl`    |
| Español (Spanish)              | `es`    |
| Português (Brasil)             | `pt_br` |
| 简体中文 (Chinese, Simplified) | `zh_cn` |
| العربية (Arabic)               | `ar`    |
| Беларуская (Belarusian)        | `be`    |
| Français (French)              | `fr`    |
| Deutsch (German)               | `de`    |
| 日本語 (Japanese)              | `ja`    |
| Русский (Russian)              | `ru`    |
| हिन्दी (Hindi)                 | `hi`    |

Pick a language from the in-app **Settings** view. To add one, copy `src/i18n/en.rs` to a new module, translate the strings, and append one entry
to the `languages!` macro in `src/i18n/mod.rs` — the `Language` enum and picker are generated from that list.

## Install & run

**Prerequisites**

- **Rust** (stable, edition 2021)
- **yt-dlp** — YouTube audio streaming and downloads
- **Python 3** + `ytmusicapi` — YouTube Music search (optional; falls back to yt-dlp)
- **D-Bus** session bus (Linux) — for MPRIS
- Network access — lyrics fetch live from [LRCLib](https://lrclib.net) (no key)

```sh
cargo build
cargo run
```

## Keyboard shortcuts

| Key                                                         | Action                                                                           |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------- |
| <kbd>Space</kbd>                                            | Toggle play/pause                                                                |
| <kbd>Esc</kbd>                                              | Close in-list search → close search history → clear selection → return to Search |
| <kbd>Delete</kbd>                                           | Delete selected tracks (playlist view only)                                      |
| <kbd>←</kbd>/<kbd>→</kbd>                                   | Move focus between queue panel and track list                                    |
| <kbd>↑</kbd>/<kbd>↓</kbd>                                   | Move through the focused list (auto-scrolls)                                     |
| <kbd>Ctrl</kbd>+<kbd>F</kbd>                                | In-list fuzzy search over the hovered track list                                 |
| <kbd>Enter</kbd>                                            | Play the focused (or hovered) track                                              |
| <kbd>Ctrl</kbd>+<kbd>C</kbd> / <kbd>Ctrl</kbd>+<kbd>V</kbd> | Copy / paste selected tracks                                                     |
| <kbd>Ctrl</kbd>+<kbd>A</kbd>                                | Select all tracks in the focused list                                            |

## Configuration

Config lives at `~/.config/goosemusic/config.json` and is also editable live from the in-app **Settings** view.

| Field                        | Description                         | Default              |
| ---------------------------- | ----------------------------------- | -------------------- |
| `download_dir`               | Download directory                  | `~/Music/goosemusic` |
| `cache_max_size_mb`          | Max stream cache size (MB)          | `1024`               |
| `max_search_history_stored`  | Search history entries kept on disk | `100`                |
| `max_search_history_visible` | Entries shown in the dropdown       | `10`                 |
| `max_recently_played`        | Tracks kept in Recently Played      | `50`                 |
| `volume_normalization`       | Consistent loudness across tracks   | `false`              |

Persistent data goes to `~/.local/share/goosemusic` (playlists, library, downloads, search history); regenerable caches (session, streamed audio,
thumbnails, lyrics) go to `~/.cache/goosemusic`.

## Technical notes

**State** — All state lives in one `MusicPlayer`; `view()` is a pure function of `&MusicPlayer`. Background work (search, download, thumbnails)
returns via an mpsc channel drained by a 250ms tick. Stores implement a `JsonStore` trait and degrade to defaults on failure.

**Audio** — A dedicated output thread runs the rodio sink. yt-dlp streams AAC-in-M4A into a still-growing cache file; a custom
`SymphoniaStreamingSource` over a non-seekable `GrowingMediaSource` decodes sequentially so playback starts within a few KB.
Cached/downloaded/local files use the same source in seekable mode. Per-track normalization gain is computed once (symphonia) and applied on
replay.

## License

MIT
