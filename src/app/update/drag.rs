use iced::widget::Id;

use super::{
    operation::{ContainingList, ListGeometry},
    BackendResult, Message, MusicPlayer, Task, Track, TrackListKind, TrackPos, DOUBLE_CLICK_MS,
    PREPEND,
};
use crate::{
    app::{
        interaction::{DropTarget, Pressed, PressedDrag},
        pane::PaneId,
        ui::{track_list_id, QUEUE_LIST_ID},
        ViewKind,
    },
    data::{
        library::{LibraryItem, LibraryKind},
        JsonStore,
    },
};

impl MusicPlayer {
    pub fn handle_left_release(&mut self) -> Task<Message> {
        let cursor = self.drag.cursor_pos;
        let input_pane = self
            .bounds
            .search_inputs
            .iter()
            .find(|(_, r)| r.contains(cursor))
            .map(|(pane, _)| *pane);
        if let Some(pane) = input_pane {
            self.focused_pane_id = pane;
            self.drag.stop();
            for other in self.pane_ids() {
                if other != pane && self.pane(other).show_search_history {
                    self.pane_mut(other).show_search_history = false;
                }
            }
            return self.activate_search_input(pane);
        }
        for pane in self.pane_ids() {
            if self.pane(pane).show_search_history {
                self.pane_mut(pane).show_search_history = false;
            }
        }
        self.drag.clear_hovered_search_history();

        let Some(pressed) = self.drag.pressed.take().map(|pd| pd.what) else {
            self.drag.stop();
            return Task::none();
        };

        let mut nav_task = Task::none();

        if self.drag.drag_active {
            match pressed {
                Pressed::Track(pos) => self.handle_track_drop(pos),
                Pressed::Card(item, _) => {
                    if let Some(target) = self
                        .card_drop_target(self.bounds.get_containing(self.drag.cursor_pos).as_ref())
                    {
                        nav_task = self.handle_card_drop(item, target);
                    }
                }
                Pressed::Playlist(_) => self.handle_playlist_drop(),
            }
        } else {
            match pressed {
                Pressed::Track(pos) => {
                    if pos.list.is_main() {
                        self.focused_pane_id = pos.pane;
                    }
                    self.toggle_selection(pos);
                }
                Pressed::Card(item, pane) => {
                    let target = pane.unwrap_or(self.focused_pane_id);
                    self.focused_pane_id = target;
                    let provider = item.provider;
                    nav_task = if item.kind == crate::data::library::LibraryKind::Artist {
                        self.open_artist(target, Some(&item.id), &item.title, provider)
                    } else {
                        self.handle_browse(target, &item.into(), provider)
                    }
                }
                // A click without a drag selects the playlist (mirrors how a
                // library card opens on a plain click).
                Pressed::Playlist(i) => {
                    nav_task = self.handle_select_playlist(i);
                }
            }
        }

        self.drag.stop();
        nav_task
    }

    pub fn handle_drag_update(&mut self) -> Task<Message> {
        self.drag.drop_target = None;

        let Some(pressed) = self.drag.pressed.as_ref().map(|pd| &pd.what) else {
            return Task::none();
        };

        let containing = self.bounds.get_containing(self.drag.cursor_pos);

        match pressed {
            Pressed::Card(_, _) => {
                self.drag.drop_target = self.card_drop_target(containing.as_ref());
            }
            Pressed::Track(_) => {
                self.drag.drop_target = self.resolve_track_drop(containing.as_ref());
            }
            Pressed::Playlist(from) => {
                self.drag.drop_target = self.resolve_playlist_drop(*from, containing.as_ref());
            }
        }

        let Some((target, geo)) = containing else {
            return Task::none();
        };

        let Some((list, pane)) = target.track_target(self.focused_pane_id) else {
            return Task::none();
        };
        if list == TrackListKind::Recent {
            return Task::none();
        }
        let count = self.track_count_in(pane, list);
        let scrollable_id = match list {
            TrackListKind::Queue => QUEUE_LIST_ID,
            TrackListKind::Active => track_list_id(pane),
            TrackListKind::Recent => return Task::none(),
        };
        let content_height = count as f32 * crate::theme::ROW_HEIGHT;
        self.handle_drag_autoscroll(geo.bounds, geo.translation_y, content_height, scrollable_id)
    }

    /// Resolve a playlist-row reorder: the press must be over the sidebar
    /// playlist list, and the resolved insertion index must differ from the
    /// row's current position (a drop onto itself is a no-op).
    fn resolve_playlist_drop(
        &self,
        from: usize,
        containing: Option<&(ContainingList, &ListGeometry)>,
    ) -> Option<DropTarget> {
        let (list, geo) = containing?;
        if *list != ContainingList::Sidebar {
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
        containing: Option<&(ContainingList, &ListGeometry)>,
    ) -> Option<DropTarget> {
        let (list, geo) = containing?;
        if *list == ContainingList::Sidebar {
            let count = self.playlists.playlists.len();
            let cursor_y = self.drag.cursor_pos.y + geo.translation_y;
            let idx = nearest_row_index(&geo.rows, cursor_y).filter(|&idx| idx < count)?;
            return Some(DropTarget::PlaylistAdd(idx));
        }
        let (list, pane) = list.track_target(self.focused_pane_id)?;
        // Only local playlists and queue can be dropped onto
        if list == TrackListKind::Active
            && !matches!(&self.view_data_in(pane).kind, ViewKind::Playlist(_))
        {
            return None;
        }
        let drop_idx = self.compute_drop_idx(pane, list);
        Some(DropTarget::Track(TrackPos::new(drop_idx, list, pane)))
    }

    fn compute_drop_idx(&self, pane: PaneId, list: TrackListKind) -> usize {
        let geo = match list {
            TrackListKind::Queue | TrackListKind::Recent => self.bounds.queue.as_ref(),
            TrackListKind::Active => self.bounds.track_geo(pane),
        };
        let Some(geo) = geo else {
            return list.first_index();
        };
        let first = list.first_index();
        let count = self.track_count_in(pane, list);
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
                    TrackListKind::Active => self.bounds.track_geo(pos.pane)?,
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
        let source_pane = pos.pane;
        let indices = self.dragged_indices(source_pane, source).to_vec();

        // Dropped on the playlist sidebar: add to that playlist (prepend). The
        // target was resolved during the drag into `drop_target` and is shown
        // by the row highlight, so there is no separate insertion bar.
        if let Some(DropTarget::PlaylistAdd(playlist_idx)) = self.drag.drop_target {
            let tracks: Vec<Track> = indices
                .iter()
                .filter_map(|&i| self.get_track_at(TrackPos::new(i, source, source_pane)))
                .collect();
            let count = self
                .playlists
                .insert_tracks_at(playlist_idx, tracks.iter(), PREPEND);
            let name = self.playlists.playlists[playlist_idx].name.clone();
            let msg = (self.strings.added_to)(count, &name);
            self.notify(msg);
            return;
        }

        let Some(DropTarget::Track(drop)) = self.drag.drop_target else {
            return;
        };
        let drop_idx = drop.index;

        // Determine if this is a cross-list copy or a same-list reorder.
        // A drop is a reorder only when source and target are the same list
        // in the same pane; cross-pane Active drops copy instead.
        if drop.list == source && (drop.list != TrackListKind::Active || drop.pane == source_pane) {
            self.handle_same_list_reorder(drop.pane, drop_idx, &indices, source);
        } else {
            match drop.list {
                TrackListKind::Queue => self.copy_to_queue(source_pane, source, &indices, drop_idx),
                TrackListKind::Active => {
                    self.copy_from_queue(drop.pane, source_pane, source, &indices, drop_idx);
                }
                TrackListKind::Recent => {}
            }
        }
    }

    pub fn dragged_indices(&self, pane: PaneId, list: TrackListKind) -> &[usize] {
        match &self.drag.dragged {
            Some((drag_pane, drag_list, indices))
                if *drag_list == list && (list != TrackListKind::Active || *drag_pane == pane) =>
            {
                indices
            }
            _ => &[],
        }
    }

    pub fn is_dragging_track(&self, pos: TrackPos) -> bool {
        self.dragged_indices(pos.pane, pos.list)
            .contains(&pos.index)
    }

    fn copy_to_queue(
        &mut self,
        source_pane: PaneId,
        source: TrackListKind,
        indices: &[usize],
        drop_idx: usize,
    ) {
        let clamped = drop_idx.min(self.queue.tracks.len());
        let tracks: Vec<Track> = indices
            .iter()
            .filter_map(|&i| self.get_track_at(TrackPos::new(i, source, source_pane)))
            .collect();
        let inserted = tracks.len();
        for (j, track) in tracks.into_iter().enumerate() {
            self.queue.tracks.insert(clamped + j, track);
        }
        self.save_session();
        let msg = (self.strings.added_to)(inserted, self.strings.queue);
        self.notify(msg);
    }

    /// Insert tracks from `source` into the target pane's playlist at the
    /// given drop index. `indices` are positions in the source list.
    fn copy_from_queue(
        &mut self,
        target_pane: PaneId,
        source_pane: PaneId,
        source: TrackListKind,
        indices: &[usize],
        drop_idx: usize,
    ) {
        let active = match &self.view_data_in(target_pane).kind {
            ViewKind::Playlist(p) => Some(p.index),
            _ => None,
        };
        let Some(sp) = active else {
            if !self.view_data_in(target_pane).is_search_like() {
                self.notify(self.strings.select_playlist_drop);
            }
            return;
        };
        if sp >= self.playlists.playlists.len() {
            return;
        }

        let clamped = drop_idx.min(self.playlists.playlists[sp].tracks.len());
        let tracks: Vec<Track> = indices
            .iter()
            .filter_map(|&i| self.get_track_at(TrackPos::new(i, source, source_pane)))
            .collect();
        let inserted = self.playlists.insert_tracks_at(sp, tracks.iter(), clamped);
        self.save_session();
        let name = self.playlists.playlists[sp].name.clone();
        if inserted > 0 {
            let msg = (self.strings.added_to)(inserted, &name);
            self.notify(msg);
        }
    }

    /// Handle reordering within the same list. The selection is always
    /// remapped to reflect the new positions of all selected tracks — both
    /// the moved ones and any that merely shifted.
    fn handle_same_list_reorder(
        &mut self,
        pane: PaneId,
        drop_idx: usize,
        indices: &[usize],
        source: TrackListKind,
    ) {
        if drop_idx > self.track_count_in(pane, source) {
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
                let selection = self.view_data_in_mut(pane).selection.clone();
                let positions =
                    self.handle_reorder_tracks_selected(pane, drop_idx, indices, &selection);
                self.view_data_in_mut(pane).selection = positions;
            }
            TrackListKind::Recent => {}
        }
    }

    fn card_drop_target(
        &self,
        containing: Option<&(ContainingList, &ListGeometry)>,
    ) -> Option<DropTarget> {
        if let Some((list, geo)) = containing {
            if *list == ContainingList::Sidebar {
                let count = self.playlists.playlists.len();
                return Some(DropTarget::Playlist(
                    self.sidebar_insertion_index(geo, count),
                ));
            }
            if *list == ContainingList::Library {
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
        self.notify(self.strings.reordered_playlist);
    }

    fn handle_card_drop(&mut self, item: LibraryItem, target: DropTarget) -> Task<Message> {
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
                    let msg = (self.strings.saved_title)(&title);
                    self.notify(msg);
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Create a local playlist from a dragged card at `insert_at`: make a new
    /// uniquely-named playlist, then fetch its contents in the background and
    /// fill it once they arrive.
    fn create_playlist_from_card(&mut self, item: &LibraryItem, insert_at: usize) -> Task<Message> {
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
        let rid = self.request_ids.next();
        Self::spawn_backend_thread(
            rid,
            move || crate::providers::browse(provider, &id, kind_str),
            move |(tracks, _)| BackendResult::CardPlaylistReady(idx, name_for_thread, tracks),
            tx,
        );

        // Jump to the freshly created (initially empty) playlist view; tracks
        // stream in as the browse result arrives. Select it directly (rather
        // than `handle_select_playlist`) so it activates even when the drop
        // index coincides with the currently selected playlist.
        self.navigate_to_playlist(idx)
    }

    /// Arm a press for dragging. A track row also supports double-click (play
    /// + select); a card (artist/album/playlist) opens on a plain click.
    ///
    /// The drag threshold is promoted in `CursorMoved`, so this only records
    /// the press origin.
    pub fn handle_drag_press(&mut self, pressed: Pressed) {
        match pressed {
            Pressed::Track(pos) => {
                if pos.list.is_main() {
                    self.focused_pane_id = pos.pane;
                }
                let now = std::time::Instant::now();
                // Comparing the full position, not just the index, keeps a click
                // in one list from completing a double-click begun in another.
                let is_double = self.last_click.is_some_and(|(last_pos, last_time)| {
                    last_pos == pos && now.duration_since(last_time).as_millis() < DOUBLE_CLICK_MS
                });
                self.last_click = Some((pos, now));
                if is_double {
                    self.drag.pressed = None;
                    self.handle_play_track(pos);
                    return;
                }
                let sel = self.selection_in(pos.pane, pos.list);
                let indices = if !sel.is_empty() && sel.contains(&pos.index) {
                    sel.to_vec()
                } else {
                    vec![pos.index]
                };
                self.drag.dragged = Some((pos.pane, pos.list, indices));
                self.drag.pressed = Some(PressedDrag {
                    what: Pressed::Track(pos),
                    origin: self.drag.cursor_pos,
                });
            }
            Pressed::Card(_, Some(pane)) => {
                self.focused_pane_id = pane;
                self.drag.pressed = Some(PressedDrag {
                    what: pressed,
                    origin: self.drag.cursor_pos,
                });
            }
            _ => {
                self.drag.pressed = Some(PressedDrag {
                    what: pressed,
                    origin: self.drag.cursor_pos,
                });
            }
        }
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
