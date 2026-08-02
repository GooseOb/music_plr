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
use crate::session::SessionState;
use crate::theme::Palette;
use crate::types::{PlayQueue, Track, View};
use crate::util::format_duration;

mod ui;
mod update;

#[derive(Debug, Clone, Default)]
pub struct NavEntry {
    pub view: View,
    pub selected_playlist: Option<usize>,
    pub playlist_name: String,
    pub search_query: String,
    pub radio_label: String,
    pub search_results: Vec<Track>,
    pub radio_tracks: Vec<Track>,
}

#[derive(Debug, Clone)]
pub enum BackendResult {
    SearchResults(Vec<Track>),
    SearchResultsAppend(Vec<Track>),
    RadioResults(String, Vec<Track>),
    DownloadComplete(String, String),
    DownloadError(String),
    SearchError(String),
    ThumbnailsReady,
}

#[derive(Debug, Clone, Default)]
pub struct ContextMenuState {
    pub visible: bool,
    pub track_index: usize,
    pub position: (f32, f32),
    pub is_youtube: bool,
    pub is_downloaded: bool,
    pub in_playlist: bool,
    pub in_queue: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    WindowClose,
    CursorMoved(Point),
    LeftButtonReleased,
    KeyPressed {
        key: iced::keyboard::key::Key,
        modifiers: iced::keyboard::Modifiers,
    },

    SearchInputChanged(String),
    SearchExecute,
    SearchLoadMore,
    ShowSearchHistory(bool),
    SearchHistorySelected(usize),
    DeleteSearchHistory(usize),

    TrackPressed(usize),
    TrackHoverStart(usize),
    TrackHoverEnd,
    TrackRightClicked(usize),
    GlobalSearchSubmit,
    PlayTrackAtIndex(usize),
    PlayTrack(usize),
    PlayQueueTrack(usize),
    TogglePlayPause,
    NextTrack,
    PreviousTrack,
    SetVolume(f32),
    Seek(f32),

    StartSongRadio(String),
    StartArtistRadio(String),

    CreatePlaylist,
    NewPlaylistNameChanged(String),
    DeletePlaylist(usize),
    SelectPlaylist(usize),
    RenamePlaylist(String),
    AddLocalMusic,
    AddToPlaylist(usize),
    RemoveFromPlaylist(usize),
    ReorderTracks {
        from: usize,
        to: usize,
    },
    TogglePicker(usize),
    ClosePicker,
    ShowDeleteConfirm(usize),
    ConfirmDeletePlaylist,
    HideDeleteConfirm,

    ToggleQueue,
    ReorderQueue {
        from: usize,
        to: usize,
    },
    RemoveFromQueue(usize),

    ToggleSelect(usize),
    CopySelected,
    DeleteSelected,
    PasteClipboard,
    ClearSelection,

    NavigateSearch,
    NavigateTo(View),
    NavigateBack,
    NavigateForward,
    ResumePlayback,

    ContextMenuPlayTrack(usize),
    ContextMenuStartSongRadio(usize),
    ContextMenuStartArtistRadio(usize),
    ContextMenuDownloadOrDelete(usize),
    ContextMenuRemoveFromPlaylist(usize),
    ContextMenuRemoveFromQueue(usize),
    CloseContextMenu,

    SearchResults(Vec<Track>),
    SearchResultsAppend(Vec<Track>),
    SearchError(String),
    DownloadComplete(String, String),
    DownloadError(String),
    ThumbnailsReady,

    Notify(String),
    ClearNotification,

    SearchScroll(f32),
    PlaylistScroll(f32),
    SidebarScroll(f32),
}

pub struct MusicPlayer {
    pub audio: AudioPlayer,
    pub config: crate::config::Config,
    pub current_view: View,
    pub search_query: String,
    pub search_results: Vec<Track>,
    pub search_offset: usize,
    pub search_loading: bool,
    pub show_search_history: bool,
    pub last_filtered_history: Vec<String>,
    pub search_history_focused_index: usize,

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
    pub loading: bool,

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

    pub stream_cache: StreamCache,
    pub pending_cache_id: Option<String>,
    pub thumbnails_pending: bool,

    pub selected_indices: Vec<usize>,
    pub clipboard: Vec<Track>,
    pub last_click_index: Option<usize>,
    pub last_click_time: Instant,

    pub result_tx: mpsc::Sender<BackendResult>,
    pub result_rx: mpsc::Receiver<BackendResult>,
    pub mpris_cmd_tx: mpsc::Sender<MprisCommand>,
    pub mpris_cmd_rx: mpsc::Receiver<MprisCommand>,
    pub mpris_update_tx: Option<mpsc::Sender<MprisUpdate>>,

    pub cursor_pos: Point,
    pub pressed_track: Option<usize>,
    pub hovered_track: Option<usize>,
    pub drag_origin: Option<Point>,
    pub drag_active: bool,

    pub context_menu: Option<ContextMenuState>,

    pub input_focused: bool,
    pub focused_list_index: usize,

    pub palette: Palette,

    pub search_list_bounds: Option<iced::Rectangle>,
    pub search_list_scroll: f32,
    pub playlist_list_bounds: Option<iced::Rectangle>,
    pub playlist_list_scroll: f32,
    pub sidebar_bounds: Option<iced::Rectangle>,
    pub sidebar_list_scroll: f32,
    pub picker_scroll: f32,

    pub window_size: (u32, u32),
}

impl Default for MusicPlayer {
    fn default() -> Self {
        let config = config::load_config();
        Self::new_with(config)
    }
}

impl MusicPlayer {
    pub fn new() -> (MusicPlayer, Task<Message>) {
        (Self::default(), Task::none())
    }

    fn new_with(config: crate::config::Config) -> Self {
        let (result_tx, result_rx) = mpsc::channel();
        let (mpris_cmd_tx, mpris_cmd_rx) = mpsc::channel();

        let mut player = Self {
            audio: AudioPlayer::new(config.volume),
            stream_cache: StreamCache::new(config.cache_max_size_mb),
            pending_cache_id: None,
            thumbnails_pending: false,
            config,
            current_view: View::Search(String::new()),
            search_query: String::new(),
            search_results: Vec::new(),
            search_offset: 0,
            search_loading: false,
            show_search_history: false,
            last_filtered_history: Vec::new(),
            search_history_focused_index: 0,
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
            loading: false,
            track_loading: false,
            playlists: PlaylistStore::load(),
            selected_playlist: None,
            selected_playlist_name: String::new(),
            playlist_create_name: String::new(),
            show_playlist_picker: None,
            show_queue: false,
            picker_focused_index: 0,
            show_delete_confirm: false,
            delete_confirm_index: None,
            nav_history: vec![NavEntry {
                view: View::Search(String::new()),
                selected_playlist: None,
                playlist_name: String::new(),
                search_query: String::new(),
                radio_label: String::new(),
                search_results: Vec::new(),
                radio_tracks: Vec::new(),
            }],
            nav_history_pos: 0,
            result_tx,
            result_rx,
            mpris_cmd_tx,
            mpris_cmd_rx,
            mpris_update_tx: None,
            cursor_pos: Point::new(0.0, 0.0),
            pressed_track: None,
            hovered_track: None,
            drag_origin: None,
            drag_active: false,
            context_menu: None,
            input_focused: false,
            focused_list_index: 0,
            palette: Palette::dark(),
            search_list_bounds: None,
            search_list_scroll: 0.0,
            playlist_list_bounds: None,
            playlist_list_scroll: 0.0,
            sidebar_bounds: None,
            sidebar_list_scroll: 0.0,
            picker_scroll: 0.0,
            window_size: (1200, 700),
            elapsed_text: String::new(),
            total_text: String::new(),
            selected_indices: Vec::new(),
            clipboard: Vec::new(),
            last_click_index: None,
            last_click_time: std::time::Instant::now(),
        };

        player.init_mpris();
        player.restore_session();
        player.update_progress_text();
        player
    }

    pub fn view(&self) -> iced::Element<'_, Message> {
        ui::view(self)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let timer = iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick);

        let events = iced::event::listen_with(MusicPlayer::event_to_message);

        Subscription::batch([timer, events])
    }

    fn event_to_message(
        event: iced::Event,
        _status: iced::event::Status,
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
                Some(Message::KeyPressed { key, modifiers })
            }
            iced::Event::Window(iced::window::Event::CloseRequested) => Some(Message::WindowClose),
            _ => None,
        }
    }

    pub fn boot() -> (MusicPlayer, Task<Message>) {
        (MusicPlayer::default(), Task::none())
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.handle_tick();
                Task::none()
            }
            Message::SearchScroll(delta) => {
                self.search_list_scroll += delta;
                if self.search_list_scroll < 0.0 {
                    self.search_list_scroll = 0.0;
                }
                Task::none()
            }
            Message::PlaylistScroll(delta) => {
                self.playlist_list_scroll += delta;
                if self.playlist_list_scroll < 0.0 {
                    self.playlist_list_scroll = 0.0;
                }
                Task::none()
            }
            Message::SidebarScroll(delta) => {
                self.sidebar_list_scroll += delta;
                if self.sidebar_list_scroll < 0.0 {
                    self.sidebar_list_scroll = 0.0;
                }
                Task::none()
            }
            Message::WindowClose => {
                self.save_session();
                Task::none()
            }
            Message::CursorMoved(pos) => {
                self.cursor_pos = pos;
                if self.pressed_track.is_some() && self.drag_origin.is_some() && !self.drag_active {
                    let origin = self.drag_origin.unwrap();
                    let dx = (pos.x - origin.x).abs();
                    let dy = (pos.y - origin.y).abs();
                    if dx > crate::theme::DRAG_THRESHOLD || dy > crate::theme::DRAG_THRESHOLD {
                        self.drag_active = true;
                    }
                }
                Task::none()
            }
            Message::LeftButtonReleased => {
                self.handle_left_release();
                Task::none()
            }
            Message::KeyPressed { key, modifiers } => {
                self.handle_key_press(&key, modifiers);
                Task::none()
            }
            Message::SearchInputChanged(query) => {
                self.search_query = query;
                self.input_focused = true;
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
            Message::ShowSearchHistory(show) => {
                self.show_search_history = show;
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
            Message::TrackPressed(index) => {
                self.handle_track_pressed(index);
                Task::none()
            }
            Message::TrackHoverStart(index) => {
                self.hovered_track = Some(index);
                Task::none()
            }
            Message::TrackHoverEnd => {
                self.hovered_track = None;
                Task::none()
            }
            Message::TrackRightClicked(index) => {
                self.hovered_track = None;
                self.show_context_menu(index);
                Task::none()
            }
            Message::GlobalSearchSubmit => {
                self.handle_global_search();
                Task::none()
            }
            Message::PlayTrackAtIndex(index) => {
                self.hovered_track = None;
                self.handle_play_track(index);
                Task::none()
            }
            Message::PlayTrack(index) => {
                self.pressed_track = None;
                self.handle_play_track(index);
                Task::none()
            }
            Message::PlayQueueTrack(index) => {
                self.pressed_track = None;
                self.handle_play_from_queue(index);
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
            Message::StartSongRadio(name) => {
                self.start_song_radio(name);
                Task::none()
            }
            Message::StartArtistRadio(name) => {
                self.start_artist_radio(name);
                Task::none()
            }
            Message::SearchResults(_tracks) => Task::none(),
            Message::SearchResultsAppend(_tracks) => Task::none(),
            Message::SearchError(_msg) => Task::none(),
            Message::DownloadComplete(url, path) => {
                self.downloading_index = None;
                self.download_registry.register(&url, &path);
                self.notify("Download complete!".into());
                Task::none()
            }
            Message::DownloadError(msg) => {
                self.downloading_index = None;
                error!("Download error: {}", msg);
                self.notify_error(msg);
                Task::none()
            }
            Message::ThumbnailsReady => {
                self.thumbnails_pending = true;
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
            Message::DeletePlaylist(index) => {
                self.handle_delete_playlist(index);
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
            Message::RemoveFromPlaylist(index) => {
                self.handle_remove_from_playlist(index);
                Task::none()
            }
            Message::ReorderTracks { from, to } => {
                self.handle_reorder_tracks(from, to);
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
            Message::ReorderQueue { from, to } => {
                self.handle_reorder_queue(from, to);
                Task::none()
            }
            Message::RemoveFromQueue(index) => {
                self.handle_remove_from_queue(index);
                Task::none()
            }
            Message::ToggleSelect(index) => {
                self.handle_toggle_select(index);
                Task::none()
            }
            Message::CopySelected => {
                self.handle_copy_selected();
                Task::none()
            }
            Message::DeleteSelected => {
                self.handle_delete_selected();
                Task::none()
            }
            Message::PasteClipboard => {
                self.handle_paste_clipboard();
                Task::none()
            }
            Message::ClearSelection => {
                self.handle_clear_selection();
                Task::none()
            }
            Message::NavigateSearch => {
                self.handle_navigate_to(View::Search(self.search_query.clone()));
                Task::none()
            }
            Message::NavigateTo(view) => {
                self.handle_navigate_to(view);
                Task::none()
            }
            Message::NavigateBack => {
                self.handle_navigate_back();
                Task::none()
            }
            Message::NavigateForward => {
                self.handle_navigate_forward();
                Task::none()
            }
            Message::ResumePlayback => {
                self.resume_playback();
                Task::none()
            }
            Message::ContextMenuPlayTrack(index) => {
                self.context_menu = None;
                self.pressed_track = None;
                self.handle_play_track(index);
                Task::none()
            }
            Message::ContextMenuStartSongRadio(index) => {
                let track = self.get_track_at(index);
                self.context_menu = None;
                if let Some(t) = track {
                    self.start_song_radio(t.title.clone());
                }
                Task::none()
            }
            Message::ContextMenuStartArtistRadio(index) => {
                let track = self.get_track_at(index);
                self.context_menu = None;
                if let Some(t) = track {
                    self.start_artist_radio(t.artist.clone());
                }
                Task::none()
            }
            Message::ContextMenuDownloadOrDelete(index) => {
                let track = self.get_track_at(index);
                self.context_menu = None;
                self.pressed_track = None;
                if let Some(track) = track {
                    if self.download_registry.contains(&track.url) {
                        self.handle_remove_download(index);
                    } else {
                        self.handle_download_track(index);
                    }
                }
                Task::none()
            }
            Message::ContextMenuRemoveFromPlaylist(index) => {
                self.handle_remove_from_playlist(index);
                self.context_menu = None;
                Task::none()
            }
            Message::ContextMenuRemoveFromQueue(index) => {
                self.handle_remove_from_queue(index);
                self.context_menu = None;
                Task::none()
            }
            Message::CloseContextMenu => {
                self.context_menu = None;
                Task::none()
            }
            Message::Notify(msg) => {
                self.notify(msg);
                Task::none()
            }
            Message::ClearNotification => {
                self.clear_notification();
                Task::none()
            }
        }
    }
}
