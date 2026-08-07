# music_plr

A music player with YouTube search, local playback, and MPRIS integration, built with iced.

## Stack

- **Language**: Rust (edition 2021)
- **UI**: iced 0.14 (functional view pattern, `iced::application(boot, update, view)` builder)
- **Audio**: rodio + symphonia (decode to WAV via ffmpeg)
- **Audio pipeline**: yt-dlp (stream/download) + ffmpeg (decode)
- **MPRIS**: zbus 4 (D-Bus, tokio-backed)
- **Config**: confy + directories; **HTTP**: ureq 3 (thumbnails); **Dialogs**: rfd 0.15
- **Logging**: tracing + tracing-subscriber

## Prerequisites

- **yt-dlp** — YouTube audio streaming and downloads; **ffmpeg** — audio decoding (webm/aac → WAV)
- **Python 3** with `ytmusicapi` — search (falls back to yt-dlp if unavailable)
- **MPRIS**: D-Bus session bus (Linux only)

## Build & Run

```sh
cargo build && cargo run
cargo fmt && cargo clippy && cargo test
```

## Code Conventions

- Comments only where logic is genuinely non-obvious (audio pipeline, drag geometry, nav-history invariants); otherwise self-documenting.
- **Single source of truth**: `MusicPlayer` in `app.rs` holds all state. `view()` is pure over `&MusicPlayer` — no `Rc<RefCell<Backend>>`, no sync methods.
- **`mpsc` channels** for cross-thread communication (backend results, MPRIS commands); **`Task`/`Subscription`** for async (timer tick, raw events); `MusicPlayer` is NOT `Clone` (contains channels); shared state via `&mut self`
- **Tracing macros** for diagnostics; `notify()` / `notify_error()` for user-facing errors
- Avoid adding new files unless necessary; prefer editing existing structure

## Architecture

```
src/
├── main.rs          # Entry point — iced::application builder
├── app.rs           # MusicPlayer: all state + Message enum + update()
├── app/ui/          # Pure functional view — reads &MusicPlayer
│   ├── mod.rs, styles.rs, content.rs, overlays.rs, playbar.rs, queue.rs, sidebar.rs, track_list.rs
├── app/update/      # Business logic handlers
│   ├── mod.rs, actions.rs, drag.rs, input.rs, navigation.rs
│   ├── playback.rs, playlists.rs, search.rs, session.rs, tick.rs
├── audio.rs         # rodio sink + yt-dlp/ffmpeg process management
├── youtube.rs       # YouTube search (yt-dlp primary, ytmusicapi fallback) + download
├── mpris.rs         # MPRIS D-Bus interface (MediaPlayer2 + Player)
├── thumbnails.rs    # Thumbnail download cache
├── downloads.rs     # DownloadRegistry persistence (Track objects keyed by URL)
├── cache.rs         # StreamCache: LRU file cache with eviction
├── config.rs        # confy config model (preferences only, no user data)
├── playlists.rs     # PlaylistStore persistence
├── search_history.rs # SearchHistory: user data (persisted query list)
├── session.rs       # SessionState for restore
├── theme.rs         # Palette, layout constants
├── types.rs         # Track, TrackSource, PlayQueue, View (payload-free)
├── icons.rs         # Compile-time SVG embedding via `include_bytes!` + `icon()` factory
└── util.rs          # format_duration, fuzzy_match
```

## State Management

- **`MusicPlayer`** (in `app.rs`): all app state — audio player, config, search_history, queue,
  playlists, search results, radio tracks, UI flags, mpsc channels, `DragState`, context menu,
  nav history, clipboard (copy/paste), last-click timing (double-click), scroll bounds,
  `download_registry`, `stream_cache`, `thumbnail_cache` (HashMap ID→exists bool, populated in tick),
  `downloaded_tracks` (synced from `DownloadRegistry` in tick, used by Downloads view),
  `picker_target_indices` (Vec<usize> — all selected indices if right-clicked track is selected,
  otherwise just that track), `downloading_index`.
- **`ContextMenuState`**: `track_index` (right-clicked track), `target_indices` (selection-aware
  resolved indices — all selected if right-clicked track is selected, otherwise `[track_index]`),
  plus flags (`is_youtube`, `is_downloaded`, `in_playlist`, `is_queue`).
- **Context menu selection-aware behavior**: mirrors drag reordering — if the right-clicked track
  is in the selection, "Remove from Playlist", "Remove from Queue", "Add to Playlist", and
  "Download/Delete" apply to all selected; otherwise just the right-clicked track. After a remove op,
  selection is cleared if it overlapped with removed indices. "Play" and radio always target only
  the right-clicked track.
- **`DragState`**: `cursor_pos`, `pressed_track`, `pressed_track_is_queue`, `hovered_track`,
  `drag_origin`, `drag_active`, `drag_drop_target`, `drag_target_list`, `sidebar_hover_playlist`.
  Cleaned via `DragState::cleanup()`. Dragging a selected track moves all selected; non-selected
  track moves only itself.
- **`BackendResult` channel**: background threads send variants via mpsc; 250ms tick drains and
  calls `process_result`. Variants: `SearchResults`, `SearchResultsAppend`, `RadioResults`,
  `DownloadComplete(Track, String)`, `DownloadError`, `SearchError`, `ThumbnailsDownloaded`
  (clears `thumbnail_cache`).
- **MPRIS commands**: D-Bus thread sends `MprisCommand` via separate channel, processed by
  `process_mpris_command` during tick. `MprisUpdate` channel flows main → MPRIS thread.
- **Navigation history**: `View` is payload-free; per-view restorable state in `NavEntry`'s
  `ViewSnapshot`. Capped at 20 entries. `push_nav_entry()` snapshots results into history.

## Data Flow & Navigation

- User → `Message` → `MusicPlayer::update()` → handler → spawns bg thread or updates state
- Bg thread sends `BackendResult` via mpsc; 250ms tick (`handle_tick`) drains `result_rx` →
  `process_result`, `mpris_cmd_rx` → `process_mpris_command`; syncs audio state; detects stream
  completion → auto-next; sends MPRIS updates; updates progress text. `view()` reads `&MusicPlayer`.
- `nav_history: Vec<NavEntry>` with `nav_history_pos`; Back if `pos > 0`, Forward if `pos + 1 < len`
- `handle_navigate_to`: truncates at `pos + 1`, pushes back-target + new current, advances `pos`
- `push_nav_entry`: called from `process_result` when search/radio results arrive on Search/SongRadio/ArtistRadio
- Results cached per `NavEntry` `ViewSnapshot` for back/forward restore; `SearchResultsAppend` syncs snapshot in-place; "Load More" hidden once a page returns < `SEARCH_PAGE_SIZE`
- `View::Downloads` clears `selected_playlist` on navigation; renders from `downloaded_tracks` (synced from `DownloadRegistry` in tick), not from playlists

## UI Layout

- **Sidebar** (`SIDEBAR_WIDTH = 300.0`): nav buttons, Search/Downloads items, scrollable playlist
  list (hover-highlighted), create playlist input, local music import
- **Main content**: global search bar + view content (Search / SongRadio / ArtistRadio / Playlist /
  Downloads)
- **Queue panel** (`QUEUE_MIN_WIDTH = 240.0`): toggled via playbar queue button; width =
  `max(window_width * 0.2, 240.0)`; same track-row style
- **Playbar** (bottom): track info, progress slider, play/pause/next/prev/queue, elapsed/total
  time, volume slider
- **Overlays**: context menu, playlist picker, delete confirm, search history dropdown — all via
  `iced::widget::Stack`

## iced API Notes

- `bg_*()` → `impl Fn(&AppTheme) -> container::Style` (in `ui/styles.rs`); `button_style_*` there too;
  `slider_style()` via `AppTheme` `slider::Catalog` impl in `theme.rs`
- `iced::alignment::Vertical::Top` (not `::Start`); `iced::widget::rule::horizontal(height)` for
  dividers
- Icons: `include_bytes!` + `icon(icon_data, color, size)` →
  `svg::Svg::new(svg::Handle::from_memory(icon_data))` (see `src/icons.rs`); embeds at compile time
- `iced::event::listen_with()` takes a `fn` pointer; use `Subscription::batch` (not `::chain`)
- `iced::widget::text::Text` uses `.center()` / `.align_x()` / `.align_y()`
- `iced::widget::Button::on_press_maybe(Option<Message>)` for disabled buttons
- `iced::widget::Stack` for overlay layering; `MouseArea::on_move` for hover (replaces `on_enter`/`on_exit`)
- `iced::widget::scrollable` with `.on_scroll()`; `iced::widget::operation::scroll_to` / `scroll_by`

- `AudioPlayer` runs a dedicated output thread with an mpsc command channel. **Stream + cache**:
  `yt-dlp -f bestaudio -o -` raw audio is tee'd to both a cache file and ffmpeg stdin; **ffmpeg**
  decodes to WAV into a temp file in `temp_dir()/music_plr/`; **rodio** plays once >2KB available
  (via `symphonia` decoder). **Cached playback**: re-pipes cache file through ffmpeg stdin → rodio.
  Stream completion detected by process exit; main loop auto-advances. Temp file lifecycle:
  cleaned by `kill_processes()` on next stream start or thread exit; NOT deleted on stream
  completion (avoids use-after-unlink if rodio is still reading)

## YouTube & Key Files

- `search()`: tries `ytmusicapi` (Python `youtube_search.py`) for the initial page (returns songs, not channels); falls back to `yt-dlp --flat-playlist` if Python unavailable or offset > 0
- Two-pass yt-dlp: flat search for stubs → batched `--batch-file` metadata pass
- `search_more()` paginates via yt-dlp; `SEARCH_PAGE_SIZE = 10` per page
- `radio_song()` / `radio_artist()`: search with query modifiers; `download()` / `download_audio()` use `yt-dlp --extract-audio` to MP3
- `theme.rs`: layout constants + `Palette`; `SEARCH_PAGE_SIZE` referenced by `youtube.rs` and `app/update/tick.rs`; `types::View` variants matched across `ui/content.rs`, `app/update/drag.rs`, `app/update/navigation.rs`, `app/update/playback.rs`

## Maintenance

After structural changes (adding/removing files, renaming types/functions), verify this file still reflects the actual state. When this file crosses 150 lines, compact it.
