//! Messages produced by the UI and results produced by background work.

use super::ViewData;
use crate::{
    app::{interaction::TrackPos, update::operation::CaptureBounds},
    lyrics::Lyrics,
    types::{QueueTab, Track},
};
use iced::Point;

#[derive(Debug, Clone)]
pub enum BackendResult {
    SearchResults(u64, Vec<Track>, crate::youtube::SearchTab),
    SearchResultsAppend(u64, Vec<Track>),
    RadioResults(u64, String, Vec<Track>),
    BrowseResults(u64, Vec<Track>),
    DownloadComplete(Track, String),
    DownloadError(String),
    SearchError(String),
    ThumbnailsDownloaded(Vec<String>),
    LyricsFetched(Option<Lyrics>, String),
}

#[derive(Debug, Clone)]
pub enum Message {
    Noop,
    Tick,
    WindowClose,
    WindowResized(iced::Size),
    CursorMoved(Point),
    LeftButtonReleased,
    ListBoundsCaptured(CaptureBounds),
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
    ToggleLibrarySave(crate::data::library::LibraryItem),
    ToggleLibraryExpanded,
    DeleteSearchHistory(usize),

    TrackPressed(TrackPos),
    TrackHoverStart(TrackPos),
    TrackRightClicked(TrackPos),
    PlayTrackAt(TrackPos),
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
    ToggleRepeat,
    ShowLyrics,
    ToggleLyricsSelectMode,
    LyricsLineClicked(f32),
    SelectLyricsProvider(crate::lyrics::LyricsProvider),
    LyricsEditorAction(iced::widget::text_editor::Action),

    NavigateTo(ViewData),
    NavigateBack,
    NavigateForward,

    ContextMenuPlayTrack(TrackPos),
    ContextMenuStartSongRadio,
    ContextMenuStartArtistRadio,
    ContextMenuDownloadOrDelete(Vec<usize>),
    ContextMenuRemoveFromPlaylist(Vec<usize>),
    ContextMenuRemoveFromQueue(Vec<usize>),
    CloseContextMenu,
}
