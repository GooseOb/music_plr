use super::{
    operation::{ListGeometry, LIBRARY_LIST_ID, SIDEBAR_LIST_ID},
    BackendResult, Message, MusicPlayer, Task, Track, TrackListKind, TrackPos, ViewData,
    DOUBLE_CLICK_MS,
};
use crate::{
    app::{
        interaction::{DropTarget, Pressed},
        ui::{QUEUE_LIST_ID, QUEUE_RECENT_LIST_ID, TRACK_LIST_ID},
        ViewKind,
    },
    data::{
        library::{LibraryItem, LibraryKind},
        JsonStore,
    },
};
use iced::widget::Id;

impl MusicPlayer {
    pub fn handle_left_release(&mut self) -> Task<Message> {
        let cursor = self.drag.cursor_pos;
        let is_cursor_in_search_input =
            self.bounds.search_input.is_some_and(|r| r.contains(cursor));
        if self.show_search_history {
            if !is_cursor_in_search_input {
                self.show_search_history = false;
                self.drag.clear_hovered_search_history();
            }
        } else if is_cursor_in_search_input {
            return self.activate_search_input();
        }

        let Some(pressed) = self.drag.pressed.take() else {
            self.drag.stop();
            return Task::none();
        };

        if self.drag.drag_active {
            match pressed {
                Pressed::Track(pos) => self.handle_track_drop(pos),
                Pressed::Card(item) => {
                    if let Some(target) = self
                        .card_drop_target(self.bounds.get_containing(self.drag.cursor_pos).as_ref())
                    {
                        self.handle_card_drop(item, target);
                    }
                }
                Pressed::Playlist(_) => self.handle_playlist_drop(),
            }
        } else {
            match pressed {
                Pressed::Track(pos) => {
                    self.toggle_selection(pos);
                }
                Pressed::Card(item) => {
                    let provider = item.provider;
                    if item.kind == crate::data::library::LibraryKind::Artist {
                        self.open_artist(&item.id, &item.title, provider);
                    } else {
                        self.handle_browse(&item.into(), provider);
                    }
                }
                // A click without a drag selects the playlist (mirrors how a
                // library card opens on a plain click).
                Pressed::Playlist(i) => self.handle_select_playlist(i),
            }
        }

        self.drag.stop();
        Task::none()
    }

    pub fn handle_drag_update(&mut self) -> Task<Message> {
        self.drag.drop_target = None;

        let Some(pressed) = &self.drag.pressed else {
            return Task::none();
        };

        let containing = self.bounds.get_containing(self.drag.cursor_pos);

        match pressed {
            Pressed::Card(_) => {
                self.drag.drop_target = self.card_drop_target(containing.as_ref());
            }
            Pressed::Track(pos) => {
                self.drag.drop_target = self.resolve_track_drop(*pos, containing.as_ref());
            }
            Pressed::Playlist(from) => {
                self.drag.drop_target = self.resolve_playlist_drop(*from, containing.as_ref());
            }
        }

        let Some((target_id, geo)) = containing else {
            return Task::none();
        };

        let count = if target_id == QUEUE_LIST_ID {
            self.track_count(TrackListKind::Queue)
        } else if target_id == TRACK_LIST_ID {
            self.track_count(TrackListKind::Active)
        } else {
            return Task::none();
        };
        let content_height = count as f32 * crate::theme::ROW_HEIGHT;
        self.handle_drag_autoscroll(geo.bounds, geo.translation_y, content_height, target_id)
    }

    /// Resolve a playlist-row reorder: the press must be over the sidebar
    /// playlist list, and the resolved insertion index must differ from the
    /// row's current position (a drop onto itself is a no-op).
    fn resolve_playlist_drop(
        &self,
        from: usize,
        containing: Option<&(Id, &ListGeometry)>,
    ) -> Option<DropTarget> {
        let (id, geo) = containing?;
        if *id != SIDEBAR_LIST_ID {
            return None;
        }
        let count = self.playlists.playlists.len();
        if from >= count {
            return None;
        }
        let to = self.sidebar_insertion_index(geo, count);
        // Dragging a row onto its own slot (or the gap just below it) is a
        // no-op.
        if to == from || to == from + 1 {
            return None;
        }
        Some(DropTarget::PlaylistReorder { from, to })
    }

    fn resolve_track_drop(
        &self,
        source: TrackPos,
        containing: Option<&(Id, &ListGeometry)>,
    ) -> Option<DropTarget> {
        let (id, geo) = containing?;
        let list = if *id == QUEUE_LIST_ID {
            TrackListKind::Queue
        } else if *id == TRACK_LIST_ID {
            TrackListKind::Active
        } else if *id == QUEUE_RECENT_LIST_ID {
            TrackListKind::Recent
        } else if *id == SIDEBAR_LIST_ID {
            let count = self.playlists.playlists.len();
            let cursor_y = self.drag.cursor_pos.y + geo.translation_y;
            let idx = nearest_row_index(&geo.rows, cursor_y).filter(|&idx| idx < count)?;
            return Some(DropTarget::PlaylistAdd(idx));
        } else {
            return None;
        };
        // Only local playlists and queue can be dropped onto
        if list == TrackListKind::Active && !matches!(&self.view_data().kind, ViewKind::Playlist(_))
        {
            return None;
        }
        let drop_idx = self.compute_drop_idx(list);
        // Same-list reorder guard: never drop onto a selected run that
        // would merely shift it. Cross-list copies are unguarded.
        if source.list == TrackListKind::Active {
            let sel = self.selection(TrackListKind::Active).to_vec();
            if let (Some(min), Some(max)) = (sel.iter().copied().min(), sel.iter().copied().max()) {
                if drop_idx > min && drop_idx < max {
                    return None;
                }
            }
        }
        Some(DropTarget::Track(TrackPos::new(drop_idx, list)))
    }

    fn compute_drop_idx(&self, list: TrackListKind) -> usize {
        let geo = match list {
            TrackListKind::Queue | TrackListKind::Recent => &self.bounds.queue,
            TrackListKind::Active => &self.bounds.track,
        };
        let Some(geo) = geo else {
            return list.first_index();
        };
        let first = list.first_index();
        let count = self.track_count(list);
        // Cursor position in the scrollable's content space (y = 0 at the top
        // of the first row).
        let cursor_y = (self.drag.cursor_pos.y - geo.bounds.y) + geo.translation_y;
        drop_index_from_cursor(cursor_y, first, count)
    }

    fn handle_drag_autoscroll(
        &self,
        bounds: iced::Rectangle,
        current_scroll: f32,
        content_height: f32,
        scrollable_id: Id,
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
    /// captured row geometry rather than an injected row (so the list layout
    /// is never perturbed). `None` when there is no drop target drawing an
    /// insertion line (`PlaylistAdd` highlights a row instead) or when the
    /// boundary falls outside the visible viewport.
    pub fn drop_indicator_rect(&self) -> Option<iced::Rectangle> {
        // Resolve to the targeted geometry and the 0-based insertion index
        // within its captured rows.
        let (geo, rel) = match self.drag.drop_target {
            Some(DropTarget::Playlist(i)) => (self.bounds.sidebar.as_ref()?, i),
            Some(DropTarget::Library(i)) => (self.bounds.library.as_ref()?, i),
            Some(DropTarget::PlaylistReorder { to, .. }) => (self.bounds.sidebar.as_ref()?, to),
            Some(DropTarget::Track(pos)) => {
                let geo = match pos.list {
                    TrackListKind::Queue => self.bounds.queue.as_ref()?,
                    TrackListKind::Active => self.bounds.track.as_ref()?,
                    TrackListKind::Recent => return None,
                };
                (geo, pos.index.saturating_sub(pos.list.first_index()))
            }
            // A track dropped on the playlist list highlights the target row
            // (handled in the sidebar view) rather than drawing an insertion
            // line, so it produces no indicator rect.
            Some(DropTarget::PlaylistAdd(_)) | None => return None,
        };

        // The queue/active lists are virtualized, so `geo.rows` is intentionally
        // empty for them; their boundary comes from the uniform `ROW_HEIGHT`.
        // Sidebar/library lists are not virtualized, so their measured `rows`
        // must be present for the drop math below.
        let is_track = matches!(self.drag.drop_target, Some(DropTarget::Track(_)));
        let rows = &geo.rows;
        if !is_track && rows.is_empty() {
            return None;
        }
        let boundary_y = if is_track {
            geo.bounds.y - geo.translation_y + rel as f32 * crate::theme::ROW_HEIGHT
        } else if rel == 0 {
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

    pub fn handle_track_drop(&mut self, pos: TrackPos) {
        let source = pos.list;
        let indices = self.dragged_indices(source).to_vec();

        // Dropped on the playlist sidebar: add to that playlist (prepend). The
        // target was resolved during the drag into `drop_target` and is shown
        // by the row highlight, so there is no separate insertion bar.
        if let Some(DropTarget::PlaylistAdd(playlist_idx)) = self.drag.drop_target {
            let tracks: Vec<Track> = indices
                .iter()
                .rev()
                .filter_map(|&i| self.get_track_at(TrackPos::new(i, source)))
                .collect();
            let count = self
                .playlists
                .insert_tracks_at(playlist_idx, tracks.iter(), 0);
            let name = self.playlists.playlists[playlist_idx].name.clone();
            self.notify_tracks("Added", count, &format!("to {name}"));
            return;
        }

        let Some(DropTarget::Track(drop)) = self.drag.drop_target else {
            return;
        };
        let drop_idx = drop.index;

        // Determine if this is a cross-list copy or a same-list reorder.
        match drop.list {
            target if target == source => {
                self.handle_same_list_reorder(drop_idx, &indices, source);
            }
            TrackListKind::Queue => self.copy_to_queue(source, &indices, drop_idx),
            TrackListKind::Active => self.copy_from_queue(source, &indices, drop_idx),
            TrackListKind::Recent => {}
        }
    }

    pub fn dragged_indices(&self, list: TrackListKind) -> &[usize] {
        match &self.drag.dragged {
            Some((drag_list, indices)) if *drag_list == list => indices,
            _ => &[],
        }
    }

    pub fn is_dragging_track(&self, pos: TrackPos) -> bool {
        self.dragged_indices(pos.list).contains(&pos.index)
    }

    fn copy_to_queue(&mut self, source: TrackListKind, indices: &[usize], drop_idx: usize) {
        let clamped = drop_idx.min(self.queue.tracks.len());
        let tracks: Vec<Track> = indices
            .iter()
            .filter_map(|&i| self.get_track_at(TrackPos::new(i, source)))
            .collect();
        let inserted = tracks.len();
        for (j, track) in tracks.into_iter().enumerate() {
            self.queue.tracks.insert(clamped + j, track);
        }
        self.save_session();
        self.notify_tracks("Added", inserted, "to queue");
    }

    /// Insert tracks from `source` into the current playlist at the given
    /// drop index. `indices` are positions in the source list.
    fn copy_from_queue(&mut self, source: TrackListKind, indices: &[usize], drop_idx: usize) {
        let active = match &self.view_data().kind {
            ViewKind::Playlist(p) => Some(p.index),
            _ => None,
        };
        let Some(sp) = active else {
            if !self.view_data().is_search_like() {
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
            .filter_map(|&i| self.get_track_at(TrackPos::new(i, source)))
            .collect();
        let inserted = self.playlists.insert_tracks_at(sp, tracks.iter(), clamped);
        self.save_session();
        let name = self.playlists.playlists[sp].name.clone();
        if inserted > 0 {
            self.notify_tracks("Added", inserted, &format!("to {name}"));
        }
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
        let (Some(min_idx), Some(max_idx)) = (indices.iter().min(), indices.iter().max()) else {
            return;
        };
        let (min_idx, max_idx) = (*min_idx, *max_idx);
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

    fn card_drop_target(&self, containing: Option<&(Id, &ListGeometry)>) -> Option<DropTarget> {
        if let Some((id, geo)) = containing {
            if *id == SIDEBAR_LIST_ID {
                let count = self.playlists.playlists.len();
                return Some(DropTarget::Playlist(
                    self.sidebar_insertion_index(geo, count),
                ));
            }
            if *id == LIBRARY_LIST_ID {
                let count = self.library.items.len();
                return Some(DropTarget::Library(
                    self.sidebar_insertion_index(geo, count),
                ));
            }
        }
        None
    }

    /// The index within a sidebar list where a drop would insert, given the
    /// cursor's position relative to the list's measured row bounds.
    fn sidebar_insertion_index(&self, geo: &ListGeometry, count: usize) -> usize {
        let cursor_y = self.drag.cursor_pos.y + geo.translation_y;
        for (i, row) in geo.rows.iter().enumerate() {
            if cursor_y < row.y + row.height / 2.0 {
                return i;
            }
        }
        geo.rows.len().min(count)
    }

    /// Reorder an existing playlist row within the sidebar list. The active
    /// `Playlist` view selection is remapped so the same playlist stays
    /// selected after its index changes.
    pub fn handle_playlist_drop(&mut self) {
        let Some(DropTarget::PlaylistReorder { from, to }) = self.drag.drop_target else {
            return;
        };
        if from >= self.playlists.playlists.len() || to > self.playlists.playlists.len() {
            return;
        }
        crate::util::reorder_tracks(&mut self.playlists.playlists, to, &[from], &[]);
        self.playlists.save();

        // Keep the active Playlist view pointed at the same playlist. After
        // `reorder_tracks`, the moved row lands at `to - removed_before` where
        // `removed_before = from < to`. Other selections shift down by one if
        // the moved row passed above them, then shift again if the moved row
        // is inserted at or before their new position.
        let removed_before = usize::from(from < to);
        let landed = to - removed_before;
        if let ViewKind::Playlist(entry) = &mut self.view_data_mut().kind {
            if entry.index == from {
                entry.index = landed;
            } else {
                let mut new_sp = entry.index - usize::from(from < entry.index);
                if landed <= new_sp {
                    new_sp += 1;
                }
                entry.index = new_sp;
            }
        }
        self.notify("Reordered playlist");
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
            _ => {}
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
        let id = item.id.clone();
        let name_for_thread = name.clone();
        let tx = self.result_tx.clone();
        let provider = match &self.view_data().kind {
            ViewKind::Search(s) => s.provider,
            _ => crate::providers::ProviderId::YouTube,
        };
        self.notify(format!("Creating playlist \"{name}\"..."));
        Self::spawn_backend_thread(
            move || crate::providers::browse(provider, &id, kind_str),
            move |(tracks, _)| BackendResult::CardPlaylistReady(idx, name_for_thread, tracks),
            tx,
        );

        // Jump to the freshly created (initially empty) playlist view; tracks
        // stream in as the browse result arrives. Select it directly (rather
        // than `handle_select_playlist`) so it activates even when the drop
        // index coincides with the currently selected playlist.
        self.push_new_view(ViewData::new_playlist(idx, name));
        self.save_session();
    }

    /// Arm a press for dragging. A track row also supports double-click (play
    /// + select); a card (artist/album/playlist) opens on a plain click.
    ///
    /// The drag threshold is promoted in `CursorMoved`, so this only records
    /// the press origin.
    pub fn handle_drag_press(&mut self, pressed: Pressed) {
        match pressed {
            Pressed::Track(pos) => {
                let now = std::time::Instant::now();
                // Comparing the full position, not just the index, keeps a click
                // in one list from completing a double-click begun in another.
                let is_double = self.last_click.is_some_and(|(last_pos, last_time)| {
                    last_pos == pos && now.duration_since(last_time).as_millis() < DOUBLE_CLICK_MS
                });
                self.last_click = Some((pos, now));
                if is_double {
                    self.drag.pressed = None;
                    self.drag.drag_origin = None;
                    self.handle_play_track(pos);
                    return;
                }
                let sel = self.selection(pos.list);
                let indices = if !sel.is_empty() && sel.contains(&pos.index) {
                    sel.to_vec()
                } else {
                    vec![pos.index]
                };
                self.drag.dragged = Some((pos.list, indices));
                self.drag.pressed = Some(Pressed::Track(pos));
            }
            _ => {
                self.drag.pressed = Some(pressed);
            }
        }
        self.drag.drag_origin = Some(self.drag.cursor_pos);
        self.drag.drag_active = false;
    }
}

/// Insertion index for a uniform-height virtualized list, from a cursor
/// position in the scrollable's content space (y = 0 at the top of the first
/// row). Snaps to the nearest row boundary, clamped into `[first, count]` —
/// correct even though only visible rows are laid out.
fn drop_index_from_cursor(cursor_y: f32, first: usize, count: usize) -> usize {
    let row_f = (cursor_y / crate::theme::ROW_HEIGHT).max(0.0);
    let idx = (row_f + 0.5).floor() as usize + first;
    idx.clamp(first, count)
}

/// Index of the row whose center is nearest to `cursor_y`, in the same space
/// as the row rectangles.
fn nearest_row_index(rows: &[iced::Rectangle], cursor_y: f32) -> Option<usize> {
    let mut best: Option<(f32, usize)> = None;
    for (i, row) in rows.iter().enumerate() {
        let d = (cursor_y - (row.y + row.height / 2.0)).abs();
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    best.map(|(_, idx)| idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: f32 = crate::theme::ROW_HEIGHT;

    #[test]
    fn drop_index_snaps_to_nearest_boundary() {
        assert_eq!(drop_index_from_cursor(0.0, 1, 10), 1);
        assert_eq!(drop_index_from_cursor(ROW * 0.49, 1, 10), 1);
        assert_eq!(drop_index_from_cursor(ROW * 0.51, 1, 10), 2);
        assert_eq!(drop_index_from_cursor(ROW * 3.5 - 0.01, 1, 10), 4);
    }

    #[test]
    fn drop_index_clamps_to_range() {
        assert_eq!(drop_index_from_cursor(-100.0, 1, 5), 1);
        assert_eq!(drop_index_from_cursor(100_000.0, 1, 5), 5);
        assert_eq!(drop_index_from_cursor(ROW * 9.9, 1, 10), 10);
    }

    #[test]
    fn nearest_row_picks_closest_center() {
        let rows: Vec<iced::Rectangle> = (0..3)
            .map(|i| iced::Rectangle {
                x: 0.0,
                y: i as f32 * ROW,
                width: 100.0,
                height: ROW,
            })
            .collect();
        assert_eq!(nearest_row_index(&rows, ROW * 0.4), Some(0));
        assert_eq!(nearest_row_index(&rows, ROW * 1.6), Some(1));
        assert_eq!(nearest_row_index(&rows, ROW * 2.4), Some(2));
        assert_eq!(nearest_row_index(&rows, ROW), Some(0));
        assert_eq!(nearest_row_index(&[], 0.0), None);
    }
}
