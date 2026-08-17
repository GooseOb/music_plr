//! Messages produced by the UI and results produced by background work.

use super::ViewData;
use crate::{
    app::{
        interaction::{self, TrackPos},
        update::operation::CaptureBounds,
    },
    data::library,
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
    /// A per-track volume-normalization gain computed in the background.
    NormalizationComputed(String, f32),
    /// Result of browsing a card (artist/album/playlist) to populate a newly
    /// created local playlist. Carries the playlist index, its (possibly
    /// de-duplicated) name, and the fetched tracks.
    CardPlaylistReady(usize, String, Vec<Track>),
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
    OpenAlbum(String, String),
    DragPress(interaction::Pressed),
    HoverStart(interaction::HoverTarget),
    ToggleLibrarySave(library::LibraryItem),
    ToggleLibraryExpanded,
    DeleteSearchHistory(usize),

    TrackRightClicked(TrackPos),
    PlayTrackAt(TrackPos),
    TogglePlayPause,
    NextTrack,
    PreviousTrack,
    SetVolume(f32),
    Seek(f32),

    CreatePlaylist,
    NewPlaylistNameChanged(String),
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

    SettingsDownloadDirChanged(String),
    SettingsMaxHistoryVisibleChanged(String),
    SettingsMaxHistoryStoredChanged(String),
    SettingsCacheMaxSizeChanged(String),
    SettingsMaxRecentlyPlayedChanged(String),
    SettingsVolumeNormalizationToggled(bool),
    SettingsResetDefaults,

    ContextMenuPlayTrack(TrackPos),
    ContextMenuStartSongRadio,
    ContextMenuStartArtistRadio,
    ContextMenuDownloadOrDelete(Vec<usize>),
    ContextMenuRemoveFromPlaylist(Vec<usize>),
    ContextMenuRemoveFromQueue(Vec<usize>),
    CloseContextMenu,
}
