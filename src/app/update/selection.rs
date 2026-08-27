//! Selection state and track-list access.
//!
//! Selection is held per list: `Active` in `ViewData`, `Queue` and `Recent`
//! in `MusicPlayer`.

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
            TrackListKind::Recent => &self.recent_selected_indices,
        }
    }

    fn selection_mut(&mut self, list: TrackListKind) -> &mut Vec<usize> {
        match list {
            TrackListKind::Queue => &mut self.queue_selected_indices,
            TrackListKind::Active => &mut self.view_data_mut().selection,
            TrackListKind::Recent => &mut self.recent_selected_indices,
        }
    }

    pub fn view_tracks(&self) -> &[Track] {
        let vd = self.view_data();
        match &vd.kind {
            ViewKind::Playlist(entry) => self
                .playlists
                .playlists
                .get(entry.index)
                .map_or(&[], |p| &p.tracks),
            _ => vd.tracks(),
        }
    }

    pub fn toggle_selection(&mut self, pos: TrackPos) {
        let sel = self.selection_mut(pos.list);
        if let Some(at) = sel.iter().position(|&i| i == pos.index) {
            sel.remove(at);
        } else {
            sel.push(pos.index);
        }
    }

    pub fn clear_selection(&mut self) {
        self.clear_selection_for(TrackListKind::Active);
        self.clear_selection_for(TrackListKind::Queue);
        self.clear_selection_for(TrackListKind::Recent);
        self.playlist_picker = None;
    }

    pub fn clear_selection_for(&mut self, list: TrackListKind) {
        self.selection_mut(list).clear();
        self.playlist_picker = None;
    }

    /// Whether any list currently holds a selection.
    pub fn has_selection(&self) -> bool {
        !self.selection(TrackListKind::Active).is_empty()
            || !self.selection(TrackListKind::Queue).is_empty()
            || !self.selection(TrackListKind::Recent).is_empty()
    }

    /// Clear the selection if any of `indices` was selected — used after a
    /// batch mutation (remove/delete) that leaves stale selection entries.
    pub fn clear_selection_if_touched(&mut self, indices: &[usize], list: TrackListKind) {
        let sel = self.selection(list);
        if indices.iter().any(|&i| sel.contains(&i)) {
            self.clear_selection();
        }
    }

    pub(crate) fn handle_select_all(&mut self) {
        let list = self
            .drag
            .hovered_track()
            .map_or(TrackListKind::Active, |h| h.list);
        let count = self.track_count(list);
        let sel = self.selection_mut(list);
        sel.clear();
        sel.extend(0..count);
    }

    pub fn get_track_at(&self, pos: TrackPos) -> Option<Track> {
        let TrackPos { index, list } = pos;
        match list {
            TrackListKind::Queue => self.queue.tracks.get(index).cloned(),
            TrackListKind::Active => self.view_tracks().get(index).cloned(),
            TrackListKind::Recent => self.queue.recently_played.get(index).cloned(),
        }
    }

    /// Whether `pos` is a match in the active track list search (any occurrence).
    pub fn is_track_list_match(&self, pos: TrackPos) -> bool {
        match &self.track_list_search {
            Some(fs) if fs.list == pos.list => fs.matches.contains(&pos.index),
            _ => false,
        }
    }

    pub fn track_list_match_position(&self) -> Option<usize> {
        let current_idx = self.drag.hovered_track()?.index;
        self.track_list_search
            .as_ref()?
            .matches
            .iter()
            .position(|&i| i == current_idx)
            .map(|p| p + 1)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::view_data::ViewData, data::config, types::Track};

    fn player() -> MusicPlayer {
        // Same headless construction as the navigation tests: MPRIS no-ops
        // without D-Bus, and nav history is reset to a deterministic Search
        // view (so `view_tracks` reads `view_data.tracks`).
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.nav_history = vec![ViewData::new_search(
            String::new(),
            crate::providers::ProviderId::YouTube,
            crate::providers::SearchScope::Songs,
        )];
        p.nav_history_pos = 0;
        p
    }

    fn track(id: &str) -> Track {
        Track::from_provider(
            crate::providers::ProviderId::YouTube,
            id.into(),
            format!("https://example.com/{id}"),
            format!("Track {id}"),
            "Artist",
            10,
            String::new(),
            None,
            None,
        )
    }

    #[test]
    fn recent_selection_toggles_like_other_lists() {
        let mut p = player();
        p.queue.recently_played.push_back(track("1"));
        p.queue.recently_played.push_back(track("2"));
        let pos0 = TrackPos::new(0, TrackListKind::Recent);
        let pos1 = TrackPos::new(1, TrackListKind::Recent);

        assert!(p.selection(TrackListKind::Recent).is_empty());
        p.toggle_selection(pos0);
        assert_eq!(p.selection(TrackListKind::Recent), &[0]);
        p.toggle_selection(pos1);
        assert_eq!(p.selection(TrackListKind::Recent), &[0, 1]);
        p.toggle_selection(pos0);
        assert_eq!(p.selection(TrackListKind::Recent), &[1]);
        p.clear_selection_for(TrackListKind::Recent);
        assert!(p.selection(TrackListKind::Recent).is_empty());
        p.clear_selection();
        assert!(p.selection(TrackListKind::Recent).is_empty());
    }

    #[test]
    fn toggle_adds_then_removes_per_list() {
        let mut p = player();
        p.queue.tracks = vec![track("1"), track("2")];
        p.view_data_mut().set_tracks(vec![track("1")]);

        let q = TrackPos::new(0, TrackListKind::Queue);
        let a = TrackPos::new(0, TrackListKind::Active);
        p.toggle_selection(q);
        p.toggle_selection(a);
        assert_eq!(p.selection(TrackListKind::Queue), &[0]);
        assert_eq!(p.selection(TrackListKind::Active), &[0]);

        // Selections are scoped per list.
        assert_eq!(p.get_track_at(q).map(|t| t.title), Some("Track 1".into()));
        assert_eq!(p.get_track_at(a).map(|t| t.title), Some("Track 1".into()));

        p.toggle_selection(q);
        assert!(p.selection(TrackListKind::Queue).is_empty());
        assert_eq!(p.selection(TrackListKind::Active), &[0]);

        p.clear_selection();
        assert!(p.selection(TrackListKind::Active).is_empty());
    }
}
