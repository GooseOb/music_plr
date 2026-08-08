use crate::{
    audio::AudioPlayer,
    data::{
        cache::StreamCache, config, downloads::DownloadRegistry, playlists::PlaylistStore,
        search_history::SearchHistory, JsonStore,
    },
    mpris::{self, MprisCommand, MprisUpdate},
    theme::{AppTheme, Palette},
    types::{PlayQueue, Track},
    util::format_duration,
};
use iced::{Subscription, Task};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
use tracing::{error, warn};

mod interaction;
mod message;
mod ui;
mod update;
mod view_data;

pub use interaction::{ContextMenuState, DragState, DragTargetList};
pub use message::{BackendResult, Message};
pub use view_data::{ViewData, ViewKind};

#[allow(clippy::struct_excessive_bools)]
pub struct MusicPlayer {
    pub audio: AudioPlayer,
    pub config: crate::data::config::Config,
    /// Back/forward navigation history. Each entry is a full `View` snapshot;
    /// `nav_history_pos` indexes the active one, which is the single source of
    /// truth for which view is active and its data (see [`Self::view_data`]).
    pub nav_history: Vec<ViewData>,
    pub nav_history_pos: usize,
    /// Source of monotonic request ids stamped onto the view slot that issues
    /// an in-flight search/radio/browse, so results can be correlated back to
    /// it regardless of which view is active when they arrive.
    pub next_request_id: u64,
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

    pub is_playing: bool,
    pub volume: f32,
    pub progress: f32,
    pub duration: f32,
    pub track_loading: bool,
    pub elapsed_text: String,
    pub total_text: String,

    pub download_registry: DownloadRegistry,

    pub notification: Option<String>,

    pub thumbnail_cache: std::collections::HashSet<String>,
    pub playlists: PlaylistStore,
    pub playlist_create_name: String,
    pub show_playlist_picker: bool,
    /// Whether the playlist picker was triggered from a queue track (so
    /// `picker_target_indices` refer to queue positions, not track-list
    /// positions).
    pub picker_is_queue: bool,
    /// Indices resolved from the context menu / picker trigger: if the
    /// right-clicked track was part of the selection, this holds all
    /// selected indices; otherwise just that one track. Used by
    /// `AddToPlaylist` to apply to the right set of tracks.
    pub picker_target_indices: Vec<usize>,
    pub show_delete_confirm: bool,
    pub delete_confirm_index: Option<usize>,

    pub search_history: SearchHistory,
    pub stream_cache: StreamCache,
    pub pending_cache_id: Option<String>,

    pub clipboard: Vec<Track>,
    pub last_click_index: Option<usize>,
    pub last_click_time: Instant,

    pub result_tx: mpsc::Sender<BackendResult>,
    pub result_rx: mpsc::Receiver<BackendResult>,
    pub mpris_cmd_tx: mpsc::Sender<MprisCommand>,
    pub mpris_cmd_rx: mpsc::Receiver<MprisCommand>,
    pub mpris_update_tx: Option<mpsc::Sender<MprisUpdate>>,
    pub mpris_dirty: bool,
    pub session_dirty: bool,
    /// Timestamp of the last `session.json` write, used to throttle flushing
    /// (see `flush_session`).
    pub last_session_flush: std::time::Instant,

    pub drag: DragState,

    pub context_menu: Option<ContextMenuState>,

    pub queue_selected_indices: Vec<usize>,

    pub app_theme: AppTheme,

    pub queue_list_bounds: Option<iced::Rectangle>,
    pub queue_list_scroll: f32,
    pub sidebar_bounds: Option<iced::Rectangle>,
    pub sidebar_list_scroll: f32,
    pub window_width: f32,
}

impl Default for MusicPlayer {
    fn default() -> Self {
        let config = config::load_config();
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
            audio: AudioPlayer::new(config.volume),
            search_history: SearchHistory::load(),
            stream_cache: StreamCache::new(config.cache_max_size_mb),
            pending_cache_id: None,
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
            show_playlist_picker: false,
            show_queue: false,
            thumbnail_cache: std::collections::HashSet::new(),
            picker_is_queue: false,
            picker_target_indices: Vec::new(),
            show_delete_confirm: false,
            delete_confirm_index: None,
            nav_history: vec![ViewData::default()],
            nav_history_pos: 0,
            next_request_id: 1,
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
            queue_list_bounds: None,
            queue_list_scroll: 0.0,
            sidebar_bounds: None,
            sidebar_list_scroll: 0.0,
            window_width: 1280.0,
            elapsed_text: String::new(),
            total_text: String::new(),
            clipboard: Vec::new(),
            last_click_index: None,
            last_click_time: std::time::Instant::now(),
        };

        player.init_mpris();
        player.restore_session();
        player.resume_playback();
        player.update_progress_text();
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
                if self.drag.pressed_track.is_some()
                    && self.drag.drag_origin.is_some()
                    && !self.drag.drag_active
                {
                    if let Some(origin) = self.drag.drag_origin {
                        let dx = (pos.x - origin.x).abs();
                        let dy = (pos.y - origin.y).abs();
                        if dx > crate::theme::DRAG_THRESHOLD || dy > crate::theme::DRAG_THRESHOLD {
                            self.drag.drag_active = true;
                        }
                    }
                }
                if self.drag.drag_active {
                    return self.handle_drag_update();
                }
                Task::none()
            }
            Message::LeftButtonReleased => {
                self.handle_left_release();
                Task::none()
            }
            Message::ListScrolled {
                offset_y,
                bounds,
                is_queue,
            } => {
                if is_queue {
                    self.queue_list_bounds = Some(bounds);
                    self.queue_list_scroll = offset_y;
                } else {
                    self.view_data_mut().scroll = offset_y;
                    self.view_data_mut().bounds = Some(bounds);
                }
                Task::none()
            }
            Message::SidebarListScrolled { offset_y, bounds } => {
                self.sidebar_bounds = Some(bounds);
                self.sidebar_list_scroll = offset_y;
                Task::none()
            }
            Message::KeyPressed { key, modifiers } => self.handle_key_press(&key, modifiers),
            Message::SearchInputChanged(query) => {
                self.search_query = query;
                self.update_search_history();
                self.show_search_history = true;
                Task::none()
            }
            Message::SearchExecute => {
                self.handle_search_execute();
                Task::none()
            }
            Message::SearchScopeChanged(scope) => {
                if scope != self.search_scope {
                    self.search_scope = scope;
                    // Re-run the current query under the new scope (only if
                    // there is one; otherwise just remember the preference).
                    if !self.search_query.is_empty() {
                        self.run_search(self.search_query.clone(), scope);
                    }
                }
                Task::none()
            }
            Message::OpenArtist(browse_id, title) => {
                self.handle_open_artist(browse_id, title);
                Task::none()
            }
            Message::OpenAlbum(browse_id, title) => {
                self.handle_open_album(browse_id, title);
                Task::none()
            }
            Message::OpenPlaylist(playlist_id, title) => {
                self.handle_open_playlist(playlist_id, title);
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
            Message::TrackPressed { index, is_queue } => {
                self.handle_track_pressed(index, is_queue);
                Task::none()
            }
            Message::TrackHoverStart { index, is_queue } => {
                self.drag.hovered_track = Some((index, is_queue));
                Task::none()
            }
            Message::TrackRightClicked { index, is_queue } => {
                if !is_queue {
                    self.drag.hovered_track = None;
                }
                self.show_context_menu(index, is_queue);
                Task::none()
            }
            Message::PlayTrackAtIndex { index, is_queue } => {
                if !is_queue {
                    self.drag.hovered_track = None;
                }
                self.handle_play_track(index, is_queue);
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
                self.handle_select_playlist(index);
                Task::none()
            }
            Message::RenamePlaylist(name) => {
                self.handle_rename_playlist(name);
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
                    let paths: Vec<String> = files
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();
                    self.handle_add_local_music(paths);
                }
                Task::none()
            }
            Message::AddToPlaylist(playlist_idx) => {
                let indices = std::mem::take(&mut self.picker_target_indices);
                let is_queue = self.picker_is_queue;
                self.handle_add_to_playlist(playlist_idx, &indices, is_queue);
                Task::none()
            }
            Message::TogglePicker(indices) => {
                let is_queue = self.context_menu.as_ref().is_some_and(|m| m.is_queue);
                self.handle_toggle_picker(indices, is_queue);
                Task::none()
            }
            Message::ClosePicker => {
                self.show_playlist_picker = false;
                self.picker_target_indices.clear();
                Task::none()
            }
            Message::ShowDeleteConfirm(index) => {
                self.delete_confirm_index = Some(index);
                self.show_delete_confirm = true;
                Task::none()
            }
            Message::ConfirmDeletePlaylist => {
                if let Some(idx) = self.delete_confirm_index {
                    self.handle_delete_playlist(idx);
                }
                self.show_delete_confirm = false;
                self.delete_confirm_index = None;
                Task::none()
            }
            Message::HideDeleteConfirm => {
                self.show_delete_confirm = false;
                self.delete_confirm_index = None;
                Task::none()
            }
            Message::ToggleQueue => {
                self.show_queue = !self.show_queue;
                self.save_session();
                Task::none()
            }
            Message::SwitchQueueTab(tab) => {
                self.queue.queue_tab = tab;
                self.drag.hovered_track = None;
                self.save_session();
                Task::none()
            }
            Message::PlayRecentTrack(index) => {
                if index < self.queue.recently_played.len() {
                    if let Some(track) = self.queue.recently_played.get(index) {
                        let track = track.clone();
                        self.play_recent_track(track);
                    }
                }
                Task::none()
            }
            Message::NavigateTo(data) => {
                self.handle_navigate_to(data);
                Task::none()
            }
            Message::NavigateBack => self.handle_navigate_back(),
            Message::NavigateForward => self.handle_navigate_forward(),
            Message::ContextMenuPlayTrack(index) => {
                let is_queue = self.take_context_menu_is_queue();
                self.drag.pressed_track = None;
                self.handle_play_track(index, is_queue);
                Task::none()
            }
            Message::ContextMenuStartSongRadio(index) => {
                let is_queue = self.take_context_menu_is_queue();
                if let Some(t) = self.get_track_at(index, is_queue) {
                    self.start_song_radio(t.title);
                }
                Task::none()
            }
            Message::ContextMenuStartArtistRadio(index) => {
                let is_queue = self.take_context_menu_is_queue();
                if let Some(t) = self.get_track_at(index, is_queue) {
                    self.start_artist_radio(t.artist);
                }
                Task::none()
            }
            Message::ContextMenuDownloadOrDelete(indices) => {
                let is_queue = self.take_context_menu_is_queue();
                self.drag.pressed_track = None;
                self.handle_download_or_remove_tracks(&indices, is_queue);
                Task::none()
            }
            Message::ContextMenuRemoveFromPlaylist(indices) => {
                self.take_context_menu_is_queue();
                self.handle_remove_from_playlist_batch(&indices);
                Task::none()
            }
            Message::ContextMenuRemoveFromQueue(indices) => {
                self.take_context_menu_is_queue();
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
