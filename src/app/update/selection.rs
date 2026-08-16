//! Selection state and track-list access.
//!
//! `Recent` is read-only: `selection` yields an empty slice and mutations
//! are no-ops.

use super::{MusicPlayer, Track};
use crate::app::{
    interaction::{TrackListKind, TrackPos},
    ViewKind,
};

impl MusicPlayer {
    pub fn selection(&self, list: TrackListKind) -> &[usize] {
        match list {
            TrackListKind::Queue => &self.queue_selected_indices,
            TrackListKind::Active => &self.view_data().selection,
            TrackListKind::Recent => &[],
        }
    }

    fn selection_mut(&mut self, list: TrackListKind) -> Option<&mut Vec<usize>> {
        match list {
            TrackListKind::Queue => Some(&mut self.queue_selected_indices),
            TrackListKind::Active => Some(&mut self.view_data_mut().selection),
            TrackListKind::Recent => None,
        }
    }

    pub fn view_tracks(&self) -> &[Track] {
        let vd = self.view_data();
        match &vd.kind {
            ViewKind::Playlist {
                selected_playlist, ..
            } => selected_playlist
                .and_then(|sp| self.playlists.playlists.get(sp))
                .map_or(&[], |p| &p.tracks),
            _ => &vd.tracks,
        }
    }

    pub fn toggle_selection(&mut self, pos: TrackPos) {
        let Some(sel) = self.selection_mut(pos.list) else {
            return;
        };
        if let Some(at) = sel.iter().position(|&i| i == pos.index) {
            sel.remove(at);
        } else {
            sel.push(pos.index);
        }
    }

    pub fn clear_selection(&mut self) {
        self.view_data_mut().selection.clear();
        self.queue_selected_indices.clear();
        self.playlist_picker = None;
    }

    /// Clear the selection if any of `indices` was selected — used after a
    /// batch mutation (remove/delete) that leaves stale selection entries.
    pub fn clear_selection_if_touched(&mut self, indices: &[usize], list: TrackListKind) {
        let sel = self.selection(list);
        if indices.iter().any(|&i| sel.contains(&i)) {
            self.clear_selection();
        }
    }

    pub fn get_track_at(&self, pos: TrackPos) -> Option<Track> {
        let TrackPos { index, list } = pos;
        match list {
            TrackListKind::Queue => self.queue.tracks.get(index).cloned(),
            TrackListKind::Active => self.view_tracks().get(index).cloned(),
            TrackListKind::Recent => self.queue.recently_played.get(index).cloned(),
        }
    }

    /// Counts the queue's now-playing entry at index 0, which `first_index`
    /// skips.
    pub fn track_count(&self, list: TrackListKind) -> usize {
        match list {
            TrackListKind::Queue => self.queue.tracks.len(),
            TrackListKind::Active => self.view_tracks().len(),
            TrackListKind::Recent => self.queue.recently_played.len(),
        }
    }
}
