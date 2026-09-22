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

#[derive(Debug, Clone, Default)]
pub struct PlaylistJump {
    pub query: String,
    pub selected: usize,
}

impl PlaylistJump {
    pub fn filtered(&self, names: &[String]) -> Vec<usize> {
        if self.query.trim().is_empty() {
            return (0..names.len()).collect();
        }
        names
            .iter()
            .enumerate()
            .filter(|(_, name)| crate::util::fuzzy_match(&self.query, name))
            .map(|(i, _)| i)
            .collect()
    }
}
