# Music PLR

A music player with YouTube search, local playback, and MPRIS integration, built with the [iced](https://iced.rs) GUI framework.

## Features

- **YouTube Music Search** — Search for songs via ytmusicapi (Python) with yt-dlp fallback
- **Local Music** — Add local audio files (MP3, FLAC, WAV, OGG, M4A, AAC, OPUS, WMA) to playlists
- **Streaming + Caching** — Streams audio via yt-dlp + ffmpeg, caches to disk for instant replay
- **Downloads** — Download tracks via yt-dlp
- **Playlists** — Create, delete, and organize playlists
- **Radio** — Song radio and artist radio based on search results
- **MPRIS** — Full D-Bus MPRIS interface for media key integration
- **Queue** — Queue panel toggle with track removal
- **Drag & Drop** — Drag tracks between views and playlists
- **Search History** — Fuzzy-searchable search history with persistent storage and inline delete
- **Navigation History** — Back/forward navigation with state and results caching
- **Context Menu** — Right-click context menu with Play, Radio, Playlist, Download, and Remove actions
- **Dark Theme** — Dark color scheme with accent green highlights

## Building

### Prerequisites

- **Rust** (stable, edition 2021)
- **yt-dlp** — for YouTube audio streaming and downloads
- **ffmpeg** — for audio decoding and format conversion
- **Python 3** with `ytmusicapi` — for YouTube Music search (optional, falls back to yt-dlp)

### Build & Run

```sh
cargo build
cargo run
cargo fmt && cargo clippy
cargo test
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| <kbd>Space</kbd> | Toggle play/pause |
| <kbd>Esc</kbd> | Clear selection / close context menu / close search history / go to Search |
| <kbd>Delete</kbd> | Delete selected tracks (in playlist) |
| <kbd>Ctrl</kbd>+<kbd>C</kbd> | Copy selected tracks to clipboard |
| <kbd>Ctrl</kbd>+<kbd>V</kbd> | Paste clipboard tracks into playlist |
| ↑/↓ | Navigate tracks (in focused list) |
| <kbd>Enter</kbd> | Play focused track |
| <kbd>Ctrl</kbd>+<kbd>F</kbd> | Focus search bar |

## Configuration

Config is stored at `~/.config/music_plr/config.json`:

| Field | Description | Default |
|-------|-------------|---------|
| `download_dir` | Directory for downloaded files | `~/Music/music_plr` |
| `volume` | Initial playback volume | `0.8` |
| `cache_max_size_mb` | Max stream cache size | `1024` |
| `max_search_history_stored` | Max search history entries | `100` |
| `max_search_history_visible` | Max visible history entries | `10` |

## Architecture

```
src/
├── main.rs          # Entry point — iced::application builder
├── app.rs           # MusicPlayer: single source of truth (all app state + Message enum)
├── app/
│   ├── ui.rs        # Pure functional view() — reads directly from &MusicPlayer
│   └── update.rs    # All business logic handlers (navigation, playback, search, etc.)
├── audio.rs         # AudioPlayer: rodio sink + yt-dlp/ffmpeg process management
├── youtube.rs       # YouTube search (ytmusicapi → yt-dlp fallback) + audio download
├── mpris.rs         # MPRIS D-Bus interface (MediaPlayer2 + Player)
├── thumbnails.rs    # Thumbnail download cache
├── downloads.rs     # DownloadRegistry persistence
├── cache.rs         # StreamCache: LRU file cache with eviction
├── config.rs        # confy config model + fuzzy_match
├── playlists.rs     # PlaylistStore persistence
├── session.rs       # Session state (view, queue, playlist selection) for restore
├── theme.rs         # Palette, layout constants, styling helpers
├── types.rs         # Track, TrackSource, PlayQueue, View
├── icons.rs         # Compile-time SVG icon embedding (match-based include_str!)
└── util.rs         # format_duration, fuzzy_match
```

### Design Principles

- **Single source of truth**: All application state lives in `MusicPlayer` in `app.rs`. There are
  no parallel state mirrors — `view()` is a pure function of `&MusicPlayer`.
- **Functional view pattern**: `view()` reads directly from `&MusicPlayer` via iced's builder API.
  No `sync_*` methods, no callback forwarding, no `Rc<RefCell<Backend>>`.
- **Async results via mpsc**: Background threads (search, download, thumbnails) send `BackendResult`
  variants through an mpsc channel, drained by the 250ms tick.
- **Navigation history**: `NavEntry` records store full view state (including search/radio results)
  for back/forward navigation. Capped at 20 entries.

## License

MIT
