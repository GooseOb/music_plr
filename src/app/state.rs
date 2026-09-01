//! Root application state: the single source of truth for the whole player.
//!
//! `MusicPlayer` owns every piece of mutable state; `view()` is a pure
//! function over `&self` (delegating to `ui`), and `update`/`subscription`
//! live in `app/update` so this file stays focused on construction and the
//! active-view accessors.

use std::{sync::mpsc, time::Instant};

use iced::Task;

use crate::{
    app::{
        dependency_dialog::{DepOpState, DependencyDialog},
        edit_track::EditTrackState,
        import::ImportPlaylistDialog,
        interaction::{ContextMenuState, DragState, TrackListSearch, TrackPos},
        lyrics_state::LyricsState,
        message::{BackendResult, Message},
        playlist_picker::PlaylistPicker,
        ui,
        update::operation::CaptureBounds,
        view_data::{RequestIdGenerator, ViewData},
    },
    audio::AudioPlayer,
    data::{
        cache::StreamCache, config::Config, downloads::DownloadRegistry, library::LibraryStore,
        playlists::PlaylistStore, search_history::SearchHistory, thumbnails::ThumbnailIndex,
        JsonStore,
    },
    i18n::Strings,
    lyrics::{LyricsClient, LyricsProvider},
    media_controls::{MediaControlEvent, MediaUpdate},
    providers::{ProviderId, SearchScope},
    theme::{AppTheme, Palette},
    types::{PlayQueue, Track},
};

#[derive(Clone)]
pub struct Toast {
    pub message: std::borrow::Cow<'static, str>,
    pub until: std::time::Instant,
    pub is_error: bool,
}

#[derive(Clone, Debug)]
pub struct PendingCache {
    pub provider_id: ProviderId,
    pub id: String,
}

#[allow(clippy::struct_excessive_bools)]
pub struct MusicPlayer {
    pub audio: AudioPlayer,
    pub config: Config,
    pub strings: &'static Strings,
    /// Back/forward navigation history. Each entry is a full `View` snapshot;
    /// `nav_history_pos` indexes the active one, which is the single source of
    /// truth for which view is active and its data (see [`Self::view_data`]).
    pub nav_history: Vec<ViewData>,
    pub nav_history_pos: usize,
    pub request_ids: RequestIdGenerator,
    /// The search-bar text. Kept on `MusicPlayer` because the search bar is
    /// always visible regardless of which view is active.
    pub search_query: String,
    /// The active search scope (All / Songs / Videos / Artists / Albums /
    /// Playlists). Global UI state, like `search_query`.
    pub search_scope: SearchScope,
    /// The active search provider (`YouTube` / `SoundCloud` / …). The scope list is
    /// filtered to this provider's supported scopes. Global UI state.
    pub search_provider: ProviderId,
    /// Snapshot of the most recent completed search view.
    /// The sidebar "Search" item restores this instead of opening
    /// a blank search; `None` until the first search completes this session.
    pub last_search_view: Option<ViewData>,
    /// Whether the search-history dropdown is open (global UI state).
    pub show_search_history: bool,
    /// Filtered history list for the dropdown (derived from
    /// `search_history` + `search_query`).
    pub last_filtered_history: Vec<String>,

    pub queue: PlayQueue,
    pub show_queue: bool,
    pub repeat: bool,
    pub lyrics_client: LyricsClient,
    pub lyrics: Option<LyricsState>,

    pub is_playing: bool,
    pub volume: f32,
    pub progress: f32,
    pub duration: f32,
    pub track_loading: bool,

    pub download_registry: DownloadRegistry,

    pub notification: Option<Toast>,

    pub artist_error_dedup: Option<(u64, ProviderId)>,

    pub thumbnail_index: ThumbnailIndex,
    pub playlists: PlaylistStore,
    pub playlist_create_name: String,
    pub playlist_picker: Option<PlaylistPicker>,
    pub delete_confirm_index: Option<usize>,
    pub import_dialog: Option<ImportPlaylistDialog>,

    pub library: LibraryStore,
    pub library_expanded: bool,

    pub search_history: SearchHistory,
    pub stream_cache: StreamCache,
    pub pending_cache_id: Option<PendingCache>,
    /// Per-track volume-normalization gains, computed in the background and
    /// kept in memory (not persisted) so subsequent plays are normalized.
    pub normalization_cache: std::collections::HashMap<String, f32>,
    /// Track id whose normalization gain should be analyzed once its stream
    /// cache finishes downloading.
    pub pending_normalization_id: Option<String>,

    pub clipboard: Vec<Track>,
    pub last_click: Option<(TrackPos, std::time::Instant)>,

    pub result_tx: mpsc::Sender<BackendResult>,
    pub result_rx: mpsc::Receiver<BackendResult>,
    pub media_event_tx: mpsc::Sender<MediaControlEvent>,
    pub media_event_rx: mpsc::Receiver<MediaControlEvent>,
    pub media_update_tx: Option<mpsc::Sender<MediaUpdate>>,
    pub media_controls_dirty: bool,
    /// Guards `init_media_controls` so it runs at most once (the Windows HWND
    /// is resolved asynchronously after the window opens).
    pub media_controls_started: bool,
    pub session_dirty: bool,
    pub last_session_flush: Instant,

    pub drag: DragState,

    pub context_menu: Option<ContextMenuState>,
    pub edit_track: Option<EditTrackState>,

    /// Startup "missing dependencies" dialog, open when external tools the
    /// app needs are absent. `None` once dismissed or when everything is
    /// present.
    pub dep_dialog: Option<DependencyDialog>,
    /// Live status of dependency install/delete operations triggered from the
    /// Settings view (and mirrored from the startup dialog), keyed by dep.
    pub dep_ops: std::collections::HashMap<crate::deps::DepKind, DepOpState>,

    pub queue_selected_indices: Vec<usize>,
    pub recent_selected_indices: Vec<usize>,

    pub now_playing_from: Option<ViewData>,

    pub track_list_search: Option<TrackListSearch>,

    pub app_theme: AppTheme,

    pub bounds: CaptureBounds,
    pub window_size: iced::Size,

    pub update_status: crate::app::update::UpdateStatus,
}

impl Default for MusicPlayer {
    fn default() -> Self {
        let config = Config::load();
        Self::new_with(config)
    }
}

impl MusicPlayer {
    pub fn new() -> (Self, Task<Message>) {
        (Self::default(), CaptureBounds::new().into())
    }

    pub(crate) fn new_with(config: Config) -> Self {
        let (result_tx, result_rx) = mpsc::channel();
        let (media_event_tx, media_event_rx) = mpsc::channel();

        let strings = config.language.strings();
        let app_theme = AppTheme::new(Palette::from(config.theme_kind));
        let missing_deps = crate::deps::detect_missing();
        let found_deps: Vec<crate::deps::DepKind> = crate::deps::DepKind::all()
            .iter()
            .copied()
            .filter(|k| crate::deps::is_available(*k) && !crate::deps::installed_via_app(*k))
            .collect();
        let mut player = Self {
            audio: AudioPlayer::new(0.8),
            search_history: SearchHistory::load(),
            stream_cache: StreamCache::new(config.cache_max_size_mb),
            pending_cache_id: None,
            normalization_cache: std::collections::HashMap::new(),
            pending_normalization_id: None,
            lyrics_client: LyricsClient::new(LyricsProvider::default()),
            lyrics: None,
            config,
            search_query: String::new(),
            search_scope: SearchScope::Songs,
            search_provider: if ProviderId::YouTube.capabilities().search {
                ProviderId::YouTube
            } else {
                ProviderId::searchable()
                    .iter()
                    .copied()
                    .find(|p| p.capabilities().search)
                    .unwrap_or(ProviderId::SoundCloud)
            },
            last_search_view: None,
            show_search_history: false,
            last_filtered_history: Vec::new(),
            queue: PlayQueue::new(),
            is_playing: false,
            volume: 0.8,
            progress: 0.0,
            duration: 0.0,
            download_registry: DownloadRegistry::load(),
            notification: None,
            artist_error_dedup: None,
            track_loading: false,
            playlists: PlaylistStore::load(),
            playlist_create_name: String::new(),
            show_queue: false,
            repeat: false,
            thumbnail_index: ThumbnailIndex::load(),
            playlist_picker: None,
            delete_confirm_index: None,
            import_dialog: None,
            library: LibraryStore::load(),
            library_expanded: false,
            nav_history: vec![ViewData::default()],
            nav_history_pos: 0,
            request_ids: RequestIdGenerator::default(),
            result_tx,
            result_rx,
            media_event_tx,
            media_event_rx,
            media_update_tx: None,
            media_controls_dirty: true,
            media_controls_started: false,
            session_dirty: true,
            // Backdate so the first `flush_session` isn't throttled.
            last_session_flush: Instant::now()
                .checked_sub(std::time::Duration::from_secs(10))
                .unwrap_or_else(Instant::now),
            drag: DragState::default(),
            context_menu: None,
            edit_track: None,
            queue_selected_indices: Vec::new(),
            recent_selected_indices: Vec::new(),
            now_playing_from: None,
            track_list_search: None,
            app_theme,
            bounds: CaptureBounds::default(),
            window_size: iced::Size::default(),
            clipboard: Vec::new(),
            last_click: None,
            strings,
            dep_dialog: (!missing_deps.is_empty())
                .then(|| DependencyDialog::new(missing_deps, found_deps)),
            dep_ops: std::collections::HashMap::new(),
            update_status: crate::app::update::UpdateStatus::default(),
        };

        // Linux and macOS need no window handle, so start immediately. Windows
        // requires the HWND, which arrives later via the window-opened
        // subscription (see `media_hwnd_subscription` in dispatch).
        #[cfg(not(target_os = "windows"))]
        player.init_media_controls(None);
        player.restore_session();
        player.resume_playback();
        for item in &player.library.items {
            if !item.thumbnail.is_empty() {
                player.thumbnail_index.ensure(&item.id, &item.thumbnail);
            }
        }
        crate::app::update::cleanup_stale_update();
        player.check_for_updates();
        player
    }

    /// Borrow the active view state (the live `nav_history[nav_history_pos]`).
    /// This is the single source of truth for the current view; there is no
    /// separate `view_data` field, so all reads go through here.
    #[inline]
    pub fn view_data(&self) -> &ViewData {
        &self.nav_history[self.nav_history_pos]
    }

    /// Mutably borrow the active view state.
    #[inline]
    pub fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.nav_history[self.nav_history_pos]
    }

    pub fn view(&self) -> iced::Element<'_, Message, AppTheme> {
        ui::view(self)
    }
}
