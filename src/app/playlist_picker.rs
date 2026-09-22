//! Transient state for the "add to playlist" picker overlay: the tracks
//! currently selected for adding and which track list they came from.

use super::pane::PaneId;
use crate::app::interaction::TrackListKind;

#[derive(Debug, Clone)]
pub struct PlaylistPicker {
    pub indices: Vec<usize>,
    pub list: TrackListKind,
    /// Owning pane for `Active` positions; ignored for `Queue`/`Recent`.
    pub pane: PaneId,
}
