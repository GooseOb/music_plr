use iced::widget::operation;

use super::{Message, MusicPlayer, Task, Track, TrackListKind, TrackPos, ViewData};
use crate::{
    app::{
        interaction::{ContextMenuFocus, HoverTarget},
        pane::PaneId,
        Dialog, TrackListSearch,
    },
    types::QueueTab,
};

impl MusicPlayer {
    /// Arrow-key navigation and Enter activation while the context menu is
    /// open. Mirrors track-list nav: Up/Down move within the focused pane and
    /// wrap at the edges; Left/Right switch between the menu and its submenu.
    pub fn handle_context_menu_key(&mut self, key: iced::keyboard::key::Physical) -> Task<Message> {
        use iced::keyboard::key::{Code, Physical};
        if !matches!(self.dialog, Some(Dialog::ContextMenu(_))) {
            return Task::none();
        }
        match key {
            Physical::Code(Code::ArrowUp) => self.step_context_menu_focus(-1),
            Physical::Code(Code::ArrowDown) => self.step_context_menu_focus(1),
            Physical::Code(Code::ArrowLeft | Code::ArrowRight) => self.context_menu_horizontal(),
            Physical::Code(Code::Enter) => {
                let Some(Dialog::ContextMenu(menu)) = &self.dialog else {
                    unreachable!("checked above")
                };
                let message = match menu.hovered {
                    Some(ContextMenuFocus::Item(i)) => menu
                        .actions(&self.stream_cache)
                        .get(i)
                        .map(|a| a.to_message(menu)),
                    Some(ContextMenuFocus::Sub(kind, i)) => menu
                        .submenu_providers(kind, &self.stream_cache)
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
            let Some(Dialog::ContextMenu(menu)) = &self.dialog else {
                return Task::none();
            };
            let (kind, count, current) = match menu.hovered {
                Some(ContextMenuFocus::Sub(kind, i)) => (
                    Some(kind),
                    menu.submenu_providers(kind, &self.stream_cache).len(),
                    Some(i),
                ),
                other => {
                    let i = match other {
                        Some(ContextMenuFocus::Item(i)) => Some(i),
                        _ => None,
                    };
                    (None, menu.actions(&self.stream_cache).len(), i)
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
        if let Some(Dialog::ContextMenu(m)) = &mut self.dialog {
            m.hovered = Some(focus);
        }
        Task::none()
    }

    fn context_menu_horizontal(&mut self) -> Task<Message> {
        let focus = {
            let Some(Dialog::ContextMenu(menu)) = &self.dialog else {
                return Task::none();
            };
            match menu.hovered {
                // Enter the open submenu from its parent row.
                Some(ContextMenuFocus::Item(i)) => {
                    let Some(kind) = menu
                        .actions(&self.stream_cache)
                        .get(i)
                        .and_then(|a| a.submenu())
                    else {
                        return Task::none();
                    };
                    ContextMenuFocus::Sub(kind, 0)
                }
                // Leave the submenu back to its parent row.
                Some(ContextMenuFocus::Sub(kind, _)) => {
                    let i = menu
                        .actions(&self.stream_cache)
                        .iter()
                        .position(|a| a.submenu() == Some(kind))
                        .unwrap_or(0);
                    ContextMenuFocus::Item(i)
                }
                _ => return Task::none(),
            }
        };
        if let Some(Dialog::ContextMenu(m)) = &mut self.dialog {
            m.hovered = Some(focus);
        }
        Task::none()
    }

    pub fn handle_cursor_moved(&mut self, pos: iced::Point) -> Task<Message> {
        self.drag.is_hover_controlled = false;
        self.drag.cursor_pos = pos;
        if self.drag.drag_active {
            return self.handle_drag_update();
        }
        if let Some(origin) = self.drag.pressed.as_ref().map(|pd| pd.origin) {
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
        }
        Task::none()
    }

    #[allow(clippy::too_many_lines)]
    pub fn handle_key_press(
        &mut self,
        key: iced::keyboard::key::Physical,
        modifiers: iced::keyboard::Modifiers,
    ) -> Task<Message> {
        use iced::keyboard::key::{Code, Physical};
        let ctrl = modifiers.control() || modifiers.logo();
        let alt = modifiers.alt();
        let shift = modifiers.shift();
        if self.dialog.is_some() {
            if matches!(key, Physical::Code(Code::Escape)) {
                self.dialog = None;
                return Task::none();
            }
            if matches!(self.dialog, Some(Dialog::ContextMenu(_))) {
                return self.handle_context_menu_key(key);
            }
            if matches!(self.dialog, Some(Dialog::PlaylistJump(_))) {
                return self.handle_playlist_jump_key(key, modifiers);
            }
        }
        let task = match key {
            Physical::Code(Code::KeyF) if ctrl && !alt => self.open_track_list_search(),
            Physical::Code(Code::KeyK) if ctrl && !alt => self.open_playlist_jump(),
            Physical::Code(Code::Slash) if !ctrl && !alt && !shift => {
                let pane = self.focused_pane_id;
                Task::batch([
                    operation::focus::<Message>(crate::app::ui::search_input_id(pane)),
                    self.activate_search_input(pane),
                ])
            }
            Physical::Code(Code::Slash) if !ctrl && !alt && shift => {
                self.open_shortcuts();
                Task::none()
            }
            Physical::Code(Code::F1) if !ctrl && !alt => {
                self.open_shortcuts();
                Task::none()
            }
            Physical::Code(Code::Backslash) if !ctrl && !alt => {
                let pane = self.focused_pane_id;
                self.split_pane(
                    pane,
                    if shift {
                        crate::app::SplitDir::Horizontal
                    } else {
                        crate::app::SplitDir::Vertical
                    },
                )
            }
            Physical::Code(Code::KeyW) if ctrl && !alt => {
                let pane = self.focused_pane_id;
                self.close_pane(pane)
            }
            Physical::Code(Code::Space) if ctrl && !alt => self.toggle_selection_on_focused(),
            Physical::Code(Code::Space) if !ctrl && !alt => {
                self.toggle_play_pause();
                Task::none()
            }
            Physical::Code(Code::ContextMenu) => self.open_context_menu_for_hovered_track(),
            Physical::Code(Code::F10) if shift && !ctrl => {
                self.open_context_menu_for_hovered_track()
            }
            Physical::Code(Code::Enter) if ctrl && !alt => {
                self.open_context_menu_for_hovered_track()
            }
            Physical::Code(Code::Enter) if alt && !ctrl => {
                let pane = self.focused_pane_id;
                self.run_search(pane)
            }
            Physical::Code(Code::Escape) => {
                let pane = self.focused_pane_id;
                self.selection_anchor = None;
                if self.track_list_search.is_some() {
                    self.track_list_search = None;
                } else if self.pane(pane).show_search_history {
                    self.pane_mut(pane).show_search_history = false;
                    self.drag.clear_hovered_search_history();
                } else if let Some(hovered) = self.focused_hovered_track() {
                    self.clear_selection_in(hovered.pane, hovered.list);
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
            Physical::Code(Code::Delete) => {
                self.handle_delete_in_hovered_list();
                Task::none()
            }
            Physical::Code(Code::ArrowLeft) if ctrl && !alt => {
                self.focus_neighbor(crate::app::pane::PaneDir::Left);
                Task::none()
            }
            Physical::Code(Code::ArrowRight) if ctrl && !alt => {
                self.focus_neighbor(crate::app::pane::PaneDir::Right);
                Task::none()
            }
            Physical::Code(Code::ArrowUp) if ctrl && !alt => {
                self.focus_neighbor(crate::app::pane::PaneDir::Up);
                Task::none()
            }
            Physical::Code(Code::ArrowDown) if ctrl && !alt => {
                self.focus_neighbor(crate::app::pane::PaneDir::Down);
                Task::none()
            }
            Physical::Code(Code::ArrowLeft) if alt && !ctrl => {
                let pane = self.focused_pane_id;
                self.handle_navigate_back(pane)
            }
            Physical::Code(Code::ArrowRight) if alt && !ctrl => {
                let pane = self.focused_pane_id;
                self.handle_navigate_forward(pane)
            }
            Physical::Code(Code::ArrowUp) if alt && !ctrl => self.cycle_playlist(-1),
            Physical::Code(Code::ArrowDown) if alt && !ctrl => self.cycle_playlist(1),
            Physical::Code(Code::ArrowLeft | Code::ArrowRight | Code::KeyH) if !ctrl && !alt => {
                self.toggle_keyboard_list()
            }
            Physical::Code(Code::KeyL) if !ctrl && !alt && !shift => self.toggle_keyboard_list(),
            Physical::Code(Code::KeyL) if !ctrl && !alt && shift => {
                let pane = self.focused_pane_id;
                self.handle_show_lyrics(pane)
            }
            Physical::Code(Code::ArrowUp) if shift && !ctrl && !alt => {
                if self.track_list_search.is_some() {
                    return self.handle_track_list_search_step(-1);
                }
                if self.pane(self.focused_pane_id).show_search_history {
                    return self.step_search_history_hover(-1);
                }
                self.extend_hovered_selection(-1)
            }
            Physical::Code(Code::ArrowDown) if shift && !ctrl && !alt => {
                if self.track_list_search.is_some() {
                    return self.handle_track_list_search_step(1);
                }
                if self.pane(self.focused_pane_id).show_search_history {
                    return self.step_search_history_hover(1);
                }
                self.extend_hovered_selection(1)
            }
            Physical::Code(Code::ArrowUp | Code::KeyK) if !ctrl && !alt && !shift => {
                if self.track_list_search.is_some() {
                    return self.handle_track_list_search_step(-1);
                }
                if self.pane(self.focused_pane_id).show_search_history {
                    return self.step_search_history_hover(-1);
                }
                self.step_hovered_track(-1)
            }
            Physical::Code(Code::ArrowDown | Code::KeyJ) if !ctrl && !alt && !shift => {
                if self.track_list_search.is_some() {
                    return self.handle_track_list_search_step(1);
                }
                if self.pane(self.focused_pane_id).show_search_history {
                    return self.step_search_history_hover(1);
                }
                self.step_hovered_track(1)
            }
            Physical::Code(Code::KeyJ) if shift && !ctrl && !alt => {
                if self.track_list_search.is_some() {
                    return self.handle_track_list_search_step(1);
                }
                if self.pane(self.focused_pane_id).show_search_history {
                    return self.step_search_history_hover(1);
                }
                self.extend_hovered_selection(1)
            }
            Physical::Code(Code::KeyK) if shift && !ctrl && !alt => {
                if self.track_list_search.is_some() {
                    return self.handle_track_list_search_step(-1);
                }
                if self.pane(self.focused_pane_id).show_search_history {
                    return self.step_search_history_hover(-1);
                }
                self.extend_hovered_selection(-1)
            }
            Physical::Code(Code::KeyG) if shift && !ctrl && !alt => {
                self.move_hovered_to_edge(false)
            }
            Physical::Code(Code::KeyG) if !ctrl && !alt && !shift => {
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
            Physical::Code(Code::Home) if !ctrl && !alt => self.move_hovered_to_edge(true),
            Physical::Code(Code::End) if !ctrl && !alt => self.move_hovered_to_edge(false),
            Physical::Code(Code::PageUp) if !ctrl && !alt => {
                let pane = self.focused_pane_id;
                let steps = self.page_stride(pane, self.hovered_list());
                self.step_hovered_track_by(-1, steps, false)
            }
            Physical::Code(Code::PageDown) if !ctrl && !alt => {
                let pane = self.focused_pane_id;
                let steps = self.page_stride(pane, self.hovered_list());
                self.step_hovered_track_by(1, steps, false)
            }
            Physical::Code(Code::KeyU) if ctrl && !alt => {
                let pane = self.focused_pane_id;
                let steps = (self.page_stride(pane, self.hovered_list()) / 2).max(1);
                self.step_hovered_track_by(-1, steps, false)
            }
            Physical::Code(Code::KeyD) if ctrl && !alt => {
                let pane = self.focused_pane_id;
                let steps = (self.page_stride(pane, self.hovered_list()) / 2).max(1);
                self.step_hovered_track_by(1, steps, false)
            }
            Physical::Code(Code::KeyP) if alt && !ctrl => {
                let pane = self.focused_pane_id;
                self.cycle_search_provider(pane, if shift { -1 } else { 1 });
                Task::none()
            }
            Physical::Code(Code::KeyS) if alt && !ctrl => {
                let pane = self.focused_pane_id;
                self.cycle_search_scope(pane, if shift { -1 } else { 1 });
                Task::none()
            }
            Physical::Code(
                Code::Digit1 | Code::Digit2 | Code::Digit3 | Code::Digit4 | Code::Digit5,
            ) if alt && !ctrl => {
                let index = match key {
                    Physical::Code(Code::Digit1) => 0,
                    Physical::Code(Code::Digit2) => 1,
                    Physical::Code(Code::Digit3) => 2,
                    Physical::Code(Code::Digit4) => 3,
                    _ => 4,
                };
                let pane = self.focused_pane_id;
                if let Some(&provider) = crate::providers::ProviderId::searchable().get(index) {
                    self.stage_search_provider(pane, provider);
                }
                Task::none()
            }
            Physical::Code(Code::Digit1 | Code::Digit2 | Code::Digit3 | Code::Digit4)
                if ctrl && !alt =>
            {
                let index = match key {
                    Physical::Code(Code::Digit1) => 0,
                    Physical::Code(Code::Digit2) => 1,
                    Physical::Code(Code::Digit3) => 2,
                    _ => 3,
                };
                self.focus_pane_at(index);
                Task::none()
            }
            Physical::Code(Code::Tab) if ctrl && !alt => {
                self.focus_next_pane(if shift { -1 } else { 1 });
                Task::none()
            }
            Physical::Code(Code::Enter) => {
                if let Some(i) = self.drag.hovered_search_history() {
                    return self.handle_search_history_select(self.focused_pane_id, i);
                } else if let Some(hovered) = self.focused_hovered_track() {
                    self.handle_play_track(hovered);
                }
                Task::none()
            }
            Physical::Code(Code::KeyN) if !ctrl && !alt => {
                if self.track_list_search.is_some() {
                    return self.handle_track_list_search_step(if shift { -1 } else { 1 });
                }
                self.next_track();
                Task::none()
            }
            Physical::Code(Code::KeyP) if !ctrl && !alt => {
                if self.track_list_search.is_some() {
                    return self.handle_track_list_search_step(-1);
                }
                self.previous_track();
                Task::none()
            }
            Physical::Code(Code::KeyQ) if !ctrl && !alt => Task::done(Message::ToggleQueue),
            Physical::Code(Code::KeyR) if !ctrl && !alt => Task::done(Message::ToggleRepeat),
            Physical::Code(Code::KeyT) if !ctrl && !alt => self.cycle_queue_tab(),
            Physical::Code(Code::KeyM) if !ctrl && !alt => {
                self.toggle_mute();
                Task::none()
            }
            Physical::Code(Code::Minus) if !ctrl && !alt => {
                self.adjust_volume(-0.05);
                Task::none()
            }
            Physical::Code(Code::Equal) if !ctrl && !alt => {
                self.adjust_volume(0.05);
                Task::none()
            }
            Physical::Code(Code::Comma) if !ctrl && !alt => {
                self.seek_by_seconds(if shift { -10.0 } else { -5.0 });
                Task::none()
            }
            Physical::Code(Code::Period) if !ctrl && !alt => {
                self.seek_by_seconds(if shift { 10.0 } else { 5.0 });
                Task::none()
            }
            Physical::Code(Code::KeyA) if ctrl && !alt => {
                self.handle_select_all();
                Task::none()
            }
            Physical::Code(Code::KeyC) if ctrl && !alt => {
                self.handle_copy_selected();
                Task::none()
            }
            Physical::Code(Code::KeyV) if ctrl && !alt => self.handle_paste_clipboard(),
            _ => Task::none(),
        };
        if !matches!(
            key,
            Physical::Code(Code::KeyG) if !ctrl && !alt && !shift
        ) {
            self.pending_vim_g = None;
        }
        task
    }

    pub fn handle_playlist_jump_key(
        &mut self,
        key: iced::keyboard::key::Physical,
        modifiers: iced::keyboard::Modifiers,
    ) -> Task<Message> {
        use iced::keyboard::key::{Code, Physical};
        if modifiers.control() || modifiers.logo() || modifiers.alt() {
            return Task::none();
        }
        match key {
            Physical::Code(Code::ArrowUp | Code::KeyK) => self.step_playlist_jump(-1),
            Physical::Code(Code::ArrowDown | Code::KeyJ) => self.step_playlist_jump(1),
            Physical::Code(Code::Home) => self.move_playlist_jump_to(true),
            Physical::Code(Code::End) => self.move_playlist_jump_to(false),
            Physical::Code(Code::Enter) => self.confirm_playlist_jump(modifiers.shift()),
            _ => Task::none(),
        }
    }

    fn move_playlist_jump_to(&mut self, first: bool) -> Task<Message> {
        let filtered = self.playlist_jump_filtered();
        if filtered.is_empty() {
            return Task::none();
        }
        if let Some(Dialog::PlaylistJump(jump)) = &mut self.dialog {
            jump.selected = if first { 0 } else { filtered.len() - 1 };
        }
        Task::none()
    }

    fn toggle_selection_on_focused(&mut self) -> Task<Message> {
        if let Some(pos) = self.focused_hovered_track() {
            self.toggle_selection(pos);
        }
        Task::none()
    }

    pub fn open_shortcuts(&mut self) {
        self.dialog = Some(Dialog::Shortcuts);
    }

    pub fn switch_queue_tab(&mut self, tab: QueueTab) -> Task<Message> {
        self.queue.queue_tab = tab;
        self.drag.clear_hovered_track();
        self.save_session();
        self.capture_bounds_task()
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

    fn page_stride(&self, pane: PaneId, list: TrackListKind) -> usize {
        let height = match list {
            TrackListKind::Queue => self.bounds.queue.as_ref().map(|g| g.bounds.height),
            TrackListKind::Active => self.bounds.track_geo(pane).map(|g| g.bounds.height),
            TrackListKind::Recent => self.bounds.recent.as_ref().map(|g| g.bounds.height),
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
        let list = self.hovered_list();
        let count = self.track_count_in(pane, list);
        let first = list.first_index();
        if count <= first {
            return Task::none();
        }
        let cur = match self.focused_hovered_track() {
            Some(pos) if pos.list == list => pos.index,
            _ => self.drag.recall_focus(pane, list).clamp(first, count - 1),
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
        self.move_hovered(TrackPos::new(new_idx, list, pane))
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
            let (list, pane) = (fs.list, fs.pane);
            return self.move_hovered(TrackPos::new(index, list, pane));
        }
        let pane = self.focused_pane_id;
        if self.pane(pane).show_search_history {
            let count = self.pane(pane).last_filtered_history.len();
            if count == 0 {
                return Task::none();
            }
            return self.move_search_history_hover_to(if first { 0 } else { count - 1 });
        }
        let list = self.hovered_list();
        let count = self.track_count_in(pane, list);
        let first_index = list.first_index();
        if count <= first_index {
            return Task::none();
        }
        let index = if first { first_index } else { count - 1 };
        self.selection_anchor = None;
        self.move_hovered(TrackPos::new(index, list, pane))
    }

    fn extend_hovered_selection(&mut self, dir: isize) -> Task<Message> {
        let pane = self.focused_pane_id;
        let list = self.hovered_list();
        let count = self.track_count_in(pane, list);
        let first = list.first_index();
        if count <= first {
            return Task::none();
        }
        let cur = match self.focused_hovered_track() {
            Some(pos) if pos.list == list => pos.index,
            _ => self.drag.recall_focus(pane, list).clamp(first, count - 1),
        };
        let anchor = match self.selection_anchor {
            Some(a) if a.list == list && (!list.is_main() || a.pane == pane) => a,
            _ => {
                let a = TrackPos::new(cur, list, pane);
                self.selection_anchor = Some(a);
                a
            }
        };
        let new_idx =
            (cur.cast_signed() + dir).clamp(first.cast_signed(), count.cast_signed() - 1) as usize;
        self.select_range_in(pane, list, anchor.index, new_idx);
        self.move_hovered(TrackPos::new(new_idx, list, pane))
    }

    fn toggle_keyboard_list(&mut self) -> Task<Message> {
        if !self.show_queue {
            return Task::none();
        }
        let pane = self.focused_pane_id;

        let target = if self.hovered_list().is_main() {
            self.queue.queue_tab.into()
        } else {
            TrackListKind::Active
        };
        if self.track_count_in(pane, target) == 0
            || self.drag.hovered_track().is_some_and(|p| {
                p.list == target && (target != TrackListKind::Active || p.pane == pane)
            })
        {
            return Task::none();
        }
        let index = self
            .drag
            .recall_focus(pane, target)
            .clamp(target.first_index(), self.track_count_in(pane, target) - 1);
        self.move_hovered(TrackPos::new(index, target, pane))
    }

    /// Scroll `pos` into view of its list. `center` forces the row to the
    /// middle of the viewport; otherwise the list only scrolls when `pos` is
    /// outside the visible viewport (reveal).
    fn scroll_track_into_view(&self, pos: TrackPos) -> Task<Message> {
        let TrackPos { index, list, pane } = pos;

        let bounds = if list.is_main() {
            self.bounds.track_geo(pane).map(|g| g.bounds)
        } else {
            self.bounds.queue.as_ref().map(|g| g.bounds)
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
        let id = match list {
            TrackListKind::Queue => crate::app::ui::QUEUE_LIST_ID,
            TrackListKind::Active => crate::app::ui::track_list_id(pane),
            TrackListKind::Recent => crate::app::ui::QUEUE_RECENT_LIST_ID,
        };
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
        let pane = self.focused_pane_id;
        let list = self.hovered_list();
        let count = self.track_count_in(pane, list);
        if count == 0 {
            return Task::none();
        }
        let first = list.first_index();
        let new_idx = match self.focused_hovered_track() {
            Some(pos) if pos.list == list => {
                let span = count - first;
                ((pos.index - first).cast_signed() + dir).rem_euclid(span.cast_signed()) as usize
                    + first
            }
            _ => self.drag.recall_focus(pane, list).clamp(first, count - 1),
        };
        self.move_hovered(TrackPos::new(new_idx, list, pane))
    }

    /// Set the hovered track and center it in its list.
    pub(crate) fn move_hovered(&mut self, pos: TrackPos) -> Task<Message> {
        self.drag.is_hover_controlled = true;
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
        self.drag.is_hover_controlled = true;
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

    fn hovered_list(&self) -> TrackListKind {
        self.drag
            .hovered_track()
            .map_or(TrackListKind::Active, |h| h.list)
    }

    pub(crate) fn open_track_list_search(&mut self) -> Task<Message> {
        let Some(pos) = self.drag.hovered_track() else {
            return Task::none();
        };
        let list = pos.list;
        let pane = if list.is_main() {
            pos.pane
        } else {
            self.focused_pane_id
        };
        let matches: Vec<usize> = (0..self.track_count_in(pane, list)).collect();
        self.track_list_search = Some(TrackListSearch {
            list,
            pane,
            query: String::new(),
            matches,
        });
        // Anchor the hovered track to the closest match so there is a current
        // occurrence immediately (and scroll it into view).
        let from = pos.index;
        let anchored = self.closest_match(from).unwrap_or(from);
        Task::batch([
            self.move_hovered(TrackPos::new(anchored, list, pane)),
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
        let (list, pane) = match &self.track_list_search {
            Some(fs) => (fs.list, fs.pane),
            None => return Task::none(),
        };
        let tracks: &[Track] = match list {
            TrackListKind::Queue => &self.queue.tracks,
            TrackListKind::Active => self.view_tracks_in(pane),
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
            Some(h) if h.list == list && (list != TrackListKind::Active || h.pane == pane) => {
                h.index
            }
            _ => 0,
        };
        let anchored = self.closest_match(from).unwrap_or(from);
        self.move_hovered(TrackPos::new(anchored, list, pane))
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
        let (list, pane) = (fs.list, fs.pane);
        let from = match self.drag.hovered_track() {
            Some(h) if h.list == list && (list != TrackListKind::Active || h.pane == pane) => {
                h.index
            }
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
        self.move_hovered(TrackPos::new(target, list, pane))
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
        let pos = |i| TrackPos::new(i, TrackListKind::Active, pane);
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
        assert_eq!(p.selection_in(pane, TrackListKind::Active), &[0, 1]);
        let _ = p.extend_hovered_selection(1);
        assert_eq!(p.selection_in(pane, TrackListKind::Active), &[0, 1, 2]);
        let _ = p.step_hovered_track_by(1, 1, true);
        assert!(p.selection_anchor.is_none());
        assert_eq!(
            p.drag.hovered_track(),
            Some(TrackPos::new(0, TrackListKind::Active, pane))
        );
    }

    #[test]
    fn t_cycles_queue_tab_and_opens_hidden_panel() {
        use iced::keyboard::{
            key::{Code, Physical},
            Modifiers,
        };
        let mut p = player();
        p.show_queue = true;
        assert_eq!(p.queue.queue_tab, QueueTab::Queue);
        let t = Physical::Code(Code::KeyT);
        let _ = p.handle_key_press(t, Modifiers::empty());
        assert_eq!(p.queue.queue_tab, QueueTab::RecentlyPlayed);
        let _ = p.handle_key_press(t, Modifiers::empty());
        assert_eq!(p.queue.queue_tab, QueueTab::Queue);

        p.show_queue = false;
        let _ = p.handle_key_press(t, Modifiers::empty());
        assert!(p.show_queue);
        assert_eq!(p.queue.queue_tab, QueueTab::RecentlyPlayed);
    }

    #[test]
    fn question_and_f1_open_shortcuts_esc_closes() {
        use iced::keyboard::{
            key::{Code, Physical},
            Modifiers,
        };
        let mut p = player();
        let question = Physical::Code(Code::Slash);
        let _ = p.handle_key_press(question, Modifiers::SHIFT);
        assert!(matches!(p.dialog, Some(crate::app::Dialog::Shortcuts)));
        let _ = p.handle_key_press(Physical::Code(Code::Escape), Modifiers::empty());
        assert!(p.dialog.is_none());
        let _ = p.handle_key_press(Physical::Code(Code::F1), Modifiers::empty());
        assert!(matches!(p.dialog, Some(crate::app::Dialog::Shortcuts)));

        assert!(!crate::app::shortcuts::SECTIONS.is_empty());
        for lang in crate::i18n::Language::ALL {
            let tr = lang.strings();
            assert!(!tr.sc_title.is_empty(), "{lang:?}");
            for section in crate::app::shortcuts::SECTIONS {
                assert!(!(section.title)(tr).is_empty(), "{lang:?}");
                assert!(!section.rows.is_empty(), "{lang:?}");
                for row in section.rows {
                    assert!(!(row.action)(tr).is_empty(), "{lang:?}");
                }
            }
        }
    }

    #[test]
    fn double_g_jumps_to_first_row() {
        use iced::keyboard::{
            key::{Code, Physical},
            Modifiers,
        };
        let mut p = player();
        let pane = p.focused_pane_id;
        let _ = p.move_hovered_to_edge(false);
        assert_eq!(
            p.drag.hovered_track(),
            Some(TrackPos::new(2, TrackListKind::Active, pane))
        );
        let g = Physical::Code(Code::KeyG);
        let _ = p.handle_key_press(g, Modifiers::empty());
        assert!(p.pending_vim_g.is_some());
        assert_eq!(
            p.drag.hovered_track(),
            Some(TrackPos::new(2, TrackListKind::Active, pane))
        );
        let _ = p.handle_key_press(g, Modifiers::empty());
        assert_eq!(
            p.drag.hovered_track(),
            Some(TrackPos::new(0, TrackListKind::Active, pane))
        );
    }
}
