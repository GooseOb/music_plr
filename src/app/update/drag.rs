use crate::app::ui::{QUEUE_LIST_ID, TRACK_LIST_ID};

use super::{DragTargetList, Message, MusicPlayer, Task, Track, DOUBLE_CLICK_MS};

impl MusicPlayer {
    fn cursor_in_sidebar(&self) -> bool {
        match self.sidebar_bounds {
            Some(b) => self.drag.cursor_pos.x < b.x + crate::theme::SIDEBAR_WIDTH,
            None => self.drag.cursor_pos.x < crate::theme::SIDEBAR_WIDTH,
        }
    }

    fn sidebar_playlist_at_cursor(&self) -> Option<usize> {
        if self.drag.cursor_pos.x >= crate::theme::SIDEBAR_WIDTH {
            return None;
        }
        let y_start = self
            .sidebar_bounds
            .map_or(crate::theme::SIDEBAR_PLAYLIST_LIST_OFFSET_Y, |b| b.y);
        let y_offset = self.drag.cursor_pos.y - y_start;
        if y_offset < 0.0 {
            return None;
        }
        let playlist_idx = ((y_offset + self.sidebar_list_scroll)
            / (crate::theme::SIDEBAR_ITEM_HEIGHT + 2.0)) as usize;
        if playlist_idx < self.playlists.playlists.len() {
            Some(playlist_idx)
        } else {
            None
        }
    }

    pub fn handle_left_release(&mut self) {
        if self.drag.drag_active {
            let is_queue = self.drag.pressed_track_is_queue;
            self.handle_drag_drop(is_queue);
        } else if let Some(track_idx) = self.drag.pressed_track {
            let is_queue = self.drag.pressed_track_is_queue;
            self.toggle_selection(track_idx, is_queue);
        }

        if self.show_search_history && !self.is_cursor_in_search_area() {
            self.show_search_history = false;
        } else if !self.show_search_history
            && self.search_input_bounds().contains(self.drag.cursor_pos)
            && !self.search_history.get().is_empty()
        {
            self.update_search_history();
            self.show_search_history = true;
        }

        self.cleanup_drag_state();
    }

    pub(super) fn cleanup_drag_state(&mut self) {
        self.drag.cleanup();
    }

    pub fn handle_drag_update(&mut self) -> Task<Message> {
        self.drag.sidebar_hover_playlist = None;
        self.drag.drag_drop_target = None;
        self.drag.drag_target_list = None;

        if self.cursor_in_sidebar() {
            self.drag.sidebar_hover_playlist = self.sidebar_playlist_at_cursor();
            return Task::none();
        }

        let is_source_queue = self.drag.pressed_track_is_queue;

        // Check if cursor is over the queue list (for cross-list copy to queue).
        if self.show_queue {
            if let (Some(qb), qs) = (self.queue_list_bounds, self.queue_list_scroll) {
                if qb.contains(self.drag.cursor_pos) {
                    self.drag.drag_target_list = Some(DragTargetList::Queue);
                    let drop_idx = self.compute_drop_idx(qb, qs, true, 1);
                    self.drag.drag_drop_target = Some(drop_idx);
                    let track_count = self.queue.tracks.len().saturating_sub(1);
                    return self.handle_drag_autoscroll(qb, qs, true, track_count);
                }
            }
        }

        // Check if cursor is over the current track list (for same-list reorder
        // or copy from queue to track list).
        if let (Some(lb), ls) = (
            self.get_current_list_bounds(),
            self.get_current_list_scroll(),
        ) {
            if lb.contains(self.drag.cursor_pos) {
                self.drag.drag_target_list = Some(DragTargetList::TrackList);
                let is_queue_target = false;
                let drop_idx = self.compute_drop_idx(lb, ls, is_queue_target, 0);
                self.drag.drag_drop_target = Some(drop_idx);

                // For same-list drags, prevent dropping onto a selected item
                // that would shift it (reorder guard). Skip this check for
                // cross-list copies where every drop position is valid.
                if is_source_queue == is_queue_target {
                    let sel = self.selection(is_queue_target).to_vec();
                    if let (Some(min), Some(max)) =
                        (sel.iter().copied().min(), sel.iter().copied().max())
                    {
                        if drop_idx > min && drop_idx < max {
                            self.drag.drag_drop_target = None;
                            self.drag.drag_target_list = None;
                            return Task::none();
                        }
                    }
                }

                let track_count = self.current_track_count(false);
                return self.handle_drag_autoscroll(lb, ls, is_queue_target, track_count);
            }
        }

        Task::none()
    }

    /// Compute the drop index (in list-relative terms, i.e. excluding the
    /// "now playing" offset for the queue) given the list bounds, scroll,
    /// whether it's the queue list, and the drag offset (index of the first
    /// item in the visible list).
    fn compute_drop_idx(
        &self,
        list_bounds: iced::Rectangle,
        list_scroll: f32,
        is_queue: bool,
        drag_offset: usize,
    ) -> usize {
        let y_offset = self.drag.cursor_pos.y - list_bounds.y;
        let row_pos = ((y_offset + list_scroll) / crate::theme::ROW_HEIGHT).max(0.0);
        let row_idx = row_pos as usize;
        let track_count = if is_queue {
            self.queue.tracks.len().saturating_sub(1)
        } else {
            self.current_track_count(false)
        };
        let drop_idx = if row_idx < track_count && row_pos.fract() >= 0.5 {
            row_idx + 1
        } else {
            row_idx
        };
        let drop_idx = drop_idx.min(track_count);
        drop_idx + drag_offset
    }

    fn handle_drag_autoscroll(
        &self,
        list_bounds: iced::Rectangle,
        current_scroll: f32,
        is_queue: bool,
        track_count: usize,
    ) -> Task<Message> {
        let cursor = self.drag.cursor_pos;
        let y_offset = cursor.y - list_bounds.y;
        let list_height = list_bounds.height;

        let total_height = track_count as f32 * crate::theme::ROW_HEIGHT;
        let max_scroll = (total_height - list_height).max(0.0);

        if max_scroll <= 0.0 {
            return Task::none();
        }

        let edge_zone = crate::theme::DRAG_AUTO_SCROLL_ZONE;
        let scroll_speed = crate::theme::DRAG_AUTO_SCROLL_SPEED;

        let scroll_amount = if y_offset < edge_zone {
            -scroll_speed
        } else if y_offset > list_height - edge_zone {
            scroll_speed
        } else {
            0.0
        };

        if scroll_amount == 0.0 {
            return Task::none();
        }

        let new_scroll = (current_scroll + scroll_amount).clamp(0.0, max_scroll);
        if (new_scroll - current_scroll).abs() < 0.1 {
            return Task::none();
        }

        let list_id = if is_queue {
            QUEUE_LIST_ID
        } else {
            TRACK_LIST_ID
        };

        iced::widget::operation::scroll_by::<Message>(
            list_id,
            iced::widget::operation::AbsoluteOffset {
                x: 0.0,
                y: scroll_amount,
            },
        )
    }

    pub fn handle_drag_drop(&mut self, is_queue: bool) {
        let Some(track_idx) = self.drag.pressed_track else {
            return;
        };

        let was_in_selection = {
            let sel = self.selection(is_queue);
            !sel.is_empty() && sel.contains(&track_idx)
        };

        let indices: Vec<usize> = if was_in_selection {
            self.selection(is_queue).to_vec()
        } else {
            vec![track_idx]
        };

        if self.cursor_in_sidebar() {
            if let Some(playlist_idx) = self.sidebar_playlist_at_cursor() {
                let tracks: Vec<Track> = indices
                    .iter()
                    .rev()
                    .filter_map(|&i| self.get_track_at(i, is_queue))
                    .collect();
                let count = tracks.len();
                if count > 0 {
                    self.playlists.insert_tracks_at(playlist_idx, &tracks, 0);
                }
                let name = self.playlists.playlists[playlist_idx].name.clone();
                self.notify_tracks("Added", count, &format!("to {name}"));
                return;
            }
        }

        let Some(drop_idx) = self.drag.drag_drop_target else {
            return;
        };

        // Determine if this is a cross-list copy or a same-list reorder.
        let target = self.drag.drag_target_list;
        let source_is_queue = is_queue;

        match target {
            Some(DragTargetList::Queue) if !source_is_queue => {
                self.copy_to_queue(&indices, drop_idx);
            }
            Some(DragTargetList::TrackList) if source_is_queue => {
                self.copy_from_queue(&indices, drop_idx);
            }
            Some(_) if Self::target_is_same_as_source(target, source_is_queue) => {
                self.handle_same_list_reorder(drop_idx, &indices, source_is_queue);
            }
            _ => {}
        }
    }

    /// Returns true when the drag target list is the same as the source list.
    const fn target_is_same_as_source(
        target: Option<DragTargetList>,
        source_is_queue: bool,
    ) -> bool {
        match target {
            Some(DragTargetList::Queue) => source_is_queue,
            Some(DragTargetList::TrackList) => !source_is_queue,
            None => false,
        }
    }

    /// Insert tracks from the current (non-queue) track list into the queue
    /// at the given drop index. `indices` are positions in the source list.
    fn copy_to_queue(&mut self, indices: &[usize], drop_idx: usize) {
        let clamped = drop_idx.min(self.queue.tracks.len());
        let tracks: Vec<Track> = indices
            .iter()
            .filter_map(|&i| self.get_track_at(i, false))
            .collect();
        let inserted = tracks.len();
        for (j, track) in tracks.into_iter().enumerate() {
            self.queue.tracks.insert(clamped + j, track);
        }
        self.save_session();
        self.notify_tracks("Added", inserted, "to queue");
    }

    /// Insert tracks from the queue into the current playlist at the given
    /// drop index. `indices` are positions in the queue's up-next list
    /// (starting after index 0, i.e. the current track).
    fn copy_from_queue(&mut self, indices: &[usize], drop_idx: usize) {
        let Some(sp) = self.view_data_mut().selected_playlist_id() else {
            if !self.view_data_mut().is_search_like() {
                self.notify("Select a playlist to drop tracks into".into());
            }
            return;
        };
        if sp >= self.playlists.playlists.len() {
            return;
        }

        let clamped = drop_idx.min(self.playlists.playlists[sp].tracks.len());
        let tracks: Vec<Track> = indices
            .iter()
            .filter_map(|&queue_idx| self.queue.tracks.get(queue_idx).cloned())
            .collect();
        let inserted = tracks.len();
        if inserted > 0 {
            self.playlists.insert_tracks_at(sp, &tracks, clamped);
        }
        self.save_session();
        let name = self.playlists.playlists[sp].name.clone();
        self.notify_tracks("Added", inserted, &format!("to {name}"));
    }

    /// Handle reordering within the same list. The selection is always
    /// remapped to reflect the new positions of all selected tracks — both
    /// the moved ones and any that merely shifted.
    fn handle_same_list_reorder(
        &mut self,
        drop_idx: usize,
        indices: &[usize],
        source_is_queue: bool,
    ) {
        let min_idx = *indices.iter().min().unwrap();
        let max_idx = *indices.iter().max().unwrap();
        let is_valid_drop = drop_idx > max_idx || drop_idx < min_idx;

        if source_is_queue {
            let count = self.queue.tracks.len();
            if drop_idx <= count && is_valid_drop {
                let selection = self.queue_selected_indices.clone();
                let new_positions = self.handle_reorder_queue(drop_idx, indices, &selection);
                self.queue_selected_indices = new_positions;
                self.save_session();
            }
        } else {
            let count = self.current_track_count(false);
            if drop_idx <= count && is_valid_drop {
                let selection = self.view_data_mut().selection.clone();
                let new_positions =
                    self.handle_reorder_tracks_selected(drop_idx, indices, &selection);
                self.view_data_mut().selection = new_positions;
            }
        }
    }

    pub fn handle_track_pressed(&mut self, index: usize, is_queue: bool) {
        let now = std::time::Instant::now();
        let is_double = self.last_click_index == Some(index)
            && now.duration_since(self.last_click_time).as_millis() < DOUBLE_CLICK_MS;

        self.last_click_index = Some(index);
        self.last_click_time = now;
        self.drag.pressed_track = Some(index);
        self.drag.pressed_track_is_queue = is_queue;
        self.drag.drag_origin = Some(self.drag.cursor_pos);
        self.drag.drag_active = false;

        if is_double {
            self.drag.pressed_track = None;
            self.drag.drag_origin = None;
            self.handle_play_track(index, is_queue);
            self.toggle_selection(index, is_queue);
        }
    }
}
