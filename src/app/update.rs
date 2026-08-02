use super::*;
use crate::types::TrackSource;
use crate::types::View;
use std::path::Path;
use std::thread;

use tracing::debug;

const DOUBLE_CLICK_MS: u128 = 300;
const SEARCH_PAGE_SIZE: usize = 10;

pub fn spawn_thumbnail_download_thread(tracks: &[Track], result_tx: &mpsc::Sender<BackendResult>) {
    let entries: Vec<(String, String)> = tracks
        .iter()
        .filter(|t| t.source == TrackSource::YouTube)
        .map(|t| (t.id.clone(), t.thumbnail.clone()))
        .collect();
    if entries.is_empty() {
        return;
    }
    let tx = result_tx.clone();
    thread::spawn(move || {
        for (id, thumb) in &entries {
            crate::thumbnails::download(id, thumb);
            let _ = tx.send(BackendResult::ThumbnailsReady);
        }
    });
}

impl MusicPlayer {
    pub fn init_mpris(&mut self) {
        let (mpris_update_tx, mpris_update_rx) = mpsc::channel();
        mpris::start(self.mpris_cmd_tx.clone(), mpris_update_rx);
        self.mpris_update_tx = Some(mpris_update_tx);
    }

    pub fn handle_tick(&mut self) {
        while let Ok(result) = self.result_rx.try_recv() {
            self.process_result(result);
        }

        while let Ok(cmd) = self.mpris_cmd_rx.try_recv() {
            self.process_mpris_command(cmd);
        }

        if self.thumbnails_pending {
            self.thumbnails_pending = false;
        }

        let s = self.audio.get_state();
        self.is_playing = s.is_playing;
        self.progress = s.progress;
        self.duration = s.duration;
        if self.track_loading && s.is_playing {
            self.track_loading = false;
        }

        if let Some(pending) = self.pending_cache_id.clone() {
            if s.stream_finished {
                if self.stream_cache.path_for(&pending).exists()
                    && self.stream_cache.insert(&pending)
                {
                    debug!("Registered cached track: {}", pending);
                }
                self.pending_cache_id = None;
            }
        }

        if s.stream_finished && !s.is_playing && !self.track_loading {
            self.next_track();
        }

        self.send_mpris_update();
        self.update_progress_text();
    }

    pub fn process_mpris_command(&mut self, cmd: MprisCommand) {
        match cmd {
            MprisCommand::TogglePlayPause => self.toggle_play_pause(),
            MprisCommand::NextTrack => self.next_track(),
            MprisCommand::PreviousTrack => self.previous_track(),
            MprisCommand::Stop => {
                if self.is_playing {
                    self.toggle_play_pause();
                }
            }
            MprisCommand::Play => {
                if !self.is_playing {
                    self.toggle_play_pause();
                }
            }
            MprisCommand::SetVolume(vol) => self.set_volume(vol),
            MprisCommand::Seek(delta_us) => {
                let delta_frac = delta_us as f32 / 1_000_000.0 / self.duration.max(0.001);
                let new_frac = (self.progress + delta_frac).clamp(0.0, 1.0);
                self.seek(new_frac);
            }
        }
    }

    pub fn update_progress_text(&mut self) {
        let elapsed = (self.progress * self.duration) as u32;
        self.elapsed_text = format_duration(elapsed);
        let total = self.duration as u32;
        self.total_text = format_duration(total);
    }

    pub fn process_result(&mut self, result: BackendResult) {
        match result {
            BackendResult::SearchResults(tracks) => {
                self.search_results = tracks;
                self.search_exhausted = self.search_results.len() < SEARCH_PAGE_SIZE;
                self.search_loading = false;
                if matches!(self.current_view, View::Search) {
                    self.push_nav_entry();
                }
                spawn_thumbnail_download_thread(&self.search_results, &self.result_tx);
                self.clear_notification();
            }
            BackendResult::SearchResultsAppend(tracks) => {
                let exhausted = tracks.len() < SEARCH_PAGE_SIZE;
                self.search_results.extend(tracks);
                self.search_loading = false;
                self.search_exhausted = exhausted;
                if let Some(entry) = self.nav_history.get_mut(self.nav_history_pos) {
                    if let ViewSnapshot::Search { results, .. } = &mut entry.snapshot {
                        *results = self.search_results.clone();
                    }
                }
                spawn_thumbnail_download_thread(&self.search_results, &self.result_tx);
                self.clear_notification();
            }
            BackendResult::RadioResults(label, tracks) => {
                self.radio_label = label;
                self.radio_tracks = tracks;
                self.search_loading = false;
                if matches!(self.current_view, View::SongRadio | View::ArtistRadio) {
                    self.push_nav_entry();
                }
                self.save_session();
                spawn_thumbnail_download_thread(&self.radio_tracks, &self.result_tx);
            }
            BackendResult::DownloadComplete(url, path) => {
                self.downloading_index = None;
                self.download_registry.register(&url, &path);
                self.notify("Download complete!".into());
            }
            BackendResult::DownloadError(msg) => {
                self.downloading_index = None;
                error!("Download error: {}", msg);
                self.notify_error(msg);
            }
            BackendResult::SearchError(msg) => {
                self.search_loading = false;
                self.clear_notification();
                self.notify_error(msg);
            }
            BackendResult::ThumbnailsReady => {
                self.thumbnails_pending = true;
            }
        }
    }

    pub fn send_mpris_update(&self) {
        if let Some(ref tx) = self.mpris_update_tx {
            let track = self.queue.current();
            let update = MprisUpdate {
                playback_status: if self.is_playing {
                    "Playing".into()
                } else if track.is_some() {
                    "Paused".into()
                } else {
                    "Stopped".into()
                },
                title: track.map(|t| t.title.clone()).unwrap_or_default(),
                artist: track.map(|t| t.artist.clone()).unwrap_or_default(),
                duration_secs: self.duration,
                position_us: (self.progress * self.duration * 1_000_000.0) as i64,
                volume: self.volume,
                has_track: track.is_some(),
            };
            let _ = tx.send(update);
        }
    }

    pub fn save_session(&self) {
        let state = SessionState {
            current_view: self.current_view.clone(),
            queue: self.queue.clone(),
            is_playing: self.is_playing,
            selected_playlist: self.selected_playlist,
            selected_playlist_name: self.selected_playlist_name.clone(),
            show_queue: self.show_queue,
        };
        state.save();
    }

    pub fn restore_session(&mut self) {
        let state = SessionState::load();
        self.current_view = state.current_view;
        self.queue = state.queue;
        self.is_playing = state.is_playing;
        self.selected_playlist = state.selected_playlist;
        self.selected_playlist_name = state.selected_playlist_name.clone();
        self.show_queue = state.show_queue;
        self.nav_history = vec![NavEntry {
            view: self.current_view.clone(),
            snapshot: self.snapshot_current(),
        }];
        self.nav_history_pos = 0;
    }

    pub fn resume_playback(&mut self) {
        if self.is_playing {
            if let Some(track) = self.queue.current() {
                let track = track.clone();
                self.play_track_internal(&track);
            } else {
                self.is_playing = false;
            }
        }
    }

    pub fn can_navigate_back(&self) -> bool {
        self.nav_history_pos > 0
    }

    pub fn can_navigate_forward(&self) -> bool {
        self.nav_history_pos + 1 < self.nav_history.len()
    }

    pub fn notify(&mut self, msg: String) {
        self.notification = Some(msg);
    }

    pub fn notify_error(&mut self, msg: String) {
        warn!("Backend error: {}", msg);
        self.notification = Some(msg);
    }

    pub fn clear_notification(&mut self) {
        self.notification = None;
    }

    fn snapshot_current(&self) -> ViewSnapshot {
        match self.current_view {
            View::Search => ViewSnapshot::Search {
                query: self.search_query.clone(),
                results: self.search_results.clone(),
                selection: self.selected_indices.clone(),
            },
            View::SongRadio | View::ArtistRadio => ViewSnapshot::Radio {
                label: self.radio_label.clone(),
                tracks: self.radio_tracks.clone(),
                selection: self.selected_indices.clone(),
            },
            View::Playlist | View::Downloads => ViewSnapshot::Playlist {
                playlist: self.selected_playlist,
                playlist_name: self.selected_playlist_name.clone(),
                selection: self.selected_indices.clone(),
            },
        }
    }

    fn restore_nav_entry(&mut self, entry: &NavEntry) {
        self.current_view = entry.view.clone();
        match &entry.snapshot {
            ViewSnapshot::Search {
                query,
                results,
                selection,
            } => {
                self.search_query = query.clone();
                self.search_results = results.clone();
                self.selected_indices = selection.clone();
            }
            ViewSnapshot::Radio {
                label,
                tracks,
                selection,
            } => {
                self.radio_label = label.clone();
                self.radio_tracks = tracks.clone();
                self.selected_indices = selection.clone();
            }
            ViewSnapshot::Playlist {
                playlist,
                playlist_name,
                selection,
            } => {
                self.selected_playlist = *playlist;
                self.selected_playlist_name = playlist_name.clone();
                self.selected_indices = selection.clone();
            }
        }
    }

    pub fn handle_navigate_to(&mut self, view: View) {
        self.nav_history.truncate(self.nav_history_pos + 1);
        self.cleanup_drag_state();

        let back_entry = NavEntry {
            view: self.current_view.clone(),
            snapshot: self.snapshot_current(),
        };
        self.nav_history.push(back_entry);

        self.current_view = view;
        self.selected_indices.clear();

        let new_entry = NavEntry {
            view: self.current_view.clone(),
            snapshot: self.snapshot_current(),
        };
        self.nav_history.push(new_entry);

        if self.nav_history.len() > 20 {
            self.nav_history.remove(0);
            self.nav_history_pos = self.nav_history_pos.saturating_sub(1);
        }
        self.nav_history_pos = self.nav_history.len() - 1;

        self.save_session();
    }

    pub fn handle_navigate_back(&mut self) {
        if self.nav_history_pos > 0 {
            self.nav_history_pos -= 1;
            let entry = self.nav_history[self.nav_history_pos].clone();
            self.restore_nav_entry(&entry);
            self.save_session();
        }
    }

    pub fn handle_navigate_forward(&mut self) {
        if self.nav_history_pos + 1 < self.nav_history.len() {
            self.nav_history_pos += 1;
            let entry = self.nav_history[self.nav_history_pos].clone();
            self.restore_nav_entry(&entry);
            self.save_session();
        }
    }

    pub fn handle_left_release(&mut self) {
        if self.drag_active {
            let is_queue = self.pressed_track_is_queue;
            self.handle_drag_drop(is_queue);
        } else if let Some(track_idx) = self.pressed_track {
            let is_queue = self.pressed_track_is_queue;
            self.toggle_selection(track_idx, is_queue);
        }
        self.cleanup_drag_state();
    }

    fn cleanup_drag_state(&mut self) {
        self.drag_active = false;
        self.drag_origin = None;
        self.pressed_track = None;
        self.drag_drop_target = None;
        self.sidebar_hover_playlist = None;
    }

    pub fn selection(&self, is_queue: bool) -> &[usize] {
        if is_queue {
            &self.queue_selected_indices
        } else {
            &self.selected_indices
        }
    }

    pub fn selection_mut(&mut self, is_queue: bool) -> &mut Vec<usize> {
        if is_queue {
            &mut self.queue_selected_indices
        } else {
            &mut self.selected_indices
        }
    }

    pub fn handle_drag_update(&mut self) -> Task<Message> {
        let cursor = self.cursor_pos;

        self.sidebar_hover_playlist = None;
        self.drag_drop_target = None;

        if let Some(sidebar_bounds) = self.sidebar_bounds {
            if cursor.x < sidebar_bounds.x + crate::theme::SIDEBAR_WIDTH {
                let y_offset = cursor.y - sidebar_bounds.y;
                if y_offset >= 0.0 {
                    let playlist_idx = ((y_offset + self.sidebar_list_scroll)
                        / crate::theme::SIDEBAR_ITEM_HEIGHT)
                        as usize;
                    if playlist_idx < self.playlists.playlists.len() {
                        self.sidebar_hover_playlist = Some(playlist_idx);
                    }
                }
                return Task::none();
            }
        }

        let is_queue_drag = self.pressed_track_is_queue;
        let (list_bounds, list_scroll, track_count) = if is_queue_drag && self.show_queue {
            match (self.queue_list_bounds, self.queue_list_scroll) {
                (Some(b), s) => (b, s, self.queue.tracks.len()),
                _ => return Task::none(),
            }
        } else {
            match (
                self.get_current_list_bounds(),
                self.get_current_list_scroll(),
            ) {
                (Some(b), s) => (b, s, self.current_track_count(false)),
                (None, _) => return Task::none(),
            }
        };

        let y_offset = cursor.y - list_bounds.y;
        let row_pos = ((y_offset + list_scroll) / crate::theme::ROW_HEIGHT).max(0.0);
        let row_idx = row_pos as usize;
        let drop_idx = if row_idx < track_count && row_pos.fract() >= 0.5 {
            row_idx + 1
        } else {
            row_idx
        };
        let drop_idx = drop_idx.min(track_count);

        let sel = self.selection(is_queue_drag).to_vec();

        if let (Some(min), Some(max)) = (sel.iter().copied().min(), sel.iter().copied().max()) {
            if drop_idx > min && drop_idx < max {
                self.drag_drop_target = None;
                return Task::none();
            }
        }

        self.drag_drop_target = Some(drop_idx);
        self.handle_drag_autoscroll(list_bounds, list_scroll, is_queue_drag, track_count)
    }

    fn handle_drag_autoscroll(
        &self,
        list_bounds: iced::Rectangle,
        current_scroll: f32,
        is_queue: bool,
        track_count: usize,
    ) -> Task<Message> {
        let cursor = self.cursor_pos;
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
            iced::widget::Id::new("queue_list")
        } else {
            iced::widget::Id::new("track_list")
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
        let Some(track_idx) = self.pressed_track else {
            return;
        };

        let cursor = self.cursor_pos;

        let was_in_selection = {
            let sel = self.selection(is_queue);
            !sel.is_empty() && sel.contains(&track_idx)
        };

        let indices: Vec<usize> = if was_in_selection {
            self.selection(is_queue).to_vec()
        } else {
            vec![track_idx]
        };

        if let Some(sidebar_bounds) = self.sidebar_bounds {
            if cursor.x < sidebar_bounds.x + crate::theme::SIDEBAR_WIDTH {
                let y_offset = cursor.y - sidebar_bounds.y;
                if y_offset >= 0.0 {
                    let playlist_idx = ((y_offset + self.sidebar_list_scroll)
                        / crate::theme::SIDEBAR_ITEM_HEIGHT)
                        as usize;
                    if playlist_idx < self.playlists.playlists.len() {
                        let mut count = 0;
                        for &i in indices.iter().rev() {
                            if let Some(track) = self.get_track_at(i, is_queue) {
                                let track = track.clone();
                                self.playlists.insert_track_at(playlist_idx, &track, 0);
                                count += 1;
                            }
                        }
                        self.playlists.save();
                        let name = self.playlists.playlists[playlist_idx].name.clone();
                        self.notify(format!(
                            "Added {} track{} to {}",
                            count,
                            if count == 1 { "" } else { "s" },
                            name
                        ));
                        return;
                    }
                }
            }
        }

        if let Some(drop_idx) = self.drag_drop_target {
            let min_idx = *indices.iter().min().unwrap();
            let max_idx = *indices.iter().max().unwrap();
            let is_valid_drop = drop_idx > max_idx || drop_idx < min_idx;

            if is_queue {
                let count = self.queue.tracks.len();
                if drop_idx <= count && is_valid_drop {
                    let new_positions = self.handle_reorder_queue(drop_idx, &indices);
                    if was_in_selection {
                        let sel = self.selection_mut(is_queue);
                        *sel = new_positions;
                    }
                    self.save_session();
                }
            } else {
                let count = self.current_track_count(false);
                if drop_idx <= count && is_valid_drop {
                    let new_positions = self.handle_reorder_tracks_selected(drop_idx, &indices);
                    if was_in_selection {
                        self.selected_indices = new_positions;
                    }
                }
            }
        }
    }

    pub fn handle_track_pressed(&mut self, index: usize, is_queue: bool) {
        let now = std::time::Instant::now();
        let is_double = self.last_click_index == Some(index)
            && now.duration_since(self.last_click_time).as_millis() < DOUBLE_CLICK_MS;

        self.last_click_index = Some(index);
        self.last_click_time = now;
        self.pressed_track = Some(index);
        self.pressed_track_is_queue = is_queue;
        self.drag_origin = Some(self.cursor_pos);
        self.drag_active = false;

        if is_double {
            self.pressed_track = None;
            self.drag_origin = None;
            self.handle_play_track(index, is_queue);
            self.toggle_selection(index, is_queue);
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

    pub fn get_track_at(&self, index: usize, is_queue: bool) -> Option<Track> {
        if is_queue {
            return self.queue.tracks.get(index).cloned();
        }
        match &self.current_view {
            View::Search => self.search_results.get(index).cloned(),
            View::SongRadio | View::ArtistRadio => self.radio_tracks.get(index).cloned(),
            View::Playlist | View::Downloads => self
                .selected_playlist
                .and_then(|sp| self.playlists.playlists.get(sp))
                .and_then(|p| p.tracks.get(index))
                .cloned(),
        }
    }

    pub fn current_track_count(&self, is_queue: bool) -> usize {
        if is_queue {
            return self.queue.tracks.len();
        }
        match &self.current_view {
            View::Search => self.search_results.len(),
            View::SongRadio | View::ArtistRadio => self.radio_tracks.len(),
            View::Playlist | View::Downloads => self
                .selected_playlist
                .and_then(|sp| self.playlists.playlists.get(sp))
                .map(|p| p.tracks.len())
                .unwrap_or(0),
        }
    }

    pub fn get_current_list_bounds(&self) -> Option<iced::Rectangle> {
        if self.current_view.is_search_like() {
            self.search_list_bounds
        } else {
            self.playlist_list_bounds
        }
    }

    pub fn get_current_list_scroll(&self) -> f32 {
        if self.current_view.is_search_like() {
            self.search_list_scroll
        } else {
            self.playlist_list_scroll
        }
    }

    pub fn handle_key_press(
        &mut self,
        key: &iced::keyboard::key::Key,
        modifiers: iced::keyboard::Modifiers,
    ) {
        use iced::keyboard::key::Named;
        match key {
            iced::keyboard::Key::Named(Named::Space) => {
                self.toggle_play_pause();
            }
            iced::keyboard::Key::Named(Named::Escape) => {
                if self.show_search_history {
                    self.show_search_history = false;
                } else if self.selected_indices.is_empty() {
                    self.handle_navigate_to(View::Search);
                } else {
                    self.clear_selection();
                }
            }
            iced::keyboard::Key::Named(Named::Delete) => {
                if self.selected_playlist.is_some() {
                    self.handle_delete_selected();
                }
            }
            iced::keyboard::Key::Named(Named::ArrowUp) => {
                if self.focused_list_index > 0 {
                    self.focused_list_index -= 1;
                }
            }
            iced::keyboard::Key::Named(Named::ArrowDown) => {
                let count = self.current_track_count(false);
                if self.focused_list_index < count.saturating_sub(1) {
                    self.focused_list_index += 1;
                }
            }
            iced::keyboard::Key::Named(Named::Enter) => {
                let count = self.current_track_count(false);
                if self.focused_list_index < count {
                    self.handle_play_track(self.focused_list_index, false);
                }
            }
            _ => {}
        }
        if modifiers.control() || modifiers.logo() {
            match key {
                iced::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("c") => {
                    self.handle_copy_selected();
                }
                iced::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("v") => {
                    self.handle_paste_clipboard();
                }
                _ => {}
            }
        }
    }

    pub fn handle_play_track(&mut self, index: usize, is_queue: bool) {
        if is_queue {
            // Jump to the selected queue entry without discarding the
            // tracks that precede it: the queue represents what's up next.
            if index < self.queue.tracks.len() {
                self.queue.current_index = index;
                if let Some(t) = self.queue.current() {
                    let t = t.clone();
                    self.play_track_internal(&t);
                }
            }
            return;
        }
        if let Some(track) = self.get_track_at(index, false) {
            let track = track.clone();
            self.play_track_internal(&track);
            self.queue.clear();
            self.queue.enqueue(track);
            let count = self.current_track_count(false);
            for i in (index + 1)..count {
                if let Some(t) = self.get_track_at(i, false) {
                    self.queue.enqueue(t);
                }
            }
        }
    }

    pub fn play_track_internal(&mut self, track: &Track) {
        self.track_loading = true;
        let id = track.id.clone();

        if self.stream_cache.contains(&id) {
            let path = self.stream_cache.path_for(&id);
            debug!("Playing from cache: {}", path.display());
            let duration = track.duration as f32;
            self.audio.play_cached(path, duration);
            self.pending_cache_id = None;
        } else {
            self.pending_cache_id = Some(id.clone());
            let cache_path = self.stream_cache.path_for(&id);
            let duration = track.duration as f32;
            self.audio
                .play_stream_cache(&track.url, duration, cache_path);
        }
    }

    pub fn handle_search_execute(&mut self) {
        if self.search_query.is_empty() {
            return;
        }

        // Switch views: push the current view (if not already Search) as a
        // back-target so Back can return to it. Re-searching on the Search
        // view is handled by push_nav_entry() once results arrive.
        if !matches!(self.current_view, View::Search) {
            self.push_nav_entry();
        }
        self.current_view = View::Search;

        self.search_loading = true;
        self.search_exhausted = false;
        self.notify(format!("Searching for \"{}\"...", self.search_query));
        self.search_results.clear();
        self.selected_indices.clear();

        if !self.config.search_history.contains(&self.search_query) {
            self.config.search_history.push(self.search_query.clone());
            if self.config.search_history.len() > self.config.max_search_history_stored {
                self.config.search_history.remove(0);
            }
            crate::config::save_config(&self.config);
        }

        let query = self.search_query.clone();
        let tx = self.result_tx.clone();

        std::thread::spawn(move || {
            let result = crate::youtube::search(&query, 0);
            match result {
                Ok(videos) => {
                    let tracks: Vec<Track> = videos.into_iter().map(|v| v.into()).collect();
                    let _ = tx.send(BackendResult::SearchResults(tracks));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::SearchError(e.to_string()));
                }
            }
        });
    }

    fn push_nav_entry(&mut self) {
        let entry = NavEntry {
            view: self.current_view.clone(),
            snapshot: self.snapshot_current(),
        };
        self.nav_history.truncate(self.nav_history_pos + 1);
        self.nav_history.push(entry);
        if self.nav_history.len() > 20 {
            self.nav_history.remove(0);
            self.nav_history_pos = self.nav_history_pos.saturating_sub(1);
        }
        self.nav_history_pos = self.nav_history.len() - 1;
    }

    pub fn handle_global_search(&mut self) {
        if self.search_query.trim().is_empty() {
            return;
        }
        self.show_search_history = false;
        self.handle_search_execute();
    }

    pub fn handle_search_load_more(&mut self) {
        // search_exhausted is set true when a page returned fewer than a full
        // SEARCH_PAGE_SIZE, so there is nothing left to fetch.
        if self.search_loading || self.search_exhausted || self.search_results.is_empty() {
            return;
        }
        self.search_loading = true;

        let query = self.search_query.clone();
        let offset = self.search_results.len();
        let tx = self.result_tx.clone();

        std::thread::spawn(move || {
            let result = crate::youtube::search_more(&query, offset);
            match result {
                Ok(videos) => {
                    let tracks: Vec<Track> = videos.into_iter().map(|v| v.into()).collect();
                    let _ = tx.send(BackendResult::SearchResultsAppend(tracks));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::SearchError(e.to_string()));
                }
            }
        });
    }

    pub fn handle_search_history_select(&mut self, index: usize) {
        if index < self.last_filtered_history.len() {
            self.search_query = self.last_filtered_history[index].clone();
            self.show_search_history = false;
            self.handle_search_execute();
        }
    }

    pub fn handle_delete_search_history(&mut self, index: usize) {
        if index < self.last_filtered_history.len() {
            let query = self.last_filtered_history[index].clone();
            self.config.search_history.retain(|q| q != &query);
            crate::config::save_config(&self.config);
            self.update_search_history();
        }
    }

    pub fn update_search_history(&mut self) {
        let query_lower = self.search_query.to_lowercase();
        self.last_filtered_history = if query_lower.is_empty() {
            self.config.search_history.clone()
        } else {
            self.config
                .search_history
                .iter()
                .filter(|q| crate::util::fuzzy_match(&query_lower, &q.to_lowercase()))
                .cloned()
                .collect()
        };
        if self.last_filtered_history.len() > self.config.max_search_history_visible {
            self.last_filtered_history
                .truncate(self.config.max_search_history_visible);
        }
        self.search_history_focused_index = 0;
    }

    pub fn start_song_radio(&mut self, song_name: String) {
        self.radio_label = format!("Radio: {}", song_name);
        self.search_loading = true;
        self.notify(format!("Generating radio for song: {}...", song_name));
        self.handle_navigate_to(View::SongRadio);

        let tx = self.result_tx.clone();
        let label = self.radio_label.clone();
        std::thread::spawn(move || {
            let result = crate::youtube::radio_song(&song_name);
            match result {
                Ok(videos) => {
                    let tracks: Vec<Track> = videos.into_iter().map(|v| v.into()).collect();
                    let _ = tx.send(BackendResult::RadioResults(label, tracks));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::SearchError(e.to_string()));
                }
            }
        });
    }

    pub fn start_artist_radio(&mut self, artist_name: String) {
        self.radio_label = format!("Radio: {}", artist_name);
        self.search_loading = true;
        self.notify(format!("Generating radio for artist: {}...", artist_name));
        self.handle_navigate_to(View::ArtistRadio);

        let tx = self.result_tx.clone();
        let label = self.radio_label.clone();
        std::thread::spawn(move || {
            let result = crate::youtube::radio_artist(&artist_name);
            match result {
                Ok(videos) => {
                    let tracks: Vec<Track> = videos.into_iter().map(|v| v.into()).collect();
                    let _ = tx.send(BackendResult::RadioResults(label, tracks));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::SearchError(e.to_string()));
                }
            }
        });
    }

    pub fn handle_create_playlist(&mut self) {
        if self.playlist_create_name.trim().is_empty() {
            return;
        }
        let name = self.playlist_create_name.trim().to_string();
        self.playlists.create(&name);
        self.playlist_create_name.clear();
        self.notify(format!("Playlist \"{}\" created", name));
    }

    pub fn handle_select_playlist(&mut self, index: usize) {
        if index < self.playlists.playlists.len() {
            self.selected_playlist = Some(index);
            self.selected_playlist_name = self.playlists.playlists[index].name.clone();
            self.show_playlist_picker = None;
            self.clear_selection();
            self.cleanup_drag_state();
            self.handle_navigate_to(View::Playlist);
        }
    }

    pub fn handle_rename_playlist(&mut self, new_name: String) {
        if let Some(idx) = self.selected_playlist {
            if !new_name.trim().is_empty() {
                self.playlists.playlists[idx].name = new_name.trim().to_string();
                self.playlists.save();
                self.selected_playlist_name = new_name.trim().to_string();
            }
        }
    }

    pub fn handle_delete_playlist(&mut self, index: usize) {
        self.playlists.delete(index);
        if self.selected_playlist == Some(index) {
            self.selected_playlist = None;
            self.selected_playlist_name.clear();
        } else if self.selected_playlist > Some(index) {
            self.selected_playlist = self.selected_playlist.map(|sp| sp - 1);
        }
        self.show_delete_confirm = false;
        self.delete_confirm_index = None;
    }

    pub fn handle_add_local_music(&mut self, paths: Vec<String>) {
        let mut new_tracks = Vec::new();
        for path_str in &paths {
            let path = Path::new(path_str);
            if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                new_tracks.push(Track {
                    id: filename.to_string(),
                    title: filename.to_string(),
                    artist: "Unknown Artist".to_string(),
                    duration: 0,
                    url: path_str.clone(),
                    source: TrackSource::Local,
                    thumbnail: String::new(),
                });
            }
        }

        let Some(idx) = self.selected_playlist else {
            let count = new_tracks.len();
            self.notify(format!(
                "Added {} local track{} (select a playlist to organize)",
                count,
                if count == 1 { "" } else { "s" }
            ));
            return;
        };

        for track in &new_tracks {
            self.playlists.insert_track_at(idx, track, usize::MAX);
        }
        self.playlists.save();

        let count = new_tracks.len();
        self.notify(format!(
            "Added {} local track{}",
            count,
            if count == 1 { "" } else { "s" }
        ));
    }

    pub fn handle_add_to_playlist(&mut self, playlist_idx: usize) {
        if playlist_idx >= self.playlists.playlists.len() {
            return;
        }
        let indices: Vec<usize> = if self.selected_indices.is_empty() {
            if let Some(t) = self.pressed_track {
                vec![t]
            } else {
                return;
            }
        } else {
            self.selected_indices.clone()
        };

        let mut count = 0;
        for &i in indices.iter().rev() {
            if let Some(track) = self.get_track_at(i, false) {
                let track = track.clone();
                self.playlists.insert_track_at(playlist_idx, &track, 0);
                count += 1;
            }
        }
        self.playlists.save();
        self.show_playlist_picker = None;
        let name = self.playlists.playlists[playlist_idx].name.clone();
        self.notify(format!(
            "Added {} track{} to {}",
            count,
            if count == 1 { "" } else { "s" },
            name
        ));
    }

    pub fn handle_remove_from_playlist(&mut self, index: usize) {
        if let Some(sp) = self.selected_playlist {
            if sp < self.playlists.playlists.len() {
                self.playlists.playlists[sp].tracks.remove(index);
                self.playlists.save();
            }
        }
    }

    pub fn handle_reorder_tracks_selected(
        &mut self,
        drop_idx: usize,
        indices: &[usize],
    ) -> Vec<usize> {
        let mut new_positions = Vec::new();
        if let Some(sp) = self.selected_playlist {
            if sp < self.playlists.playlists.len() {
                let tracks = &mut self.playlists.playlists[sp].tracks;
                let sorted_indices: Vec<usize> = {
                    let mut s = indices.to_vec();
                    s.sort_unstable();
                    s
                };
                let extracted: Vec<Track> = sorted_indices
                    .iter()
                    .filter_map(|&i| tracks.get(i).cloned())
                    .collect();
                for &i in sorted_indices.iter().rev() {
                    if i < tracks.len() {
                        tracks.remove(i);
                    }
                }
                let removed_before = sorted_indices.iter().filter(|&&i| i < drop_idx).count();
                let adjusted_drop = (drop_idx - removed_before).min(tracks.len());
                let new_count = extracted.len();
                for (j, track) in extracted.into_iter().enumerate() {
                    tracks.insert(adjusted_drop + j, track);
                }
                self.playlists.save();

                new_positions = (adjusted_drop..adjusted_drop + new_count).collect();
            }
        }
        new_positions
    }

    pub fn handle_reorder_queue(&mut self, drop_idx: usize, indices: &[usize]) -> Vec<usize> {
        let sorted_indices: Vec<usize> = {
            let mut s = indices.to_vec();
            s.sort_unstable();
            s
        };
        let extracted: Vec<Track> = sorted_indices
            .iter()
            .filter_map(|&i| self.queue.tracks.get(i).cloned())
            .collect();
        for &i in sorted_indices.iter().rev() {
            if i < self.queue.tracks.len() {
                self.queue.tracks.remove(i);
            }
        }
        let removed_before = sorted_indices.iter().filter(|&&i| i < drop_idx).count();
        let adjusted_drop = (drop_idx - removed_before).min(self.queue.tracks.len());
        let new_count = extracted.len();
        for (j, track) in extracted.into_iter().enumerate() {
            self.queue.tracks.insert(adjusted_drop + j, track);
        }
        self.save_session();

        (adjusted_drop..adjusted_drop + new_count).collect()
    }

    pub fn handle_remove_from_queue(&mut self, index: usize) {
        if index < self.queue.tracks.len() {
            self.queue.tracks.remove(index);
            self.save_session();
        }
    }

    pub fn handle_download_track(&mut self, index: usize, is_queue: bool) {
        if let Some(track) = self.get_track_at(index, is_queue) {
            let track = track.clone();
            self.downloading_index = Some(index);
            self.notify(format!("Downloading \"{}\"...", track.title));
            let download_dir = self.config.download_dir.clone();
            let tx = self.result_tx.clone();
            std::thread::spawn(move || {
                let result = crate::youtube::download(&track.url, &download_dir);
                match result {
                    Ok(path) => {
                        let _ = tx.send(BackendResult::DownloadComplete(track.url, path));
                    }
                    Err(e) => {
                        let _ = tx.send(BackendResult::DownloadError(e.to_string()));
                    }
                }
            });
        }
    }

    pub fn handle_remove_download(&mut self, index: usize, is_queue: bool) {
        if let Some(track) = self.get_track_at(index, is_queue) {
            let url = track.url.clone();
            self.download_registry.remove(&url);
        }
    }

    pub fn handle_copy_selected(&mut self) {
        self.clipboard.clear();
        for &i in &self.selected_indices {
            if let Some(track) = self.get_track_at(i, false) {
                self.clipboard.push(track.clone());
            }
        }
    }

    pub fn handle_paste_clipboard(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        let Some(idx) = self.selected_playlist else {
            return;
        };
        for track in self.clipboard.iter().rev() {
            self.playlists.insert_track_at(idx, track, 0);
        }
        self.playlists.save();
        let count = self.clipboard.len();
        let name = self.playlists.playlists[idx].name.clone();
        self.notify(format!(
            "Pasted {} track{} into {}",
            count,
            if count == 1 { "" } else { "s" },
            name
        ));
        self.clipboard.clear();
    }

    pub fn handle_delete_selected(&mut self) {
        if self.selected_indices.is_empty() {
            return;
        }
        match &self.current_view {
            View::Playlist | View::Downloads => {
                if let Some(sp) = self.selected_playlist {
                    if sp < self.playlists.playlists.len() {
                        let indices: Vec<usize> = self.selected_indices.clone();
                        let mut removed = 0;
                        for &i in indices.iter().rev() {
                            if i < self.playlists.playlists[sp].tracks.len() {
                                self.playlists.playlists[sp].tracks.remove(i);
                                removed += 1;
                            }
                        }
                        self.playlists.save();
                        self.notify(format!(
                            "Removed {} track{}",
                            removed,
                            if removed == 1 { "" } else { "s" }
                        ));
                    }
                }
            }
            _ => {}
        }
        self.clear_selection();
    }

    pub fn clear_selection(&mut self) {
        self.selected_indices.clear();
        self.queue_selected_indices.clear();
        self.show_playlist_picker = None;
    }

    pub fn toggle_play_pause(&mut self) {
        if self.queue.current().is_some() {
            if self.is_playing {
                self.audio.pause();
            } else {
                self.audio.resume();
            }
        } else {
            self.is_playing = !self.is_playing;
        }
        self.send_mpris_update();
    }

    pub fn next_track(&mut self) {
        if self.queue.next().is_some() {
            self.track_loading = true;
            if let Some(t) = self.queue.current() {
                let t = t.clone();
                self.play_track_internal(&t);
            }
            self.send_mpris_update();
        }
    }

    pub fn previous_track(&mut self) {
        if self.queue.previous().is_some() {
            self.track_loading = true;
            if let Some(t) = self.queue.current() {
                let t = t.clone();
                self.play_track_internal(&t);
            }
            self.send_mpris_update();
        }
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol.clamp(0.0, 1.0);
        self.audio.set_volume(self.volume);
    }

    pub fn seek(&mut self, frac: f32) {
        let frac = frac.clamp(0.0, 1.0);
        self.progress = frac;
        self.audio
            .seek(std::time::Duration::from_secs_f32(frac * self.duration));
    }

    pub fn handle_toggle_picker(&mut self, index: usize) {
        if self.show_playlist_picker == Some(index) {
            self.show_playlist_picker = None;
        } else {
            self.show_playlist_picker = Some(index);
            self.picker_focused_index = 0;
        }
    }

    pub fn show_context_menu(&mut self, index: usize, is_queue: bool) {
        let track = self.get_track_at(index, is_queue);
        let Some(track) = track else {
            return;
        };
        self.context_menu = Some(ContextMenuState {
            visible: true,
            track_index: index,
            position: (self.cursor_pos.x, self.cursor_pos.y),
            is_youtube: track.source == TrackSource::YouTube,
            is_downloaded: self.download_registry.contains(&track.url),
            in_playlist: matches!(self.current_view, View::Playlist),
            is_queue,
        });
    }
}
