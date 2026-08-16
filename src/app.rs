use crate::{
    audio::AudioPlayer,
    data::{
        cache::StreamCache, config, downloads::DownloadRegistry, playlists::PlaylistStore,
        search_history::SearchHistory, JsonStore,
    },
    mpris::{self, MprisCommand, MprisUpdate},
    theme::{AppTheme, Palette},
    types::{PlayQueue, Track},
};
use iced::{Subscription, Task};
use std::{sync::mpsc, time::Duration};
use tracing::{error, warn};

mod interaction;
mod message;
mod ui;
mod update;
mod view_data;

pub use interaction::{ContextMenuState, DragState, TrackListKind, TrackPos};
pub use message::{BackendResult, Message};
pub use view_data::{RequestIdGenerator, ViewData, ViewKind};

#[derive(Debug, Clone)]
pub struct LyricsState {
    pub lyrics: Option<crate::lyrics::Lyrics>,
    pub track_id: Option<String>,
    pub loading: bool,
    pub select_mode: bool,
    pub editor: Option<iced::widget::text_editor::Content>,
}

impl LyricsState {
    fn new() -> Self {
        Self {
            lyrics: None,
            track_id: None,
            loading: false,
            select_mode: false,
            editor: Some(iced::widget::text_editor::Content::with_text("")),
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
pub struct MusicPlayer {
    pub audio: AudioPlayer,
    pub config: crate::data::config::Config,
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
    pub search_scope: crate::youtube::SearchScope,
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

    pub thumbnail_index: crate::data::thumbnails::ThumbnailIndex,
    pub playlists: PlaylistStore,
    pub playlist_create_name: String,
    pub playlist_picker: Option<PlaylistPicker>,
    pub delete_confirm_index: Option<usize>,

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

    pub queue_selected_indices: Vec<usize>,

    pub app_theme: AppTheme,

    pub bounds: crate::app::update::operation::CaptureBounds,
    pub window_width: f32,
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
        (Self::default(), Task::none())
    }

    fn new_with(config: crate::data::config::Config) -> Self {
        let (result_tx, result_rx) = mpsc::channel();
        let (mpris_cmd_tx, mpris_cmd_rx) = mpsc::channel();

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
            search_scope: crate::youtube::SearchScope::Songs,
            show_search_history: false,
            last_filtered_history: Vec::new(),
            queue: PlayQueue::new(),
            is_playing: false,
            volume: 0.8,
            progress: 0.0,
            duration: 0.0,
            download_registry: DownloadRegistry::load(),
            notification: None,
            track_loading: false,
            playlists: PlaylistStore::load(),
            playlist_create_name: String::new(),
            show_queue: false,
            repeat: false,
            thumbnail_index: crate::data::thumbnails::ThumbnailIndex::load(),
            playlist_picker: None,
            delete_confirm_index: None,
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
            queue_selected_indices: Vec::new(),
            app_theme: AppTheme::new(Palette::dark()),
            bounds: crate::app::update::operation::CaptureBounds::default(),
            window_width: 1280.0,
            clipboard: Vec::new(),
            last_click: None,
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
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                if status == iced::event::Status::Captured {
                    return None;
                }
                Some(Message::KeyPressed { key, modifiers })
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
            Message::Noop => Task::none(),
            Message::Tick => {
                self.handle_tick();
                Task::none()
            }
            Message::WindowResized(size) => {
                self.window_width = size.width;
                Task::none()
            }
            Message::WindowClose => {
                self.flush_session();
                Task::none()
            }
            Message::CursorMoved(pos) => {
                self.drag.cursor_pos = pos;
                if self.drag.pressed.is_some()
                    && self.drag.drag_origin.is_some()
                    && !self.drag.drag_active
                {
                    if let Some(origin) = self.drag.drag_origin {
                        let dx = (pos.x - origin.x).abs();
                        let dy = (pos.y - origin.y).abs();
                        if dx > crate::theme::DRAG_THRESHOLD || dy > crate::theme::DRAG_THRESHOLD {
                            self.drag.drag_active = true;
                            // Reveal the library so it can receive drops.
                            if self.drag.is_pressed_card() {
                                self.library_expanded = true;
                            }
                        }
                    }
                }
                // Refresh scrollable geometry every cursor move so drag,
                // drop hit-testing, and scroll-into-view read live bounds
                // without depending on `on_scroll` (which never fires when a
                // list doesn't overflow).
                let capture = iced_runtime::task::widget(update::operation::CaptureBounds::new());
                if self.drag.drag_active {
                    iced::Task::batch([capture, self.handle_drag_update()])
                } else {
                    capture
                }
            }
            Message::LeftButtonReleased => {
                self.handle_left_release();
                Task::none()
            }
            Message::ListBoundsCaptured(bounds) => {
                let scroll = bounds.track.as_ref().map_or(0.0, |b| b.translation_y);
                self.bounds = bounds;
                self.view_data_mut().scroll = scroll;

                Task::none()
            }
            Message::KeyPressed { key, modifiers } => self.handle_key_press(&key, modifiers),
            Message::LyricsEditorAction(action) => {
                if let Some(state) = &mut self.lyrics {
                    if let Some(editor) = &mut state.editor {
                        if !matches!(action, iced::widget::text_editor::Action::Edit(_)) {
                            editor.perform(action);
                        }
                    }
                }
                Task::none()
            }
            Message::SearchInputChanged(query) => {
                self.search_query = query;
                self.update_search_history();
                self.show_search_history = true;
                Task::none()
            }
            Message::SearchExecute => {
                self.run_search();
                Task::none()
            }
            Message::SearchScopeChanged(scope) => {
                if scope != self.search_scope {
                    self.search_scope = scope;
                    self.run_search();
                }
                Task::none()
            }
            Message::OpenAlbum(browse_id, title) => {
                self.handle_open_album(browse_id, &title);
                Task::none()
            }
            Message::ToggleLibrarySave(item) => {
                let saved = self.toggle_library_save(item);
                self.notify(if saved {
                    "Saved to library"
                } else {
                    "Removed from library"
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
                self.drag.hovered = Some(target);
                Task::none()
            }
            Message::TrackRightClicked(pos) => {
                self.show_context_menu(pos);
                Task::none()
            }
            Message::PlayTrackAt(pos) => {
                if pos.list == TrackListKind::Active {
                    self.drag.clear_hovered_track();
                }
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
            Message::SelectPlaylist(index) => {
                self.lyrics = None;
                self.handle_select_playlist(index);
                Task::none()
            }
            Message::RenamePlaylist(name) => {
                self.handle_rename_playlist(&name);
                Task::none()
            }
            Message::AddLocalMusic => {
                let files = rfd::FileDialog::new()
                    .add_filter(
                        "Audio",
                        &["mp3", "flac", "wav", "ogg", "m4a", "aac", "opus", "wma"],
                    )
                    .pick_files();
                if let Some(files) = files {
                    self.handle_add_local_music(&files);
                }
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
            Message::ToggleLyricsSelectMode => {
                if let Some(state) = &mut self.lyrics {
                    state.select_mode = !state.select_mode;
                }
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
                self.handle_settings_download_dir(&dir);
                Task::none()
            }
            Message::SettingsMaxHistoryVisibleChanged(v) => {
                self.handle_settings_max_history_visible(&v);
                Task::none()
            }
            Message::SettingsMaxHistoryStoredChanged(v) => {
                self.handle_settings_max_history_stored(&v);
                Task::none()
            }
            Message::SettingsCacheMaxSizeChanged(v) => {
                self.handle_settings_cache_max_size(&v);
                Task::none()
            }
            Message::SettingsMaxRecentlyPlayedChanged(v) => {
                self.handle_settings_max_recently_played(&v);
                Task::none()
            }
            Message::SettingsVolumeNormalizationToggled(enabled) => {
                self.handle_settings_volume_normalization(enabled);
                Task::none()
            }
            Message::SettingsResetDefaults => {
                self.handle_settings_reset_defaults();
                Task::none()
            }
            Message::ContextMenuPlayTrack(pos) => {
                self.context_menu = None;
                self.handle_play_track(pos);
                Task::none()
            }
            Message::ContextMenuStartSongRadio => {
                if let Some(track) = self.context_menu.take().map(|m| m.track) {
                    self.start_song_radio(track.title);
                }
                Task::none()
            }
            Message::ContextMenuStartArtistRadio => {
                if let Some(track) = self.context_menu.take().map(|m| m.track) {
                    self.start_artist_radio(track.artist);
                }
                Task::none()
            }
            Message::ContextMenuDownloadOrDelete(indices) => {
                let list = self.context_menu.as_ref().map(|m| m.pos.list);
                self.drag.pressed = None;
                // Resolved per-index inside the handler (it reports per-track
                // download state), so only the list kind is needed here.
                if let Some(list) = list {
                    self.handle_download_or_remove_tracks(&indices, list);
                }
                Task::none()
            }
            Message::ContextMenuRemoveFromPlaylist(indices) => {
                self.context_menu = None;
                self.handle_remove_from_playlist_batch(&indices);
                Task::none()
            }
            Message::ContextMenuRemoveFromQueue(indices) => {
                self.context_menu = None;
                self.handle_remove_from_queue_batch(&indices);
                Task::none()
            }
            Message::CloseContextMenu => {
                self.context_menu = None;
                Task::none()
            }
        }
    }
}
