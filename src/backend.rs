use std::sync::mpsc;
use std::time::Duration;

use slint::Model;

use crate::audio::AudioPlayer;
use crate::config::{self, fuzzy_match, Config};
use crate::downloads::DownloadRegistry;
use crate::mpris::{MprisCommand, MprisUpdate};
use crate::playlists::PlaylistStore;
use crate::types::{Track as RustTrack, TrackSource};
use crate::youtube;

slint::include_modules!();

#[derive(Debug, Clone)]
pub enum BackendResult {
    SearchResults(Vec<RustTrack>),
    SearchResultsAppend(Vec<RustTrack>),
    RadioResults(String, Vec<RustTrack>),
    DownloadComplete(usize, String, String),
    DownloadError(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum View {
    Search,
    Radio,
}

#[derive(Debug, Clone)]
pub struct PlayQueue {
    pub tracks: Vec<RustTrack>,
    pub current_index: usize,
}

impl PlayQueue {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            current_index: 0,
        }
    }

    pub fn current(&self) -> Option<&RustTrack> {
        self.tracks.get(self.current_index)
    }

    pub fn next(&mut self) -> Option<usize> {
        if self.current_index + 1 < self.tracks.len() {
            self.current_index += 1;
            Some(self.current_index)
        } else {
            None
        }
    }

    pub fn previous(&mut self) -> Option<usize> {
        if self.current_index > 0 {
            self.current_index -= 1;
            Some(self.current_index)
        } else {
            None
        }
    }
}

pub struct Backend {
    pub audio: AudioPlayer,
    pub config: Config,
    pub current_view: View,
    pub search_query: String,
    pub search_results: Vec<RustTrack>,
    pub search_offset: usize,
    pub radio_tracks: Vec<RustTrack>,
    pub radio_label: String,
    pub queue: PlayQueue,
    pub is_playing: bool,
    pub volume: f32,
    pub progress: f32,
    pub duration: f32,
    pub download_registry: DownloadRegistry,
    pub downloading_index: Option<usize>,
    pub notification: Option<String>,
    pub loading: bool,
    pub track_loading: bool,
    pub mpris_update_tx: Option<mpsc::Sender<MprisUpdate>>,
    pub mpris_cmd_rx: mpsc::Receiver<MprisCommand>,
    pub playlists: PlaylistStore,
    pub selected_playlist: Option<usize>,
    pub selected_playlist_name: String,
    pub playlist_create_name: String,
    pub show_playlist_picker: Option<usize>,
    pub nav_history: Vec<View>,
    pub nav_history_pos: usize,
    pub result_rx: mpsc::Receiver<BackendResult>,
    pub result_tx: mpsc::Sender<BackendResult>,
    pub ui: slint::Weak<AppWindow>,
    pub _timer: Option<slint::Timer>,
    focus_lost_ticks: u32,
    pending_search_text: Option<String>,
    last_filtered_history: Vec<String>,
    selected_indices: Vec<usize>,
    clipboard: Vec<RustTrack>,
    last_click_index: Option<usize>,
    last_click_time: std::time::Instant,
    search_model_handle: Option<std::rc::Rc<slint::VecModel<Track>>>,
    radio_model_handle: Option<std::rc::Rc<slint::VecModel<Track>>>,
    playlist_model_handle: Option<std::rc::Rc<slint::VecModel<Track>>>,
}

fn format_dur(dur: i32) -> String {
    if dur > 0 {
        format!("{}:{:02}", dur / 60, dur % 60)
    } else {
        "--:--".to_string()
    }
}

fn safe_filename(artist: &str, title: &str) -> String {
    let filename = format!("{} - {}", artist, title);
    filename
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn to_slint_track(t: &RustTrack, registry: &DownloadRegistry, selected: bool) -> Track {
    Track {
        id: t.id.clone().into(),
        title: t.title.clone().into(),
        artist: t.artist.clone().into(),
        duration: t.duration as i32,
        duration_text: format_dur(t.duration as i32).into(),
        url: t.url.clone().into(),
        source: match t.source {
            TrackSource::YouTube => "youtube".into(),
            TrackSource::Local => "local".into(),
        },
        is_downloaded: registry.contains(&t.url),
        is_downloading: false,
        is_selected: selected,
    }
}

impl Backend {
    pub fn new(config: Config) -> (Self, mpsc::Sender<MprisCommand>) {
        let (mpris_cmd_tx, mpris_cmd_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let volume = config.volume;

        (
            Self {
                audio: AudioPlayer::new(volume),
                config,
                current_view: View::Search,
                search_query: String::new(),
                search_results: Vec::new(),
                search_offset: 0,
                radio_tracks: Vec::new(),
                radio_label: String::new(),
                queue: PlayQueue::new(),
                is_playing: false,
                volume,
                progress: 0.0,
                duration: 0.0,
                download_registry: DownloadRegistry::load(),
                downloading_index: None,
                notification: None,
                loading: false,
                track_loading: false,
                mpris_update_tx: None,
                mpris_cmd_rx,
                playlists: PlaylistStore::load(),
                selected_playlist: None,
                selected_playlist_name: String::new(),
                playlist_create_name: String::new(),
                show_playlist_picker: None,
                nav_history: vec![View::Search],
                nav_history_pos: 0,
                result_rx,
                result_tx,
                ui: slint::Weak::default(),
                _timer: None,
                focus_lost_ticks: 0,
                pending_search_text: None,
                last_filtered_history: Vec::new(),
                selected_indices: Vec::new(),
                clipboard: Vec::new(),
                last_click_index: None,
                last_click_time: std::time::Instant::now(),
                search_model_handle: None,
                radio_model_handle: None,
                playlist_model_handle: None,
            },
            mpris_cmd_tx,
        )
    }

    fn get_track_at(&self, index: usize) -> Option<RustTrack> {
        if let Some(pl_idx) = self.selected_playlist {
            return self
                .playlists
                .playlists
                .get(pl_idx)
                .and_then(|pl| pl.tracks.get(index).cloned());
        }
        match self.current_view {
            View::Radio => self.radio_tracks.get(index).cloned(),
            _ => self.search_results.get(index).cloned(),
        }
    }

    fn get_current_tracks(&self) -> Vec<RustTrack> {
        if let Some(pl_idx) = self.selected_playlist {
            return self
                .playlists
                .playlists
                .get(pl_idx)
                .map(|pl| pl.tracks.clone())
                .unwrap_or_default();
        }
        match &self.current_view {
            View::Radio => self.radio_tracks.clone(),
            _ => self.search_results.clone(),
        }
    }

    fn play_youtube_track(&mut self, track: &RustTrack) {
        let track_url = track.url.clone();
        let duration = track.duration as f32;
        self.track_loading = true;
        self.is_playing = false;
        self.duration = duration;
        self.progress = 0.0;

        if let Some(path) = self
            .download_registry
            .get_path(&track_url)
            .map(|s| s.to_string())
        {
            if std::path::Path::new(&path).exists() {
                match std::fs::read(&path) {
                    Ok(data) => {
                        self.track_loading = false;
                        self.audio.play(data, duration);
                        self.is_playing = true;
                    }
                    Err(e) => {
                        eprintln!("[play] failed to read local file: {}", e);
                        self.download_registry.remove(&track_url);
                        self.audio.play_stream(&track_url, duration);
                    }
                }
                return;
            } else {
                self.download_registry.remove(&track_url);
            }
        }
        self.audio.play_stream(&track_url, duration);
    }

    fn play_local_track(&mut self, track: &RustTrack) {
        self.track_loading = true;
        self.is_playing = false;
        if let Ok(data) = std::fs::read(&track.url) {
            let dur = track.duration as f32;
            self.track_loading = false;
            self.audio.play(data, dur);
            self.duration = dur;
            self.is_playing = true;
        }
    }

    fn play_track_internal(&mut self, track: &RustTrack) {
        match track.source {
            TrackSource::YouTube => self.play_youtube_track(track),
            TrackSource::Local => self.play_local_track(track),
        }
    }

    pub fn tick(&mut self) {
        let s = self.audio.get_state();
        self.is_playing = s.is_playing;
        self.progress = s.progress;
        self.duration = s.duration;
        if self.track_loading && s.is_playing {
            self.track_loading = false;
        }

        while let Ok(cmd) = self.mpris_cmd_rx.try_recv() {
            match cmd {
                MprisCommand::TogglePlayPause => {
                    if self.is_playing {
                        self.audio.pause();
                        self.is_playing = false;
                    } else if self.queue.current().is_some() {
                        self.audio.resume();
                        self.is_playing = true;
                    }
                }
                MprisCommand::NextTrack => {
                    if self.queue.next().is_some() {
                        if let Some(track) = self.queue.current().cloned() {
                            self.play_track_internal(&track);
                        }
                    }
                }
                MprisCommand::PreviousTrack => {
                    if self.queue.previous().is_some() {
                        if let Some(track) = self.queue.current().cloned() {
                            self.play_track_internal(&track);
                        }
                    }
                }
                MprisCommand::SetVolume(vol) => {
                    self.volume = vol;
                    self.config.volume = vol;
                    self.audio.set_volume(vol);
                    config::save_config(&self.config);
                }
                MprisCommand::Seek(delta_us) => {
                    let current_progress = self.progress;
                    let delta_frac = delta_us as f32 / 1_000_000.0 / self.duration.max(0.001);
                    let new_frac = (current_progress + delta_frac).clamp(0.0, 1.0);
                    self.handle_seek(new_frac);
                }
            }
        }

        let mut models_changed = false;
        while let Ok(result) = self.result_rx.try_recv() {
            models_changed = true;
            match result {
                BackendResult::SearchResults(tracks) => {
                    self.search_results = tracks;
                    self.search_offset = self.search_results.len();
                    self.loading = false;
                }
                BackendResult::SearchResultsAppend(tracks) => {
                    self.search_offset += tracks.len();
                    self.search_results.extend(tracks);
                    self.loading = false;
                }
                BackendResult::RadioResults(label, tracks) => {
                    self.radio_label = label;
                    self.radio_tracks = tracks;
                    self.loading = false;
                    self.current_view = View::Radio;
                }
                BackendResult::DownloadComplete(_idx, url, path) => {
                    self.downloading_index = None;
                    self.download_registry.register(&url, &path);
                    self.notification = Some("Download complete!".into());
                }
                BackendResult::DownloadError(msg) => {
                    self.downloading_index = None;
                    eprintln!("[backend] Download error: {}", msg);
                }
            }
        }

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

        let showing = self
            .ui
            .upgrade()
            .map(|w| w.get_show_search_history())
            .unwrap_or(false);

        if let Some(window) = self.ui.upgrade() {
            let has_focus = window.get_search_has_focus();
            if has_focus && !showing {
                self.update_search_history(&window);
                window.set_show_search_history(true);
            } else if !has_focus && showing {
                self.focus_lost_ticks += 1;
                if self.focus_lost_ticks >= 2 {
                    window.set_show_search_history(false);
                    self.focus_lost_ticks = 0;
                }
            } else if has_focus {
                self.focus_lost_ticks = 0;
            }
        }

        if let Some(text) = self.pending_search_text.take() {
            eprintln!("[tick.pending] text='{}'", text);
            if let Some(w) = self.ui.upgrade() {
                eprintln!("[tick.pending] calling set_search_input_text");
                w.set_search_input_text(text.into());
                w.set_show_search_history(false);
            }
            self.handle_search_execute();
        }

        if models_changed {
            self.update_ui();
        } else if !showing {
            self.update_playback_ui();
        }
    }

    fn update_playback_ui(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        window.set_current_view(self.current_view as i32);
        window.set_is_playing(self.is_playing);
        window.set_progress(self.progress);
        window.set_duration_secs(self.duration);
        window.set_volume(self.volume);
        window.set_track_loading(self.track_loading);
        window.set_can_go_back(self.nav_history_pos > 0);
        window.set_can_go_forward(self.nav_history_pos + 1 < self.nav_history.len());

        if let Some(track) = self.queue.current() {
            window.set_current_title(track.title.clone().into());
            window.set_current_artist(track.artist.clone().into());
            window.set_current_track_id(track.id.clone().into());
        } else {
            window.set_current_title("".into());
            window.set_current_artist("".into());
            window.set_current_track_id("".into());
        }

        let elapsed = (self.progress * self.duration) as u32;
        let total = self.duration as u32;
        window.set_elapsed_text(format!("{}:{:02}", elapsed / 60, elapsed % 60).into());
        window.set_total_text(format!("{}:{:02}", total / 60, total % 60).into());
        window.set_loading(self.loading);
        window.set_notification(self.notification.as_deref().unwrap_or("").into());
    }

    pub fn update_search_history(&mut self, window: &AppWindow) {
        let filtered: Vec<String> = self
            .filtered_history()
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        if filtered != self.last_filtered_history {
            self.last_filtered_history = filtered.clone();
            let slint_items: Vec<slint::SharedString> =
                filtered.iter().map(|s| s.as_str().into()).collect();
            window.set_search_history_items(
                std::rc::Rc::new(slint::VecModel::from(slint_items)).into(),
            );
        }
    }

    fn filtered_history(&self) -> Vec<(usize, String)> {
        let history = &self.config.search_history;
        let max_visible = self.config.max_search_history_visible;
        let query = self.search_query.trim().to_lowercase();
        if query.is_empty() {
            history
                .iter()
                .take(max_visible)
                .enumerate()
                .map(|(i, s)| (i, s.clone()))
                .collect()
        } else {
            history
                .iter()
                .enumerate()
                .filter(|(_, s)| fuzzy_match(&query, s))
                .take(max_visible)
                .map(|(i, s)| (i, s.clone()))
                .collect()
        }
    }

    fn is_selected(&self, index: usize) -> bool {
        self.selected_indices.contains(&index)
    }

    fn clear_selection(&mut self) {
        self.selected_indices.clear();
    }

    pub fn update_ui(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        self.update_playback_ui();
        self.update_search_history(&window);

        let registry = &self.download_registry;

        let search_model: Vec<Track> = self
            .search_results
            .iter()
            .enumerate()
            .map(|(i, t)| to_slint_track(t, registry, self.is_selected(i)))
            .collect();
        let search_model_rc = std::rc::Rc::new(slint::VecModel::from(search_model));
        self.search_model_handle = Some(search_model_rc.clone());
        window.set_search_results(search_model_rc.into());

        let radio_model: Vec<Track> = self
            .radio_tracks
            .iter()
            .enumerate()
            .map(|(i, t)| to_slint_track(t, registry, self.is_selected(i)))
            .collect();
        let radio_model_rc = std::rc::Rc::new(slint::VecModel::from(radio_model));
        self.radio_model_handle = Some(radio_model_rc.clone());
        window.set_radio_tracks(radio_model_rc.into());

        let playlist_list: Vec<PlaylistInfo> = self
            .playlists
            .playlists
            .iter()
            .map(|pl| PlaylistInfo {
                name: pl.name.clone().into(),
                track_count: pl.tracks.len() as i32,
            })
            .collect();
        window.set_playlist_list(std::rc::Rc::new(slint::VecModel::from(playlist_list)).into());

        let picker_names: Vec<slint::SharedString> = self
            .playlists
            .playlists
            .iter()
            .map(|pl| pl.name.clone().into())
            .collect();
        window.set_picker_playlists(std::rc::Rc::new(slint::VecModel::from(picker_names)).into());

        if let Some(idx) = self.selected_playlist {
            if let Some(pl) = self.playlists.playlists.get(idx) {
                let pt_model: Vec<Track> = pl
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| to_slint_track(t, registry, self.is_selected(i)))
                    .collect();
                let pt_model_rc = std::rc::Rc::new(slint::VecModel::from(pt_model));
                self.playlist_model_handle = Some(pt_model_rc.clone());
                window.set_playlist_tracks(pt_model_rc.into());
                window.set_selected_playlist_name(self.selected_playlist_name.clone().into());
                window.set_selected_playlist(idx as i32);
                window.set_playlist_create_name(self.playlist_create_name.clone().into());
                return;
            }
        }
        self.playlist_model_handle = None;
        window.set_selected_playlist(-1);
        window.set_selected_playlist_name("".into());
        window.set_playlist_tracks(std::rc::Rc::new(slint::VecModel::<Track>::from(vec![])).into());
        window.set_playlist_create_name(self.playlist_create_name.clone().into());
    }

    pub fn handle_search_execute(&mut self) {
        let query = self.search_query.clone();
        if query.trim().is_empty() {
            return;
        }
        self.selected_playlist = None;
        self.selected_playlist_name.clear();
        self.clear_selection();
        self.loading = true;

        // Add to search history
        self.config.search_history.retain(|h| h != &query);
        self.config.search_history.insert(0, query.clone());
        self.config
            .search_history
            .truncate(self.config.max_search_history_stored);
        self.config.last_search_query = query.clone();
        config::save_config(&self.config);

        let result_tx = self.result_tx.clone();
        std::thread::spawn(move || match youtube::search(&query, 0) {
            Ok(videos) => {
                let tracks: Vec<RustTrack> = videos.into_iter().map(RustTrack::from).collect();
                let _ = result_tx.send(BackendResult::SearchResults(tracks));
            }
            Err(e) => {
                eprintln!("[backend] Search error: {}", e);
                let _ = result_tx.send(BackendResult::SearchResults(Vec::new()));
            }
        });
    }

    pub fn handle_search_load_more(&mut self) {
        let query = self.search_query.clone();
        let offset = self.search_offset;
        self.loading = true;
        let result_tx = self.result_tx.clone();
        std::thread::spawn(move || match youtube::search(&query, offset) {
            Ok(videos) => {
                let tracks: Vec<RustTrack> = videos.into_iter().map(RustTrack::from).collect();
                let _ = result_tx.send(BackendResult::SearchResultsAppend(tracks));
            }
            Err(e) => {
                eprintln!("[backend] Search load more error: {}", e);
                let _ = result_tx.send(BackendResult::SearchResultsAppend(Vec::new()));
            }
        });
    }

    pub fn handle_search_history_select(&mut self, index: usize) {
        let items: Vec<String> = self
            .filtered_history()
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        if let Some(selected) = items.get(index) {
            self.search_query = selected.clone();
            self.pending_search_text = Some(selected.clone());
        }
    }

    pub fn handle_delete_search_history(&mut self, index: usize) {
        let items: Vec<usize> = self
            .filtered_history()
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        if let Some(&real_idx) = items.get(index) {
            self.config.search_history.remove(real_idx);
            config::save_config(&self.config);
            if let Some(w) = self.ui.upgrade() {
                self.update_search_history(&w);
            }
        }
    }

    pub fn handle_play_track(&mut self, index: usize) {
        let tracks = self.get_current_tracks();
        if tracks.is_empty() {
            return;
        }
        let track = match tracks.get(index) {
            Some(t) => t.clone(),
            None => return,
        };
        self.queue = PlayQueue {
            tracks: tracks.clone(),
            current_index: index,
        };

        self.play_track_internal(&track);
    }

    pub fn handle_toggle_play_pause(&mut self) {
        if self.is_playing {
            self.audio.pause();
            self.is_playing = false;
        } else if self.queue.current().is_some() {
            self.audio.resume();
            self.is_playing = true;
        }
    }

    pub fn handle_next_track(&mut self) {
        if self.queue.next().is_none() {
            return;
        }
        let track = match self.queue.current() {
            Some(t) => t.clone(),
            None => return,
        };
        self.play_track_internal(&track);
    }

    pub fn handle_previous_track(&mut self) {
        if self.queue.previous().is_none() {
            return;
        }
        let track = match self.queue.current() {
            Some(t) => t.clone(),
            None => return,
        };
        self.play_track_internal(&track);
    }

    pub fn handle_set_volume(&mut self, vol: f32) {
        self.volume = vol;
        self.config.volume = vol;
        self.audio.set_volume(vol);
        config::save_config(&self.config);
    }

    pub fn handle_seek(&mut self, frac: f32) {
        let pos = Duration::from_secs_f32(frac * self.duration);
        self.progress = frac;
        self.audio.seek(pos);
    }

    pub fn handle_start_song_radio(&mut self, track_name: String) {
        self.clear_selection();
        self.loading = true;
        self.radio_label = format!("Song Radio: {}", track_name);
        let query = format!("similar to {}", track_name);
        let result_tx = self.result_tx.clone();
        std::thread::spawn(move || match youtube::search(&query, 0) {
            Ok(videos) => {
                let tracks: Vec<RustTrack> = videos.into_iter().map(RustTrack::from).collect();
                let _ = result_tx.send(BackendResult::RadioResults(
                    format!("Song Radio: {}", track_name),
                    tracks,
                ));
            }
            Err(e) => {
                eprintln!("[backend] Radio error: {}", e);
                let _ = result_tx.send(BackendResult::RadioResults(
                    format!("Song Radio: {}", track_name),
                    Vec::new(),
                ));
            }
        });
    }

    pub fn handle_start_artist_radio(&mut self, artist_name: String) {
        self.clear_selection();
        self.loading = true;
        self.radio_label = format!("Artist Radio: {}", artist_name);
        let query = format!("top tracks by {}", artist_name);
        let result_tx = self.result_tx.clone();
        std::thread::spawn(move || match youtube::search(&query, 0) {
            Ok(videos) => {
                let tracks: Vec<RustTrack> = videos.into_iter().map(RustTrack::from).collect();
                let _ = result_tx.send(BackendResult::RadioResults(
                    format!("Artist Radio: {}", artist_name),
                    tracks,
                ));
            }
            Err(e) => {
                eprintln!("[backend] Artist radio error: {}", e);
                let _ = result_tx.send(BackendResult::RadioResults(
                    format!("Artist Radio: {}", artist_name),
                    Vec::new(),
                ));
            }
        });
    }

    pub fn handle_download_track(&mut self, index: usize) {
        let track = match self.get_track_at(index) {
            Some(t) => t,
            None => return,
        };
        let download_dir = self.config.download_dir.clone();
        let output_path = format!(
            "{}/{}.%(ext)s",
            download_dir,
            safe_filename(&track.artist, &track.title)
        );
        self.downloading_index = Some(index);

        let track_url = track.url.clone();
        let result_tx = self.result_tx.clone();
        std::thread::spawn(move || {
            match crate::youtube::download_audio(&track_url, &output_path) {
                Ok(path) => {
                    let _ = result_tx.send(BackendResult::DownloadComplete(index, track_url, path));
                }
                Err(e) => {
                    let _ = result_tx.send(BackendResult::DownloadError(e.to_string()));
                }
            }
        });
    }

    pub fn handle_remove_download(&mut self, index: usize) {
        let track = match self.get_track_at(index) {
            Some(t) => t,
            None => return,
        };
        if let Some(path) = self.download_registry.remove(&track.url) {
            let _ = std::fs::remove_file(&path);
            self.notification = Some("Download removed".into());
        }
        self.update_ui();
    }

    pub fn handle_download_current(&mut self) {
        let track = match self.queue.current() {
            Some(t) => t.clone(),
            None => return,
        };
        let download_dir = self.config.download_dir.clone();
        let output_path = format!(
            "{}/{}.%(ext)s",
            download_dir,
            safe_filename(&track.artist, &track.title)
        );

        let track_url = track.url.clone();
        let result_tx = self.result_tx.clone();
        std::thread::spawn(move || {
            match crate::youtube::download_audio(&track_url, &output_path) {
                Ok(path) => {
                    let _ = result_tx.send(BackendResult::DownloadComplete(0, track_url, path));
                }
                Err(e) => {
                    let _ = result_tx.send(BackendResult::DownloadError(e.to_string()));
                }
            }
        });
    }

    pub fn handle_add_local_music(&mut self, paths: Vec<String>) {
        let tracks: Vec<RustTrack> = paths
            .iter()
            .map(|path_str| {
                let file_stem = std::path::Path::new(path_str)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let id = format!("local:{}", path_str);
                RustTrack {
                    id,
                    title: file_stem,
                    artist: "Local File".to_string(),
                    duration: 0,
                    url: path_str.clone(),
                    source: TrackSource::Local,
                }
            })
            .collect();

        let count = tracks.len();
        if let Some(pl_idx) = self.selected_playlist {
            for t in tracks {
                self.playlists.add_track(pl_idx, &t);
            }
            self.notification = Some(format!(
                "Added {} file{}",
                count,
                if count == 1 { "" } else { "s" }
            ));
        } else if count > 0 {
            let name = "Local Music".to_string();
            self.playlists.create(&name);
            self.selected_playlist = Some(0);
            for t in tracks {
                self.playlists.add_track(0, &t);
            }
            self.notification = Some(format!(
                "Created playlist and added {} file{}",
                count,
                if count == 1 { "" } else { "s" }
            ));
        }
        self.update_ui();
    }

    pub fn handle_create_playlist(&mut self) {
        let name = self.playlist_create_name.trim().to_string();
        if !name.is_empty() {
            self.playlists.create(&name);
            self.playlist_create_name.clear();
            if self.playlists.playlists.len() == 1 {
                self.selected_playlist = Some(0);
            }
        }
        self.update_ui();
    }

    pub fn handle_delete_playlist(&mut self, index: usize) {
        self.playlists.delete(index);
        if self.selected_playlist == Some(index) {
            self.selected_playlist = None;
        } else if let Some(i) = self.selected_playlist {
            if i > index {
                self.selected_playlist = Some(i - 1);
            }
        }
        self.clear_selection();
        self.update_ui();
    }

    pub fn handle_select_playlist(&mut self, index: usize) {
        if cfg!(debug_assertions) {
            eprintln!("[backend] select playlist {}", index);
        }
        if self.selected_playlist == Some(index) {
            self.selected_playlist = None;
            self.selected_playlist_name.clear();
        } else {
            self.selected_playlist = Some(index);
            self.selected_playlist_name = self
                .playlists
                .playlists
                .get(index)
                .map(|pl| pl.name.clone())
                .unwrap_or_default();
        }
        self.clear_selection();
        self.update_ui();
    }

    pub fn handle_navigate_to(&mut self, view: View) {
        self.clear_selection();
        self.nav_history.truncate(self.nav_history_pos + 1);
        self.nav_history.push(view);
        self.nav_history_pos = self.nav_history.len() - 1;
        self.current_view = view;
    }

    pub fn handle_navigate_back(&mut self) {
        if self.nav_history_pos > 0 {
            self.nav_history_pos -= 1;
            self.current_view = self.nav_history[self.nav_history_pos];
        }
    }

    pub fn handle_navigate_forward(&mut self) {
        if self.nav_history_pos + 1 < self.nav_history.len() {
            self.nav_history_pos += 1;
            self.current_view = self.nav_history[self.nav_history_pos];
        }
    }

    pub fn handle_toggle_picker(&mut self, index: usize) {
        if self.show_playlist_picker == Some(index) {
            self.show_playlist_picker = None;
            if let Some(window) = self.ui.upgrade() {
                window.set_show_picker(false);
            }
        } else if self.playlists.playlists.is_empty() {
            self.notification = Some("Create a playlist first".into());
        } else {
            self.show_playlist_picker = Some(index);
            if let Some(window) = self.ui.upgrade() {
                window.set_picker_track_idx(index as i32);
                window.set_show_picker(true);
            }
        }
    }

    pub fn handle_add_to_playlist(&mut self, playlist_idx: usize) {
        let track_idx = self.show_playlist_picker.unwrap_or(0);
        let track = match self.get_track_at(track_idx) {
            Some(t) => t,
            None => return,
        };
        self.playlists.add_track(playlist_idx, &track);
        self.notification = Some(format!(
            "Added to {}",
            self.playlists.playlists[playlist_idx].name
        ));
        self.show_playlist_picker = None;
        if let Some(window) = self.ui.upgrade() {
            window.set_show_picker(false);
        }
        self.update_ui();
    }

    pub fn handle_remove_from_playlist(&mut self, track_idx: usize) {
        if let Some(pl_idx) = self.selected_playlist {
            self.playlists.remove_track(pl_idx, track_idx);
        }
        self.clear_selection();
        self.update_ui();
    }

    pub fn handle_radio_at(&mut self, index: usize) {
        if let Some(track) = self.get_track_at(index) {
            self.handle_start_song_radio(track.title);
        }
    }

    pub fn handle_artist_at(&mut self, index: usize) {
        if let Some(track) = self.get_track_at(index) {
            self.handle_start_artist_radio(track.artist);
        }
    }

    pub fn handle_download_or_delete_at(&mut self, index: usize) {
        let track = match self.get_track_at(index) {
            Some(t) => t,
            None => return,
        };
        if self.download_registry.contains(&track.url) {
            self.handle_remove_download(index);
        } else {
            self.handle_download_track(index);
        }
    }

    fn update_selection_row(&mut self, index: usize) {
        let tracks = self.get_current_tracks();
        if index >= tracks.len() {
            return;
        }
        let model = if self.selected_playlist.is_some() {
            self.playlist_model_handle.as_ref()
        } else if self.current_view == View::Radio {
            self.radio_model_handle.as_ref()
        } else {
            self.search_model_handle.as_ref()
        };
        if let Some(model) = model {
            if let Some(track) = tracks.get(index) {
                let registry = &self.download_registry;
                let t = to_slint_track(track, registry, self.is_selected(index));
                model.set_row_data(index, t);
            }
        }
    }

    pub fn handle_toggle_select(&mut self, index: usize) {
        let now = std::time::Instant::now();
        let is_double = self.last_click_index == Some(index)
            && now.duration_since(self.last_click_time).as_millis() < 300;

        self.last_click_index = Some(index);
        self.last_click_time = now;

        if is_double {
            self.handle_play_track(index);
        }
        // Absence of else here means that a double click will also toggle selection, which is intended behavior.
        if let Some(pos) = self.selected_indices.iter().position(|&i| i == index) {
            self.selected_indices.remove(pos);
        } else {
            self.selected_indices.push(index);
        }
        self.update_selection_row(index);
        self.update_playback_ui();
    }

    pub fn handle_copy_selected(&mut self) {
        self.clipboard.clear();
        let tracks = self.get_current_tracks();
        for &i in &self.selected_indices {
            if let Some(t) = tracks.get(i) {
                self.clipboard.push(t.clone());
            }
        }
        if !self.clipboard.is_empty() {
            self.notification = Some(format!(
                "Copied {} track{}",
                self.clipboard.len(),
                if self.clipboard.len() == 1 { "" } else { "s" }
            ));
            self.update_ui();
        }
    }

    pub fn handle_delete_selected(&mut self) {
        if self.selected_playlist.is_none() {
            self.notification = Some("Only playlist tracks can be deleted".into());
            self.update_ui();
            return;
        }
        let pl_idx = match self.selected_playlist {
            Some(i) => i,
            None => return,
        };
        let mut indices: Vec<usize> = self.selected_indices.clone();
        indices.sort_unstable_by(|a, b| b.cmp(a));
        for &i in &indices {
            self.playlists.remove_track(pl_idx, i);
        }
        let count = indices.len();
        self.clear_selection();
        self.notification = Some(format!(
            "Deleted {} track{}",
            count,
            if count == 1 { "" } else { "s" }
        ));
        self.update_ui();
    }

    pub fn handle_paste_clipboard(&mut self) {
        if self.clipboard.is_empty() {
            self.notification = Some("Nothing to paste".into());
            self.update_ui();
            return;
        }
        let pl_idx = match self.selected_playlist {
            Some(i) => i,
            None => {
                self.notification = Some("Select a playlist first".into());
                self.update_ui();
                return;
            }
        };
        let count = self.clipboard.len();
        for t in &self.clipboard {
            self.playlists.add_track(pl_idx, t);
        }
        self.notification = Some(format!(
            "Pasted {} track{}",
            count,
            if count == 1 { "" } else { "s" }
        ));
        self.update_ui();
    }
}
