# Music PLR

A music player with YouTube search, local playback, and MPRIS integration.

## Features

- **YouTube Music Search** — Search for songs via ytmusicapi (Python) with yt-dlp fallback
- **Local Music** — Add local audio files (MP3, FLAC, WAV, OGG, M4A, AAC, OPUS, WMA) to playlists
- **Streaming + Caching** — Streams audio via yt-dlp + ffmpeg, caches to disk for instant replay
- **Downloads** — Download tracks as MP3 via yt-dlp
- **Playlists** — Create, delete, and organize playlists with multi-select clipboard operations
- **Radio** — Song radio and artist radio based on search results
- **MPRIS** — Full D-Bus MPRIS interface for media key integration
- **Queue** — Queue with reordering and removal
- **Drag & Drop** — Drag tracks between views, playlists, and queue
- **Search History** — Fuzzy-searchable search history with persistent storage
- **Dark/Light Theme** — Automatic based on system preference

## Building

### Prerequisites

- **Rust** (stable, edition 2021)
- **yt-dlp** — for YouTube audio streaming and downloads
- **ffmpeg** — for audio decoding and format conversion
- **Python 3** with `ytmusicapi` — for YouTube Music search (optional, falls back to yt-dlp)

### Build

```sh
cargo build --release
```

### Run

```sh
cargo run
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| <kbd>Space</kbd> | Toggle play/pause |
| <kbd>Esc</kbd> | Clear selection / close context menu / close search history |
| <kbd>Delete</kbd> | Delete selected tracks (in playlist) |
| <kbd>Ctrl</kbd>+<kbd>C</kbd> | Copy selected tracks |
| <kbd>Ctrl</kbd>+<kbd>V</kbd> | Paste clipboard tracks |
| ↑/↓ | Navigate tracks (in focused list) |
| <kbd>Enter</kbd> | Play focused track |

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
├── main.rs          # Entry point, callback wiring, thread setup
├── backend/
│   ├── mod.rs       # Backend struct, BackendResult, event types
│   ├── search.rs    # YouTube search + search history
│   ├── playback.rs  # Playback queue, track controls
│   ├── playlist.rs  # Playlist CRUD
│   ├── radio.rs     # Song/artist radio
│   ├── download.rs  # Download management
│   ├── tick.rs      # Audio progress tick + UI sync
│   └── selection.rs # Multi-select clipboard operations
├── audio.rs         # rodio audio sink + yt-dlp/ffmpeg process management
├── youtube.rs       # YouTube search (ytmusicapi/yt-dlp) + audio download
├── mpris.rs         # MPRIS D-Bus interface
├── thumbnails.rs    # Thumbnail caching
├── downloads.rs     # Download registry
├── cache.rs         # Stream cache (LRU eviction)
├── config.rs        # confy config model + fuzzy matching
├── playlists.rs     # Playlist persistence
└── types.rs         # Shared Track/TrackSource types
```

## License

MIT
