# music_plr

A music player with YouTube search, local playback, and MPRIS integration.

## Stack

- **Language**: Rust (edition 2021)
- **UI**: Slint 1.x (`.slint` files in `ui/`)
- **Audio**: rodio with symphonia-all codecs
- **Async**: tokio (full features)
- **MPRIS**: zbus 4 (tokio)
- **Config**: confy + directories
- **HTTP**: ureq 3
- **Dialogs**: rfd 0.15

## Build & Run

```sh
cargo build
cargo run
cargo fmt && cargo clippy
```

## Code Conventions

- No comments in source code
- `Rc<RefCell<Backend>>` pattern for shared state
- `mpsc` channels for cross-thread communication (events, MPRIS commands)
- Slint callbacks wired in `main.rs` via `setup_callbacks`
- `Weak` references (`Rc::downgrade`) to avoid cycles in closures
- Avoid adding new files unless necessary; prefer editing existing structure

## Architecture

```
src/
├── main.rs           # Entry point, callbacks, thread setup
├── backend/
│   ├── mod.rs        # Backend struct, EventFn, AppWindow, View enum
│   ├── search.rs     # YouTube search + search history
│   ├── playback.rs   # Playback queue, track controls
│   ├── playlist.rs   # Playlist CRUD
│   ├── radio.rs      # Song/artist radio
│   ├── download.rs   # Download management
│   ├── tick.rs       # Audio progress tick
│   └── selection.rs  # Multi-select clipboard operations
├── audio.rs          # rodio audio sink wrapper
├── youtube.rs        # YouTube fetching via rustypipe/Invidious
├── mpris.rs          # MPRIS D-Bus interface
├── thumbnails.rs     # Thumbnail caching
├── downloads.rs      # yt-dlp downloader
├── config.rs         # confy config model
├── cache.rs          # Generic file cache
└── types.rs          # Shared types (Track, Playlist, etc.)

ui/
├── appwindow.slint   # Main window layout
├── theme.slint       # Colors, fonts, spacing
├── types.slint       # Slint type definitions
├── track-row.slint   # Track list item component
├── playlist-card.slint   # Playlist card component
└── icons.slint       # SVG icons
```

## Agent Skills

- `.agents/skills/slint/` — loaded automatically when editing `.slint` files
