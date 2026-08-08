//! Messages produced by the UI and results produced by background work.

use super::ViewData;
use crate::types::{QueueTab, Track};
use iced::Point;

#[derive(Debug, Clone)]
pub enum BackendResult {
    SearchResults(u64, Vec<Track>, crate::youtube::SearchTab),
    SearchResultsAppend(u64, Vec<Track>),
    RadioResults(u64, String, Vec<Track>),
    /// Tracks returned by drilling into an artist/album/playlist. Carries the
    /// request id of the slot that issued the browse so results land in the
    /// correct view even after navigation.
    BrowseResults(u64, Vec<Track>),
    DownloadComplete(Track, String),
    DownloadError(String),
    SearchError(String),
    ThumbnailsDownloaded,
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
    SearchScopeChanged(crate::youtube::SearchScope),
    SearchLoadMore,
    SearchHistorySelected(usize),
    OpenArtist(String, String),
    OpenAlbum(String, String),
    OpenPlaylist(String, String),
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
    TogglePicker(Vec<usize>),
    ClosePicker,
    ShowDeleteConfirm(usize),
    ConfirmDeletePlaylist,
    HideDeleteConfirm,

    ToggleQueue,
    SwitchQueueTab(QueueTab),
    PlayRecentTrack(usize),

    NavigateTo(ViewData),
    NavigateBack,
    NavigateForward,

    ContextMenuPlayTrack(usize),
    ContextMenuStartSongRadio(usize),
    ContextMenuStartArtistRadio(usize),
    ContextMenuDownloadOrDelete(Vec<usize>),
    ContextMenuRemoveFromPlaylist(Vec<usize>),
    ContextMenuRemoveFromQueue(Vec<usize>),
    CloseContextMenu,
}
