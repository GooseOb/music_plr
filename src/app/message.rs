//! Messages produced by the UI and results produced by background work.

use super::ViewData;
use crate::{
    app::{
        interaction::{self, TrackListKind, TrackPos},
        update::operation::CaptureBounds,
        ViewKind,
    },
    data::library,
    lyrics::Lyrics,
    providers::ProviderId,
    types::{QueueTab, Track},
};
use iced::Point;

#[derive(Debug, Clone)]
pub enum BackendResult {
    SearchResults(u64, Vec<Track>, crate::providers::SearchTab),
    SearchResultsAppend(u64, Vec<Track>),
    RadioResults(u64, String, Vec<Track>),
    BrowseResults(u64, Vec<Track>),
    DownloadComplete(Track, String),
    DownloadError(String),
    SearchError(String),
    ThumbnailsDownloaded(Vec<String>),
    LyricsFetched(Option<Lyrics>, String),
    NormalizationComputed(String, f32),
    CardPlaylistReady(usize, String, Vec<Track>),
    ProviderResolved {
        original: Track,
        provider: ProviderId,
        resolved: Option<Track>,
        /// Where the track was selected from, so the resolved provider id can
        /// be written back into the source list (search/playlist/queue). `None`
        /// when the resolve was triggered automatically (no source row).
        pos: Option<TrackPos>,
    },
    /// Like [`BackendResult::ProviderResolved`], but the resolved track should
    /// be downloaded (not played) once its id is known.
    ProviderResolvedDownload {
        original: Track,
        provider: ProviderId,
        resolved: Option<Track>,
        /// Where the track was selected from, so the resolved provider id can
        /// be written back into the source list.
        pos: Option<TrackPos>,
    },
    ProviderResolveError {
        /// Track title
        title: String,
        provider: ProviderId,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    WindowClose,
    WindowResized(iced::Size),
    CursorMoved(Point),
    LeftButtonReleased,
    ListBoundsCaptured(CaptureBounds),
    SearchHistoryBoundsCaptured(crate::app::update::operation::ListGeometry),
    ListScrolled {
        list: TrackListKind,
        translation_y: f32,
    },
    KeyPressed {
        key: iced::keyboard::key::Physical,
        modifiers: iced::keyboard::Modifiers,
    },

    SearchInputChanged(String),
    SearchExecute,
    SearchScopeChanged(crate::providers::SearchScope),
    SearchProviderChanged(ProviderId),
    SearchLoadMore,
    SearchHistorySelected(usize),
    DeleteSearchHistory(usize),
    Browse(ViewKind, ProviderId),
    DragPress(interaction::Pressed),
    HoverStart(interaction::HoverTarget),
    ToggleLibrarySave(library::LibraryItem),
    ToggleLibraryExpanded,

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
    OpenAndPlayPlaylist(usize),

    TrackListSearchInput(String),
    TrackListSearchNext,
    TrackListSearchPrev,
    TrackListSearchClose,

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
    SettingsDefaultProviderChanged(ProviderId),
    SettingsResetDefaults,

    ContextMenuPlayTrack(TrackPos),
    ContextMenuGoToArtist,
    ContextMenuPlayViaProvider(ProviderId, TrackPos),
    ContextMenuDownloadViaProvider(ProviderId, Vec<usize>),
    ContextMenuSongRadioProvider(ProviderId),
    ContextMenuArtistRadioProvider(ProviderId),
    ContextMenuRemoveFromPlaylist(Vec<usize>),
    ContextMenuRemoveFromQueue(Vec<usize>),
    ContextMenuEditTrack,
    EditTrackField(EditTrackField, String),
    EditTrackSelectProvider(ProviderId),
    SaveEditTrack,
    CloseEditTrack,
    CloseContextMenu,
}

/// Editable text fields of a [`Track`](crate::types::Track) in the track
/// editing popup. `source` is excluded: it is changed only via the provider
/// "select" buttons, never a text input.
#[derive(Debug, Clone, Copy)]
pub enum EditTrackField {
    Title,
    Artist,
}
