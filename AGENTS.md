# goosemusic

YouTube-search music player with local playback and OS media controls, built with iced.

## Stack

- **Language**: Rust (edition 2021); **UI**: iced 0.14 (`iced::application(boot, update, view)`)
- **Audio**: rodio + symphonia; **pipeline**: yt-dlp (stream/download)
- **Media controls**: souvlaki (MPRIS/D-Bus on Linux, SMTC on Windows, Now Playing on macOS); **Config**: JsonStore + directories; **HTTP**: ureq 3 (json); **Dialogs**: rfd 0.15; **Logging**: tracing + tracing-subscriber
- **Lyrics**: pluggable provider trait (`lyrics.rs`), LRCLib default, plus named user-added per-track lyrics (plain or LRC-synced) shown as tabs after the providers, editable/deletable from the lyrics view; on-disk cache in `data/lyrics_cache.rs`; AI translate (`translate.rs`, OpenAI-compatible/Ollama, BYOK in Settings) opens the result in the custom-lyrics editor under the target language

## Prerequisites

- **yt-dlp** (stream/download, serves AAC-in-M4A which symphonia decodes)
- **Python 3** + `ytmusicapi` for search (falls back to yt-dlp); **D-Bus** session bus (Linux) for MPRIS; Windows/macOS use their native media controls (no extra deps)

## Build & Run

```sh
cargo build && cargo run
# rustfmt.toml uses unstable import options; plain `cargo fmt` silently ignores them
cargo +nightly fmt && cargo clippy && cargo test
```

## Version Control

**Do not commit unless explicitly asked.** Leave changes in the working tree and report what changed; the user decides when and how it lands. This applies to `git commit` and anything that implicitly commits (`git merge`, `git rebase`, `git stash`, `git checkout` over local edits). Never `push`, amend, or rewrite history unprompted.

When a commit _is_ requested: one logical change per commit, imperative subject under ~72 chars,
and a body explaining **why** when it isn't obvious from the diff. Run `cargo +nightly fmt && cargo clippy && cargo test` before handing work back, committed or not.

## Conventions

- No comments in code, unless logic is really non-obvious. Comments describe _current_ state, not what's changed.
- **Single source of truth**: `MusicPlayer` (`app/state.rs`) holds all state; `view()` is pure over `&MusicPlayer` — no `Rc<RefCell<Backend>>`, no sync methods. `MusicPlayer` is NOT `Clone` (channels).
- **Async**: `mpsc` channels for cross-thread results (backend, media controls); `Task`/`Subscription` for timer tick + raw events; shared state via `&mut self`.
- `notify()` / `notify_error()` for user-facing errors; `notify_tracks(verb, n, suffix)` for pluralized counts.
- Persistence goes through the `JsonStore` trait (`data/mod.rs`): implementors declare only `FILE`.

## Architecture

```
src/
├── main.rs            # Entry point
├── app.rs             # Module index: declares submodules + re-exports public types (MusicPlayer, Message, …)
├── app/state.rs       # MusicPlayer (all state) + new()/Default + pane/view accessors + view()
├── app/pane.rs         # Pane (per-pane nav history + search bar + lyrics) + SplitNode tiling tree
├── app/lyrics_state.rs # LyricsState + LyricsViewMode (per-pane lyrics state)
├── app/dialog.rs       # Dialog (exclusive overlay) + accessors
├── app/edit_track.rs   # EditTrackState (track-editing popup working copy)
├── app/playlist_picker.rs # PlaylistPicker (add-to-playlist) + PlaylistJump (Ctrl+K jumper state)
├── app/shortcuts.rs      # In-app cheatsheet table (?/F1, Dialog::Shortcuts; labels via Strings)
├── app/view_data.rs   # ViewData / ViewKind (per-view state)
├── app/message.rs     # Message + BackendResult (pane-scoped messages carry PaneId)
├── app/interaction.rs # TrackListKind, TrackPos (+pane), DragState, ContextMenuState
├── app/import.rs     # ImportPlaylistDialog + filename-pattern matching/conflict engine
├── app/translate_dialog.rs # TranslateDialog (AI-translate popup: language + prompt + in-flight flag)
├── app/ui/            # Pure functional view (mod, styles, content, overlays, playbar, queue, sidebar, split, track_list)
├── app/update/        # Per-domain handlers; dispatch.rs holds the top-level update()/subscription() dispatcher
├── audio/mod.rs       # AudioPlayer: rodio sink + yt-dlp process management
├── audio/growing.rs   # GrowingMediaSource (the still-growing file reader)
├── audio/symphonia_source.rs # SymphoniaStreamingSource (rodio Source + Iterator); applies the normalization gain
├── audio/normalization.rs # compute_normalization_gain: RMS-based loudness analysis via symphonia
├── data/mod.rs        # JsonStore trait + config_path()/cache_path()
├── data/              # cache, config, downloads, playlists, library, search_history, session, thumbnails, lyrics_cache
├── theme/mod.rs       # Palette + AppTheme
├── theme/layout.rs    # Spacing / size / geometry constants (re-exported from theme)
├── theme/catalog.rs   # widget::*::Catalog impls for AppTheme
├── providers/         # Provider types + dispatch (mod.rs) and per-provider backends (bandcamp, lastfm, musicbrainz, soundcloud, youtube)
├── media_controls.rs  # OS media controls via souvlaki (MPRIS/SMTC/Now Playing)
├── types.rs           # Track, TrackSource, PlayQueue
├── lyrics.rs          # LyricsProvider enum + LyricsClient (provider registry)
├── translate.rs       # AI translation via OpenAI-compatible chat completions (hosted or Ollama-local)
├── icons.rs           # SVG embedding via include_bytes! + icon()
├── load_state.rs      # LoadState<T, E>: Ready/Failed/Loading fetch-state wrapper
└── util.rs            # format_duration, fuzzy_match, remove_at, reorder_tracks
```

## State Management

- **`MusicPlayer`** (`app/state.rs`): the single source of truth. Holds audio/queue/playlists/config,
  mpsc channels, `DragState`, `dialog: Option<Dialog>` (exclusive overlay),
  `panes: HashMap<PaneId, Pane>` + `split_root: SplitNode` + `focused_pane_id` (tiled main views;
  sidebar/queue/playbar stay global), `download_registry`, `stream_cache`, `thumbnail_index`,
  and `track_list_search` (in-list Ctrl+F: owning pane + `TrackListKind`, query, matches).
  **All per-view state** lives in `view_data`; no separate `View`/`RadioKind` enum.
- **`Pane`** (`app/pane.rs`): per-pane `nav_history` (capped at 20), search-bar state, and
  `lyrics: Option<LyricsState>`. Splitting forks the pane (lyrics included; dropdowns reset);
  `SplitNode` is an equally-weighted `Leaf`/`Row`/`Column` tree (max 4 panes, `SplitDir::Horizontal` =
  side-by-side); `focused_pane_id` receives sidebar clicks, pane hover, and keyboard nav.
- **`TrackListKind`** (`app/interaction.rs`): `Queue` / `Active` / `Recent` — the single carrier for "which track list?" across messages, `DragState`, selection, and scroll targeting. Helpers: `first_index()` (1 for Queue, whose now-playing row renders outside the scrollable). Pass this instead of a bool. Scrollable/input/scroll widget ids are per-pane (`track_list_id(pane)` etc. in `ui/`), so scroll ops can't hit the wrong pane.
- **`TrackPos`** (`app/interaction.rs`): `{ index, list, pane }` — an index is only meaningful against its list, so they travel together (`pane` matters for `Active`; `Queue`/`Recent` normalize to `0` and ignore it). Carried by `TrackPressed`/`TrackRightClicked`/`PlayTrackAt`/`ContextMenuPlayTrack`, `DragState`'s `pressed` (`Pressed::Track`), `last_click`, and the `get_track_at`/`toggle_selection` accessors. Pass this instead of a loose `(usize, TrackListKind)` pair.
- **`ContextMenuState`**: `pos: TrackPos` + selection-aware `target_indices`. Ops apply to all
  selected if the right-clicked track is selected, else just it; "Play"/radio target only it.
  `Recent` tracks come from `recently_played` (queue/playlist items suppressed). Right-click focuses the source pane.
- **`DragState`** (`app/interaction.rs`): one `pressed: Option<Pressed>` (dragged thing),
  one `hovered: Option<HoverTarget>` (cursor target; `Track` doubles as keyboard focus),
  and `dragged: Option<(PaneId, TrackListKind, Vec<usize>)>` (indices resolved at press time).
  `drop_target`: `Track`/`Playlist`/`Library` (insertion line), `PlaylistAdd`, `PlaylistReorder`.
  Same-pane reorders; cross-list/pane copies move all selected; cards dropped on playlists become local playlists.
  Cleaned via `cleanup()`; accessors `hovered_track()`/`set_hovered*`.
- **Selection / list access** (`app/update/selection.rs`): `selection_in`, `toggle_selection`, `clear_selection`, `view_tracks_in`, `get_track_at`, `track_count_in` — keyed by pane + `TrackListKind` (unscoped shims target the focused pane).
- **`BackendResult`** (mpsc): `SearchResults`, `SearchResultsAppend`, `RadioResults`, `DownloadComplete(Track,String)`, `DownloadError`, `SearchError(u64, String)` (the rid routes to the requesting pane's slot), `ThumbnailDownloaded(provider, id)` (marks that entry downloaded), `LyricsFetched(Result<Lyrics, String>, String, LyricsProvider)` (applies to lyrics panes waiting on that track+provider; tick refetches per pane on track change), `NormalizationComputed(String, f32)` (caches a per-track gain in memory; read on subsequent plays), `CardPlaylistReady(usize, String, Vec<Track>)` (a dragged card became a playlist; fills the playlist at the given index with the browsed tracks), `TranslationDone/Error` (AI translation opens in the custom-lyrics editor under the target language). 250ms tick drains → `process_result`.
- **Media controls**: souvlaki thread → souvlaki's `MediaControlEvent` → `process_media_event` (tick); `MediaUpdate` flows main → thread.

## Data Flow & Navigation

- User → `Message` → `update()` → handler (spawns bg thread or mutates state). 250ms tick drains `result_rx` → `process_result`, `media_event_rx` → `process_media_event`; syncs audio, detects stream end → auto-next, sends media-control updates, updates progress. `view()` reads `&MusicPlayer`.
- `nav_history: Vec<ViewData>` + `nav_history_pos` per pane; Back if `pos > 0`, Forward if `pos + 1 < len`. `\` splits side-by-side, `Shift+\` stacks, `Ctrl+W`/header `X` closes (max 4 panes), `Ctrl`+Arrows move focus between adjacent panes with wraparound holding row/column (`SplitNode::neighbor`, `wrap_edge`); sidebar clicks, `/`, and keyboard nav target the focused pane (click or hover any pane to focus; `push_new_view` focuses its pane).
- `handle_navigate_to(pane, data: ViewData)`: truncates at `pos+1`, installs target `ViewData` (no-op self-nav skipped via `ViewData::same_kind`), pushes, advances `pos`.
- Results route by request id to the requesting pane's slot (`slot_for_request` scans all panes). `SearchResultsAppend` syncs in place; "Load More" hidden once a page returns < `SEARCH_PAGE_SIZE`.
- `Downloads` kind renders `ViewData.tracks` (synced from `DownloadRegistry` in tick); `Playlist` tracks read from `PlaylistStore` via `MusicPlayer::view_tracks_in`.

## UI Layout

- **Sidebar** (`SIDEBAR_WIDTH = 300.0`): nav buttons (Search/Downloads), scrollable playlist list, create-playlist input, local import.
- **Main**: `SplitNode` tiling of panes; each pane has its own search bar + view (Search / SongRadio / ArtistRadio / Playlist / Downloads / per-pane Lyrics). Split panes show a header (Back/Forward, title, close); single-pane mode has no extra chrome.
- **Queue panel** (`QUEUE_MIN_WIDTH = 240.0`, width `max(window_width*0.2, 240.0)`); **Playbar** (bottom): track info, progress, play/pause/next/prev/queue, volume.
- **Overlays** (exclusive `Dialog`, drop indicator, per-pane search-history dropdown, toast) via `iced::widget::Stack`.

## iced API Notes

- `bg_*()` / `button_style_*` → `impl Fn(&AppTheme) -> container::Style` (in `ui/styles.rs`); `slider_style()` via `AppTheme` `slider::Catalog`.
- `Vertical::Top` (not `::Start`); `rule::horizontal(height)` for dividers.
- Icons: `include_bytes!` + `icon(data,color,size)` → `svg::Svg::new(svg::Handle::from_memory(data))`.
- `event::listen_with()` takes an `fn` pointer; `Subscription::batch` (not `::chain`).
- `Text` uses `.center()`/`.align_x()`/`.align_y()`; `Button::on_press_maybe(Option<Message>)` for disabled.
- `Stack` for overlays; `MouseArea::on_move` for hover; `scrollable` `.on_scroll()`; `operation::scroll_to`/`scroll_by`.

## Audio Pipeline

`AudioPlayer` runs a dedicated output thread (mpsc command channel). Decoding is fully native via symphonia.

- **Stream+cache**: `yt-dlp -f bestaudio[ext=m4a]/bestaudio/best[ext=mp4]/best -o -` writes AAC audio (muxed MP4 fallback for age-restricted tracks, whose AAC symphonia still decodes) to the cache file (`.cache`, owned by `StreamCache`); a copy thread drains stdout and flips `writer_alive` when done.
- **Decoding**: a custom `SymphoniaStreamingSource` (rodio `Source` + `Iterator<Item=i16>`) wraps a non-seekable `GrowingMediaSource`, so symphonia demuxes sequentially and plays a still-growing file without `rodio::Decoder::new`'s `SeekError`. The reader blocks at EOF while `writer_alive`, so playback starts within a few KB.
- **Cached/downloaded/local** (`PlayCached`) reuse the same source with `writer_alive = None` (real `byte_len`) so seeking works on replay.
- **Stream completion**: when yt-dlp and the copy thread both finish (`writer_alive` false), the tick loop registers the cache; track end → sink empties → auto-advance.
- **Volume normalization** (optional `config.volume_normalization`): a per-track RMS/peak gain from `compute_normalization_gain`, applied per-sample inside `SymphoniaStreamingSource` (composes with `set_volume`, survives seeking). First play uses gain 1.0; `request_normalization_analysis` fills the cache afterwards.

## YouTube & Key Files

- `search()`/`browse()`: YouTube scoped search via `ytmusicapi` (`youtube_search.py`), `scope` → ytmusicapi `filter=`; YouTube pagination (`search_more`) falls back to `yt-dlp --flat-playlist`. `browse()` is dispatched by `ProviderId`: YouTube drills via `get_artist`/`get_album`/`get_playlist`; SoundCloud's songs/artists/albums/playlists search and drill-downs all use the `rsoundcloud` crate (no API key; artist browse → `get_user_tracks`, album/playlist → `get_playlist_tracks`; `yt-dlp` is used only for the actual SoundCloud stream/download since rsoundcloud exposes no plain stream URL); Bandcamp uses the `bandcamp` crate (async, driven via a shared multi-thread runtime; Songs scope merges direct track hits with album expansions fetched concurrently in a thread scope; keyless with direct 128k MP3 `streaming_url`s, so it streams via the direct-HTTP path and downloads without `yt-dlp`; artwork uses `Px300` (`_4.jpg`, ~34KB) instead of `Full` originals (~2.4MB) that stall the thumbnail pipeline); Last.fm is hand-rolled `ureq` against `ws.audioscrobbler.com` (no crate covers catalog search — existing crates are user-stats/scrobbling only; ships a bundled app key, search-only like MusicBrainz but with radio via `track.getsimilar`; all failures propagate as errors so a missing/invalid key shows a toast instead of an empty list; `track.search` serves one generic star placeholder for everything, so it is filtered out and Songs results are enriched with real album art via `track.getInfo` plus an artist top-album fallback, both fetched in parallel `thread::scope`s); MusicBrainz uses the `musicbrainz_rs` crate (`MbRecording::browse().by_artist()` for artist pages, `MbRelease::fetch().with_recordings()` for albums — the bare `artist/{id}/recordings` and `release/{id}/recordings` endpoints do NOT return recordings). Artists/Albums scopes return `CardData` (`SearchTab::Artists`/`Albums`), not fake `Track` stubs — only `Songs` produces playable tracks. `SEARCH_PAGE_SIZE = 10`. A blank/whitespace query is a browse, not a search: YouTube serves the global charts (`get_charts` → top music-video playlist / chart artists), SoundCloud and MusicBrainz return their own trending/match-all hits, and `run_search` no longer blocks empty queries (blank ones are skipped in search history and the browse page is marked exhausted — no pagination).
- `radio_song()`/`radio_artist()`: YouTube uses ytmusicapi's watch-playlist engine (`watch` mode, seeded by `video_id`/`browseId`, no yt-dlp fallback — needs Python + ytmusicapi); Last.fm uses `track.getsimilar` / top-tracks of similar artists; `download()`/`download_audio()` → `yt-dlp --extract-audio` MP3.
- `theme/`: `Palette`+`AppTheme` (`mod.rs`), constants (`layout.rs`, re-exported), `Catalog` impls (`catalog.rs`).
- **Artist page**: `ViewKind::Artist(ArtistEntry { id, name, source, page })` carries a serializable `ArtistPageState` (`providers/artist_page.rs`) - known per-provider artist ids, header, and a `sections: [ArtistSection; 4]` array indexed by `ArtistSectionKind` (each: selected provider + `LoadState<SectionContent>`); section ops (`start_section_load`, `serve_cached_section`, `merge_kind`, `fail_section`, `card_thumbs`) live on the type. `spawn_artist_kinds_fetch(provider, id, kinds)` (`ArtistDataKind`: Header/Popular/Albums/Playlists/Related) fetches only the requested pieces and delivers them incrementally - YT/MusicBrainz/Bandcamp/Last.fm answer in one call and split it, SoundCloud runs each endpoint as its own tokio task so sections land (and fail) independently. `open_artist()` loads the source provider's full page plus a Header-only companion fetch for each other header-capable provider (YouTube, SoundCloud, Bandcamp, Last.fm), in parallel; a section-picker switch serves from cache when covered, else loads just that kind. Results arrive as `BackendResult::ArtistIdResolved` + one `BackendResult::ArtistSectionLoaded` per kind and merge into the section currently selecting that provider; fetched kinds accumulate per provider in `CachedArtistPage` (page + which kinds arrived) so switching back is request-free. Popular tracks double as the view's track list.
- `ViewKind` (`app/view_data.rs`): `Search(SearchData)`/`SongRadio`/`ArtistRadio`/`Artist`/`Album(BrowseRef)`/`PlaylistView(BrowseRef)`/`Playlist(PlaylistEntry)`/`Downloads`/`Settings`. Variants hold data structs; callers destructure `kind` once and read child fields directly (no per-field accessor methods).
- `load_state.rs`: `LoadState<T, E = String>` (`Ready(T)` / `Failed(E)` / `Loading`) — used by `ViewData.content` (tracks + loading + error), `ArtistSection.state`, and `LyricsState.lyrics`.
- `util.rs`: `format_duration`, `fuzzy_match`, `plural_suffix`, `try_probe_duration`, `remove_at`, `reorder_tracks` (unit-tested).

## Maintenance

Keep `AGENTS.md` (and `README.md`) under ~150 lines and in sync after structural changes: new/removed files, renamed types/functions, module layout, keyboard shortcuts (`app/update/input.rs`), config fields (`data/config.rs`), on-disk paths (`data/*.rs` `FILE` consts), external tool requirements, or the audio pipeline.

- **i18n** (`src/i18n/`): all UI strings live in one `Strings` struct (`mod.rs`); each locale is a module exposing `pub const STRINGS: Strings` (copy `en.rs` and translate). Add a language by creating `src/i18n/<code>.rs` and appending one entry to the `languages!` macro in `mod.rs` — the `Language` enum, `label()`, and `strings()` are generated from it, so `config.language`, the Settings picker, and `view()` need no other edits. Parameterized/pluralized messages are `fn` fields (e.g. `added`/`n_saved`) so each locale owns word order and plural forms.
