use iced::widget::operation;

use super::{Message, MusicPlayer, Task, Track, TrackListKind, TrackPos, ViewData};
use crate::app::interaction::ContextMenuFocus;
use crate::app::{ui::SEARCH_HISTORY_LIST_ID, view_data::ViewKind, TrackListSearch};

impl MusicPlayer {
    /// Arrow-key navigation and Enter activation while the context menu is
    /// open. Mirrors track-list nav: Up/Down move within the focused pane and
    /// wrap at the edges; Left/Right switch between the menu and its submenu.
    pub fn handle_context_menu_key(
        &mut self,
        key: iced::keyboard::key::Physical,
        modifiers: iced::keyboard::Modifiers,
    ) -> Task<Message> {
        use iced::keyboard::key::{Code, Physical};
        if matches!(key, Physical::Code(Code::Escape)) && !modifiers.control() {
            self.close_context_menu();
            return Task::none();
        }
        if self.context_menu.is_none() {
            return Task::none();
        }
        match key {
            Physical::Code(Code::ArrowUp) => self.step_context_menu_focus(-1),
            Physical::Code(Code::ArrowDown) => self.step_context_menu_focus(1),
            Physical::Code(Code::ArrowLeft) => self.context_menu_horizontal(-1),
            Physical::Code(Code::ArrowRight) => self.context_menu_horizontal(1),
            Physical::Code(Code::Enter) => {
                let menu = self.context_menu.as_ref().expect("checked above");
                let message = match menu.hovered {
                    Some(ContextMenuFocus::Item(i)) => {
                        menu.actions().get(i).map(|a| a.to_message(menu))
                    }
                    Some(ContextMenuFocus::Sub(kind, i)) => kind
                        .providers()
                        .get(i)
                        .map(|p| kind.entry_message(*p, menu)),
                    None => None,
                };
                match message {
                    Some(m) => iced::Task::done(m),
                    None => Task::none(),
                }
            }
            _ => Task::none(),
        }
    }

    fn step_context_menu_focus(&mut self, dir: isize) -> Task<Message> {
        // Move within whichever pane focus is currently in; an unfocused menu
        // starts in the main list.
        let focus = {
            let Some(menu) = self.context_menu.as_ref() else {
                return Task::none();
            };
            let (_in_submenu, kind, count, current) = match menu.hovered {
                Some(ContextMenuFocus::Sub(kind, i)) => {
                    (true, Some(kind), kind.providers().len(), Some(i))
                }
                other => {
                    let i = match other {
                        Some(ContextMenuFocus::Item(i)) => Some(i),
                        _ => None,
                    };
                    (false, None, menu.actions().len(), i)
                }
            };
            if count == 0 {
                return Task::none();
            }
            let next = current.map_or(if dir < 0 { count - 1 } else { 0 }, |i| {
                (i.cast_signed() + dir).rem_euclid(count.cast_signed()) as usize
            });
            match kind {
                Some(kind) => ContextMenuFocus::Sub(kind, next),
                None => ContextMenuFocus::Item(next),
            }
        };
        if let Some(m) = self.context_menu.as_mut() {
            m.hovered = Some(focus);
        }
        Task::none()
    }

    fn context_menu_horizontal(&mut self, dir: isize) -> Task<Message> {
        let focus = {
            let Some(menu) = self.context_menu.as_ref() else {
                return Task::none();
            };
            match (menu.hovered, dir) {
                // Enter the open submenu from its parent row.
                (Some(ContextMenuFocus::Item(i)), 1) => {
                    let Some(kind) = menu.actions().get(i).and_then(|a| a.submenu()) else {
                        return Task::none();
                    };
                    ContextMenuFocus::Sub(kind, 0)
                }
                // Leave the submenu back to its parent row.
                (Some(ContextMenuFocus::Sub(kind, _)), -1) => {
                    let i = menu
                        .actions()
                        .iter()
                        .position(|a| a.submenu() == Some(kind))
                        .unwrap_or(0);
                    ContextMenuFocus::Item(i)
                }
                _ => return Task::none(),
            }
        };
        if let Some(m) = self.context_menu.as_mut() {
            m.hovered = Some(focus);
        }
        Task::none()
    }

    pub fn handle_cursor_moved(&mut self, pos: iced::Point) -> Task<Message> {
        self.drag.is_hover_controlled = false;
        self.drag.cursor_pos = pos;
        if !self.drag.drag_active && self.drag.drag_origin.is_some() && self.drag.pressed.is_some()
        {
            let origin = self.drag.drag_origin.unwrap();
            let dx = (pos.x - origin.x).abs();
            let dy = (pos.y - origin.y).abs();
            if dx > crate::theme::DRAG_THRESHOLD || dy > crate::theme::DRAG_THRESHOLD {
                self.drag.drag_active = true;
                // Reveal the library so it can receive drops.
                if self.drag.is_pressed_card() {
                    self.library_expanded = true;
                }
                return Task::batch([
                    iced_runtime::task::widget(super::operation::CaptureBounds::new()),
                    self.handle_drag_update(),
                ]);
            }
        }
        if self.drag.drag_active {
            self.handle_drag_update()
        } else {
            Task::none()
        }
    }

    pub fn handle_key_press(
        &mut self,
        key: iced::keyboard::key::Physical,
        modifiers: iced::keyboard::Modifiers,
    ) -> Task<Message> {
        use iced::keyboard::key::{Code, Physical};
        if self.context_menu.is_some() {
            return self.handle_context_menu_key(key, modifiers);
        }
        let task = match key {
            Physical::Code(Code::KeyF) if modifiers.control() || modifiers.logo() => {
                self.open_track_list_search()
            }
            Physical::Code(Code::Slash)
                if !modifiers.control() && !modifiers.logo() && !modifiers.alt() =>
            {
                Task::batch([
                    operation::focus::<Message>(crate::app::ui::SEARCH_INPUT_ID),
                    self.activate_search_input(),
                ])
            }
            Physical::Code(Code::Space) => {
                self.toggle_play_pause();
                Task::none()
            }
            Physical::Code(Code::Escape) => {
                if self.track_list_search.is_some() {
                    self.track_list_search = None;
                } else if self.show_search_history {
                    self.show_search_history = false;
                    self.drag.clear_hovered_search_history();
                } else if let Some(hovered) = self.drag.hovered_track() {
                    self.clear_selection_for(hovered.list);
                } else if self.has_selection() {
                    self.clear_selection();
                } else {
                    self.handle_navigate_to(ViewData::new_search(
                        String::new(),
                        self.search_provider,
                        self.search_scope,
                    ));
                }
                Task::none()
            }
            Physical::Code(Code::Delete) => {
                if matches!(&self.view_data().kind, ViewKind::Playlist(_)) {
                    self.handle_delete_selected();
                }
                Task::none()
            }
            Physical::Code(Code::Tab) => self.toggle_keyboard_list(),
            Physical::Code(Code::ArrowUp) => {
                if self.track_list_search.is_some() {
                    return self.handle_track_list_search_step(-1);
                }
                if self.show_search_history {
                    return self.step_search_history_hover(-1);
                }
                self.step_hovered_track(-1)
            }
            Physical::Code(Code::ArrowDown) => {
                if self.track_list_search.is_some() {
                    return self.handle_track_list_search_step(1);
                }
                if self.show_search_history {
                    return self.step_search_history_hover(1);
                }
                self.step_hovered_track(1)
            }
            Physical::Code(Code::Enter) => {
                if let Some(i) = self.drag.hovered_search_history() {
                    self.handle_search_history_select(i);
                } else if let Some(hovered) = self.drag.hovered_track() {
                    self.handle_play_track(hovered);
                }
                Task::none()
            }
            Physical::Code(Code::KeyC) if modifiers.control() || modifiers.logo() => {
                self.handle_copy_selected();
                Task::none()
            }
            Physical::Code(Code::KeyV) if modifiers.control() || modifiers.logo() => {
                self.handle_paste_clipboard();
                Task::none()
            }
            _ => Task::none(),
        };
        task
    }

    fn toggle_keyboard_list(&mut self) -> Task<Message> {
        if !self.show_queue {
            return Task::none();
        }

        let target = if self.hovered_list().in_queue_panel() {
            TrackListKind::Active
        } else {
            self.queue.queue_tab.into()
        };
        if self.track_count(target) == 0 {
            return Task::none();
        }
        self.move_hovered(TrackPos::new(target.first_index(), target))
    }

    /// Scroll `pos` into view of its list. `center` forces the row to the
    /// middle of the viewport; otherwise the list only scrolls when `pos` is
    /// outside the visible viewport (reveal).
    fn scroll_track_into_view(&self, pos: TrackPos) -> Task<Message> {
        let TrackPos { index, list } = pos;

        let bounds = if list.in_queue_panel() {
            self.bounds.queue.as_ref().map(|g| g.bounds)
        } else {
            self.bounds.track.as_ref().map(|g| g.bounds)
        };

        let Some(bounds) = bounds else {
            return Task::none();
        };

        // The queue's now-playing track renders in its own header, so the
        // scrollable's rows are shifted down by `first_index`.
        let visual_index = index - list.first_index().min(index);
        let row_y = visual_index as f32 * crate::theme::ROW_HEIGHT;

        // `scroll_to` with `AbsoluteOffset` sets the scroll position
        // directly, so center the row within the viewport height.
        let absolute = (row_y + crate::theme::ROW_HEIGHT / 2.0 - bounds.height / 2.0).max(0.0);
        operation::scroll_to::<Message>(
            list,
            operation::AbsoluteOffset {
                x: 0.0,
                y: absolute,
            },
        )
    }

    /// Move the hovered track by `dir` (-1 up, +1 down) within its list, looping
    /// at both edges, and center it. Starts from the first row when nothing is
    /// hovered yet.
    fn step_hovered_track(&mut self, dir: isize) -> Task<Message> {
        let list = self.hovered_list();
        let count = self.track_count(list);
        if count == 0 {
            return Task::none();
        }
        let first = list.first_index();
        let new_idx = match self.drag.hovered_track() {
            Some(pos) if pos.list == list => {
                let span = count - first;
                ((pos.index - first).cast_signed() + dir).rem_euclid(span.cast_signed()) as usize
                    + first
            }
            _ => first,
        };
        self.move_hovered(TrackPos::new(new_idx, list))
    }

    /// Set the hovered track and center it in its list.
    fn move_hovered(&mut self, pos: TrackPos) -> Task<Message> {
        self.drag.is_hover_controlled = true;
        self.drag.set_hovered_track(pos);
        self.scroll_track_into_view(pos)
    }

    /// Move the hovered search-history entry by `dir` (-1 up, +1 down),
    /// looping at both edges (last+1 → first, first-1 → last), and center it
    /// in the dropdown viewport — mirroring track-list keyboard nav. Starts
    /// from the first entry when nothing is hovered yet.
    fn step_search_history_hover(&mut self, dir: isize) -> Task<Message> {
        let count = self.last_filtered_history.len();
        if count == 0 {
            return Task::none();
        }
        let new_idx = match self.drag.hovered_search_history() {
            Some(i) => (i.cast_signed() + dir).rem_euclid(count.cast_signed()) as usize,
            None => 0,
        };
        self.drag.is_hover_controlled = true;
        self.drag.set_hovered_search_history(new_idx);
        let y = self
            .bounds
            .search_history
            .as_ref()
            .and_then(|g| g.rows.get(new_idx).map(|row| (g, row)))
            .map_or(0.0, |(g, row)| {
                // Center the row in the viewport, like `scroll_track_into_view`.
                let row_center = row.y + row.height / 2.0;
                (row_center - g.bounds.y + g.translation_y - g.bounds.height / 2.0).max(0.0)
            });
        operation::scroll_to::<Message>(
            SEARCH_HISTORY_LIST_ID,
            iced::widget::operation::AbsoluteOffset { x: 0.0, y },
        )
    }

    fn hovered_list(&self) -> TrackListKind {
        self.drag
            .hovered_track()
            .map_or(TrackListKind::Active, |h| h.list)
    }

    pub(crate) fn open_track_list_search(&mut self) -> Task<Message> {
        let Some(pos) = self.drag.hovered_track() else {
            return Task::none();
        };
        if !pos.list.is_interactive() {
            return Task::none();
        }
        let list = pos.list;
        let matches: Vec<usize> = (0..self.track_count(list)).collect();
        self.track_list_search = Some(TrackListSearch {
            list,
            query: String::new(),
            matches,
        });
        // Anchor the hovered track to the closest match so there is a current
        // occurrence immediately (and scroll it into view).
        let from = pos.index;
        let anchored = self.closest_match(from).unwrap_or(from);
        Task::batch([
            self.move_hovered(TrackPos::new(anchored, list)),
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
        let list = match &self.track_list_search {
            Some(fs) => fs.list,
            None => return Task::none(),
        };
        let tracks: &[Track] = match list {
            TrackListKind::Queue => &self.queue.tracks,
            TrackListKind::Active => self.view_tracks(),
            TrackListKind::Recent => self.queue.recently_played.as_slices().0,
        };
        let matches: Vec<usize> = tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                crate::util::fuzzy_match(query, &t.title)
                    || crate::util::fuzzy_match(query, &t.artist)
            })
            .map(|(i, _)| i)
            .collect();
        let fs = self.track_list_search.as_mut().expect("checked above");
        fs.query = query.to_string();
        fs.matches = matches;
        let from = match self.drag.hovered_track() {
            Some(h) if h.list == list => h.index,
            _ => 0,
        };
        let anchored = self.closest_match(from).unwrap_or(from);
        self.move_hovered(TrackPos::new(anchored, list))
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
        let from = match self.drag.hovered_track() {
            Some(h) if h.list == fs.list => h.index,
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
        self.move_hovered(TrackPos::new(target, fs.list))
    }
}
