//! Messages produced by the UI and results produced by background work.

use iced::{Point, Rectangle};

use super::ViewData;
use crate::{
    app::{
        interaction::{self, ContextMenuFocus, DefaultCtxAction, TrackListKind, TrackPos},
        update::operation::CaptureBounds,
        CsvPreset, ImportCsvField, ImportMethod, ViewKind,
    },
    data::library,
    lyrics::Lyrics,
    providers::ProviderId,
    types::{QueueTab, Track},
};

#[derive(Debug, Clone)]
pub enum BackendResult {
    SearchResults(u64, Vec<Track>, crate::providers::SearchTab),
    SearchResultsAppend(u64, Vec<Track>),
    RadioResults(u64, String, Vec<Track>),
    BrowseResults(u64, Vec<Track>, Option<crate::providers::AlbumMeta>),
    DownloadComplete(Track, String),
    DownloadError(String),
    SearchError(String),
    ThumbnailsDownloaded(Vec<String>),
    LyricsFetched(Result<Lyrics, String>, String),
    NormalizationComputed(String, f32),
    CardPlaylistReady(usize, String, Vec<Track>),
    /// An artist id was resolved on `provider` (by name) for the page that
    /// issued request `rid`; cached into the page's known provider ids.
    ArtistIdResolved {
        rid: u64,
        provider: ProviderId,
        resolved_id: String,
    },
    /// One artist-page section (`kind`) finished fetching for `provider`;
    /// the payload is exactly that kind's data. The error fails just its
    /// section.
    ArtistSectionLoaded {
        rid: u64,
        provider: ProviderId,
        kind: crate::providers::ArtistDataKind,
        data: Box<Result<crate::providers::ArtistKindData, String>>,
    },
    LocalFilesPicked(Vec<std::path::PathBuf>),
    /// Playlist import: the user picked a source (single file for Native/CSV,
    /// a folder for File-list) and the dialog's current settings should be
    /// applied. `method` is captured so the result stays correct even if the
    /// dialog was closed before the picker thread replied.
    ImportPathsPicked {
        method: ImportMethod,
        paths: Vec<std::path::PathBuf>,
    },
    ProviderResolved {
        original: Track,
        provider: ProviderId,
        resolved: Option<Track>,
        /// Where the track was selected from, so the resolved provider id can
        /// be written back into the source list (search/playlist/queue). `None`
        /// when the resolve was triggered automatically (no source row).
        pos: Option<TrackPos>,
        /// Whether the resolved track should be played (true) or downloaded
        /// (false) once its id is known.
        play: bool,
    },
    ProviderResolveError {
        /// Track title
        title: String,
        provider: ProviderId,
        message: String,
    },
    EditTrackProviderResolved(ProviderId, Option<Track>),
    /// The Edit Track "Find" action failed to resolve `provider`.
    EditTrackProviderError(ProviderId, String),
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    WindowClose,
    WindowResized(iced::Size),
    CursorMoved(Point),
    LeftButtonReleased,
    ListBoundsCaptured(Box<CaptureBounds>),
    SearchHistoryBoundsCaptured(crate::app::update::operation::ListGeometry),
    ContextMenuBoundsCaptured {
        panel: Rectangle,
        row_offsets: Vec<f32>,
    },
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
    OpenArtist {
        id: String,
        name: String,
        source: ProviderId,
    },
    ArtistSectionProviderChanged(crate::providers::ArtistSectionKind, ProviderId),
    ArtistHeaderProviderChanged(ProviderId),
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

    OpenImportPlaylist,
    CloseImportPlaylist,
    ImportMethodChanged(ImportMethod),
    ImportCsvColChanged(ImportCsvField, String),
    ImportCsvPresetChanged(CsvPreset),
    ImportPlaylistNameChanged(String),
    ImportPatternChanged(usize, String),
    ImportAddPattern,
    ImportRemovePattern(usize),
    ImportSelectFiles,

    TrackListSearchInput(String),
    TrackListSearchNext,
    TrackListSearchPrev,
    TrackListSearchClose,

    ToggleQueue,
    SwitchQueueTab(QueueTab),
    RevealNowPlaying,
    ToggleRepeat,
    ShowLyrics,
    SetLyricsViewMode(crate::app::LyricsViewMode),
    LyricsLineClicked(f32),
    SelectLyricsProvider(crate::lyrics::LyricsProvider),
    LyricsEditorAction(iced::widget::text_editor::Action),
    CopyLyrics,

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
    SettingsLanguageChanged(crate::i18n::Language),
    SettingsThemeChanged(crate::theme::ThemeKind),
    SettingsResetDefaults,

    ContextMenuPlayTrack(TrackPos),
    ContextMenuHover(Option<ContextMenuFocus>),
    ContextMenuGoToArtist,
    ContextMenuGoToArtistProvider(ProviderId),
    ContextMenuDefault(DefaultCtxAction),
    ContextMenuPlayViaProvider(ProviderId, TrackPos),
    ContextMenuDownloadViaProvider(ProviderId),
    ContextMenuSongRadioProvider(ProviderId),
    ContextMenuArtistRadioProvider(ProviderId),
    ContextMenuRemoveFromPlaylist(Vec<usize>),
    ContextMenuRemoveFromQueue(Vec<usize>),
    ContextMenuEditTrack,
    EditTrackField(EditTrackField, String),
    EditTrackSelectProvider(ProviderId),
    EditTrackFindProvider(ProviderId),
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
