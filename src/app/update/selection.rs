//! Selection state and track-list access.
//!
//! Selection is held per list: `Active` in `ViewData`, `Queue` and `Recent`
//! in `MusicPlayer`.

use super::{MusicPlayer, Track, QUEUE_PANE_ID, RECENT_PANE_ID};
use crate::app::{
    interaction::TrackPos,
    pane::{is_main_pane, PaneId},
    ViewKind,
};

impl MusicPlayer {
    pub fn selection_in(&self, pane: PaneId) -> &[usize] {
        match pane {
            QUEUE_PANE_ID => &self.queue_selected_indices,
            RECENT_PANE_ID => &self.recent_selected_indices,
            _ => &self.pane(pane).view_data().selection,
        }
    }

    /// Whether `index` is selected in the list addressed by `pane`. Selection
    /// vecs are kept sorted (see [`Self::toggle_selection`]), so this is a
    /// binary search.
    pub fn is_selected_in(&self, pane: PaneId, index: usize) -> bool {
        self.selection_in(pane).binary_search(&index).is_ok()
    }

    fn selection_mut_in(&mut self, pane: PaneId) -> &mut Vec<usize> {
        match pane {
            QUEUE_PANE_ID => &mut self.queue_selected_indices,
            RECENT_PANE_ID => &mut self.recent_selected_indices,
            _ => &mut self.pane_mut(pane).view_data_mut().selection,
        }
    }

    pub fn view_tracks_in(&self, pane: PaneId) -> &[Track] {
        let vd = self.view_data_in(pane);
        match &vd.kind {
            ViewKind::Playlist(entry) => self
                .playlists
                .playlists
                .get(entry.index)
                .map_or(&[], |p| &p.tracks),
            ViewKind::Trashbin => &self.trashbin.tracks,
            _ => vd.tracks(),
        }
    }

    pub fn is_trashed(&self, track: &Track) -> bool {
        self.trashbin.contains(track)
    }

    pub fn select_range_in(&mut self, pane: PaneId, from: usize, to: usize) {
        let sel = self.selection_mut_in(pane);
        sel.clear();
        sel.extend(from.min(to)..=from.max(to));
    }

    pub fn toggle_selection(&mut self, pos: TrackPos) {
        let sel = self.selection_mut_in(pos.pane);
        match sel.binary_search(&pos.index) {
            Ok(at) => {
                sel.remove(at);
            }
            Err(at) => {
                sel.insert(at, pos.index);
            }
        }
    }

    pub fn clear_selection(&mut self) {
        let panes: Vec<PaneId> = self.panes.keys().copied().collect();
        for pane in panes {
            self.clear_selection_in(pane);
        }
        self.clear_selection_in(QUEUE_PANE_ID);
        self.clear_selection_in(RECENT_PANE_ID);
    }

    pub fn clear_selection_in(&mut self, pane: PaneId) {
        self.selection_mut_in(pane).clear();
    }

    /// Whether any list currently holds a selection.
    pub fn has_selection(&self) -> bool {
        !self.selection_in(QUEUE_PANE_ID).is_empty()
            || !self.selection_in(RECENT_PANE_ID).is_empty()
            || self.panes.keys().any(|p| !self.selection_in(*p).is_empty())
    }

    /// Clear the selection of the list addressed by `pane` if any of `indices`
    /// was selected — used after a batch mutation (remove/delete) that leaves
    /// stale selection entries. Selections in the other lists are left
    /// untouched.
    pub fn clear_selection_if_touched_in(&mut self, pane: PaneId, indices: &[usize]) {
        let sel = self.selection_in(pane);
        if indices.iter().any(|&i| sel.binary_search(&i).is_ok()) {
            self.clear_selection_in(pane);
        }
    }

    pub(crate) fn handle_select_all(&mut self) {
        let pane = self
            .drag
            .hovered_track()
            .map_or(self.focused_pane_id, |h| h.pane);
        let count = self.track_count_in(pane);
        let sel = self.selection_mut_in(pane);
        sel.clear();
        sel.extend(super::pane_first_index(pane)..count);
    }

    pub fn get_track_ref_at(&self, pos: TrackPos) -> Option<&Track> {
        let TrackPos { index, pane } = pos;
        match pane {
            QUEUE_PANE_ID => self.queue.tracks.get(index),
            RECENT_PANE_ID => self.queue.recently_played.get(index),
            _ => self.view_tracks_in(pane).get(index),
        }
    }

    pub fn get_track_at(&self, pos: TrackPos) -> Option<Track> {
        self.get_track_ref_at(pos).cloned()
    }

    /// The hovered track when it belongs to the focused pane. Keyboard
    /// navigation always acts on the focused pane; a hover in another pane is
    /// ignored. Queue/Recent rows live in the global panel and always count.
    pub fn focused_hovered_track(&self) -> Option<TrackPos> {
        let pos = self.drag.hovered_track()?;
        if is_main_pane(pos.pane) && pos.pane != self.focused_pane_id {
            None
        } else {
            Some(pos)
        }
    }

    /// Whether `pos` is a match in the active track list search (any occurrence).
    pub fn is_track_list_match(&self, pos: TrackPos) -> bool {
        match &self.track_list_search {
            Some(fs) if fs.pane == pos.pane => fs.matches.contains(&pos.index),
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

    /// Counts the queue's now-playing entry at index 0, which the queue's
    /// first index skips.
    pub fn track_count_in(&self, pane: PaneId) -> usize {
        match pane {
            QUEUE_PANE_ID => self.queue.tracks.len(),
            RECENT_PANE_ID => self.queue.recently_played.len(),
            _ => self.view_tracks_in(pane).len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::view_data::ViewData, data::config, types::Track};

    fn player() -> MusicPlayer {
        // Same headless construction as the navigation tests: media controls no-op
        // without D-Bus, and nav history is reset to a deterministic Search
        // view (so `view_tracks` reads `view_data.tracks`).
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.reset_test_pane(vec![ViewData::new_search(
            String::new(),
            crate::providers::ProviderId::YouTube,
            crate::providers::SearchScope::Songs,
        )]);
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
        let pos0 = TrackPos::new(0, RECENT_PANE_ID);
        let pos1 = TrackPos::new(1, RECENT_PANE_ID);

        assert!(p.selection_in(RECENT_PANE_ID).is_empty());
        p.toggle_selection(pos0);
        assert_eq!(p.selection_in(RECENT_PANE_ID), &[0]);
        p.toggle_selection(pos1);
        assert_eq!(p.selection_in(RECENT_PANE_ID), &[0, 1]);
        p.toggle_selection(pos0);
        assert_eq!(p.selection_in(RECENT_PANE_ID), &[1]);
        p.clear_selection_in(RECENT_PANE_ID);
        assert!(p.selection_in(RECENT_PANE_ID).is_empty());
        p.clear_selection();
        assert!(p.selection_in(RECENT_PANE_ID).is_empty());
    }

    #[test]
    fn toggle_adds_then_removes_per_list() {
        let mut p = player();
        p.queue.tracks = vec![track("1"), track("2")];
        p.view_data_mut().set_tracks(vec![track("1")]);

        let q = TrackPos::new(0, QUEUE_PANE_ID);
        let a = TrackPos::new(0, p.focused_pane_id);
        p.toggle_selection(q);
        p.toggle_selection(a);
        assert_eq!(p.selection_in(QUEUE_PANE_ID), &[0]);
        assert_eq!(p.selection_in(p.focused_pane_id), &[0]);

        // Selections are scoped per list.
        assert_eq!(p.get_track_at(q).map(|t| t.title), Some("Track 1".into()));
        assert_eq!(p.get_track_at(a).map(|t| t.title), Some("Track 1".into()));

        p.toggle_selection(q);
        assert!(p.selection_in(QUEUE_PANE_ID).is_empty());
        assert_eq!(p.selection_in(p.focused_pane_id), &[0]);

        p.clear_selection();
        assert!(p.selection_in(p.focused_pane_id).is_empty());
    }
}
