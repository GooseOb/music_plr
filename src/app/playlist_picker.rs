//! Transient state for the playlist jumper overlay (`Ctrl+K`): fuzzy
//! navigation to a playlist, or picking one to add tracks to when `pending`
//! is set.

use super::pane::PaneId;
use crate::app::interaction::TrackListKind;

#[derive(Debug, Clone)]
pub struct PendingAdd {
    pub indices: Vec<usize>,
    pub list: TrackListKind,
    /// Owning pane for `Active` positions; ignored for `Queue`/`Recent`.
    pub pane: PaneId,
}

#[derive(Debug, Clone, Default)]
pub struct PlaylistJump {
    pub query: String,
    pub selected: usize,
    pub pending: Option<PendingAdd>,
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
