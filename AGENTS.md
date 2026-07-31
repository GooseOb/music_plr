# music_plr

A music player with YouTube search, local playback, and MPRIS integration.

## Stack

- **Language**: Rust (edition 2021)
- **UI**: Slint 1.x (`.slint` files in `ui/`)
- **Audio**: rodio with symphonia-all codecs
- **Audio pipeline**: yt-dlp (stream/download) + ffmpeg (decode to WAV)
- **Async**: tokio (full features, used by zbus/MPRIS)
- **MPRIS**: zbus 4 (tokio)
- **Config**: confy + directories
- **HTTP**: ureq 3 (thumbnails)
- **Dialogs**: rfd 0.15 (file picker)
- **Logging**: tracing + tracing-subscriber

## Prerequisites

- **yt-dlp** — for YouTube audio streaming and downloads
- **ffmpeg** — for audio decoding (webm/aac → WAV)
- **Python 3** with `ytmusicapi` — for search (falls back to yt-dlp if unavailable)
- **MPRIS**: D-Bus session bus (Linux only)

## Build & Run

```sh
cargo build
cargo run
cargo fmt && cargo clippy
cargo test
```

## Code Conventions

- No comments in source code
- `Rc<RefCell<Backend>>` pattern for shared state between UI thread and backend
- `mpsc` channels for cross-thread communication (events, MPRIS commands, results)
- Slint callbacks wired in `main.rs` via `setup_callbacks`
- `Weak` references (`Rc::downgrade`) to avoid cycles in closures
- Tracing macros (`debug!`, `warn!`, `error!`, `info!`) for all diagnostics
- Error notifications shown to user via `notify_error()`
- Avoid adding new files unless necessary; prefer editing existing structure

## Architecture

```
src/
├── main.rs          # Entry point, callback wiring, thread setup
├── backend/
│   ├── mod.rs       # Backend struct, BackendResult, View, PlayQueue, to_slint_track
│   ├── search.rs    # YouTube search + search history
│   ├── playback.rs  # Playback queue, track controls, seek, volume
│   ├── playlist.rs  # Playlist CRUD, multi-select, reordering
│   ├── radio.rs     # Song/artist radio
│   ├── download.rs  # Download management
│   ├── tick.rs      # Audio progress tick + all UI sync methods
│   └── selection.rs # Multi-select clipboard operations
├── audio.rs         # AudioPlayer: rodio sink + yt-dlp/ffmpeg process management
├── youtube.rs       # YouTube search (ytmusicapi → yt-dlp fallback) + audio download
├── mpris.rs         # MPRIS D-Bus interface (MediaPlayer2 + Player)
├── thumbnails.rs    # Thumbnail download cache
├── downloads.rs     # DownloadRegistry persistence
├── cache.rs         # StreamCache: LRU file cache with eviction
├── config.rs        # Config model + fuzzy_match
├── playlists.rs     # PlaylistStore persistence
└── types.rs         # Track, TrackSource

ui/
├── appwindow.slint      # Main window layout, callbacks, keyboard shortcuts
├── theme.slint          # Colors, fonts, spacing, layout constants
├── types.slint          # Shared Slint type definitions (Track, PlaylistInfo)
├── track-row.slint      # Reusable track list item component (13 callbacks)
├── playbar.slint        # Playback controls bar
├── sidebar.slint        # Navigation sidebar with playlist list
├── search-view.slint    # Search results view
├── playlist-view.slint  # Playlist tracks view with drag-reorder
├── radio-view.slint     # Radio results view
├── queue-panel.slint    # Queue panel with drag-reorder
├── context-menu.slint   # Right-click context menu
├── search-history.slint # Search history dropdown
├── picker-dialog.slint  # Add-to-playlist picker
├── delete-confirm.slint # Playlist deletion confirmation
├── icons.slint          # SVG icon components
├── playback-state.slint # Global: PlaybackState
├── search-state.slint   # Global: SearchState
├── playlist-state.slint # Global: PlaylistState
├── queue-state.slint    # Global: QueueState
├── navigation-state.slint # Global: NavigationState
├── drag-state.slint     # Global: DragState (drag-and-drop state)
└── context-menu-state.slint # Global: ContextMenuState
```

## State Management

- **Backend state** (`Backend` in `backend/mod.rs`): All application state lives in a single
  `Rc<RefCell<Backend>>` struct. The UI thread borrows it mutably during callbacks and ticks.
- **Slint globals**: UI state that needs to be reactive (playback, search, playlist, queue,
  navigation, drag, context menu) is stored in Slint `global` properties. These are synced
  from the Backend via `sync_*` methods in `backend/tick.rs`.
- **BackendResult channel**: Background threads (search, download, thumbnails) send `BackendResult`
  variants through an mpsc channel. The audio tick drains these and calls `process_result`.
- **MPRIS commands**: The MPRIS D-Bus thread sends `MprisCommand` through a separate channel,
  which is processed by `setup_mpris_processor` into `EventFn` closures queued for the tick.

## Data Flow

1. User interacts with UI → Slint callback fires → `Backend` method called
2. `Backend` method either updates state directly or spawns a background thread
3. Background thread sends `BackendResult` via `result_tx`
4. Audio tick (`audio_tick`) drains `event_rx` (which includes results and MPRIS commands)
5. UI sync methods (`sync_search_model`, `sync_radio_model`, etc.) update Slint globals
6. Slint re-renders reactively from global property changes

## Agent Skills

- `.agents/skills/slint/` — loaded automatically when editing `.slint` files
