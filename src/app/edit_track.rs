//! Mutable working copy of a track being edited in the track-editing popup.
//!
//! Holds only the text-editable fields (`source` is changed via the provider
//! "select" buttons) plus the original position so the edit can be written
//! back to the correct list.

use crate::{app::interaction::TrackPos, providers::ProviderId, types::Track};

#[derive(Debug, Clone)]
pub struct EditTrackState {
    pub title: String,
    pub artist: String,
    pub source: ProviderId,
    pub original: Track,
    pub pos: TrackPos,
    /// The provider whose "Find" action is currently in flight, if any.
    pub finding: Option<ProviderId>,
}
