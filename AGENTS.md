# music_plr

A music player with YouTube search, local playback, and MPRIS integration, built with the iced GUI framework.

## Stack

- **Language**: Rust (edition 2021)
- **UI**: iced 0.14 (functional view pattern, `iced::application(boot, update, view)` builder)
- **Audio**: rodio with symphonia (decode to WAV via ffmpeg)
- **Audio pipeline**: yt-dlp (stream/download) + ffmpeg (decode)
- **MPRIS**: zbus 4 (D-Bus, tokio-backed)
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

- Comments only where logic is genuinely non-obvious (audio pipeline, drag geometry,
  nav-history invariants); otherwise self-documenting.
- **Single source of truth**: `MusicPlayer` in `app.rs` holds all state. `view()` is a
  pure function of `&MusicPlayer` — no `Rc<RefCell<Backend>>`, no sync methods.
- **`mpsc` channels** for cross-thread communication (backend results, MPRIS commands)
- **`iced::application` builder**: `new()` / `update()` / `view()` / `subscription()`
- **`Task` and `Subscription`** for async operations (timer tick, raw events)
- **`MusicPlayer` is NOT `Clone`** (contains channels); shared state via `&mut self`
- **Tracing macros** (`debug!`, `warn!`, `error!`) for all diagnostics
- **Error notifications** via `notify()` / `notify_error()`
- Avoid adding new files unless necessary; prefer editing existing structure

## Architecture

```
src/
├── main.rs          # Entry point — iced::application builder
├── app.rs           # MusicPlayer: all app state + Message enum + update()
├── app/
│   ├── ui/          # Pure functional view — reads directly from &MusicPlayer
│   │   ├── mod.rs      # root layout: bg(), button_style(), view()
│   │   ├── content.rs  # search bar, search/radio/playlist/download views
│   │   ├── overlays.rs # context menu, playlist picker, delete confirm
│   │   ├── playbar.rs  # bottom playbar (track info, slider, controls, volume)
│   │   ├── queue.rs    # queue panel (tabs, now-playing, up-next, recently played)
│   │   ├── sidebar.rs  # left sidebar (nav, playlists, create/import)
│   │   └── track_list.rs # reusable track rows + shared helpers (row_layout, etc.)
│   └── update/     # Business logic handlers
│       ├── mod.rs      # spawn_thumbnail_download_thread()
│       ├── actions.rs  # download/remove, context menu, picker
│       ├── drag.rs     # drag-drop geometry, reordering, autoscroll
│       ├── input.rs    # key press handling (arrows, enter, copy/paste)
│       ├── navigation.rs # nav history push/pop/restore, ViewSnapshot
│       ├── playback.rs # play track, queue reorder, next/prev, volume, seek
│       ├── playlists.rs # create/select/rename/delete, add tracks, copy/paste
│       ├── search.rs   # execute search, load more, radio, history
│       ├── session.rs  # save/restore session state
│       └── tick.rs     # handle_tick: drain channels, audio sync, MPRIS update
├── audio.rs         # AudioPlayer: rodio sink + yt-dlp/ffmpeg process management
├── youtube.rs       # YouTube search (ytmusicapi → yt-dlp fallback) + download
├── mpris.rs         # MPRIS D-Bus interface (MediaPlayer2 + Player)
├── thumbnails.rs    # Thumbnail download cache
├── downloads.rs     # DownloadRegistry persistence
├── cache.rs         # StreamCache: LRU file cache with eviction
├── config.rs        # confy config model
├── playlists.rs     # PlaylistStore persistence
├── session.rs       # SessionState for restore
├── theme.rs         # Palette, layout constants
├── types.rs         # Track, TrackSource, PlayQueue, View (payload-free)
├── icons.rs         # Compile-time SVG embedding (match-based include_str!)
└── util.rs         # format_duration, fuzzy_match
```

## State Management

- **`MusicPlayer`** (in `app.rs`): holds audio player, config, queue, playlists, search
  results, radio tracks, UI flags, mpsc channels, drag state, context menu, nav history,
  thumbnail tracking, clipboard (copy/paste), last-click timing (double-click), scroll bounds.
- **`BackendResult` channel**: background threads (search, download, thumbnails) send
  variants via mpsc. The 250ms tick drains and calls `process_result`.
- **MPRIS commands**: D-Bus thread sends `MprisCommand` via a separate channel, processed
  by `process_mpris_command` during the tick. A `MprisUpdate` channel flows in the opposite
  direction (main → MPRIS thread).
- **Navigation history**: `View` is payload-free; per-view restorable state lives in each
  `NavEntry`'s `ViewSnapshot`. Capped at 20 entries. `push_nav_entry()` snapshots search
  results into history when they arrive.

## Data Flow

1. User interacts → `Message` → `MusicPlayer::update()` → handler
2. Handler updates state or spawns a background thread
3. Background thread sends `BackendResult` via mpsc
4. 250ms tick (`handle_tick`): drains `result_rx` → `process_result`, `mpris_cmd_rx` →
   `process_mpris_command`; syncs audio state; detects stream completion → auto-next;
   sends MPRIS updates; updates progress text
5. `view()` reads directly from `&MusicPlayer` on next render

## Navigation Model

- `nav_history: Vec<NavEntry>` with `nav_history_pos` tracking current position
- `handle_navigate_to`: truncates at `nav_history_pos + 1`, pushes back-target + new
  current as two entries, advances `nav_history_pos`
- `push_nav_entry`: called from `process_result` when search/radio results arrive on
  Search/SongRadio/ArtistRadio views, to snapshot results into history
- `can_navigate_back() = nav_history_pos > 0`; disabled via `on_press_maybe(None)`
- `can_navigate_forward() = nav_history_pos + 1 < nav_history.len()`
- Search/radio results cached per `NavEntry` `ViewSnapshot` for back/forward restore
- `SearchResultsAppend` (Load More) syncs current `ViewSnapshot` results in-place
- "Load More" hidden once a page returns < `SEARCH_PAGE_SIZE` (`search_exhausted`)

## UI Layout

- **Sidebar** (`SIDEBAR_WIDTH = 300.0`): nav buttons, Search/Downloads items, scrollable
  playlist list (hover-highlighted), create playlist input, local music import
- **Main content**: global search bar (input + button, doubles as history dropdown) +
  view content (Search / SongRadio / ArtistRadio / Playlist / Downloads)
- **Queue panel** (`QUEUE_MIN_WIDTH = 240.0`): toggled via playbar queue button; width =
  `max(window_width * 0.2, 240.0)`; same track-row style
- **Playbar** (bottom): track info, progress slider, play/pause/next/prev/queue,
  elapsed/total time, volume slider
- **Overlays**: context menu, playlist picker, delete confirm, search history dropdown
  — all via `iced::widget::Stack`

## iced API Notes

- `bg(color)` → `impl Fn(&iced::Theme) -> container::Style` (defined in `ui/mod.rs`)
- `button_style()` / `slider_style()` / `button_style_accent()` / `button_style_green()`
  defined in `ui/mod.rs`
- `iced::alignment::Vertical::Top` (not `::Start`)
- `iced::widget::rule::horizontal(height)` for dividers
- Icons: match-based `include_str!` (no runtime `concat!`)
- `iced::event::listen_with()` takes a `fn` pointer
- `Subscription::batch` (not `Subscription::chain`)
- `iced::widget::text::Text` uses `.center()` / `.align_x()` / `.align_y()`
- `iced::widget::Button::on_press_maybe(Option<Message>)` for disabled buttons
- `iced::widget::Stack` for overlay layering
- `MouseArea::on_move` for hover (replaces `on_enter`/`on_exit`)
- `iced::widget::scrollable` with `.on_scroll()` for scroll position;
  `iced::widget::operation::scroll_to` / `scroll_by` for programmatic scroll

## Audio Pipeline

- `AudioPlayer` runs a dedicated output thread with an mpsc command channel
- **Stream + cache**: `yt-dlp -f bestaudio -o -` raw audio is tee'd to both a cache file
  and ffmpeg stdin simultaneously
- **ffmpeg** decodes to WAV (raw PCM) into a temp file in `temp_dir()/music_plr/`
- **rodio** plays the WAV once >2KB is available (via `symphonia` decoder)
- **Cached playback**: re-pipes cache file through ffmpeg stdin → rodio
- Stream completion detected by ffmpeg/yt-dlp process exit; main loop auto-advances
- Process lifecycle managed in `kill_processes()` (kill, cleanup temp files)

## YouTube Integration

- `search()`: tries `ytmusicapi` (Python `youtube_search.py`) first; falls back to
  `yt-dlp --flat-playlist` if Python unavailable
- Two-pass yt-dlp: flat search for stubs → batched `--batch-file` metadata pass
- `search_more()` paginates; `SEARCH_PAGE_SIZE = 10` per page
- `radio_song()` / `radio_artist()`: search with query modifiers
- `download()` / `download_audio()`: `yt-dlp --extract-audio` to MP3

## Key Files & Dependencies

- `theme.rs`: layout constants + `Palette`; `SEARCH_PAGE_SIZE` referenced by `youtube.rs`
  and `app/update/tick.rs`
- `types::View` variants matched across `ui/content.rs`, `app/update/drag.rs`,
  `app/update/navigation.rs`, `app/update/playback.rs`

## Maintenance

After structural changes (adding/removing files, renaming types/functions), verify this
file still reflects the actual state — check the [Architecture](#architecture) tree, the
fields in [State Management](#state-management), `View` behavior in [Navigation Model](#navigation-model),
and the patterns in [iced API Notes](#iced-api-notes).
