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
    types::{QueueTab, Track},
};
use iced::Point;

#[derive(Debug, Clone)]
pub enum BackendResult {
    SearchResults(u64, Vec<Track>, crate::provider::SearchTab),
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
    /// A track resolved on `provider` (its id discovered via search) for a
    /// track that lacked a streamable provider. `id` is `None` if no match
    /// was found. Drives the "play via / download from [provider]" flow.
    ProviderResolved {
        original: Track,
        provider: crate::provider::ProviderId,
        /// Resolved `(id, url)` for the provider.
        id: Option<(String, String)>,
        /// Where the track was selected from, so the resolved provider id can
        /// be written back into the source list (search/playlist/queue). `None`
        /// when the resolve was triggered automatically (no source row).
        pos: Option<TrackPos>,
    },
    /// Like [`BackendResult::ProviderResolved`], but the resolved track should
    /// be downloaded (not played) once its id is known.
    ProviderResolvedDownload {
        original: Track,
        provider: crate::provider::ProviderId,
        /// Resolved `(id, url)` for the provider.
        id: Option<(String, String)>,
        /// Where the track was selected from, so the resolved provider id can
        /// be written back into the source list.
        pos: Option<TrackPos>,
    },
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
    ListScrolled {
        list: TrackListKind,
        translation_y: f32,
    },
    KeyPressed {
        key: iced::keyboard::key::Key,
        modifiers: iced::keyboard::Modifiers,
    },

    SearchInputChanged(String),
    SearchExecute,
    SearchScopeChanged(crate::provider::SearchScope),
    SearchProviderChanged(crate::provider::ProviderId),
    SearchLoadMore,
    SearchHistorySelected(usize),
    DeleteSearchHistory(usize),
    Browse(ViewKind),
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

    FloatingSearchInput(String),
    FloatingSearchNext,
    FloatingSearchPrev,
    FloatingSearchClose,

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
    SettingsDefaultProviderChanged(crate::provider::ProviderId),
    SettingsJamendoClientIdChanged(String),
    SettingsResetDefaults,

    ContextMenuPlayTrack(TrackPos),
    ContextMenuGoToArtist,
    /// Play the track via the given provider. If the track already carries
    /// that provider's id, play directly; otherwise resolve its id first.
    ContextMenuPlayViaProvider(crate::provider::ProviderId, TrackPos),
    /// Download the track from the given provider (resolving its id first if
    /// needed).
    ContextMenuDownloadViaProvider(crate::provider::ProviderId, Vec<usize>),
    /// Start a song radio seeded by the given provider (only providers that
    /// support similarity search offer this).
    ContextMenuSongRadioProvider(crate::provider::ProviderId),
    /// Start an artist radio seeded by the given provider.
    ContextMenuArtistRadioProvider(crate::provider::ProviderId),
    ContextMenuRemoveFromPlaylist(Vec<usize>),
    ContextMenuRemoveFromQueue(Vec<usize>),
    CloseContextMenu,
}
