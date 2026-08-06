use std::sync::mpsc;
use std::time::{Duration, Instant};

use iced::{Point, Subscription, Task};
use tracing::{error, warn};

use crate::audio::AudioPlayer;
use crate::cache::StreamCache;
use crate::config;
use crate::downloads::DownloadRegistry;
use crate::mpris::{self, MprisCommand, MprisUpdate};
use crate::playlists::PlaylistStore;
use crate::search_history::SearchHistory;
use crate::theme::{AppTheme, Palette};
use crate::types::{PlayQueue, QueueTab, Track, View};
use crate::util::format_duration;
use serde::{Deserialize, Serialize};

mod ui;
mod update;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NavEntry {
    pub view: View,
    pub snapshot: ViewSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewSnapshot {
    Search {
        query: String,
        results: Vec<Track>,
        selection: Vec<usize>,
        scroll: f32,
    },
    Radio {
        label: String,
        tracks: Vec<Track>,
        selection: Vec<usize>,
        scroll: f32,
    },
    TrackList {
        playlist: Option<usize>,
        playlist_name: String,
        selection: Vec<usize>,
        scroll: f32,
    },
}

impl Default for ViewSnapshot {
    fn default() -> Self {
        Self::Search {
            query: String::new(),
            results: Vec::new(),
            selection: Vec::new(),
            scroll: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum BackendResult {
    SearchResults(Vec<Track>),
    SearchResultsAppend(Vec<Track>),
    RadioResults(String, Vec<Track>),
    DownloadComplete(Track, String),
    DownloadError(String),
    SearchError(String),
    ThumbnailsDownloaded,
}

#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ContextMenuState {
    pub visible: bool,
    pub track_index: usize,
    pub position: (f32, f32),
    pub is_youtube: bool,
    pub is_downloaded: bool,
    pub in_playlist: bool,
    pub is_queue: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Noop,
    Tick,
    WindowClose,
    WindowResized(iced::Size),
    CursorMoved(Point),
    LeftButtonReleased,
    ListScrolled {
        offset_y: f32,
        bounds: iced::Rectangle,
        is_queue: bool,
    },
    SidebarListScrolled {
        offset_y: f32,
        bounds: iced::Rectangle,
    },
    KeyPressed {
        key: iced::keyboard::key::Key,
        modifiers: iced::keyboard::Modifiers,
    },

    SearchInputChanged(String),
    SearchExecute,
    SearchLoadMore,
    SearchHistorySelected(usize),
    DeleteSearchHistory(usize),

    TrackPressed {
        index: usize,
        is_queue: bool,
    },
    TrackHoverStart {
        index: usize,
        is_queue: bool,
    },
    TrackRightClicked {
        index: usize,
        is_queue: bool,
    },
    PlayTrackAtIndex {
        index: usize,
        is_queue: bool,
    },
    TogglePlayPause,
    NextTrack,
    PreviousTrack,
    SetVolume(f32),
    Seek(f32),

    CreatePlaylist,
    NewPlaylistNameChanged(String),
    SelectPlaylist(usize),
    RenamePlaylist(String),
    AddLocalMusic,
    AddToPlaylist(usize),
    TogglePicker(usize),
    ClosePicker,
    ShowDeleteConfirm(usize),
    ConfirmDeletePlaylist,
    HideDeleteConfirm,

    ToggleQueue,
    SwitchQueueTab(QueueTab),
    PlayRecentTrack(usize),

    NavigateTo(View),
    NavigateBack,
    NavigateForward,

    ContextMenuPlayTrack(usize),
    ContextMenuStartSongRadio(usize),
    ContextMenuStartArtistRadio(usize),
    ContextMenuDownloadOrDelete(usize),
    ContextMenuRemoveFromPlaylist(usize),
    ContextMenuRemoveFromQueue(usize),
    CloseContextMenu,
}

/// Mouse and drag interaction state, grouped for clarity.
#[derive(Debug, Clone, Default)]
pub struct DragState {
    pub cursor_pos: Point,
    pub pressed_track: Option<usize>,
    pub pressed_track_is_queue: bool,
    pub hovered_track: Option<(usize, bool)>,
    pub drag_origin: Option<Point>,
    pub drag_active: bool,
    pub drag_drop_target: Option<usize>,
    /// Which list the cursor is currently hovering over during a drag.
    /// `None` means no list is targeted (e.g. hovering over the sidebar).
    /// `Some(DragTargetList::Queue)` when over the queue's up-next list.
    /// `Some(DragTargetList::TrackList)` when over the main track list.
    pub drag_target_list: Option<DragTargetList>,
    pub sidebar_hover_playlist: Option<usize>,
}

impl DragState {
    const fn cleanup(&mut self) {
        self.drag_active = false;
        self.drag_origin = None;
        self.pressed_track = None;
        self.drag_drop_target = None;
        self.drag_target_list = None;
        self.sidebar_hover_playlist = None;
    }
}

/// Identifies which track list a drag is currently hovering over.
/// Used to distinguish same-list reordering from cross-list copying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragTargetList {
    TrackList,
    Queue,
}

#[allow(clippy::struct_excessive_bools)]
pub struct MusicPlayer {
    pub audio: AudioPlayer,
    pub config: crate::config::Config,
    pub current_view: View,
    pub search_query: String,
    pub search_results: Vec<Track>,
    pub search_loading: bool,
    pub search_exhausted: bool,
    pub show_search_history: bool,
    pub last_filtered_history: Vec<String>,

    pub radio_tracks: Vec<Track>,
    pub radio_label: String,

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
    pub downloading_index: Option<usize>,

    pub notification: Option<String>,

    pub thumbnail_cache: std::collections::HashMap<String, bool>,
    pub downloaded_tracks: Vec<Track>,
    pub playlists: PlaylistStore,
    pub selected_playlist: Option<usize>,
    pub selected_playlist_name: String,
    pub playlist_create_name: String,
    pub show_playlist_picker: Option<usize>,
    pub picker_focused_index: usize,
    pub show_delete_confirm: bool,
    pub delete_confirm_index: Option<usize>,

    pub nav_history: Vec<NavEntry>,
    pub nav_history_pos: usize,

    pub search_history: SearchHistory,
    pub stream_cache: StreamCache,
    pub pending_cache_id: Option<String>,

    pub selected_indices: Vec<usize>,
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

    pub drag: DragState,

    pub context_menu: Option<ContextMenuState>,

    pub queue_selected_indices: Vec<usize>,

    pub app_theme: AppTheme,

    pub search_list_bounds: Option<iced::Rectangle>,
    pub search_list_scroll: f32,
    pub playlist_list_bounds: Option<iced::Rectangle>,
    pub playlist_list_scroll: f32,
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

    fn new_with(config: crate::config::Config) -> Self {
        let (result_tx, result_rx) = mpsc::channel();
        let (mpris_cmd_tx, mpris_cmd_rx) = mpsc::channel();

        let mut player = Self {
            audio: AudioPlayer::new(config.volume),
            search_history: SearchHistory::load(),
            stream_cache: StreamCache::new(config.cache_max_size_mb),
            pending_cache_id: None,
            config,
            current_view: View::Search,
            search_query: String::new(),
            search_results: Vec::new(),
            search_loading: false,
            search_exhausted: false,
            show_search_history: false,
            last_filtered_history: Vec::new(),
            radio_tracks: Vec::new(),
            radio_label: String::new(),
            queue: PlayQueue::new(),
            is_playing: false,
            volume: 0.8,
            progress: 0.0,
            duration: 0.0,
            download_registry: DownloadRegistry::load(),
            downloading_index: None,
            notification: None,
            track_loading: false,
            playlists: PlaylistStore::load(),
            selected_playlist: None,
            selected_playlist_name: String::new(),
            playlist_create_name: String::new(),
            show_playlist_picker: None,
            show_queue: false,
            thumbnail_cache: std::collections::HashMap::new(),
            downloaded_tracks: Vec::new(),
            picker_focused_index: 0,
            show_delete_confirm: false,
            delete_confirm_index: None,
            nav_history: vec![NavEntry {
                view: View::Search,
                snapshot: ViewSnapshot::Search {
                    query: String::new(),
                    results: Vec::new(),
                    selection: Vec::new(),
                    scroll: 0.0,
                },
            }],
            nav_history_pos: 0,
            result_tx,
            result_rx,
            mpris_cmd_tx,
            mpris_cmd_rx,
            mpris_update_tx: None,
            mpris_dirty: true,
            session_dirty: true,
            drag: DragState::default(),
            context_menu: None,
            queue_selected_indices: Vec::new(),
            app_theme: AppTheme::new(Palette::dark()),
            search_list_bounds: None,
            search_list_scroll: 0.0,
            playlist_list_bounds: None,
            playlist_list_scroll: 0.0,
            queue_list_bounds: None,
            queue_list_scroll: 0.0,
            sidebar_bounds: None,
            sidebar_list_scroll: 0.0,
            window_width: 1280.0,
            elapsed_text: String::new(),
            total_text: String::new(),
            selected_indices: Vec::new(),
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
                } else if self.current_view.is_search_like() {
                    self.search_list_bounds = Some(bounds);
                    self.search_list_scroll = offset_y;
                } else {
                    self.playlist_list_bounds = Some(bounds);
                    self.playlist_list_scroll = offset_y;
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
                self.handle_add_to_playlist(playlist_idx);
                Task::none()
            }
            Message::TogglePicker(index) => {
                self.handle_toggle_picker(index);
                Task::none()
            }
            Message::ClosePicker => {
                self.show_playlist_picker = None;
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
            Message::NavigateTo(view) => {
                self.handle_navigate_to(view);
                Task::none()
            }
            Message::NavigateBack => self.handle_navigate_back(),
            Message::NavigateForward => self.handle_navigate_forward(),
            Message::ContextMenuPlayTrack(index) => {
                let menu = self.take_context_menu();
                let is_queue = menu.as_ref().is_some_and(|m| m.is_queue);
                self.drag.pressed_track = None;
                self.handle_play_track(index, is_queue);
                Task::none()
            }
            Message::ContextMenuStartSongRadio(index) => {
                let menu = self.take_context_menu();
                let is_queue = menu.as_ref().is_some_and(|m| m.is_queue);
                let track = self.get_track_at(index, is_queue);
                if let Some(t) = track {
                    self.start_song_radio(t.title);
                }
                Task::none()
            }
            Message::ContextMenuStartArtistRadio(index) => {
                let menu = self.take_context_menu();
                let is_queue = menu.as_ref().is_some_and(|m| m.is_queue);
                let track = self.get_track_at(index, is_queue);
                if let Some(t) = track {
                    self.start_artist_radio(t.artist);
                }
                Task::none()
            }
            Message::ContextMenuDownloadOrDelete(index) => {
                let menu = self.take_context_menu();
                let is_queue = menu.as_ref().is_some_and(|m| m.is_queue);
                let track = self.get_track_at(index, is_queue);
                self.drag.pressed_track = None;
                if let Some(track) = track {
                    if self.download_registry.contains(&track.url) {
                        self.handle_remove_download(index, is_queue);
                    } else {
                        self.handle_download_track(index, is_queue);
                    }
                }
                Task::none()
            }
            Message::ContextMenuRemoveFromPlaylist(index) => {
                self.take_context_menu();
                self.handle_remove_from_playlist(index);
                Task::none()
            }
            Message::ContextMenuRemoveFromQueue(index) => {
                self.take_context_menu();
                self.handle_remove_from_queue(index);
                Task::none()
            }
            Message::CloseContextMenu => {
                self.context_menu = None;
                Task::none()
            }
        }
    }
}
