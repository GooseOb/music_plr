//! Messages produced by the UI and results produced by background work.

use iced::{Point, Rectangle};

use super::{
    pane::{is_main_pane, PaneId},
    shortcut::Shortcut,
    ViewData,
};
use crate::{
    app::{
        interaction::{self, ContextMenuFocus, DefaultCtxAction, TrackPos},
        update::{
            operation::{CaptureBounds, ScrollTarget},
            VersionCheckOutcome,
        },
        CsvPreset, ImportCsvField, ImportMethod, ViewKind,
    },
    data::library,
    deps::DepKind,
    lyrics::Lyrics,
    media_controls::MediaControlEvent,
    providers::ProviderId,
    types::{QueueTab, Track},
};

#[derive(Debug, Clone)]
pub enum BackendResult {
    SearchResults(u64, Vec<Track>, crate::providers::SearchTab),
    SearchResultsAppend(u64, Vec<Track>),
    RadioResults(u64, Vec<Track>),
    BrowseResults(u64, Vec<Track>, Option<crate::providers::AlbumMeta>),
    DownloadComplete(Track, String),
    DownloadError(String),
    /// A `YouTube` player-client race event from a download thread; the tick
    /// loop surfaces it as a toast.
    PlayerClientEvent(crate::providers::ClientEvent),
    SearchError(u64, String),
    ThumbnailDownloaded(ProviderId, String),
    LyricsFetched(
        Result<Lyrics, String>,
        String,
        crate::lyrics::LyricsProvider,
    ),
    /// One lazy translation finished loading for the provider-namespaced
    /// `slug:id` track key from `provider`; merged into the ready lyrics of
    /// waiting panes.
    LyricsTranslationFetched(
        Result<crate::lyrics::TranslatedLyrics, String>,
        String,
        crate::lyrics::LyricsProvider,
    ),
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
    /// Background install of a dependency finished (success or error).
    DependencyInstalled(DepKind, Result<(), String>),
    DependencyProgress(DepKind, u64, u64),
    DependencyDeleted(DepKind, Result<(), String>),
    ProviderResolved {
        original: Track,
        provider: ProviderId,
        resolved: Option<Track>,
        rid: u64,
        /// Where the track was selected from, so the resolved provider id can
        /// be written back into the source list (search/playlist/queue).
        pos: TrackPos,
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
    /// An Edit Track "Manual search" query finished; the payload is its
    /// track results. It applies to the popup only while it still awaits this
    /// request id, so a superseded query's late reply is dropped.
    ManualSearchResults {
        rid: u64,
        results: Result<Vec<Track>, String>,
    },
    /// A background version-check completed.
    VersionChecked(Result<VersionCheckOutcome, String>),
    /// Download progress for an in-flight self-update.
    UpdateProgress(u64, u64),
    /// The update download/extract/staged-apply finished.
    UpdateComplete(Result<String, String>),
    /// An AI lyrics translation finished; the text opens in the custom-lyrics
    /// editor of `pane` under `language` when the track still matches.
    TranslationDone {
        pane: PaneId,
        track_id: String,
        language: String,
        text: String,
    },
    TranslationError {
        pane: PaneId,
        message: String,
    },
    TranslationModels(Vec<String>),
    TranslationModelsError(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    WindowClose,
    WindowResized(iced::Size),
    /// A window was opened; carries its id so the Windows HWND can be resolved.
    WindowOpened(iced::window::Id),
    /// The OS window handle (HWND on Windows) resolved for media controls.
    MediaHwnd(Option<u64>),
    /// An OS media-control event (media keys, MPRIS/SMTC), bridged from the
    /// controls thread as it arrives — no tick polling involved.
    MediaEvent(MediaControlEvent),
    CursorMoved(Point),
    LeftButtonReleased,
    ListBoundsCaptured(Box<CaptureBounds>),
    SearchHistoryBoundsCaptured(PaneId, crate::app::update::operation::ListGeometry),
    ContextMenuBoundsCaptured {
        panel: Rectangle,
        row_offsets: Vec<f32>,
    },
    /// A scrollable moved. Reported live because the capture pass only runs on
    /// navigation, results and resize (see [`CaptureBounds`]), so an offset
    /// that changes in between has to arrive here to stay fresh.
    Scrolled(crate::app::update::operation::ScrollTarget, iced::Vector),
    LyricsScrolled {
        pane: PaneId,
        translation_y: f32,
        viewport_h: f32,
        content_h: f32,
    },
    Shortcut(Shortcut),

    SearchInputChanged(PaneId, String),
    SearchInputClicked(PaneId),
    SearchExecute(PaneId),
    SearchScopeChanged(PaneId, crate::providers::SearchScope),
    SearchProviderChanged(PaneId, ProviderId),
    SearchLoadMore(PaneId),
    SearchHistorySelected(PaneId, usize),
    DeleteSearchHistory(PaneId, usize),
    Browse(PaneId, ViewKind, ProviderId),
    OpenArtist {
        pane: PaneId,
        id: String,
        name: String,
        source: ProviderId,
        /// Public page for the artist, empty when the provider advertises none.
        url: String,
    },
    ArtistSectionProviderChanged(PaneId, crate::providers::ArtistSectionKind, ProviderId),
    ArtistHeaderProviderChanged(PaneId, ProviderId),
    DragPress(interaction::Pressed),
    HoverStart(interaction::HoverTarget),
    HoverEnd(interaction::HoverTarget),
    ToggleLibrarySave(library::LibraryItem),
    ToggleLibraryExpanded,

    TrackRightClicked(TrackPos, Point),
    PlayTrackAt(TrackPos),
    TogglePlayPause,
    NextTrack,
    PreviousTrack,
    SetVolume(f32),
    Seek(f32),

    CreatePlaylist,
    ToggleCurrentTrackLike,
    ToggleTrackLike(TrackPos),
    SaveBrowseAsPlaylist(PaneId),
    OpenPlaylistJump,
    PlaylistJumpInput(String),
    PlaylistJumpOpen(usize),
    PlaylistJumpHover(usize),
    PlaylistJumpOpenPlay(usize),
    NewPlaylistNameChanged(String),
    RenamePlaylist(String),
    AddLocalMusic,
    OpenPlaylistAdd(Vec<usize>),
    ShowDeleteConfirm(usize),
    ConfirmDeletePlaylist,
    OpenAndPlayPlaylist(usize),

    OpenImportPlaylist,
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
    ShowLyrics(PaneId),
    SetLyricsViewMode(PaneId, crate::app::LyricsViewMode),
    LyricsLineClicked(f32),
    SelectLyricsProvider(PaneId, crate::lyrics::LyricsProvider),
    SelectLyricsTranslation(PaneId, Option<String>),
    LyricsEditorAction(PaneId, iced::widget::text_editor::Action),
    CopyLyrics(PaneId),
    SelectLyricLine(PaneId, usize),
    LyricNoteAction(PaneId, iced::widget::text_editor::Action),
    StartCustomLyricsEdit(PaneId),
    EditCustomLyrics(PaneId, String),
    SelectCustomLyrics(PaneId, String),
    CustomLyricsNameChanged(PaneId, String),
    CustomLyricsEditorAction(PaneId, iced::widget::text_editor::Action),
    SaveCustomLyrics(PaneId),
    CancelCustomLyricsEdit(PaneId),
    DeleteCustomLyrics(PaneId),
    OpenTranslateDialog(PaneId),
    TranslateLanguageChanged(String),
    TranslateLanguageSubmit,
    TranslatePromptAction(iced::widget::text_editor::Action),
    TranslateResetPrompt,
    TranslateSubmit,
    RequestTranslationModels,
    TranslationModelsFilterChanged(String),

    NavigateTo(PaneId, ViewData),
    SidebarSearch,
    NavigateBack(PaneId),
    NavigateForward(PaneId),
    SplitHorizontal(PaneId),
    SplitVertical(PaneId),
    ClosePane(PaneId),
    FocusPane(PaneId),

    SettingsChanged(crate::app::update::SettingsChange),
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
    ContextMenuClearCache,
    ContextMenuClearCacheProvider(ProviderId),
    ContextMenuAddToQueue(PaneId, Vec<usize>),
    ContextMenuAddToTrashbin(PaneId, Vec<usize>),
    ContextMenuRemoveFromTrashbin(PaneId, Vec<usize>),
    ContextMenuRemoveFromList(PaneId, Vec<usize>),
    ContextMenuEditTrack,
    ContextMenuOpenInBrowser,
    CardRightClicked {
        /// The scrollable the card sits in, whose offset shifted `point`.
        host: ScrollTarget,
        pane: PaneId,
        /// The exact view the card's click opens, so the menu's "Open" lands
        /// on the same page (and every other entry reads the card's identity
        /// off it).
        target: ViewKind,
        /// The artist-page shelf the card sits in, when it has one (that one
        /// scrolls sideways).
        row: Option<crate::providers::ArtistSectionKind>,
        point: Point,
    },
    CardMenuOpen,
    CardMenuOpenInBrowser,
    CardMenuSaveAsPlaylist,
    CardMenuToggleLibrary,
    /// Open a page in the desktop's default browser (the browse-page header
    /// button); the card and track menus resolve their own page instead.
    OpenInBrowser(String),
    EditTrackField(EditTrackField, String),
    EditTrackSelectProvider(ProviderId),
    EditTrackFindProvider(ProviderId),
    /// Open the Edit Track popup's "Manual search" step for one provider.
    EditTrackManualSearch(ProviderId),
    EditTrackManualQuery(String),
    EditTrackManualScope(crate::providers::SearchScope),
    EditTrackManualRun,
    EditTrackManualConfirm,
    EditTrackManualCancel,
    SaveEditTrack,
    CloseContextMenu,
    CloseDialog,

    DepToggle(DepKind),
    DepInstall,
    DepDismiss,
    DepSettingsInstall(DepKind),
    DepSettingsDelete(DepKind),

    /// Trigger a background version check (GitHub releases API).
    CheckForUpdates,
    /// Download and stage the available update, then restart.
    UpdateApp,
}

impl Message {
    /// The pane this message targets, if any. The dispatcher drops messages
    /// for panes that no longer exist (e.g. an action sent before a close was
    /// processed). `Queue`/`Recent` positions live in the global panel and
    /// carry no pane.
    pub fn pane(&self) -> Option<PaneId> {
        let active_pane = |pos: &TrackPos| is_main_pane(pos.pane).then_some(pos.pane);
        match self {
            Message::SearchInputChanged(pane, _)
            | Message::SearchInputClicked(pane)
            | Message::SearchExecute(pane)
            | Message::SearchScopeChanged(pane, _)
            | Message::SearchProviderChanged(pane, _)
            | Message::SearchLoadMore(pane)
            | Message::SearchHistorySelected(pane, _)
            | Message::DeleteSearchHistory(pane, _)
            | Message::Browse(pane, _, _)
            | Message::ArtistSectionProviderChanged(pane, _, _)
            | Message::ArtistHeaderProviderChanged(pane, _)
            | Message::ShowLyrics(pane)
            | Message::SetLyricsViewMode(pane, _)
            | Message::SelectLyricsProvider(pane, _)
            | Message::SelectLyricsTranslation(pane, _)
            | Message::LyricsEditorAction(pane, _)
            | Message::CopyLyrics(pane)
            | Message::SelectLyricLine(pane, _)
            | Message::LyricNoteAction(pane, _)
            | Message::StartCustomLyricsEdit(pane)
            | Message::EditCustomLyrics(pane, _)
            | Message::SelectCustomLyrics(pane, _)
            | Message::CustomLyricsNameChanged(pane, _)
            | Message::CustomLyricsEditorAction(pane, _)
            | Message::SaveCustomLyrics(pane)
            | Message::CancelCustomLyricsEdit(pane)
            | Message::DeleteCustomLyrics(pane)
            | Message::OpenTranslateDialog(pane)
            | Message::NavigateTo(pane, _)
            | Message::SaveBrowseAsPlaylist(pane)
            | Message::NavigateBack(pane)
            | Message::NavigateForward(pane)
            | Message::SplitHorizontal(pane)
            | Message::SplitVertical(pane)
            | Message::ClosePane(pane)
            | Message::FocusPane(pane)
            | Message::OpenArtist { pane, .. }
            | Message::CardRightClicked { pane, .. }
            | Message::SearchHistoryBoundsCaptured(pane, _)
            | Message::LyricsScrolled { pane, .. }
            | Message::Scrolled(ScrollTarget::Cards(pane) | ScrollTarget::CardRow(pane, _), _) => {
                Some(*pane)
            }
            // The queue/recent virtual ids aren't panes, and the sidebar's
            // library list belongs to no pane.
            Message::Scrolled(ScrollTarget::Tracks(pane), _) => {
                is_main_pane(*pane).then_some(*pane)
            }
            Message::DragPress(interaction::Pressed::Track(pos))
            | Message::TrackRightClicked(pos, _)
            | Message::PlayTrackAt(pos)
            | Message::ToggleTrackLike(pos)
            | Message::ContextMenuPlayTrack(pos)
            | Message::HoverStart(interaction::HoverTarget::Track(pos))
            | Message::HoverEnd(interaction::HoverTarget::Track(pos))
            | Message::ContextMenuPlayViaProvider(_, pos) => active_pane(pos),
            _ => None,
        }
    }
}

/// Editable text fields of a [`Track`](crate::types::Track) in the track
/// editing popup. `source` is excluded: it is changed only via the provider
/// "select" buttons, never a text input.
#[derive(Debug, Clone, Copy)]
pub enum EditTrackField {
    Title,
    Artist,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{
            interaction::{HoverTarget, Pressed},
            pane::{QUEUE_PANE_ID, RECENT_PANE_ID},
        },
        providers::{ArtistSectionKind, SearchScope},
    };

    /// Every pane-scoped message must report its pane so the dispatcher can
    /// drop actions for closed panes. Extend this list when adding a
    /// `PaneId`-carrying variant.
    #[test]
    fn pane_scoped_messages_report_their_pane() {
        let pane = 7;
        let pos = TrackPos::new(0, pane);
        let editor = || iced::widget::text_editor::Action::SelectAll;
        let scrolled = |target| Message::Scrolled(target, iced::Vector::ZERO);
        let messages = vec![
            Message::SearchInputChanged(pane, String::new()),
            Message::SearchInputClicked(pane),
            Message::SearchExecute(pane),
            Message::SearchScopeChanged(pane, SearchScope::Songs),
            Message::SearchProviderChanged(pane, ProviderId::YouTube),
            Message::SearchLoadMore(pane),
            Message::SearchHistorySelected(pane, 0),
            Message::DeleteSearchHistory(pane, 0),
            Message::Browse(pane, ViewKind::Downloads, ProviderId::YouTube),
            Message::ArtistSectionProviderChanged(
                pane,
                ArtistSectionKind::Popular,
                ProviderId::YouTube,
            ),
            Message::ArtistHeaderProviderChanged(pane, ProviderId::YouTube),
            Message::ShowLyrics(pane),
            Message::SetLyricsViewMode(pane, crate::app::LyricsViewMode::Synced),
            Message::SelectLyricsProvider(pane, crate::lyrics::LyricsProvider::LrcLib),
            Message::LyricsEditorAction(pane, editor()),
            Message::CopyLyrics(pane),
            Message::SelectLyricLine(pane, 0),
            Message::LyricNoteAction(pane, editor()),
            Message::StartCustomLyricsEdit(pane),
            Message::EditCustomLyrics(pane, String::new()),
            Message::SelectCustomLyrics(pane, String::new()),
            Message::CustomLyricsNameChanged(pane, String::new()),
            Message::CustomLyricsEditorAction(pane, editor()),
            Message::SaveCustomLyrics(pane),
            Message::CancelCustomLyricsEdit(pane),
            Message::DeleteCustomLyrics(pane),
            Message::OpenTranslateDialog(pane),
            Message::NavigateTo(pane, ViewData::default()),
            Message::SaveBrowseAsPlaylist(pane),
            Message::NavigateBack(pane),
            Message::NavigateForward(pane),
            Message::SplitHorizontal(pane),
            Message::SplitVertical(pane),
            Message::ClosePane(pane),
            Message::FocusPane(pane),
            Message::OpenArtist {
                pane,
                id: String::new(),
                name: String::new(),
                source: ProviderId::YouTube,
                url: String::new(),
            },
            scrolled(ScrollTarget::Tracks(pane)),
            scrolled(ScrollTarget::Cards(pane)),
            scrolled(ScrollTarget::CardRow(pane, ArtistSectionKind::Albums)),
            Message::LyricsScrolled {
                pane,
                translation_y: 0.0,
                viewport_h: 0.0,
                content_h: 0.0,
            },
            Message::SearchHistoryBoundsCaptured(
                pane,
                crate::app::update::operation::ListGeometry::default(),
            ),
            Message::DragPress(Pressed::Track(pos)),
            Message::TrackRightClicked(pos, Point::ORIGIN),
            Message::PlayTrackAt(pos),
            Message::ToggleTrackLike(pos),
            Message::ContextMenuPlayTrack(pos),
            Message::HoverStart(HoverTarget::Track(pos)),
            Message::HoverEnd(HoverTarget::Track(pos)),
            Message::ContextMenuPlayViaProvider(ProviderId::YouTube, pos),
            Message::CardRightClicked {
                host: ScrollTarget::Cards(pane),
                pane,
                target: ViewKind::Downloads,
                row: None,
                point: Point::ORIGIN,
            },
        ];
        assert!(!messages.is_empty());
        for msg in messages {
            assert_eq!(msg.pane(), Some(pane), "{msg:?}");
        }

        // Queue/recent rows live in the global panel and carry no real pane.
        assert_eq!(
            Message::PlayTrackAt(TrackPos::new(0, QUEUE_PANE_ID)).pane(),
            None
        );
        assert_eq!(
            Message::PlayTrackAt(TrackPos::new(0, RECENT_PANE_ID)).pane(),
            None
        );
        assert_eq!(scrolled(ScrollTarget::Tracks(QUEUE_PANE_ID)).pane(), None);
        assert_eq!(Message::Tick.pane(), None);
        // The sidebar's library list is not pane-scoped either.
        assert_eq!(scrolled(ScrollTarget::Library).pane(), None);
    }
}
