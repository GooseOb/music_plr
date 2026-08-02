# music_plr

A music player with YouTube search, local playback, and MPRIS integration, built with the iced GUI framework.

## Stack

- **Language**: Rust (edition 2021)
- **UI**: iced 0.14 (functional view pattern, `iced::application(boot, update, view)` builder)
- **Audio**: rodio with symphonia
- **Audio pipeline**: yt-dlp (stream/download) + ffmpeg (decode to WAV)
- **MPRIS**: zbus 4 (D-Bus)
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

- Comments are allowed only where logic is genuinely non-obvious (the audio
  process pipeline, drag-drop geometry, nav-history invariants); otherwise the
  code should be self-documenting.
- **Single source of truth**: `MusicPlayer` in `app.rs` holds all application
  state (one large struct). `view()` is a pure function of `&MusicPlayer` — no
  `Rc<RefCell<Backend>>`, no sync methods.
- **`mpsc` channels** for cross-thread communication (backend results, MPRIS commands)
- **`iced::application` builder**: `new()` / `update()` / `view()` / `subscription()` on `MusicPlayer`
- **`Task` and `Subscription`** for async operations (timer tick, raw event listening)
- **`Weak`/clone patterns**: `MusicPlayer` is NOT `Clone` (contains channels); shared state passed by `&mut self`
- **Tracing macros** (`debug!`, `warn!`, `error!`) for all diagnostics
- **Error notifications** shown to user via `notify()` / `notify_error()`
- Avoid adding new files unless necessary; prefer editing existing structure

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
├── config.rs        # confy config model
├── playlists.rs     # PlaylistStore persistence
├── session.rs       # Session state (view, queue, playlist selection) for restore
├── theme.rs         # Palette, layout constants, styling helpers (bg, button_style, slider_style)
├── types.rs         # Track, TrackSource, PlayQueue, View (payload-free) + View helpers
├── icons.rs         # Compile-time SVG icon embedding (match-based include_str!)
└── util.rs         # format_duration, fuzzy_match
```

## State Management

- **Application state** (`MusicPlayer` in `app.rs`): All application state lives in a single struct.
  It holds audio player, config, queue, playlists, search results, UI state flags, channels,
  drag state, context menu, navigation history, thumbnail download tracking.
- **`BackendResult` channel**: Background threads (search, download, thumbnails) send
  `BackendResult` variants through an mpsc channel. The 250ms tick drains this and calls
  `process_result`.
- **MPRIS commands**: The MPRIS D-Bus thread sends `MprisCommand` through a separate channel,
  processed by `process_mpris_command` during the tick.
- **Navigation history**: `View` is a payload-free selector; per-view restorable state (query,
  cached search/radio results, selected playlist and selection) lives only in each `NavEntry`'s
  `ViewSnapshot`, so history entries never carry data unrelated to their view.
  Capped at 20 entries.

## Data Flow

1. User interacts with UI → `Message` sent → `MusicPlayer::update()` matches and calls handler
2. Handler either updates state directly or spawns a background thread
3. Background thread sends `BackendResult` via mpsc channel
4. 250ms tick (`handle_tick`) drains `result_rx` and calls `process_result`
5. `view()` reads directly from `&MusicPlayer` on next render

## Navigation Model

- `nav_history: Vec<NavEntry>` with `nav_history_pos` tracking current position
- `handle_navigate_to` pushes both back-target + new current state as two entries
- `can_navigate_back() = nav_history_pos > 0`; disabled back button via `on_press_maybe(None)`
- `can_navigate_forward() = nav_history_pos + 1 < nav_history.len()`
- Search/radio results cached in each `NavEntry`'s `ViewSnapshot` so back/forward restores
  correct content per query
- `SearchResultsAppend` (Load More) syncs the current `NavEntry`'s `ViewSnapshot` results
  in-place
- "Load More" is hidden once a page returned fewer than a full `SEARCH_PAGE_SIZE` (`search_exhausted`)

## UI Layout

- **Sidebar** (`SIDEBAR_WIDTH = 240.0`): Back/forward nav buttons, Search/Downloads nav items,
  playlist list, create playlist input, local music import
- **Main content**: Global search bar (header) + view content (Search/Radio/Playlist/Downloads)
- **Queue panel** (`QUEUE_MIN_WIDTH = 240.0`): Toggle via queue button in playbar; uses same
  track-row style with thumbnails, title, artist, duration, and inline remove button
- **Playbar** (bottom): Track info, progress slider, play/pause/next/prev/queue controls,
  time display, volume slider
- **Overlays**: Context menu, playlist picker, delete confirmation — all via `iced::widget::Stack`

## iced API Notes

- `iced::widget::container::Style` via `bg(color)` helper returning `Fn(&iced::Theme) -> Style`
- `iced::alignment::Vertical::Top` (not `::Start`)
- `iced::widget::rule::horizontal(height)` for dividers
- Icons use match-based `include_str!` (can't use runtime strings with `concat!`)
- `iced::event::listen_with()` takes a `fn` pointer for event-to-message mapping
- `Subscription::batch` (not `Subscription::chain`)
- `iced::widget::text::Text` uses `.center()` / `.align_x()` / `.align_y()`
- `iced::widget::Button::on_press_maybe(Option<Message>)` for disabled buttons
- `iced::widget::Stack` for overlay layering (context menu, picker, delete confirm)
- `MouseArea::on_move` for hover tracking (replaces `on_enter`/`on_exit` which fire in tree order)

## Icons

- `/home/gooseob/projects/music_plr/icons/` — 16 SVG files
- `src/icons.rs` uses match-based `include_str!` to embed each icon at compile time

## Agent Skills

- `.agents/skills/slint/` — available for reference when working with Slint (historical context only)
