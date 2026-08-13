# Music PLR

A music player with YouTube search, local playback, and MPRIS integration, built with the [iced](https://iced.rs) GUI framework.

## Features

- **YouTube Music Search** — Scoped search (Songs / Videos / Artists / Albums / Playlists) via ytmusicapi with yt-dlp fallback; click artists/albums/playlists to drill down into their tracks; paginated "Load More"
- **Local Music** — Add local audio files (MP3, FLAC, WAV, OGG, M4A, AAC, OPUS, WMA) to playlists
- **Streaming + Caching** — Streams audio via yt-dlp with fully native decoding (symphonia, **no ffmpeg**), caching to disk for instant replay
- **Downloads** — Download tracks to MP3 via yt-dlp, with a Downloads view and on-row indicators
- **Playlists** — Create, rename, delete, and organize playlists
- **Library** — Save albums, artists, and playlists to a persistent Library; toggle save from the card or the view header, and browse saved items from a list in the sidebar
- **Radio** — Song radio and artist radio based on search results
- **MPRIS** — Full D-Bus MPRIS interface for media key integration
- **Queue** — Queue panel with Up Next and Recently Played tabs
- **Drag & Drop** — Drag tracks between views, into the queue, and onto sidebar playlists
- **Search History** — Fuzzy-searchable search history with persistent storage and inline delete
- **Navigation History** — Back/forward navigation restoring view, results, selection, and scroll
- **Context Menu** — Right-click menu with Play, Radio, Playlist, Download, and Remove actions; selection-aware
- **Session Restore** — Reopens with your last view, queue, and volume
- **Lyrics** — Free, no-API-key lyrics via [LRCLib](https://lrclib.net), behind a pluggable provider interface so more sources can be added later. The playbar Lyrics button opens a dedicated view with synced (timed) lines that seek playback when clicked, falling back to plain text or a "not found" state. Lyrics are cached on disk per track.
- **Dark Theme** — Dark color scheme with accent green highlights

## Building

### Prerequisites

- **Rust** (stable, edition 2021)
- **yt-dlp** — for YouTube audio streaming and downloads
- **Python 3** with `ytmusicapi` — for YouTube Music search (optional, falls back to yt-dlp)
- **D-Bus** session bus (Linux) — for MPRIS
- **Network access** — lyrics are fetched live from [LRCLib](https://lrclib.net) (no API key)

> ffmpeg is **not** required. Audio is decoded natively via symphonia.

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
| <kbd>Esc</kbd> | Close search history → clear selection → return to Search |
| <kbd>Delete</kbd> | Delete selected tracks (playlist view only) |
| <kbd>Tab</kbd> | Switch keyboard focus between the track list and the queue |
| <kbd>↑</kbd>/<kbd>↓</kbd> | Move through the focused list (auto-scrolls) |
| <kbd>Enter</kbd> | Play the focused track |
| <kbd>Ctrl</kbd>+<kbd>C</kbd> | Copy selected tracks to clipboard |
| <kbd>Ctrl</kbd>+<kbd>V</kbd> | Paste clipboard tracks into the current playlist |

<kbd>Esc</kbd> applies the first action that matches, in the order listed. The context menu and
dialogs are dismissed by clicking outside them.

## Configuration

Config is stored at `~/.config/music_plr/config.toml` (managed by [confy](https://docs.rs/confy)):

| Field | Description | Default |
|-------|-------------|---------|
| `download_dir` | Directory for downloaded files | `~/Music/music_plr` |
| `volume` | Initial playback volume | `0.8` |
| `cache_max_size_mb` | Max stream cache size, in MB | `1024` |
| `max_search_history_stored` | Max search history entries kept on disk | `100` |
| `max_search_history_visible` | Max entries shown in the dropdown | `10` |
| `max_recently_played` | Max tracks kept in Recently Played | `50` |

### Data locations

| Path | Contents |
|------|----------|
| `~/.config/music_plr/playlists.json` | Playlists and their tracks |
| `~/.config/music_plr/library.json` | Saved albums, artists, and playlists |
| `~/.config/music_plr/downloads.json` | Registry of downloaded tracks |
| `~/.config/music_plr/search_history.json` | Past search queries |
| `~/.config/music_plr/session.json` | Last view, queue, and volume |
| `~/.cache/music_plr/youtube/` | Streamed audio cache (LRU-evicted) |
| `~/.cache/music_plr/thumbnails/` | Downloaded track thumbnails |
| `~/.cache/music_plr/lyrics_cache.json` | Fetched lyrics, keyed by track id |

## Architecture

```
src/
├── main.rs                    # Entry point — iced::application builder
├── app.rs                     # MusicPlayer: all state + subscription + update() dispatch
├── app/
│   ├── view_data.rs           # ViewData / ViewKind / NavEntry — per-view state
│   ├── message.rs             # Message + BackendResult
│   ├── interaction.rs         # TrackListKind, TrackPos, DragState, ContextMenuState
│   ├── ui/                    # Pure functional view() over &MusicPlayer
│   └── update/                # Handlers: playback, search, playlists, drag,
│                              #   selection, navigation, input, session, tick
├── audio/
│   ├── mod.rs                 # AudioPlayer: rodio sink + yt-dlp process management
│   ├── growing.rs             # MediaSource over a still-downloading cache file
│   └── symphonia_source.rs    # Streaming symphonia decoder (rodio Source)
├── data/
│   ├── mod.rs                 # JsonStore trait + config_path()/cache_path()
│   ├── cache.rs               # StreamCache: LRU file cache with eviction
│   ├── config.rs              # confy config model
│   ├── downloads.rs           # DownloadRegistry
│   ├── playlists.rs           # PlaylistStore
│   ├── library.rs            # LibraryStore: saved albums/artists/playlists
│   ├── search_history.rs      # SearchHistory
│   ├── session.rs             # SessionState for restore
│   └── thumbnails.rs          # Thumbnail download cache
├── theme/
│   ├── mod.rs                 # Palette + AppTheme
│   ├── layout.rs              # Spacing, size, and geometry constants
│   └── catalog.rs             # widget::*::Catalog impls for AppTheme
├── youtube.rs                 # Search (ytmusicapi → yt-dlp fallback) + download
├── mpris.rs                   # MPRIS D-Bus interface (MediaPlayer2 + Player)
├── types.rs                   # Track, TrackSource, PlayQueue, QueueTab
├── lyrics.rs                  # LyricsProvider enum + LyricsClient + LRCLib fetch
├── icons.rs                   # Compile-time SVG icon embedding
└── util.rs                    # format_duration, fuzzy_match, remove_at, reorder_tracks
```

### Design Principles

- **Single source of truth**: All application state lives in `MusicPlayer` in `app.rs`. There are
  no parallel state mirrors — `view()` is a pure function of `&MusicPlayer`.
- **Functional view pattern**: `view()` reads directly from `&MusicPlayer` via iced's builder API.
  No `sync_*` methods, no callback forwarding, no `Rc<RefCell<Backend>>`.
- **Async results via mpsc**: Background threads (search, download, thumbnails) send `BackendResult`
  variants through an mpsc channel, drained by the 250ms tick.
- **Flat per-view state**: All view-specific state lives in a single `ViewData` struct whose
  `kind: ViewKind` carries only what actually differs between views (search `exhausted`, radio
  label, selected playlist). Navigation history stores whole `ViewData` snapshots, capped at 20.
- **Uniform persistence**: Every JSON-backed store implements the `JsonStore` trait and declares
  only its filename; `load`/`save` and path resolution are shared. Failures degrade to defaults
  rather than panicking, since none of this data is critical to playback.

### Audio Pipeline

`AudioPlayer` runs a dedicated output thread driven by an mpsc command channel. Decoding is fully
native — there is no ffmpeg transmux step and exactly one copy of the audio on disk.

- **Streaming**: `yt-dlp -f bestaudio[ext=m4a]/bestaudio -o -` writes raw AAC-in-M4A bytes straight
  to the cache file. A copy thread drains yt-dlp's stdout and clears a `writer_alive` flag when done.
- **Decoding**: A custom `SymphoniaStreamingSource` wraps a non-seekable `GrowingMediaSource`. Being
  non-seekable makes symphonia demux *sequentially*, so it can probe and play a still-growing file;
  reads block at EOF while the writer is alive. Playback starts within a few KB of download.
- **Replay**: Cached, downloaded, and local files use the same source with `writer_alive = None`,
  making them seekable.

## License

MIT
