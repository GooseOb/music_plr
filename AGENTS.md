# music_plr

YouTube-search music player with local playback and MPRIS, built with iced.

## Stack

- **Language**: Rust (edition 2021); **UI**: iced 0.14 (`iced::application(boot, update, view)`)
- **Audio**: rodio + symphonia (native decode, no ffmpeg); **pipeline**: yt-dlp (stream/download)
- **MPRIS**: zbus 4 (D-Bus, tokio); **Config**: confy + directories; **HTTP**: ureq 3; **Dialogs**: rfd 0.15
- **Logging**: tracing + tracing-subscriber

## Prerequisites

- **yt-dlp** (stream/download, serves AAC-in-M4A which symphonia decodes)
- **Python 3** + `ytmusicapi` for search (falls back to yt-dlp); **D-Bus** session bus (Linux) for MPRIS

## Build & Run

```sh
cargo build && cargo run
cargo fmt && cargo clippy && cargo test
```

## Version Control

**Do not commit unless explicitly asked.** Leave changes in the working tree and report what was
changed; the user decides when and how it lands. This applies to `git commit` as well as anything
that implicitly commits (`git merge`, `git rebase`, `git stash`, `git checkout` over local edits).
Never `push`, amend, or rewrite history unprompted.

When a commit *is* requested: one logical change per commit, imperative subject under ~72 chars,
and a body explaining **why** when it isn't obvious from the diff.

Run `cargo fmt && cargo clippy && cargo test` before handing work back, committed or not.

## Conventions

- Comments describe the code's *current* state, not what a change did or undid. Comment hygiene: keep them short; never restate what the code obviously does, never narrate history ("now does X instead of Y", "replaces the old field"), and don't repeat the same explanatory comment in every caller of a shared pattern — say it once at the canonical routine (e.g. `util::remove_at`/`reorder_tracks`).
- **Single source of truth**: `MusicPlayer` (`app.rs`) holds all state; `view()` is pure over `&MusicPlayer` — no `Rc<RefCell<Backend>>`, no sync methods. `MusicPlayer` is NOT `Clone` (channels).
- **Async**: `mpsc` channels for cross-thread results (backend, MPRIS); `Task`/`Subscription` for timer tick + raw events; shared state via `&mut self`.
- `notify()` / `notify_error()` for user-facing errors; `notify_tracks(verb, n, suffix)` for pluralized counts.
- Persistence goes through the `JsonStore` trait (`data/mod.rs`): implementors declare only `FILE`.

## Architecture

```
src/
├── main.rs            # Entry point
├── app.rs             # MusicPlayer (all state) + subscription + update() dispatch
├── app/view_data.rs   # ViewData / ViewKind / NavEntry (per-view state)
├── app/message.rs     # Message + BackendResult
├── app/interaction.rs # DragState, DragTargetList, ContextMenuState
├── app/ui/            # Pure functional view (mod, styles, content, overlays, playbar, queue, sidebar, track_list)
├── app/update/        # Handlers (mod, actions, drag, input, navigation, playback, playlists, search, selection, session, tick)
├── audio/mod.rs       # AudioPlayer: rodio sink + yt-dlp process management
├── audio/growing.rs   # GrowingMediaSource (MediaSource over a still-growing file)
├── audio/symphonia_source.rs # SymphoniaStreamingSource (rodio Source + Iterator)
├── data/mod.rs        # JsonStore trait + config_path()/cache_path()
├── data/              # cache, config, downloads, playlists, search_history, session, thumbnails
├── theme/mod.rs       # Palette + AppTheme
├── theme/layout.rs    # Spacing / size / geometry constants (re-exported from theme)
├── theme/catalog.rs   # widget::*::Catalog impls for AppTheme
├── youtube.rs         # Search (yt-dlp primary, ytmusicapi fallback) + download
├── mpris.rs           # MPRIS D-Bus (MediaPlayer2 + Player)
├── types.rs           # Track, TrackSource, PlayQueue
├── icons.rs           # SVG embedding via include_bytes! + icon()
└── util.rs            # format_duration, fuzzy_match, remove_at, reorder_tracks
```

## State Management

- **`MusicPlayer`** (`app.rs`): audio, config, `search_history`, `queue`, `playlists`, UI flags, mpsc
  channels, `DragState`, context menu, `nav_history`, clipboard, last-click timing, `download_registry`,
  `stream_cache`, `thumbnail_cache` (ID→exists bool, filled in tick), `picker_target_indices`
  (all selected indices if right-clicked track is selected, else just it). **All per-view state** lives
  in one `view_data: ViewData` field — a flat struct whose `kind: ViewKind` enum carries only what
  differs per view (search `exhausted`, radio label, selected playlist). No separate `View`/`RadioKind`
  enum or per-view fields exist.
- **`ContextMenuState`**: `track_index`, `target_indices` (selection-aware), flags `is_youtube`/`is_downloaded`/`in_playlist`/`is_queue`. Menu ops apply to all selected if the right-clicked track is selected, else just it; "Play"/radio target only the right-clicked track.
- **`DragState`** (`app/interaction.rs`): cursor/track/hover/drop-target/origin/active flags + `drag_target_list`/`sidebar_hover_playlist`; cleaned via `DragState::cleanup()`. Dragging a selected track moves all selected.
- **Selection / list access** (`app/update/selection.rs`): `selection(_mut)`, `toggle_selection`, `clear_selection`, `view_tracks`, `get_track_at`, `current_track_count` — all keyed by an `is_queue` flag choosing queue vs. active view.
- **`BackendResult`** (mpsc): `SearchResults`, `SearchResultsAppend`, `RadioResults`, `DownloadComplete(Track,String)`, `DownloadError`, `SearchError`, `ThumbnailsDownloaded` (clears `thumbnail_cache`). 250ms tick drains → `process_result`.
- **MPRIS**: D-Bus thread → `MprisCommand` → `process_mpris_command` (tick); `MprisUpdate` flows main → thread.
- **Nav history**: full `ViewData` in `NavEntry.data` (no separate `view`/`snapshot`); capped at 20. `push_nav_entry()` snapshots live `view_data`.

## Data Flow & Navigation

- User → `Message` → `update()` → handler (spawns bg thread or mutates state). 250ms tick drains `result_rx` → `process_result`, `mpris_cmd_rx` → `process_mpris_command`; syncs audio, detects stream end → auto-next, sends MPRIS, updates progress. `view()` reads `&MusicPlayer`.
- `nav_history: Vec<NavEntry>` + `nav_history_pos`; Back if `pos > 0`, Forward if `pos + 1 < len`.
- `handle_navigate_to(data: ViewData)`: truncates at `pos+1`, installs target `ViewData` (no-op self-nav skipped via `ViewData::same_kind`), pushes, advances `pos`.
- `push_nav_entry`: from `process_result` when search/radio results arrive. `SearchResultsAppend` syncs in place; "Load More" hidden once a page returns < `SEARCH_PAGE_SIZE`.
- `Downloads` kind renders `ViewData.tracks` (synced from `DownloadRegistry` in tick); `Playlist` tracks read from `PlaylistStore` via `MusicPlayer::view_tracks`.

## UI Layout

- **Sidebar** (`SIDEBAR_WIDTH = 300.0`): nav buttons (Search/Downloads), scrollable playlist list, create-playlist input, local import.
- **Main**: global search bar + view (Search / SongRadio / ArtistRadio / Playlist / Downloads).
- **Queue panel** (`QUEUE_MIN_WIDTH = 240.0`, width `max(window_width*0.2, 240.0)`); **Playbar** (bottom): track info, progress, play/pause/next/prev/queue, volume.
- **Overlays** (context menu, playlist picker, delete confirm, search-history dropdown) via `iced::widget::Stack`.

## iced API Notes

- `bg_*()` / `button_style_*` → `impl Fn(&AppTheme) -> container::Style` (in `ui/styles.rs`); `slider_style()` via `AppTheme` `slider::Catalog`.
- `Vertical::Top` (not `::Start`); `rule::horizontal(height)` for dividers.
- Icons: `include_bytes!` + `icon(data,color,size)` → `svg::Svg::new(svg::Handle::from_memory(data))`.
- `event::listen_with()` takes an `fn` pointer; `Subscription::batch` (not `::chain`).
- `Text` uses `.center()`/`.align_x()`/`.align_y()`; `Button::on_press_maybe(Option<Message>)` for disabled.
- `Stack` for overlays; `MouseArea::on_move` for hover; `scrollable` `.on_scroll()`; `operation::scroll_to`/`scroll_by`.

## Audio Pipeline

`AudioPlayer` runs a dedicated output thread (mpsc command channel). **No ffmpeg** — decoding is fully native via symphonia.

- **Stream+cache**: `yt-dlp -f bestaudio[ext=m4a]/bestaudio -o -` writes raw AAC-in-M4A bytes straight to the cache file (`.cache`, owned by `StreamCache`). A copy thread drains yt-dlp stdout → cache file and flips `writer_alive` when done.
- **Decoding** goes through a custom `SymphoniaStreamingSource` (rodio `Source` + `Iterator<Item=i16>`) wrapping a non-seekable `GrowingMediaSource`. The non-seekable source makes symphonia demux _sequentially_ (no init seek), so it can probe and play a still-growing file without the `SeekError` panic that `rodio::Decoder::new` hits (it hardcodes `byte_len() = None`). The reader blocks at EOF while `writer_alive`, so playback starts within a few KB and runs seamlessly to the end.
- **Cached/downloaded/local playback** (`PlayCached`) uses the same `SymphoniaStreamingSource` with `writer_alive = None` (complete, seekable file with real `byte_len`), so seeking works on replay. `rodio::Decoder::new` is intentionally avoided for both paths.
- **Stream completion**: detected when yt-dlp exits AND the copy thread finishes (`writer_alive` false) → `cache_ready` flips, tick loop calls `stream_cache.insert(id)` to register the cache. Track end → sink empties → `stream_finished` → auto-advance (no subprocess exit polling needed).

## YouTube & Key Files

- `search()`: `ytmusicapi` (`youtube_search.py`) for first page, else `yt-dlp --flat-playlist`; two-pass yt-dlp (stubs → batched `--batch-file` metadata). `search_more()` paginates; `SEARCH_PAGE_SIZE = 10`.
- `radio_song()`/`radio_artist()`: query-modified search; `download()`/`download_audio()` use `yt-dlp --extract-audio` → MP3.
- `theme/`: `Palette` + `AppTheme` (`mod.rs`), constants (`layout.rs`, re-exported so `crate::theme::SPACING_SM` still resolves), `Catalog` impls (`catalog.rs`). `SEARCH_PAGE_SIZE` referenced by `youtube.rs` + `app/update/tick.rs`.
- `ViewKind` (`app/view_data.rs`) selects the active view in `ui/content.rs`, `drag.rs`, `navigation.rs`, `playback.rs`.
- `util.rs`: `format_duration`, `fuzzy_match`, `plural_suffix`, `try_probe_duration`, plus the two index-manipulation routines `remove_at` and `reorder_tracks` (generic, unit-tested in one place).

## Maintenance

After structural changes (new/removed files, renamed types/functions), verify `AGENTS.md` reflects actual state. Keep it under ~150 lines.

**`README.md` must be kept up to date too.** It is user-facing, so it drifts in ways this file
does not — check it whenever any of the following change:

- **Module layout** — it carries its own `src/` tree (less granular than the one above).
- **Keyboard shortcuts** — handled in `app/update/input.rs`. Never document a binding without
  confirming a handler exists; a stale <kbd>Ctrl</kbd>+<kbd>F</kbd> row survived there for a while.
- **Config fields** — must match `data/config.rs` exactly, including defaults. The file is
  `config.toml` (confy), not JSON.
- **On-disk paths** — the `FILE` consts in `data/*.rs` and the dirs in `data/mod.rs`.
- **External tool requirements** — e.g. ffmpeg was dropped, but the README still listed it.
- **Features / audio pipeline** — keep the pipeline summary consistent with the one above.
