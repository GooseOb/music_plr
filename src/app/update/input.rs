use iced::widget::operation;

use super::{
    is_main_pane, pane_first_index, Message, MusicPlayer, Task, Track, TrackPos, ViewData,
    MANUAL_SEARCH_PANE_ID, QUEUE_PANE_ID, RECENT_PANE_ID,
};
use crate::{
    app::{
        interaction::{ContextMenuFocus, HoverTarget},
        pane::PaneId,
        CardMenuState, ContextMenuState, Dialog, TrackListSearch,
    },
    data::cache::StreamCache,
    types::QueueTab,
};

/// The next index in a menu list of `count` entries, wrapping at both ends;
/// an unfocused list starts at the end nearest the direction. `None` when
/// there is nothing to step through.
fn step(current: Option<usize>, count: usize, dir: isize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(current.map_or(if dir < 0 { count - 1 } else { 0 }, |i| {
        (i.cast_signed() + dir)
            .rem_euclid(count.cast_signed())
            .cast_unsigned()
    }))
}

/// Arrow-key navigation and Enter activation while a context menu is open.
/// Mirrors track-list nav: Up/Down move within the focused pane and wrap at
/// the edges; Left/Right switch between the menu and its submenu. Only menu
/// shortcuts arrive here — the listener gates the rest out.
fn apply_menu_shortcut(
    shortcut: crate::app::Shortcut,
    menu: &mut ContextMenuState,
    cache: &StreamCache,
) -> Task<Message> {
    use crate::app::Shortcut as S;
    match shortcut {
        S::MenuUp => {
            let hovered = step_context_menu_focus(menu, cache, -1);
            menu.anchor.hovered = hovered;
            Task::none()
        }
        S::MenuDown => {
            let hovered = step_context_menu_focus(menu, cache, 1);
            menu.anchor.hovered = hovered;
            Task::none()
        }
        S::MenuAcross => {
            context_menu_horizontal(menu, cache);
            Task::none()
        }
        S::MenuActivate => {
            let message = match menu.anchor.hovered {
                Some(ContextMenuFocus::Item(i)) => {
                    menu.actions(cache).get(i).map(|a| a.to_message(menu))
                }
                Some(ContextMenuFocus::Sub(kind, i)) => menu
                    .submenu_providers(kind, cache)
                    .get(i)
                    .map(|p| kind.entry_message(*p, menu)),
                None => None,
            };
            match message {
                Some(m) => Task::done(m),
                None => Task::none(),
            }
        }
        _ => Task::none(),
    }
}

/// Up/Down move the card menu's focus and Enter activates it. A card menu
/// has no submenus, so Left/Right do nothing.
fn apply_card_menu_shortcut(
    shortcut: crate::app::Shortcut,
    menu: &mut CardMenuState,
) -> Task<Message> {
    use crate::app::Shortcut as S;
    let actions = menu.actions();
    let current = match menu.anchor.hovered {
        Some(ContextMenuFocus::Item(i)) => Some(i),
        _ => None,
    };
    let step = |dir| step(current, actions.len(), dir).map(ContextMenuFocus::Item);
    match shortcut {
        S::MenuUp => {
            menu.anchor.hovered = step(-1);
            Task::none()
        }
        S::MenuDown => {
            menu.anchor.hovered = step(1);
            Task::none()
        }
        S::MenuActivate => match menu.anchor.hovered {
            Some(ContextMenuFocus::Item(i)) => menu
                .actions()
                .get(i)
                .map_or_else(Task::none, |a| Task::done(a.to_message())),
            _ => Task::none(),
        },
        _ => Task::none(),
    }
}

fn step_context_menu_focus(
    menu: &ContextMenuState,
    cache: &StreamCache,
    dir: isize,
) -> Option<ContextMenuFocus> {
    // Move within whichever pane focus is currently in; an unfocused menu
    // starts in the main list.
    let (kind, count, current) = match menu.anchor.hovered {
        Some(ContextMenuFocus::Sub(kind, i)) => (
            Some(kind),
            menu.submenu_providers(kind, cache).len(),
            Some(i),
        ),
        other => {
            let i = match other {
                Some(ContextMenuFocus::Item(i)) => Some(i),
                _ => None,
            };
            (None, menu.actions(cache).len(), i)
        }
    };
    let next = step(current, count, dir)?;
    Some(match kind {
        Some(kind) => ContextMenuFocus::Sub(kind, next),
        None => ContextMenuFocus::Item(next),
    })
}

fn context_menu_horizontal(menu: &mut ContextMenuState, cache: &StreamCache) {
    let focus = match menu.anchor.hovered {
        // Enter the open submenu from its parent row.
        Some(ContextMenuFocus::Item(i)) => {
            let Some(kind) = menu.actions(cache).get(i).and_then(|a| a.submenu()) else {
                return;
            };
            ContextMenuFocus::Sub(kind, 0)
        }
        // Leave the submenu back to its parent row.
        Some(ContextMenuFocus::Sub(kind, _)) => {
            let i = menu
                .actions(cache)
                .iter()
                .position(|a| a.submenu() == Some(kind))
                .unwrap_or(0);
            ContextMenuFocus::Item(i)
        }
        _ => return,
    };
    menu.anchor.hovered = Some(focus);
}

impl MusicPlayer {
    /// Cursor tracking runs only while a press is down (see
    /// `drag_subscription`): every call here is drag work. The first move
    /// after a press re-anchors the threshold origin, which the press-time
    /// cursor can't provide (press messages carry no position).
    pub fn handle_cursor_moved(&mut self, pos: iced::Point) -> Task<Message> {
        self.drag.claim_mouse();
        self.drag.cursor_pos = pos;
        if self.drag.drag_active {
            return self.handle_drag_update();
        }
        if !self.drag.origin_fresh {
            if let Some(pressed) = self.drag.pressed.as_mut() {
                pressed.origin = pos;
            }
            self.drag.origin_fresh = true;
            return Task::none();
        }
        let Some(origin) = self.drag.pressed.as_ref().map(|pd| pd.origin) else {
            return Task::none();
        };
        let dx = (pos.x - origin.x).abs();
        let dy = (pos.y - origin.y).abs();
        if dx > crate::theme::DRAG_THRESHOLD || dy > crate::theme::DRAG_THRESHOLD {
            self.drag.drag_active = true;
            // Reveal the library so it can receive drops.
            if self.drag.is_pressed_card() {
                self.library_expanded = true;
            }
            return Task::batch([self.capture_bounds_task(), self.handle_drag_update()]);
        }
        Task::none()
    }

    #[allow(clippy::too_many_lines)]
    pub fn apply_shortcut(&mut self, shortcut: crate::app::Shortcut) -> Task<Message> {
        use crate::app::Shortcut as S;
        if let Some(dialog) = self.dialog.as_mut() {
            if matches!(shortcut, S::Escape) {
                if matches!(dialog, Dialog::Edit(_)) && self.edit_manual_open() {
                    return self.handle_edit_manual_cancel();
                }
                self.save_translate_prefs();
                self.dialog = None;
                return Task::none();
            }
            match dialog {
                Dialog::ContextMenu(menu) => {
                    return apply_menu_shortcut(shortcut, menu, &self.stream_cache);
                }
                Dialog::CardMenu(menu) => {
                    return apply_card_menu_shortcut(shortcut, menu);
                }
                Dialog::Palette(_) => {
                    return self.apply_palette_shortcut(shortcut);
                }
                _ => {}
            }
        }
        let task = match shortcut {
            S::OpenTrackSearch => self.open_track_list_search(),
            S::OpenPalette => self.open_palette(),
            S::OpenPlaylistPicker => self.open_playlist_picker(),
            S::FocusSearch => {
                let pane = self.focused_pane_id;
                Task::batch([
                    operation::focus::<Message>(crate::app::ui::search_input_id(pane)),
                    self.activate_search_input(pane),
                ])
            }
            S::SplitVertical => {
                let pane = self.focused_pane_id;
                self.split_pane(pane, crate::app::SplitDir::Vertical)
            }
            S::SplitHorizontal => {
                let pane = self.focused_pane_id;
                self.split_pane(pane, crate::app::SplitDir::Horizontal)
            }
            S::ClosePane => {
                let pane = self.focused_pane_id;
                self.close_pane(pane)
            }
            S::ToggleSelection => self.toggle_selection_on_focused(),
            S::PlayPause => {
                self.toggle_play_pause();
                Task::none()
            }
            S::OpenMenu => self.open_context_menu_for_hovered_track(),
            S::ExecuteSearch => {
                let pane = self.focused_pane_id;
                self.run_search(pane)
            }
            S::Escape => {
                let pane = self.focused_pane_id;
                self.selection_anchor = None;
                if self.track_list_search.is_some() {
                    self.track_list_search = None;
                } else if self.pane(pane).show_search_history {
                    self.pane_mut(pane).show_search_history = false;
                    self.drag.clear_hovered_search_history();
                } else if let Some(hovered) = self.focused_hovered_track() {
                    self.clear_selection_in(hovered.pane);
                } else if self.has_selection() {
                    self.clear_selection();
                } else {
                    let provider = self.pane(pane).search_provider;
                    let scope = self.pane(pane).search_scope;
                    return self.handle_navigate_to(
                        pane,
                        ViewData::new_search(String::new(), provider, scope),
                    );
                }
                Task::none()
            }
            S::DeleteHovered => {
                self.handle_delete_in_hovered_list();
                Task::none()
            }
            S::FocusNeighbor(dir) => self.focus_neighbor(dir),
            S::NavBack => {
                let pane = self.focused_pane_id;
                self.handle_navigate_back(pane)
            }
            S::NavForward => {
                let pane = self.focused_pane_id;
                self.handle_navigate_forward(pane)
            }
            S::CyclePlaylist(dir) => self.cycle_playlist(dir),
            S::ToggleListFocus => self.toggle_keyboard_list(),
            S::OpenLyrics => {
                let pane = self.focused_pane_id;
                self.handle_show_lyrics(pane)
            }
            S::HoverMove { dir, extend } => {
                if self.track_list_search.is_some() {
                    return self.handle_track_list_search_step(dir);
                }
                if self.pane(self.focused_pane_id).show_search_history {
                    return self.step_search_history_hover(dir);
                }
                if extend {
                    self.extend_hovered_selection(dir)
                } else {
                    self.step_hovered_track(dir)
                }
            }
            S::VimG => {
                let now = std::time::Instant::now();
                let is_double = self
                    .pending_vim_g
                    .is_some_and(|t| now.duration_since(t) < std::time::Duration::from_secs(2));
                if is_double {
                    self.pending_vim_g = None;
                    self.move_hovered_to_edge(true)
                } else {
                    self.pending_vim_g = Some(now);
                    Task::none()
                }
            }
            S::EdgeTo(first) => self.move_hovered_to_edge(first),
            S::PageStep(dir) => {
                let steps = self.page_stride(self.hovered_pane());
                self.step_hovered_track_by(dir, steps, false)
            }
            S::HalfPageStep(dir) => {
                let steps = (self.page_stride(self.hovered_pane()) / 2).max(1);
                self.step_hovered_track_by(dir, steps, false)
            }
            S::CycleProvider(dir) => {
                let pane = self.focused_pane_id;
                self.cycle_search_provider(pane, dir);
                Task::none()
            }
            S::CycleScope(dir) => {
                let pane = self.focused_pane_id;
                self.cycle_search_scope(pane, dir);
                Task::none()
            }
            S::ProviderSlot(index) => {
                let pane = self.focused_pane_id;
                if let Some(&provider) = crate::providers::ProviderId::searchable().get(index) {
                    self.stage_search_provider(pane, provider);
                }
                Task::none()
            }
            S::PaneIndex(index) => self.focus_pane_at(index),
            S::FocusNext(dir) => self.focus_next_pane(dir),
            S::ActivateHovered => {
                if let Some(i) = self.drag.hovered_search_history() {
                    return self.handle_search_history_select(self.focused_pane_id, i);
                } else if let Some(hovered) = self.focused_hovered_track() {
                    self.handle_play_track(hovered);
                }
                Task::none()
            }
            S::NextMaybeSearch(backwards) => {
                if self.track_list_search.is_some() {
                    return self.handle_track_list_search_step(if backwards { -1 } else { 1 });
                }
                self.next_track();
                Task::none()
            }
            S::PrevTrack => {
                if self.track_list_search.is_some() {
                    return self.handle_track_list_search_step(-1);
                }
                self.previous_track();
                Task::none()
            }
            S::CycleQueueTab => self.cycle_queue_tab(),
            S::ToggleMute => {
                self.toggle_mute();
                Task::none()
            }
            S::VolumeDown => {
                self.adjust_volume(-0.05);
                Task::none()
            }
            S::VolumeUp => {
                self.adjust_volume(0.05);
                Task::none()
            }
            S::SeekSecs(secs) => {
                self.seek_by_seconds(secs);
                Task::none()
            }
            S::SelectAll => {
                self.handle_select_all();
                Task::none()
            }
            S::CopySelection => {
                self.handle_copy_selected();
                Task::none()
            }
            S::PasteClipboard => self.handle_paste_clipboard(),
            _ => Task::none(),
        };
        if !matches!(shortcut, S::VimG) {
            self.pending_vim_g = None;
        }
        task
    }

    pub fn apply_palette_shortcut(&mut self, shortcut: crate::app::Shortcut) -> Task<Message> {
        use crate::app::Shortcut as S;
        match shortcut {
            S::PaletteStep(dir) => self.step_palette(dir),
            S::PaletteEdge(first) => self.move_palette_to(first),
            S::PaletteConfirm { play } => self.confirm_palette(play),
            _ => Task::none(),
        }
    }

    fn toggle_selection_on_focused(&mut self) -> Task<Message> {
        if let Some(pos) = self.focused_hovered_track() {
            self.toggle_selection(pos);
        }
        Task::none()
    }

    pub fn switch_queue_tab(&mut self, tab: QueueTab) -> Task<Message> {
        self.queue.queue_tab = tab;
        self.drag.clear_hovered_track();
        self.save_session();
        Task::none()
    }

    pub fn cycle_queue_tab(&mut self) -> Task<Message> {
        if !self.show_queue {
            self.show_queue = true;
        }
        let next = match self.queue.queue_tab {
            QueueTab::Queue => QueueTab::RecentlyPlayed,
            QueueTab::RecentlyPlayed => QueueTab::Queue,
        };
        self.switch_queue_tab(next)
    }

    fn page_stride(&self, pane: PaneId) -> usize {
        let height = match pane {
            QUEUE_PANE_ID => self.bounds.queue.as_ref().map(|g| g.bounds.height),
            RECENT_PANE_ID => self.bounds.recent.as_ref().map(|g| g.bounds.height),
            _ => self.bounds.track_geo(pane).map(|g| g.bounds.height),
        };
        height
            .map(|px| (px / crate::theme::ROW_HEIGHT) as usize)
            .filter(|&n| n > 0)
            .unwrap_or(10)
    }

    fn step_hovered_track_by(&mut self, dir: isize, steps: usize, wrap: bool) -> Task<Message> {
        if self.track_list_search.is_some() {
            return self.handle_track_list_search_step(dir);
        }
        let pane = self.focused_pane_id;
        if self.pane(pane).show_search_history {
            let mut task = Task::none();
            for _ in 0..steps.max(1) {
                task = task.chain(self.step_search_history_hover(dir));
            }
            return task;
        }
        let list = self.hovered_pane();
        let count = self.track_count_in(list);
        let first = pane_first_index(list);
        if count <= first {
            return Task::none();
        }
        let cur = match self.focused_hovered_track() {
            Some(pos) if pos.pane == list => pos.index,
            _ => self.drag.recall_focus(list).clamp(first, count - 1),
        };
        let new_idx = if wrap {
            let span = count - first;
            (cur.cast_signed() - first.cast_signed() + dir * steps.max(1).cast_signed())
                .rem_euclid(span.cast_signed()) as usize
                + first
        } else {
            (cur.cast_signed() + dir * steps.max(1).cast_signed())
                .clamp(first.cast_signed(), count.cast_signed() - 1) as usize
        };
        self.selection_anchor = None;
        self.move_hovered(TrackPos::new(new_idx, list))
    }

    fn move_hovered_to_edge(&mut self, first: bool) -> Task<Message> {
        if let Some(fs) = self.track_list_search.as_ref() {
            if fs.matches.is_empty() {
                return Task::none();
            }
            let index = if first {
                fs.matches[0]
            } else {
                *fs.matches.last().expect("checked above")
            };
            let pane = fs.pane;
            return self.move_hovered(TrackPos::new(index, pane));
        }
        let pane = self.focused_pane_id;
        if self.pane(pane).show_search_history {
            let count = self.pane(pane).last_filtered_history.len();
            if count == 0 {
                return Task::none();
            }
            return self.move_search_history_hover_to(if first { 0 } else { count - 1 });
        }
        let list = self.hovered_pane();
        let count = self.track_count_in(list);
        let first_index = pane_first_index(list);
        if count <= first_index {
            return Task::none();
        }
        let index = if first { first_index } else { count - 1 };
        self.selection_anchor = None;
        self.move_hovered(TrackPos::new(index, list))
    }

    fn extend_hovered_selection(&mut self, dir: isize) -> Task<Message> {
        let list = self.hovered_pane();
        let count = self.track_count_in(list);
        let first = pane_first_index(list);
        if count <= first {
            return Task::none();
        }
        let cur = match self.focused_hovered_track() {
            Some(pos) if pos.pane == list => pos.index,
            _ => self.drag.recall_focus(list).clamp(first, count - 1),
        };
        let anchor = match self.selection_anchor {
            Some(a) if a.pane == list => a,
            _ => {
                let a = TrackPos::new(cur, list);
                self.selection_anchor = Some(a);
                a
            }
        };
        let new_idx =
            (cur.cast_signed() + dir).clamp(first.cast_signed(), count.cast_signed() - 1) as usize;
        self.select_range_in(list, anchor.index, new_idx);
        self.move_hovered(TrackPos::new(new_idx, list))
    }

    fn toggle_keyboard_list(&mut self) -> Task<Message> {
        if !self.show_queue {
            return Task::none();
        }
        let pane = self.focused_pane_id;

        let target = if is_main_pane(self.hovered_pane()) {
            self.queue.queue_tab.into()
        } else {
            pane
        };
        if self.track_count_in(target) == 0
            || self.drag.hovered_track().is_some_and(|p| p.pane == target)
        {
            return Task::none();
        }
        let index = self
            .drag
            .recall_focus(target)
            .clamp(pane_first_index(target), self.track_count_in(target) - 1);
        self.move_hovered(TrackPos::new(index, target))
    }

    /// Scroll `pos` into view of its list. `center` forces the row to the
    /// middle of the viewport; otherwise the list only scrolls when `pos` is
    /// outside the visible viewport (reveal).
    fn scroll_track_into_view(&self, pos: TrackPos) -> Task<Message> {
        let TrackPos { index, pane } = pos;

        let Some(bounds) = self.bounds.list_geo(pane).map(|geo| geo.bounds) else {
            return Task::none();
        };

        // The queue's now-playing track renders in its own header, so the
        // scrollable's rows are shifted down by the list's first index.
        let visual_index = index - pane_first_index(pane).min(index);
        let row_y = visual_index as f32 * crate::theme::ROW_HEIGHT;

        // `scroll_to` with `AbsoluteOffset` sets the scroll position
        // directly, so center the row within the viewport height.
        let absolute = (row_y + crate::theme::ROW_HEIGHT / 2.0 - bounds.height / 2.0).max(0.0);
        let id = crate::app::ui::track_list_id(pane);
        operation::scroll_to::<Message>(
            id,
            operation::AbsoluteOffset {
                x: 0.0,
                y: absolute,
            },
        )
    }

    /// Move the hovered track by `dir` (-1 up, +1 down) within its list, looping
    /// at both edges, and center it. Starts from the first row when nothing is
    /// hovered yet. Keyboard navigation always acts on the focused pane; a
    /// hover in another pane is ignored.
    fn step_hovered_track(&mut self, dir: isize) -> Task<Message> {
        let list = self.hovered_pane();
        let count = self.track_count_in(list);
        if count == 0 {
            return Task::none();
        }
        let first = pane_first_index(list);
        let new_idx = match self.focused_hovered_track() {
            Some(pos) if pos.pane == list => {
                let span = count - first;
                ((pos.index - first).cast_signed() + dir).rem_euclid(span.cast_signed()) as usize
                    + first
            }
            _ => self.drag.recall_focus(list).clamp(first, count - 1),
        };
        self.move_hovered(TrackPos::new(new_idx, list))
    }

    /// Set the hovered track and center it in its list.
    pub(crate) fn move_hovered(&mut self, pos: TrackPos) -> Task<Message> {
        self.drag.claim_keyboard_hover();
        self.drag.set_hovered(HoverTarget::Track(pos));
        self.scroll_track_into_view(pos)
    }

    /// Move the hovered search-history entry by `dir` (-1 up, +1 down),
    /// looping at both edges (last+1 → first, first-1 → last), and center it
    /// in the dropdown viewport — mirroring track-list keyboard nav. Starts
    /// from the first entry when nothing is hovered yet.
    fn step_search_history_hover(&mut self, dir: isize) -> Task<Message> {
        let pane = self.focused_pane_id;
        let count = self.pane(pane).last_filtered_history.len();
        if count == 0 {
            return Task::none();
        }
        let new_idx = match self.drag.hovered_search_history() {
            Some(i) => (i.cast_signed() + dir).rem_euclid(count.cast_signed()) as usize,
            None => 0,
        };
        self.move_search_history_hover_to(new_idx)
    }

    fn move_search_history_hover_to(&mut self, new_idx: usize) -> Task<Message> {
        let pane = self.focused_pane_id;
        self.drag.claim_keyboard_hover();
        self.drag.set_hovered_search_history(new_idx);
        let y = self
            .bounds
            .search_history
            .get(&pane)
            .and_then(|g| g.rows.get(new_idx).map(|row| (g, row)))
            .map_or(0.0, |(g, row)| {
                // Center the row in the viewport, like `scroll_track_into_view`.
                let row_center = row.y + row.height / 2.0;
                (row_center - g.bounds.y + g.translation_y - g.bounds.height / 2.0).max(0.0)
            });
        operation::scroll_to::<Message>(
            crate::app::ui::search_history_list_id(pane),
            iced::widget::operation::AbsoluteOffset { x: 0.0, y },
        )
    }

    fn hovered_pane(&self) -> PaneId {
        // The Edit popup's manual search is the only list a key can reach while
        // its step is open (the panes behind the modal take no input), so it
        // owns navigation until the step closes.
        if self.edit_manual_open() && self.track_count_in(MANUAL_SEARCH_PANE_ID) > 0 {
            return MANUAL_SEARCH_PANE_ID;
        }
        self.drag
            .hovered_track()
            .map_or(self.focused_pane_id, |h| h.pane)
    }

    pub(crate) fn open_track_list_search(&mut self) -> Task<Message> {
        let Some(pos) = self.drag.hovered_track() else {
            return Task::none();
        };
        let pane = pos.pane;
        let matches: Vec<usize> = (0..self.track_count_in(pane)).collect();
        self.track_list_search = Some(TrackListSearch {
            pane,
            query: String::new(),
            matches,
        });
        // Anchor the hovered track to the closest match so there is a current
        // occurrence immediately (and scroll it into view).
        let from = pos.index;
        let anchored = self.closest_match(from).unwrap_or(from);
        Task::batch([
            self.move_hovered(TrackPos::new(anchored, pane)),
            operation::focus::<Message>(crate::app::ui::track_list_search::TRACK_LIST_SEARCH_ID),
        ])
    }

    /// The matched track index nearest to `from` (by absolute row distance,
    /// ties resolved to the smaller index), or `None` when there are no
    /// matches.
    fn closest_match(&self, from: usize) -> Option<usize> {
        let fs = self.track_list_search.as_ref()?;
        let mut best: Option<(usize, usize)> = None;
        for &m in &fs.matches {
            let dist = m.abs_diff(from);
            match best {
                Some((b_dist, b_idx)) if b_dist < dist || (b_dist == dist && b_idx < m) => {}
                _ => best = Some((dist, m)),
            }
        }
        best.map(|(_, idx)| idx)
    }

    /// Recompute the match set for the active track list search against the
    /// live query. The hovered track is re-anchored to the closest match
    /// (kept as-is when it still matches) so the current occurrence follows
    /// the query, and the new current is scrolled into view.
    pub(crate) fn handle_track_list_search_input(&mut self, query: &str) -> Task<Message> {
        let pane = match &self.track_list_search {
            Some(fs) => fs.pane,
            None => return Task::none(),
        };
        let tracks: &[Track] = match pane {
            QUEUE_PANE_ID => &self.queue.tracks,
            RECENT_PANE_ID => self.queue.recently_played.as_slices().0,
            _ => self.view_tracks_in(pane),
        };
        let needle = crate::util::FuzzyQuery::new(query);
        let matches: Vec<usize> = tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| needle.matches(&t.title) || needle.matches(&t.artist))
            .map(|(i, _)| i)
            .collect();
        let fs = self.track_list_search.as_mut().expect("checked above");
        fs.query = query.to_string();
        fs.matches = matches;
        let from = match self.drag.hovered_track() {
            Some(h) if h.pane == pane => h.index,
            _ => 0,
        };
        let anchored = self.closest_match(from).unwrap_or(from);
        self.move_hovered(TrackPos::new(anchored, pane))
    }

    /// Move the hovered track to the next (`dir = 1`) or previous (`dir = -1`)
    /// match relative to its current row, wrapping around the match list.
    /// The hovered track is the current occurrence, so this is how the user
    /// walks between matches; the new current is scrolled into view.
    pub(crate) fn handle_track_list_search_step(&mut self, dir: isize) -> Task<Message> {
        let Some(fs) = self.track_list_search.as_ref() else {
            return Task::none();
        };
        if fs.matches.len() <= 1 {
            return Task::none();
        }
        let pane = fs.pane;
        let from = match self.drag.hovered_track() {
            Some(h) if h.pane == pane => h.index,
            _ => 0,
        };
        let target = {
            let back = dir < 0;
            let mut lo = 0usize;
            let mut hi = fs.matches.len();
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                // forward: skip the exact match; backward: keep it (step back)
                if fs.matches[mid] < from || (!back && fs.matches[mid] == from) {
                    lo = mid + 1; // match is at or before `from`
                } else {
                    hi = mid; // match is after `from`
                }
            }
            if dir < 0 {
                // previous match; wrap to last past the start
                *fs.matches
                    .get(lo.wrapping_sub(1))
                    .unwrap_or_else(|| fs.matches.last().unwrap())
            } else {
                // next match; wrap to first past the end
                fs.matches.get(lo).copied().unwrap_or(fs.matches[0])
            }
        };
        self.move_hovered(TrackPos::new(target, pane))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data::config,
        providers::{ProviderId, SearchScope},
    };

    fn track(id: &str) -> Track {
        Track::from_provider(
            ProviderId::YouTube,
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

    fn player() -> MusicPlayer {
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.reset_test_pane(vec![ViewData::new_search(
            String::new(),
            ProviderId::YouTube,
            SearchScope::Songs,
        )]);
        p.view_data_mut()
            .set_tracks(vec![track("1"), track("2"), track("3")]);
        p
    }

    #[test]
    fn vim_step_wraps_and_page_clamps() {
        let mut p = player();
        let pane = p.focused_pane_id;
        let pos = |i| TrackPos::new(i, pane);
        let _ = p.step_hovered_track_by(1, 1, true);
        assert_eq!(p.drag.hovered_track(), Some(pos(1)));
        let _ = p.step_hovered_track_by(-1, 1, true);
        assert_eq!(p.drag.hovered_track(), Some(pos(0)));
        let _ = p.step_hovered_track_by(-1, 1, true);
        assert_eq!(p.drag.hovered_track(), Some(pos(2)));
        let _ = p.step_hovered_track_by(1, 99, false);
        assert_eq!(p.drag.hovered_track(), Some(pos(2)));
        let _ = p.move_hovered_to_edge(true);
        assert_eq!(p.drag.hovered_track(), Some(pos(0)));
        let _ = p.move_hovered_to_edge(false);
        assert_eq!(p.drag.hovered_track(), Some(pos(2)));
    }

    #[test]
    fn shift_extend_selects_range_and_plain_move_clears_anchor() {
        let mut p = player();
        let pane = p.focused_pane_id;
        let _ = p.extend_hovered_selection(1);
        assert_eq!(p.selection_in(pane), &[0, 1]);
        let _ = p.extend_hovered_selection(1);
        assert_eq!(p.selection_in(pane), &[0, 1, 2]);
        let _ = p.step_hovered_track_by(1, 1, true);
        assert!(p.selection_anchor.is_none());
        assert_eq!(p.drag.hovered_track(), Some(TrackPos::new(0, pane)));
    }

    #[test]
    fn t_cycles_queue_tab_and_opens_hidden_panel() {
        use crate::app::Shortcut;
        let mut p = player();
        p.show_queue = true;
        assert_eq!(p.queue.queue_tab, QueueTab::Queue);
        let _ = p.update(Message::Shortcut(Shortcut::CycleQueueTab));
        assert_eq!(p.queue.queue_tab, QueueTab::RecentlyPlayed);
        let _ = p.update(Message::Shortcut(Shortcut::CycleQueueTab));
        assert_eq!(p.queue.queue_tab, QueueTab::Queue);

        p.show_queue = false;
        let _ = p.update(Message::Shortcut(Shortcut::CycleQueueTab));
        assert!(p.show_queue);
        assert_eq!(p.queue.queue_tab, QueueTab::RecentlyPlayed);
    }

    #[test]
    fn escape_closes_palette() {
        use crate::app::{Dialog, Shortcut};
        let mut p = player();
        let _ = p.update(Message::Shortcut(Shortcut::OpenPalette));
        assert!(matches!(p.dialog, Some(Dialog::Palette(_))));
        let _ = p.update(Message::Shortcut(Shortcut::Escape));
        assert!(p.dialog.is_none());

        let _ = p.update(Message::OpenPlaylistAdd(vec![0]));
        assert!(matches!(p.dialog, Some(Dialog::Palette(_))));
        let _ = p.update(Message::Shortcut(Shortcut::Escape));
        assert!(p.dialog.is_none());
    }

    #[test]
    fn escape_closes_context_menu() {
        use crate::app::{Dialog, Shortcut};
        let mut p = player();
        let pane = p.focused_pane_id;
        let _ = p.move_hovered(TrackPos::new(0, pane));
        let _ = p.update(Message::Shortcut(Shortcut::OpenMenu));
        assert!(matches!(p.dialog, Some(Dialog::ContextMenu(_))));
        let _ = p.update(Message::Shortcut(Shortcut::Escape));
        assert!(p.dialog.is_none());
    }

    #[test]
    fn palette_shortcuts_pick_the_mode_and_esc_closes() {
        use crate::app::{palette::PaletteGroup, Dialog, PaletteMode, Shortcut};
        let mut p = player();

        let _ = p.update(Message::Shortcut(Shortcut::OpenPalette));
        let Some(Dialog::Palette(palette)) = &p.dialog else {
            panic!("expected the palette");
        };
        assert_eq!(palette.mode, PaletteMode::All);
        let _ = p.update(Message::Shortcut(Shortcut::Escape));
        assert!(p.dialog.is_none());

        let _ = p.update(Message::Shortcut(Shortcut::OpenPlaylistPicker));
        let Some(Dialog::Palette(palette)) = &p.dialog else {
            panic!("expected the palette");
        };
        assert_eq!(palette.mode, PaletteMode::Playlists);
        let rows = palette.rows(&p);
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|row| row.group == PaletteGroup::Playlists));
        let _ = p.update(Message::Shortcut(Shortcut::Escape));
        assert!(p.dialog.is_none());
    }

    #[test]
    fn double_g_jumps_to_first_row() {
        use crate::app::Shortcut;
        let mut p = player();
        let pane = p.focused_pane_id;
        let _ = p.move_hovered_to_edge(false);
        assert_eq!(p.drag.hovered_track(), Some(TrackPos::new(2, pane)));
        let _ = p.update(Message::Shortcut(Shortcut::VimG));
        assert!(p.pending_vim_g.is_some());
        assert_eq!(p.drag.hovered_track(), Some(TrackPos::new(2, pane)));
        let _ = p.update(Message::Shortcut(Shortcut::VimG));
        assert_eq!(p.drag.hovered_track(), Some(TrackPos::new(0, pane)));
    }
}
