//! Module index for the application core.
//!
//! `MusicPlayer` (the single source of truth) lives in `state`; its `update`
//! dispatcher and per-domain handlers live in `update`; the pure `view` lives
//! in `ui`. This file only declares the submodules and re-exports the public
//! types so callers can keep using `crate::app::*`.

mod dependency_dialog;
mod dialog;
mod edit_track;
mod import;
mod interaction;
mod lyrics_state;
mod message;
pub mod pane;
mod playlist_picker;
mod shortcuts;
mod state;
mod translate_dialog;
mod ui;
mod update;
mod view_data;

pub use dependency_dialog::DependencyDialog;
pub use dialog::Dialog;
pub use edit_track::EditTrackState;
pub use import::{CsvPreset, ImportCsvField, ImportMethod, ImportPlaylistDialog};
pub use interaction::{ContextMenuState, TrackListKind, TrackListSearch};
pub use lyrics_state::{LyricsState, LyricsViewMode, LyricsViewport};
pub use message::{BackendResult, EditTrackField, Message};
pub use pane::{Pane, PaneData, SplitDir, SplitNode};
pub use playlist_picker::{PlaylistJump, PlaylistPicker};
pub use state::{MusicPlayer, PendingCache, Toast};
pub use translate_dialog::TranslateDialog;
pub use view_data::{ViewData, ViewKind};
