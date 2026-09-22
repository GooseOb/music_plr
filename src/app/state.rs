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
        dialog::Dialog,
        interaction::{DragState, TrackListSearch, TrackPos},
        message::{BackendResult, Message},
        pane::{Pane, PaneId, SplitNode},
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
    load_state::LoadState,
    lyrics::LyricsProvider,
    media_controls::{MediaControlEvent, MediaUpdate},
    providers::ProviderId,
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
    /// Tiled main-view panes. Each pane owns its navigation history,
    /// search-bar state, and lyrics overlay; `split_root` arranges them and
    /// `focused_pane_id` receives ambiguous global actions (sidebar clicks,
    /// keyboard navigation).
    pub panes: std::collections::HashMap<PaneId, Pane>,
    pub split_root: SplitNode,
    pub focused_pane_id: PaneId,
    pub next_pane_id: PaneId,
    pub request_ids: RequestIdGenerator,
    /// Snapshot of the most recent completed search view.
    /// The sidebar "Search" item restores this instead of opening
    /// a blank search; `None` until the first search completes this session.
    /// Intentionally global (not per-pane): whichever pane searched last wins,
    /// and sidebar clicks target the focused pane.
    pub last_search_view: Option<ViewData>,

    pub queue: PlayQueue,
    pub show_queue: bool,
    pub repeat: bool,
    /// Default lyrics provider for newly opened lyrics panes. Each pane
    /// remembers its own selection in `LyricsState::provider`.
    pub lyrics_provider: LyricsProvider,

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
    /// Exclusive overlay dialog. Only one can be visible at a time; the view
    /// renders it on top of the main layout.
    pub dialog: Option<Dialog>,

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
    pub modifiers: iced::keyboard::Modifiers,
    pub last_click: Option<(TrackPos, std::time::Instant)>,
    pub selection_anchor: Option<TrackPos>,
    pub pending_vim_g: Option<Instant>,
    pub muted_volume: Option<f32>,

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

    /// Live status of dependency install/delete operations triggered from the
    /// Settings view (and mirrored from the startup dialog), keyed by dep.
    pub dep_ops: std::collections::HashMap<crate::deps::DepKind, DepOpState>,

    pub queue_selected_indices: Vec<usize>,
    pub recent_selected_indices: Vec<usize>,
    /// Model ids fetched from the configured translation server for the
    /// Settings picker. Session-only (`None` until first requested).
    pub translation_models: Option<LoadState<Vec<String>>>,
    /// Session-only substring filter for the fetched translation model list.
    pub translation_models_filter: String,

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
        (Self::default(), CaptureBounds::with_panes(vec![0]).into())
    }

    pub(crate) fn new_with(config: Config) -> Self {
        let (result_tx, result_rx) = mpsc::channel();
        let (media_event_tx, media_event_rx) = mpsc::channel();
        crate::deps::set_cookie_browser(config.cookie_browser.clone());

        let strings = config.language.strings();
        let app_theme = AppTheme::new(&Palette::from(config.theme_kind));
        let missing_deps = crate::deps::detect_missing();
        let found_deps: Vec<crate::deps::DepKind> = crate::deps::DepKind::all()
            .iter()
            .copied()
            .filter(|k| crate::deps::is_available(*k) && !crate::deps::installed_via_app(*k))
            .collect();
        let panes = std::collections::HashMap::from([(0, Pane::new(0))]);
        let mut player = Self {
            audio: AudioPlayer::new(0.8),
            search_history: SearchHistory::load(),
            stream_cache: StreamCache::new(config.cache_max_size_mb),
            pending_cache_id: None,
            normalization_cache: crate::audio::load_gains(),
            pending_normalization_id: None,
            lyrics_provider: LyricsProvider::default(),
            config,
            panes,
            split_root: SplitNode::Leaf(0),
            focused_pane_id: 0,
            next_pane_id: 1,
            last_search_view: None,
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
            dialog: (!missing_deps.is_empty())
                .then(|| Dialog::Dependencies(DependencyDialog::new(missing_deps, found_deps))),
            library: LibraryStore::load(),
            library_expanded: false,
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
            queue_selected_indices: Vec::new(),
            recent_selected_indices: Vec::new(),
            translation_models: None,
            translation_models_filter: String::new(),
            now_playing_from: None,
            track_list_search: None,
            app_theme,
            bounds: CaptureBounds::default(),
            window_size: iced::Size::default(),
            clipboard: Vec::new(),
            modifiers: iced::keyboard::Modifiers::empty(),
            last_click: None,
            selection_anchor: None,
            pending_vim_g: None,
            muted_volume: None,
            strings,
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
                player
                    .thumbnail_index
                    .ensure(item.provider, &item.id, &item.thumbnail);
            }
        }
        crate::app::update::cleanup_stale_update();
        player.check_for_updates();
        player
    }

    #[inline]
    pub fn pane(&self, id: PaneId) -> &Pane {
        &self.panes[&id]
    }

    #[inline]
    pub fn pane_mut(&mut self, id: PaneId) -> &mut Pane {
        self.panes.get_mut(&id).expect("unknown pane")
    }

    #[inline]
    pub fn focused_pane(&self) -> &Pane {
        self.pane(self.focused_pane_id)
    }

    /// Reset to a deterministic single pane for tests. `new_with` restores
    /// the real on-disk session, so tests must not assume its shape.
    #[cfg(test)]
    pub(crate) fn reset_test_pane(&mut self, history: Vec<ViewData>) {
        let mut pane = Pane::new(0);
        pane.nav_history = history;
        pane.nav_history_pos = 0;
        self.panes = std::collections::HashMap::from([(0, pane)]);
        self.split_root = SplitNode::Leaf(0);
        self.focused_pane_id = 0;
        self.next_pane_id = 1;
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        let mut ids: Vec<PaneId> = self.panes.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// Borrow a pane's active view state (its live
    /// `nav_history[nav_history_pos]`).
    #[inline]
    pub fn view_data_in(&self, id: PaneId) -> &ViewData {
        self.pane(id).view_data()
    }

    /// Mutably borrow a pane's active view state.
    #[inline]
    pub fn view_data_in_mut(&mut self, id: PaneId) -> &mut ViewData {
        self.pane_mut(id).view_data_mut()
    }

    /// Borrow the focused pane's active view state. This is the single source
    /// of truth for the current view; there is no separate `view_data` field,
    /// so all reads go through here. Prefer [`Self::view_data_in`] when a
    /// pane id is already at hand.
    #[inline]
    pub fn view_data(&self) -> &ViewData {
        self.focused_pane().view_data()
    }

    /// Mutably borrow the focused pane's active view state. Prefer
    /// [`Self::view_data_in_mut`] when a pane id is already at hand.
    #[inline]
    pub fn view_data_mut(&mut self) -> &mut ViewData {
        let id = self.focused_pane_id;
        self.view_data_in_mut(id)
    }

    pub fn view(&self) -> iced::Element<'_, Message, AppTheme> {
        ui::view(self)
    }
}
