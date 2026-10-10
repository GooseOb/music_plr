//! Transient state for the playlist jumper overlay (`Ctrl+K`): fuzzy
//! navigation to a playlist, or picking one to add tracks to when `pending`
//! is set.

use super::pane::PaneId;

#[derive(Debug, Clone)]
pub struct PendingAdd {
    pub indices: Vec<usize>,
    /// Source list: a main pane id, or [`super::pane::QUEUE_PANE_ID`] /
    /// [`super::pane::RECENT_PANE_ID`] for the global panel lists.
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
