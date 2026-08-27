use std::{sync::mpsc, time::Duration};

use iced::{Subscription, Task};
use tracing::{error, warn};

use crate::{
    app::update::{operation::ContextMenuGeometry, settings::SettingsChange},
    audio::AudioPlayer,
    data::{
        cache::StreamCache, config, downloads::DownloadRegistry, playlists::PlaylistStore,
        search_history::SearchHistory, JsonStore,
    },
    mpris::{self, MprisCommand, MprisUpdate},
    theme::{AppTheme, Palette},
    types::{PlayQueue, Track},
};

mod import;
mod interaction;
mod message;
mod ui;
mod update;
mod view_data;

pub use import::{ImportCsvField, ImportMethod, ImportPlaylistDialog};
pub use interaction::{
    ContextMenuState, DefaultCtxAction, DragState, TrackListKind, TrackListSearch, TrackPos,
};
pub use message::{BackendResult, EditTrackField, Message};
pub use view_data::{RequestIdGenerator, ViewData, ViewKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LyricsViewMode {
    Selectable,
    Synced,
    Plain,
}

#[derive(Debug, Clone)]
pub struct LyricsState {
    pub track_id: Option<String>,
    pub lyrics: crate::load_state::LoadState<crate::lyrics::Lyrics>,
    pub mode: LyricsViewMode,
    pub editor: iced::widget::text_editor::Content,
}

/// Mutable working copy of a track being edited in the track-editing popup.
/// Holds only the text-editable fields (`source` is changed via the provider
/// "select" buttons) plus the original position so the edit can be written
/// back to the correct list.
#[derive(Debug, Clone)]
pub struct EditTrackState {
    pub title: String,
    pub artist: String,
    pub source: crate::providers::ProviderId,
    pub original: Track,
    pub pos: TrackPos,
    /// The provider whose "Find" action is currently in flight, if any.
    pub finding: Option<crate::providers::ProviderId>,
}

impl LyricsViewMode {
    pub fn for_lyrics(lyrics: &crate::lyrics::Lyrics) -> Self {
        if lyrics.timed.is_empty() {
            Self::Plain
        } else {
            Self::Synced
        }
    }
}

impl LyricsState {
    fn new() -> Self {
        Self {
            track_id: None,
            lyrics: crate::load_state::LoadState::Loading,
            mode: LyricsViewMode::Selectable,
            editor: iced::widget::text_editor::Content::default(),
        }
    }

    pub fn mode_available(&self, mode: LyricsViewMode) -> bool {
        let crate::load_state::LoadState::Ready(lyrics) = &self.lyrics else {
            return false;
        };
        match mode {
            LyricsViewMode::Selectable => !(lyrics.timed.is_empty() && lyrics.plain.is_empty()),
            LyricsViewMode::Synced => !lyrics.timed.is_empty(),
            LyricsViewMode::Plain => !lyrics.plain.is_empty(),
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
pub struct MusicPlayer {
    pub audio: AudioPlayer,
    pub config: crate::data::config::Config,
    pub strings: &'static crate::i18n::Strings,
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
    pub search_scope: crate::providers::SearchScope,
    /// The active search provider (`YouTube` / `SoundCloud` / …). The scope list is
    /// filtered to this provider's supported scopes. Global UI state.
    pub search_provider: crate::providers::ProviderId,
    /// Whether the search-history dropdown is open (global UI state).
    pub show_search_history: bool,
    /// Filtered history list for the dropdown (derived from
    /// `search_history` + `search_query`).
    pub last_filtered_history: Vec<String>,

    pub queue: PlayQueue,
    pub show_queue: bool,
    pub repeat: bool,
    pub lyrics_client: crate::lyrics::LyricsClient,
    pub lyrics: Option<LyricsState>,

    pub is_playing: bool,
    pub volume: f32,
    pub progress: f32,
    pub duration: f32,
    pub track_loading: bool,

    pub download_registry: DownloadRegistry,

    pub notification: Option<std::borrow::Cow<'static, str>>,

    pub artist_error_dedup: Option<(u64, crate::providers::ProviderId)>,

    pub thumbnail_index: crate::data::thumbnails::ThumbnailIndex,
    pub playlists: PlaylistStore,
    pub playlist_create_name: String,
    pub playlist_picker: Option<PlaylistPicker>,
    pub delete_confirm_index: Option<usize>,
    pub import_dialog: Option<ImportPlaylistDialog>,

    pub library: crate::data::library::LibraryStore,
    pub library_expanded: bool,

    pub search_history: SearchHistory,
    pub stream_cache: StreamCache,
    pub pending_cache_id: Option<String>,
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
    pub mpris_cmd_tx: mpsc::Sender<MprisCommand>,
    pub mpris_cmd_rx: mpsc::Receiver<MprisCommand>,
    pub mpris_update_tx: Option<mpsc::Sender<MprisUpdate>>,
    pub mpris_dirty: bool,
    pub session_dirty: bool,
    pub last_session_flush: std::time::Instant,

    pub drag: DragState,

    pub context_menu: Option<ContextMenuState>,
    pub edit_track: Option<EditTrackState>,

    pub queue_selected_indices: Vec<usize>,
    pub recent_selected_indices: Vec<usize>,

    pub now_playing_from: Option<ViewData>,

    pub track_list_search: Option<TrackListSearch>,

    pub app_theme: AppTheme,

    pub bounds: crate::app::update::operation::CaptureBounds,
    pub window_size: iced::Size,
}

pub struct PlaylistPicker {
    pub indices: Vec<usize>,
    pub list: TrackListKind,
}

impl Default for MusicPlayer {
    fn default() -> Self {
        let config = config::Config::load();
        Self::new_with(config)
    }
}

impl MusicPlayer {
    pub fn new() -> (Self, Task<Message>) {
        // Capture scrollable geometry once on the first render so the track
        // lists can virtualize immediately at startup (before any mouse move
        // or scroll). Live scroll offsets are then kept fresh by `on_scroll`
        // via `Message::ListScrolled`; `CursorMoved` re-captures for drag
        // hit-testing.
        (
            Self::default(),
            iced_runtime::task::widget(update::operation::CaptureBounds::new()),
        )
    }

    fn new_with(config: crate::data::config::Config) -> Self {
        let (result_tx, result_rx) = mpsc::channel();
        let (mpris_cmd_tx, mpris_cmd_rx) = mpsc::channel();

        let strings = config.language.strings();
        let app_theme = AppTheme::new(Palette::from(config.theme_kind));
        let mut player = Self {
            audio: AudioPlayer::new(0.8),
            search_history: SearchHistory::load(),
            stream_cache: StreamCache::new(config.cache_max_size_mb),
            pending_cache_id: None,
            normalization_cache: std::collections::HashMap::new(),
            pending_normalization_id: None,
            lyrics_client: crate::lyrics::LyricsClient::new(
                crate::lyrics::LyricsProvider::default(),
            ),
            lyrics: None,
            config,
            search_query: String::new(),
            search_scope: crate::providers::SearchScope::Songs,
            search_provider: crate::providers::ProviderId::YouTube,
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
            thumbnail_index: crate::data::thumbnails::ThumbnailIndex::load(),
            playlist_picker: None,
            delete_confirm_index: None,
            import_dialog: None,
            library: crate::data::library::LibraryStore::load(),
            library_expanded: false,
            nav_history: vec![ViewData::default()],
            nav_history_pos: 0,
            request_ids: RequestIdGenerator::default(),
            result_tx,
            result_rx,
            mpris_cmd_tx,
            mpris_cmd_rx,
            mpris_update_tx: None,
            mpris_dirty: true,
            session_dirty: true,
            // Backdate so the first `flush_session` isn't throttled.
            last_session_flush: std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(10))
                .unwrap_or_else(std::time::Instant::now),
            drag: DragState::default(),
            context_menu: None,
            edit_track: None,
            queue_selected_indices: Vec::new(),
            recent_selected_indices: Vec::new(),
            now_playing_from: None,
            track_list_search: None,
            app_theme,
            bounds: crate::app::update::operation::CaptureBounds::default(),
            window_size: iced::Size::default(),
            clipboard: Vec::new(),
            last_click: None,
            strings,
        };

        player.init_mpris();
        player.restore_session();
        player.resume_playback();
        for item in &player.library.items {
            if !item.thumbnail.is_empty() {
                player.thumbnail_index.ensure(&item.id, &item.thumbnail);
            }
        }
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

    #[allow(clippy::unused_self)]
    pub fn subscription(&self) -> Subscription<Message> {
        let timer = iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick);

        let events = iced::event::listen_with(Self::event_to_message);

        Subscription::batch([timer, events])
    }

    // `iced::event::listen_with` hands the event over by value, so the
    // by-value parameter is mandated by the API.
    #[allow(clippy::needless_pass_by_value)]
    fn event_to_message(
        event: iced::Event,
        status: iced::event::Status,
        _window: iced::window::Id,
    ) -> Option<Message> {
        match event {
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                Some(Message::CursorMoved(position))
            }
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Some(Message::LeftButtonReleased)
            }
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                physical_key,
                modifiers,
                ..
            }) => {
                if status == iced::event::Status::Captured {
                    return None;
                }
                Some(Message::KeyPressed {
                    key: physical_key,
                    modifiers,
                })
            }
            iced::Event::Window(iced::window::Event::CloseRequested) => Some(Message::WindowClose),
            iced::Event::Window(iced::window::Event::Resized(size)) => {
                Some(Message::WindowResized(size))
            }
            _ => None,
        }
    }

    /// Flat dispatch over every `Message` variant. Long by nature: each arm is
    /// a one-to-three-line delegation to a handler in `app/update/`.
    #[allow(clippy::too_many_lines)]
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.handle_tick();
                Task::none()
            }
            Message::WindowResized(size) => {
                self.window_size = size;
                iced_runtime::task::widget(update::operation::CaptureBounds::new())
            }
            Message::WindowClose => {
                self.flush_session();
                Task::none()
            }
            Message::CursorMoved(pos) => self.handle_cursor_moved(pos),
            Message::LeftButtonReleased => self.handle_left_release(),
            Message::ListBoundsCaptured(bounds) => {
                let scroll = bounds.track.as_ref().map_or(0.0, |b| b.translation_y);
                self.bounds = *bounds;
                self.view_data_mut().scroll = scroll;

                Task::none()
            }
            Message::SearchHistoryBoundsCaptured(geo) => {
                self.bounds.search_history = Some(geo);
                Task::none()
            }
            Message::ListScrolled {
                list,
                translation_y,
            } => {
                let geo = match list {
                    TrackListKind::Queue => &mut self.bounds.queue,
                    TrackListKind::Active => &mut self.bounds.track,
                    TrackListKind::Recent => &mut self.bounds.recent,
                };
                if let Some(g) = geo {
                    g.translation_y = translation_y;
                }
                Task::none()
            }
            Message::KeyPressed { key, modifiers } => self.handle_key_press(key, modifiers),
            Message::LyricsEditorAction(action) => {
                if let Some(state) = &mut self.lyrics {
                    if !matches!(action, iced::widget::text_editor::Action::Edit(_)) {
                        state.editor.perform(action);
                    }
                }
                Task::none()
            }
            Message::CopyLyrics => {
                let Some(state) = &self.lyrics else {
                    return Task::none();
                };
                let text = match &state.lyrics {
                    crate::load_state::LoadState::Ready(lyrics) => lyrics.plain.clone(),
                    _ => return Task::none(),
                };
                if text.is_empty() {
                    return Task::none();
                }
                self.notify(self.strings.lyrics_copied);
                iced::clipboard::write(text)
            }
            Message::SearchInputChanged(query) => {
                self.search_query = query;
                self.update_search_history();
                self.drag.clear_hovered_search_history();
                iced_runtime::task::widget(update::operation::CaptureSearchHistoryRows::new())
            }
            Message::SearchExecute => {
                self.handle_search_execute();
                Task::none()
            }
            Message::SearchScopeChanged(scope) => {
                self.handle_search_scope_changed(scope);
                Task::none()
            }
            Message::SearchProviderChanged(provider) => {
                self.handle_search_provider_changed(provider);
                Task::none()
            }
            Message::Browse(kind, provider) => {
                self.handle_browse(&kind, provider);
                Task::none()
            }
            Message::OpenArtist { id, name, source } => {
                self.open_artist(&id, &name, source);
                Task::none()
            }
            Message::ArtistSectionProviderChanged(section, provider) => {
                self.handle_artist_section_provider_changed(section, provider);
                Task::none()
            }
            Message::ArtistHeaderProviderChanged(provider) => {
                self.handle_artist_header_provider_changed(provider);
                Task::none()
            }
            Message::ToggleLibrarySave(item) => {
                let saved = self.toggle_library_save(item);
                self.notify(if saved {
                    self.strings.saved_to_library
                } else {
                    self.strings.removed_from_library
                });
                Task::none()
            }
            Message::ToggleLibraryExpanded => {
                self.library_expanded = !self.library_expanded;
                Task::none()
            }
            Message::SearchLoadMore => {
                self.handle_search_load_more();
                Task::none()
            }
            Message::SearchHistorySelected(index) => {
                self.handle_search_history_select(index);
                Task::none()
            }
            Message::DeleteSearchHistory(index) => {
                self.handle_delete_search_history(index);
                Task::none()
            }
            Message::DragPress(pressed) => {
                self.handle_drag_press(pressed);
                Task::none()
            }
            Message::HoverStart(target) => {
                if !self.drag.is_hover_controlled {
                    self.drag.set_hovered(target);
                }
                Task::none()
            }
            Message::TrackRightClicked(pos) => {
                self.show_context_menu(pos);
                iced_runtime::task::widget(update::operation::CaptureContextMenu::default())
            }
            Message::PlayTrackAt(pos) => {
                self.handle_play_track(pos);
                Task::none()
            }
            Message::TogglePlayPause => {
                self.toggle_play_pause();
                Task::none()
            }
            Message::NextTrack => {
                self.next_track();
                Task::none()
            }
            Message::PreviousTrack => {
                self.previous_track();
                Task::none()
            }
            Message::SetVolume(vol) => {
                self.set_volume(vol);
                Task::none()
            }
            Message::Seek(frac) => {
                self.seek(frac);
                Task::none()
            }
            Message::CreatePlaylist => {
                self.handle_create_playlist();
                Task::none()
            }
            Message::NewPlaylistNameChanged(name) => {
                self.playlist_create_name = name;
                Task::none()
            }
            Message::RenamePlaylist(name) => {
                self.handle_rename_playlist(&name);
                Task::none()
            }
            Message::AddLocalMusic => {
                let tx = self.result_tx.clone();
                std::thread::spawn(move || {
                    let files = rfd::FileDialog::new()
                        .add_filter(
                            "Audio",
                            &["mp3", "flac", "wav", "ogg", "m4a", "aac", "opus", "wma"],
                        )
                        .pick_files();
                    if let Some(files) = files.filter(|f| !f.is_empty()) {
                        let _ = tx.send(BackendResult::LocalFilesPicked(files));
                    }
                });
                Task::none()
            }
            Message::AddToPlaylist(playlist_idx) => {
                if let Some(picker) = self.playlist_picker.take() {
                    self.handle_add_to_playlist(playlist_idx, &picker.indices, picker.list);
                }
                Task::none()
            }
            Message::TogglePicker(indices) => {
                let list = self
                    .context_menu
                    .as_ref()
                    .map_or(TrackListKind::Active, |m| m.pos.list);
                self.handle_toggle_picker(indices, list);
                Task::none()
            }
            Message::ClosePicker => {
                self.playlist_picker = None;
                Task::none()
            }
            Message::ShowDeleteConfirm(index) => {
                self.delete_confirm_index = Some(index);
                Task::none()
            }
            Message::ConfirmDeletePlaylist => {
                if let Some(idx) = self.delete_confirm_index {
                    self.handle_delete_playlist(idx);
                }
                self.delete_confirm_index = None;
                Task::none()
            }
            Message::HideDeleteConfirm => {
                self.delete_confirm_index = None;
                Task::none()
            }
            Message::OpenImportPlaylist => {
                self.import_dialog = Some(ImportPlaylistDialog::default());
                Task::none()
            }
            Message::CloseImportPlaylist => {
                self.import_dialog = None;
                Task::none()
            }
            Message::ImportMethodChanged(method) => {
                if let Some(dialog) = &mut self.import_dialog {
                    dialog.method = method;
                }
                Task::none()
            }
            Message::ImportCsvColChanged(field, value) => {
                if let Some(dialog) = &mut self.import_dialog {
                    match field {
                        ImportCsvField::Name => dialog.csv_name_col = value,
                        ImportCsvField::Artist => dialog.csv_artist_col = value,
                        ImportCsvField::Album => dialog.csv_album_col = value,
                    }
                }
                Task::none()
            }
            Message::ImportPlaylistNameChanged(value) => {
                if let Some(dialog) = &mut self.import_dialog {
                    dialog.playlist_name = value;
                }
                Task::none()
            }
            Message::ImportPatternChanged(index, value) => {
                if let Some(dialog) = &mut self.import_dialog {
                    if let Some(slot) = dialog.patterns.get_mut(index) {
                        *slot = value;
                    }
                }
                Task::none()
            }
            Message::ImportAddPattern => {
                if let Some(dialog) = &mut self.import_dialog {
                    dialog.patterns.push(String::new());
                }
                Task::none()
            }
            Message::ImportRemovePattern(index) => {
                if let Some(dialog) = &mut self.import_dialog {
                    if index < dialog.patterns.len() {
                        dialog.patterns.remove(index);
                    }
                }
                Task::none()
            }
            Message::ImportSelectFiles => {
                self.handle_import_pick();
                Task::none()
            }
            Message::OpenAndPlayPlaylist(index) => {
                self.handle_open_and_play_playlist(index);
                Task::none()
            }
            Message::TrackListSearchInput(query) => self.handle_track_list_search_input(&query),
            Message::TrackListSearchNext => self.handle_track_list_search_step(1),
            Message::TrackListSearchPrev => self.handle_track_list_search_step(-1),
            Message::TrackListSearchClose => {
                self.track_list_search = None;
                Task::none()
            }
            Message::ToggleQueue => {
                self.show_queue = !self.show_queue;
                self.save_session();
                Task::none()
            }
            Message::ToggleRepeat => {
                self.repeat = !self.repeat;
                self.save_session();
                Task::none()
            }
            Message::ShowLyrics => {
                self.handle_show_lyrics();
                Task::none()
            }
            Message::RevealNowPlaying => self.handle_reveal_now_playing(),
            Message::SetLyricsViewMode(mode) => {
                self.set_lyrics_view_mode(mode);
                Task::none()
            }
            Message::LyricsLineClicked(secs) => {
                self.seek_to_seconds(secs);
                Task::none()
            }
            Message::SelectLyricsProvider(id) => {
                self.handle_select_lyrics_provider(id);
                Task::none()
            }
            Message::SwitchQueueTab(tab) => {
                self.queue.queue_tab = tab;
                self.drag.clear_hovered_track();
                self.save_session();
                Task::none()
            }
            Message::NavigateTo(data) => {
                self.lyrics = None;
                self.handle_navigate_to(data);
                Task::none()
            }
            Message::NavigateBack => {
                if self.lyrics.is_some() {
                    self.lyrics = None;
                    Task::none()
                } else {
                    self.handle_navigate_back()
                }
            }
            Message::NavigateForward => self.handle_navigate_forward(),
            Message::SettingsDownloadDirChanged(dir) => {
                self.handle_settings_change(SettingsChange::DownloadDir(dir));
                Task::none()
            }
            Message::SettingsMaxHistoryVisibleChanged(v) => {
                self.handle_settings_change(SettingsChange::MaxHistoryVisible(v));
                Task::none()
            }
            Message::SettingsMaxHistoryStoredChanged(v) => {
                self.handle_settings_change(SettingsChange::MaxHistoryStored(v));
                Task::none()
            }
            Message::SettingsCacheMaxSizeChanged(v) => {
                self.handle_settings_change(SettingsChange::CacheMaxSize(v));
                Task::none()
            }
            Message::SettingsMaxRecentlyPlayedChanged(v) => {
                self.handle_settings_change(SettingsChange::MaxRecentlyPlayed(v));
                Task::none()
            }
            Message::SettingsVolumeNormalizationToggled(enabled) => {
                self.handle_settings_change(SettingsChange::VolumeNormalization(enabled));
                Task::none()
            }
            Message::SettingsLanguageChanged(language) => {
                self.handle_settings_change(SettingsChange::Language(language));
                Task::none()
            }
            Message::SettingsDefaultProviderChanged(provider) => {
                self.handle_settings_change(SettingsChange::DefaultProvider(provider));
                Task::none()
            }
            Message::SettingsThemeChanged(kind) => {
                self.handle_settings_change(SettingsChange::Theme(kind));
                Task::none()
            }
            Message::SettingsResetDefaults => {
                self.handle_settings_reset_defaults();
                Task::none()
            }
            Message::ContextMenuPlayTrack(pos) => {
                self.close_context_menu();
                self.handle_play_track(pos);
                Task::none()
            }
            Message::ContextMenuGoToArtist => {
                let provider = match self.context_menu.as_ref() {
                    Some(menu) => menu.default_go_to_artist_provider(self.config.default_provider),
                    None => return Task::none(),
                };
                self.handle_context_menu_go_to_artist(provider);
                Task::none()
            }
            Message::ContextMenuGoToArtistProvider(provider) => {
                self.handle_context_menu_go_to_artist(provider);
                Task::none()
            }
            Message::ContextMenuPlayViaProvider(provider, pos) => {
                self.close_context_menu();
                self.play_track_via_provider(provider, pos);
                Task::none()
            }
            Message::ContextMenuDownloadViaProvider(provider) => {
                self.download_track_via_provider(provider);
                Task::none()
            }
            Message::ContextMenuSongRadioProvider(provider) => {
                self.handle_context_menu_song_radio(provider);
                Task::none()
            }
            Message::ContextMenuArtistRadioProvider(provider) => {
                self.handle_context_menu_artist_radio(provider);
                Task::none()
            }
            Message::ContextMenuHover(focus) => {
                if let Some(menu) = &mut self.context_menu {
                    menu.hovered = focus;
                }
                Task::none()
            }
            Message::ContextMenuBoundsCaptured { panel, row_offsets } => {
                let prev = self.bounds.context_menu.take();
                let width_changed = prev
                    .as_ref()
                    .is_none_or(|p| (p.panel.width - panel.width).abs() > f32::EPSILON);
                // Recompute the flip from the original cursor point using
                // the latest measurement. A panel flush with the window edge
                // means its measurement was clipped by the remaining space,
                // so that counts as overflow too. Flipped menus keep their
                // bottom/right edge at the cursor.
                let edge_epsilon = 1.0;
                let moved = if let Some(menu) = &mut self.context_menu {
                    let (cx, cy) = menu.cursor;
                    let nx = if cx + panel.width > self.window_size.width - edge_epsilon {
                        (cx - panel.width).max(0.0)
                    } else {
                        cx
                    };
                    let ny = if cy + panel.height > self.window_size.height - edge_epsilon {
                        (cy - panel.height).max(0.0)
                    } else {
                        cy
                    };
                    let moved = (nx, ny) != menu.position;
                    menu.position = (nx, ny);
                    moved
                } else {
                    false
                };
                let stable = !moved && !width_changed;
                self.bounds.context_menu = Some(ContextMenuGeometry {
                    panel,
                    row_offsets,
                    stable,
                });
                // Re-measure after a flip or a clipped-width correction; the
                // captures converge once position and width stop changing.
                if stable {
                    Task::none()
                } else {
                    iced_runtime::task::widget(update::operation::CaptureContextMenu::default())
                }
            }
            Message::ContextMenuDefault(action) => {
                let Some(menu) = self.context_menu.as_ref() else {
                    return Task::none();
                };
                let provider = menu.default_provider(action, self.config.default_provider);
                match action {
                    DefaultCtxAction::Download => {
                        self.download_track_via_provider(provider);
                    }
                    DefaultCtxAction::SongRadio => self.handle_context_menu_song_radio(provider),
                    DefaultCtxAction::ArtistRadio => {
                        self.handle_context_menu_artist_radio(provider);
                    }
                }
                Task::none()
            }
            Message::ContextMenuRemoveFromPlaylist(indices) => {
                self.close_context_menu();
                self.handle_remove_from_playlist_batch(&indices);
                Task::none()
            }
            Message::ContextMenuRemoveFromQueue(indices) => {
                self.close_context_menu();
                self.handle_remove_from_queue_batch(&indices);
                Task::none()
            }
            Message::ContextMenuEditTrack => {
                let pos = match self.context_menu.as_ref() {
                    Some(menu) => menu.pos,
                    None => return Task::none(),
                };
                self.close_context_menu();
                self.open_edit_track(pos);
                Task::none()
            }
            Message::EditTrackField(field, value) => {
                if let Some(edit) = &mut self.edit_track {
                    match field {
                        EditTrackField::Title => edit.title = value,
                        EditTrackField::Artist => edit.artist = value,
                    }
                }
                Task::none()
            }
            Message::EditTrackSelectProvider(provider) => {
                if let Some(edit) = &mut self.edit_track {
                    edit.source = provider;
                }
                Task::none()
            }
            Message::EditTrackFindProvider(provider) => {
                self.handle_edit_track_find_provider(provider);
                Task::none()
            }
            Message::CloseEditTrack => {
                self.edit_track = None;
                Task::none()
            }
            Message::SaveEditTrack => {
                self.apply_edit_track();
                Task::none()
            }
            Message::CloseContextMenu => {
                self.close_context_menu();
                Task::none()
            }
        }
    }
}
