# music_plr

YouTube-search music player with local playback and MPRIS, built with iced.

## Stack

- **Language**: Rust (edition 2021); **UI**: iced 0.14 (`iced::application(boot, update, view)`)
- **Audio**: rodio + symphonia (native decode, no ffmpeg); **pipeline**: yt-dlp (stream/download)
- **MPRIS**: zbus 4 (D-Bus, tokio); **Config**: JsonStore + directories; **HTTP**: ureq 3 (json); **Dialogs**: rfd 0.15
- **Lyrics**: pluggable provider trait (`lyrics.rs`), LRCLib default; on-disk cache in `data/lyrics_cache.rs`
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

When a commit _is_ requested: one logical change per commit, imperative subject under ~72 chars,
and a body explaining **why** when it isn't obvious from the diff.

Run `cargo fmt && cargo clippy && cargo test` before handing work back, committed or not.

## Conventions

- No comments in code, unless logic is really non-obvious. Comments describe _current_ state, not what's changed.
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
├── app/interaction.rs # TrackListKind, TrackPos, DragState, ContextMenuState
├── app/ui/            # Pure functional view (mod, styles, content, overlays, playbar, queue, sidebar, track_list)
├── app/update/        # Handlers (mod, actions, drag, input, navigation, playback, playlists, search, selection, session, tick)
├── audio/mod.rs       # AudioPlayer: rodio sink + yt-dlp process management
├── audio/growing.rs   # GrowingMediaSource (the still-growing file reader)
├── audio/symphonia_source.rs # SymphoniaStreamingSource (rodio Source + Iterator); applies the normalization gain
├── audio/normalization.rs # compute_normalization_gain: RMS-based loudness analysis via symphonia
├── data/mod.rs        # JsonStore trait + config_path()/cache_path()
├── data/              # cache, config, downloads, playlists, library, search_history, session, thumbnails, lyrics_cache
├── theme/mod.rs       # Palette + AppTheme
├── theme/layout.rs    # Spacing / size / geometry constants (re-exported from theme)
├── theme/catalog.rs   # widget::*::Catalog impls for AppTheme
├── providers/         # Provider types + dispatch (mod.rs) and per-provider backends (jamendo, musicbrainz, soundcloud, youtube)
├── mpris.rs           # MPRIS D-Bus (MediaPlayer2 + Player)
├── types.rs           # Track, TrackSource, PlayQueue
├── lyrics.rs          # LyricsProvider enum + LyricsClient (provider registry)
├── icons.rs           # SVG embedding via include_bytes! + icon()
└── util.rs            # format_duration, fuzzy_match, remove_at, reorder_tracks
```

## State Management

- **`MusicPlayer`** (`app.rs`): the single source of truth. Holds audio/queue/playlists/config,
  mpsc channels, `DragState`, context menu, `nav_history`, `download_registry`, `stream_cache`,
  `thumbnail_cache`, `picker` (resolved target indices for the playlist-picker overlay),
  `lyrics`/`lyrics_track_id`/`lyrics_loading`, and `floating_search` (the in-list Ctrl+F overlay:
  the active `TrackListKind`, live query, matched indices; the current match is just
  `drag.hovered` when it is among the matches). **All per-view state** lives in `view_data`
  (search `exhausted`, radio label, selected playlist); no separate `View`/`RadioKind` enum.
- **`TrackListKind`** (`app/interaction.rs`): `Queue` / `Active` / `Recent` — the single carrier for "which track list?" across messages, `DragState`, selection, and scroll targeting. Helpers: `scrollable_id()` (each list has its own, so scroll ops can't hit the wrong widget), `first_index()` (1 for Queue, whose now-playing row renders outside the scrollable), `is_interactive()` (false for read-only `Recent`), `in_queue_panel()` (Queue+Recent share a geometry slot). Pass this instead of a bool.
- **`TrackPos`** (`app/interaction.rs`): `{ index, list }` — an index is only meaningful against its list, so they travel together. Carried by `TrackPressed`/`TrackHoverStart`/`TrackRightClicked`/`PlayTrackAtIndex`/`ContextMenuPlayTrack`, `DragState`'s `pressed` (`Pressed::Track`), `last_click`, and the `get_track_at`/`toggle_selection` accessors. Pass this instead of a loose `(usize, TrackListKind)` pair.
- **`ContextMenuState`**: `pos: TrackPos` + selection-aware `target_indices`. Ops apply to all
  selected if the right-clicked track is selected, else just it; "Play"/radio target only it.
  `Recent` tracks come from `recently_played` (queue/playlist items suppressed).
- **`DragState`** (`app/interaction.rs`): one `pressed: Option<Pressed>` (dragged thing:
  `Track(TrackPos)` / `Card(LibraryItem)` / `Playlist(usize)` row) and one `hovered: Option<HoverTarget>`
  (cursor target; `Track` doubles as the keyboard-navigation focus). Single enum field each. `drop_target`
  resolves the active drag: `Track`/`Playlist`/`Library` (insertion line), `PlaylistAdd` (track→existing
  playlist, row highlighted), `PlaylistReorder { from, to }`. Same-list reorders; cross-list copies move
  all selected; cards dropped on the playlist list become local playlists (`create_at` + bg `browse`).
  Cleaned via `cleanup()`; accessors `pressed_track()`/`hovered_track()`/`set_hovered*`.
- **Selection / list access** (`app/update/selection.rs`): `selection`, `toggle_selection`, `clear_selection`, `view_tracks`, `get_track_at`, `track_count` — all keyed by a `TrackListKind`. `Recent` has no selection: `selection` returns `&[]` and mutations are no-ops.
- **`BackendResult`** (mpsc): `SearchResults`, `SearchResultsAppend`, `RadioResults`, `DownloadComplete(Track,String)`, `DownloadError`, `SearchError`, `ThumbnailsDownloaded` (clears `thumbnail_cache`), `LyricsFetched(Option<Lyrics>, String)` (sets `lyrics`, caches to `lyrics_cache.json`, auto-cleared on track change), `NormalizationComputed(String, f32)` (caches a per-track gain in memory; read on subsequent plays), `CardPlaylistReady(usize, String, Vec<Track>)` (a dragged card became a playlist; fills the playlist at the given index with the browsed tracks). 250ms tick drains → `process_result`.
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

- **Stream+cache**: `yt-dlp -f bestaudio[ext=m4a]/bestaudio -o -` writes AAC-in-M4A to the cache file (`.cache`, owned by `StreamCache`); a copy thread drains stdout and flips `writer_alive` when done.
- **Decoding**: a custom `SymphoniaStreamingSource` (rodio `Source` + `Iterator<Item=i16>`) wraps a non-seekable `GrowingMediaSource`, so symphonia demuxes sequentially and plays a still-growing file without `rodio::Decoder::new`'s `SeekError`. The reader blocks at EOF while `writer_alive`, so playback starts within a few KB.
- **Cached/downloaded/local** (`PlayCached`) reuse the same source with `writer_alive = None` (real `byte_len`) so seeking works on replay.
- **Stream completion**: when yt-dlp and the copy thread both finish (`writer_alive` false), the tick loop registers the cache; track end → sink empties → auto-advance.
- **Volume normalization** (optional `config.volume_normalization`): a per-track RMS/peak gain from `compute_normalization_gain`, applied per-sample inside `SymphoniaStreamingSource` (composes with `set_volume`, survives seeking). First play uses gain 1.0; `request_normalization_analysis` fills the cache afterwards.

## YouTube & Key Files

- `search()`/`browse()`: scoped search via `ytmusicapi` (`youtube_search.py`), `scope` → ytmusicapi `filter=`; pagination (`search_more`) falls back to `yt-dlp --flat-playlist`. `browse()` drills via `get_artist`/`get_album`/`get_playlist`. `SEARCH_PAGE_SIZE = 10`.
- `radio_song()`/`radio_artist()`: query-modified search; `download()`/`download_audio()` → `yt-dlp --extract-audio` MP3.
- `theme/`: `Palette`+`AppTheme` (`mod.rs`), constants (`layout.rs`, re-exported), `Catalog` impls (`catalog.rs`).
- `ViewKind` (`app/view_data.rs`): `Search`/`SongRadio`/`ArtistRadio`/`Artist`/`Album`/`PlaylistView`/`Playlist`/`Downloads`/`Settings`/`Lyrics` (active view selector).
- `util.rs`: `format_duration`, `fuzzy_match`, `plural_suffix`, `try_probe_duration`, `remove_at`, `reorder_tracks` (unit-tested).

## Maintenance

After structural changes (new/removed files, renamed types/functions), verify `AGENTS.md` reflects actual state. Keep it under ~150 lines.

**`README.md` must be kept up to date too** — user-facing, drifts. Check it whenever module layout,
keyboard shortcuts (`app/update/input.rs`), config fields (`data/config.rs`, `config.json`),
on-disk paths (`data/*.rs` `FILE` consts), external tool requirements, or the audio pipeline change.
