//! Selection state and track-list access.
//!
//! The `is_queue` flag threaded through these methods selects between the
//! two lists the user can interact with: the queue panel and the main
//! track list of whatever view is active.

use super::{MusicPlayer, Track};
use crate::app::ViewKind;

impl MusicPlayer {
    pub fn selection(&self, is_queue: bool) -> &[usize] {
        if is_queue {
            &self.queue_selected_indices
        } else {
            &self.view_data().selection
        }
    }

    pub fn selection_mut(&mut self, is_queue: bool) -> &mut Vec<usize> {
        if is_queue {
            &mut self.queue_selected_indices
        } else {
            &mut self.view_data_mut().selection
        }
    }

    /// The track list for the current (non-queue) view. For `Playlist` the
    /// tracks live in the `PlaylistStore`; for all other kinds they are the
    /// view's own `tracks`.
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

    pub fn toggle_selection(&mut self, index: usize, is_queue: bool) {
        let sel = self.selection_mut(is_queue);
        if let Some(pos) = sel.iter().position(|&i| i == index) {
            sel.remove(pos);
        } else {
            sel.push(index);
        }
    }

    pub fn clear_selection(&mut self) {
        self.view_data_mut().selection.clear();
        self.queue_selected_indices.clear();
        self.picker = None;
    }

    pub fn get_track_at(&self, index: usize, is_queue: bool) -> Option<Track> {
        if is_queue {
            return self.queue.tracks.get(index).cloned();
        }
        let vd = self.view_data();
        match &vd.kind {
            ViewKind::Playlist { .. } => self.view_tracks().get(index).cloned(),
            _ => vd.tracks.get(index).cloned(),
        }
    }

    pub fn current_track_count(&self, is_queue: bool) -> usize {
        if is_queue {
            return self.queue.tracks.len();
        }
        self.view_tracks().len()
    }

    pub fn get_current_list_bounds(&self) -> Option<iced::Rectangle> {
        self.view_data().bounds
    }

    pub fn get_current_list_scroll(&self) -> f32 {
        self.view_data().scroll
    }
}
