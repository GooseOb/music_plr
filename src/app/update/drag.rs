use super::{
    operation::{LIBRARY_LIST_ID, SIDEBAR_LIST_ID},
    BackendResult, Message, MusicPlayer, Task, Track, TrackListKind, TrackPos, ViewData,
    DOUBLE_CLICK_MS,
};
use crate::app::interaction::DropTarget;
use crate::data::library::{LibraryItem, LibraryKind};

impl MusicPlayer {
    fn cursor_in_sidebar(&self) -> bool {
        match &self.bounds.sidebar {
            Some(b) => self.drag.cursor_pos.x < b.bounds.x + crate::theme::SIDEBAR_WIDTH,
            None => self.drag.cursor_pos.x < crate::theme::SIDEBAR_WIDTH,
        }
    }

    fn sidebar_playlist_at_cursor(&self) -> Option<usize> {
        if self.drag.cursor_pos.x >= crate::theme::SIDEBAR_WIDTH {
            return None;
        }
        let sidebar = self.bounds.sidebar.as_ref()?;
        let cursor_y = self.drag.cursor_pos.y + sidebar.translation_y;
        let mut best: Option<(f32, usize)> = None;
        for (i, row) in sidebar.rows.iter().enumerate() {
            let d = (cursor_y - (row.y + row.height / 2.0)).abs();
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, i));
            }
        }
        best.map(|(_, idx)| idx)
            .filter(|&idx| idx < self.playlists.playlists.len())
    }

    pub fn handle_left_release(&mut self) {
        if let Some(item) = self.drag.take_pressed_card() {
            // A card press: a drag onto a sidebar list becomes a playlist (or
            // adds/reorders in the library); a plain click drills down.
            if self.drag.drag_active {
                if let Some(target) = self.card_drop_target() {
                    self.handle_card_drop(item, target);
                }
            } else {
                self.open_library_item(item);
            }
            self.cleanup_drag_state();
            return;
        }

        let Some(pressed) = self.drag.pressed_track() else {
            self.cleanup_drag_state();
            return;
        };
        if self.drag.drag_active {
            self.handle_drag_drop(pressed.list);
        } else {
            self.toggle_selection(pressed);
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
        if matches!(
            self.drag.hovered,
            Some(crate::app::interaction::HoverTarget::SidebarPlaylist(_))
        ) {
            self.drag.hovered = None;
        }
        self.drag.drop_target = None;

        // Card drags resolve to a sidebar list (playlists or library) rather
        // than the track lists, so they're handled entirely here. Edge
        // autoscroll still applies to whichever sidebar list is under the
        // cursor.
        if self.drag.is_pressed_card() {
            self.drag.drop_target = self.card_drop_target();
            return self.sidebar_autoscroll();
        }

        if self.cursor_in_sidebar() {
            if let Some(idx) = self.sidebar_playlist_at_cursor() {
                self.drag
                    .set_hovered(crate::app::interaction::HoverTarget::SidebarPlaylist(idx));
            }
            return self.sidebar_autoscroll();
        }

        let Some(source) = self.drag.pressed_track().map(|p| p.list) else {
            return Task::none();
        };

        // Check if cursor is over the queue list (for cross-list copy to queue).
        if self.show_queue {
            if let Some(q) = &self.bounds.queue {
                if q.bounds.contains(self.drag.cursor_pos) {
                    let target = TrackListKind::Queue;
                    let drop_idx = self.compute_drop_idx(target);
                    self.drag.drop_target =
                        Some(DropTarget::Track(TrackPos::new(drop_idx, target)));
                    return self.handle_drag_autoscroll(
                        q.bounds,
                        q.translation_y,
                        q.content_height,
                        target.scrollable_id(),
                    );
                }
            }
        }

        // Check if cursor is over the current track list (for same-list reorder
        // or copy from queue to track list). Track bounds come from the live
        // capture; the scroll offset stays on `view_data` (persisted per slot).
        if let Some(t) = &self.bounds.track {
            let lb = t.bounds;
            if lb.contains(self.drag.cursor_pos) {
                let target = TrackListKind::Active;
                let drop_idx = self.compute_drop_idx(target);
                self.drag.drop_target = Some(DropTarget::Track(TrackPos::new(drop_idx, target)));

                // For same-list drags, prevent dropping onto a selected item
                // that would shift it (reorder guard). Skip this check for
                // cross-list copies where every drop position is valid.
                if source == target {
                    let sel = self.selection(target).to_vec();
                    if let (Some(min), Some(max)) =
                        (sel.iter().copied().min(), sel.iter().copied().max())
                    {
                        if drop_idx > min && drop_idx < max {
                            self.drag.drop_target = None;
                            return Task::none();
                        }
                    }
                }

                return self.handle_drag_autoscroll(
                    lb,
                    t.translation_y,
                    t.content_height,
                    target.scrollable_id(),
                );
            }
        }

        Task::none()
    }

    /// Returns a drop index in `list`'s own space (the queue's now-playing
    /// offset folded back in), found by comparing the cursor against the
    /// measured bounds of each row rather than assuming a fixed row height.
    fn compute_drop_idx(&self, list: TrackListKind) -> usize {
        let geo = match list {
            TrackListKind::Queue | TrackListKind::Recent => &self.bounds.queue,
            TrackListKind::Active => &self.bounds.track,
        };
        let Some(geo) = geo else {
            return list.first_index();
        };
        // Rows are captured in the scrollable's untranslated content space, so
        // convert the screen-space cursor into that space before comparing.
        let cursor_y = self.drag.cursor_pos.y + geo.translation_y;
        for (i, row) in geo.rows.iter().enumerate() {
            if cursor_y < row.y + row.height / 2.0 {
                return i + list.first_index();
            }
        }
        geo.rows.len() + list.first_index()
    }

    /// Edge autoscroll for whichever sidebar list (playlist or library) is
    /// under the cursor while a drag is active.
    fn sidebar_autoscroll(&self) -> Task<Message> {
        let cursor = self.drag.cursor_pos;
        if let Some(geo) = &self.bounds.sidebar {
            if geo.bounds.contains(cursor) {
                return self.handle_drag_autoscroll(
                    geo.bounds,
                    geo.translation_y,
                    geo.content_height,
                    SIDEBAR_LIST_ID,
                );
            }
        }
        if let Some(geo) = &self.bounds.library {
            if geo.bounds.contains(cursor) {
                return self.handle_drag_autoscroll(
                    geo.bounds,
                    geo.translation_y,
                    geo.content_height,
                    LIBRARY_LIST_ID,
                );
            }
        }
        Task::none()
    }

    fn handle_drag_autoscroll(
        &self,
        bounds: iced::Rectangle,
        current_scroll: f32,
        content_height: f32,
        scrollable_id: iced::widget::Id,
    ) -> Task<Message> {
        let cursor = self.drag.cursor_pos;
        let y_offset = cursor.y - bounds.y;
        let list_height = bounds.height;

        let max_scroll = (content_height - list_height).max(0.0);

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

        iced::widget::operation::scroll_by::<Message>(
            scrollable_id,
            iced::widget::operation::AbsoluteOffset {
                x: 0.0,
                y: scroll_amount,
            },
        )
    }

    /// Screen-space rectangle of the single drop indicator, derived from
    /// captured row geometry (rather than an injected row, so the list layout
    /// is never perturbed). `None` when there is no active drop target or it
    /// falls outside the visible viewport. A drag targets at most one list at
    /// a time: a card drag targets a sidebar list (`DropTarget::Playlist` /
    /// `DropTarget::Library`), while a track drag targets the queue/active list
    /// (`DropTarget::Track`), so
    /// there is never more than one indicator.
    pub fn drop_indicator_rect(&self) -> Option<iced::Rectangle> {
        // Resolve to the targeted geometry and the 0-based insertion index
        // within its captured rows.
        let (geo, rel) = match self.drag.drop_target {
            Some(DropTarget::Playlist(i)) => (self.bounds.sidebar.as_ref()?, i),
            Some(DropTarget::Library(i)) => (self.bounds.library.as_ref()?, i),
            Some(DropTarget::Track(pos)) => {
                let geo = match pos.list {
                    TrackListKind::Queue => self.bounds.queue.as_ref()?,
                    TrackListKind::Active => self.bounds.track.as_ref()?,
                    TrackListKind::Recent => return None,
                };
                (geo, pos.index.saturating_sub(pos.list.first_index()))
            }
            None => return None,
        };

        let rows = &geo.rows;
        if rows.is_empty() {
            return None;
        }
        let boundary_y = if rel == 0 {
            rows[0].y - geo.translation_y
        } else if rel <= rows.len() {
            let k = rel - 1;
            rows[k].y + rows[k].height - geo.translation_y
        } else {
            let last = rows.last().unwrap();
            last.y + last.height - geo.translation_y
        };
        if boundary_y < geo.bounds.y || boundary_y > geo.bounds.y + geo.bounds.height {
            return None;
        }
        Some(iced::Rectangle {
            x: geo.bounds.x,
            y: boundary_y - crate::theme::DROP_LINE_HEIGHT / 2.0,
            width: geo.bounds.width,
            height: crate::theme::DROP_LINE_HEIGHT,
        })
    }

    pub fn handle_drag_drop(&mut self, source: TrackListKind) {
        let Some(pressed) = self.drag.pressed_track() else {
            return;
        };
        let track_idx = pressed.index;

        let was_in_selection = {
            let sel = self.selection(source);
            !sel.is_empty() && sel.contains(&track_idx)
        };

        let indices: Vec<usize> = if was_in_selection {
            self.selection(source).to_vec()
        } else {
            vec![track_idx]
        };

        if self.cursor_in_sidebar() {
            if let Some(playlist_idx) = self.sidebar_playlist_at_cursor() {
                let tracks: Vec<Track> = indices
                    .iter()
                    .rev()
                    .filter_map(|&i| self.get_track_at(TrackPos::new(i, source)))
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

        let Some(DropTarget::Track(pos)) = self.drag.drop_target else {
            return;
        };
        let drop_idx = pos.index;

        // Determine if this is a cross-list copy or a same-list reorder.
        match pos.list {
            target if target == source => {
                self.handle_same_list_reorder(drop_idx, &indices, source);
            }
            TrackListKind::Queue => self.copy_to_queue(&indices, drop_idx),
            TrackListKind::Active => self.copy_from_queue(&indices, drop_idx),
            TrackListKind::Recent => {}
        }
    }

    fn copy_to_queue(&mut self, indices: &[usize], drop_idx: usize) {
        let clamped = drop_idx.min(self.queue.tracks.len());
        let tracks: Vec<Track> = indices
            .iter()
            .filter_map(|&i| self.get_track_at(TrackPos::new(i, TrackListKind::Active)))
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
                self.notify("Select a playlist to drop tracks into");
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
        source: TrackListKind,
    ) {
        let min_idx = *indices.iter().min().unwrap();
        let max_idx = *indices.iter().max().unwrap();
        let is_valid_drop = drop_idx > max_idx || drop_idx < min_idx;
        if drop_idx > self.track_count(source) || !is_valid_drop {
            return;
        }

        match source {
            TrackListKind::Queue => {
                let selection = self.queue_selected_indices.clone();
                self.queue_selected_indices =
                    self.handle_reorder_queue(drop_idx, indices, &selection);
                self.save_session();
            }
            TrackListKind::Active => {
                let selection = self.view_data_mut().selection.clone();
                self.view_data_mut().selection =
                    self.handle_reorder_tracks_selected(drop_idx, indices, &selection);
            }
            TrackListKind::Recent => {}
        }
    }

    /// Resolve where a dragged card would land, given the current cursor: the
    /// playlist list (insert a new playlist at an index) or the library list
    /// (insert/reorder a saved item). Returns `None` when the cursor isn't
    /// over either sidebar list.
    fn card_drop_target(&self) -> Option<DropTarget> {
        if let Some(pl) = &self.bounds.sidebar {
            if pl.bounds.contains(self.drag.cursor_pos) {
                let count = self.playlists.playlists.len();
                return Some(DropTarget::Playlist(
                    self.sidebar_insertion_index(pl, count),
                ));
            }
        }
        if self.cursor_in_sidebar() {
            if let Some(lib) = &self.bounds.library {
                if lib.bounds.contains(self.drag.cursor_pos) {
                    let count = self.library.items.len();
                    return Some(DropTarget::Library(
                        self.sidebar_insertion_index(lib, count),
                    ));
                }
            }
            // Collapsed (or not-yet-captured) library: the lower sidebar is
            // the library, so append there.
            if !self.library_expanded || self.bounds.library.is_none() {
                return Some(DropTarget::Library(self.library.items.len()));
            }
        }
        None
    }

    /// The index within a sidebar list where a drop would insert, given the
    /// cursor's position relative to the list's measured row bounds.
    fn sidebar_insertion_index(
        &self,
        geo: &crate::app::update::operation::ListGeometry,
        count: usize,
    ) -> usize {
        let cursor_y = self.drag.cursor_pos.y + geo.translation_y;
        for (i, row) in geo.rows.iter().enumerate() {
            if cursor_y < row.y + row.height / 2.0 {
                return i;
            }
        }
        geo.rows.len().min(count)
    }

    fn handle_card_drop(&mut self, item: LibraryItem, target: DropTarget) {
        match target {
            DropTarget::Playlist(idx) => self.create_playlist_from_card(&item, idx),
            DropTarget::Library(idx) => {
                if self.library.contains(item.kind, &item.id) {
                    if let Some(from) = self
                        .library
                        .items
                        .iter()
                        .position(|it| it.kind == item.kind && it.id == item.id)
                    {
                        // Adjust for the removed row so the indicator position
                        // maps to the post-removal list.
                        let to = if idx > from { idx - 1 } else { idx };
                        self.library.move_item(from, to);
                    }
                } else {
                    let title = item.title.clone();
                    self.library.insert(item, idx);
                    self.notify(format!("Saved \"{title}\" to library"));
                }
            }
            DropTarget::Track(_) => {}
        }
    }

    /// Create a local playlist from a dragged card at `insert_at`: make a new
    /// uniquely-named playlist, then fetch its contents in the background and
    /// fill it once they arrive.
    fn create_playlist_from_card(&mut self, item: &LibraryItem, insert_at: usize) {
        let idx = self.playlists.create_at(&item.title, insert_at);
        let name = self.playlists.playlists[idx].name.clone();

        let kind_str = match item.kind {
            LibraryKind::Artist => "artist",
            LibraryKind::Album => "album",
            LibraryKind::Playlist => "playlist",
        };
        let browse_id = item.id.clone();
        let name_for_thread = name.clone();
        let tx = self.result_tx.clone();
        self.notify(format!("Creating playlist \"{name}\"..."));
        Self::spawn_backend_thread(
            move || {
                crate::youtube::browse(&browse_id, kind_str)
                    .map(|videos| videos.into_iter().map(Track::from).collect())
            },
            move |tracks| BackendResult::CardPlaylistReady(idx, name_for_thread, tracks),
            tx,
        );

        // Jump to the freshly created (initially empty) playlist view; tracks
        // stream in as the browse result arrives. Select it directly (rather
        // than `handle_select_playlist`) so it activates even when the drop
        // index coincides with the currently selected playlist.
        let name = self.playlists.playlists[idx].name.clone();
        self.push_new_view(ViewData::new_playlist(Some(idx), name, None));
        self.save_session();
    }

    /// Open a card the same way its drill-down message would.
    fn open_library_item(&mut self, item: LibraryItem) {
        match item.kind {
            LibraryKind::Artist => self.handle_open_artist(item.id, &item.title),
            LibraryKind::Album => self.handle_open_album(item.id, &item.title),
            LibraryKind::Playlist => self.handle_open_playlist(item.id, &item.title),
        }
    }

    /// Arm a card (artist/album/playlist) for dragging. A release without a
    /// drag opens the card; a drag onto the playlist list converts it to a
    /// local playlist, and a drag onto the library adds/reorders it.
    pub fn handle_card_pressed(&mut self, item: LibraryItem) {
        self.drag.set_pressed_card(item);
        self.drag.drag_origin = Some(self.drag.cursor_pos);
        self.drag.drag_active = false;
    }

    pub fn handle_track_pressed(&mut self, pos: TrackPos) {
        let now = std::time::Instant::now();
        // Comparing the full position, not just the index, keeps a click in
        // one list from completing a double-click begun in another.
        let is_double = self.last_click == Some(pos)
            && now.duration_since(self.last_click_time).as_millis() < DOUBLE_CLICK_MS;

        self.last_click = Some(pos);
        self.last_click_time = now;
        // Read-only lists neither select nor drag, so they never arm the
        // press state that `CursorMoved` promotes into a drag.
        if pos.list.is_interactive() {
            self.drag.set_pressed_track(pos);
            self.drag.drag_origin = Some(self.drag.cursor_pos);
        }
        self.drag.drag_active = false;

        if is_double {
            self.drag.pressed = None;
            self.drag.drag_origin = None;
            self.handle_play_track(pos);
            self.toggle_selection(pos);
        }
    }
}
