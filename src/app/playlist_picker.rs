//! Transient state for the "add to playlist" picker overlay: the tracks
//! currently selected for adding and which track list they came from.

use crate::app::interaction::TrackListKind;

pub struct PlaylistPicker {
    pub indices: Vec<usize>,
    pub list: TrackListKind,
}
