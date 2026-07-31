use std::sync::mpsc;

use slint::ComponentHandle;

use crate::audio::AudioPlayer;
use crate::cache::StreamCache;
use crate::config::Config;
use crate::downloads::DownloadRegistry;
use crate::mpris::MprisUpdate;
use crate::playlists::PlaylistStore;
use crate::types::{Track as RustTrack, TrackSource};

use serde::{Deserialize, Serialize};
use tracing::{debug, error};

slint::include_modules!();

pub mod download;
pub mod playback;
pub mod playlist;
pub mod radio;
pub mod search;
pub mod selection;
pub mod tick;

#[derive(Debug, Clone)]
pub enum BackendResult {
    SearchResults(Vec<RustTrack>),
    SearchResultsAppend(Vec<RustTrack>),
    RadioResults(String, Vec<RustTrack>),
    DownloadComplete(String, String),
    DownloadError(String),
    SearchError(String),
    ThumbnailsReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum View {
    #[default]
    Search,
    Radio,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

pub type EventFn = Box<dyn FnOnce(&mut Backend) + Send + 'static>;

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
    pub playlists: PlaylistStore,
    pub show_queue: bool,
    pub selected_playlist: Option<usize>,
    pub selected_playlist_name: String,
    pub playlist_create_name: String,
    pub show_playlist_picker: Option<usize>,
    pub nav_history: Vec<View>,
    pub nav_history_pos: usize,
    pub result_tx: mpsc::Sender<BackendResult>,
    pub event_tx: mpsc::Sender<EventFn>,
    pub event_rx: mpsc::Receiver<EventFn>,
    pub ui: slint::Weak<AppWindow>,
    pub audio_timer: Option<slint::Timer>,
    pub focus_lost_ticks: u32,
    pub last_filtered_history: Vec<String>,
    pub selected_indices: Vec<usize>,
    pub clipboard: Vec<RustTrack>,
    pub last_click_index: Option<usize>,
    pub last_click_time: std::time::Instant,
    pub search_model_handle: Option<std::rc::Rc<slint::VecModel<Track>>>,
    pub radio_model_handle: Option<std::rc::Rc<slint::VecModel<Track>>>,
    pub playlist_model_handle: Option<std::rc::Rc<slint::VecModel<Track>>>,
    pub queue_model_handle: Option<std::rc::Rc<slint::VecModel<Track>>>,
    pub stream_cache: StreamCache,
    pub pending_cache_id: Option<String>,
}

pub fn format_duration(secs: u32) -> String {
    if secs > 0 {
        format!("{}:{:02}", secs / 60, secs % 60)
    } else {
        "--:--".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(0), "--:--");
        assert_eq!(format_duration(30), "0:30");
        assert_eq!(format_duration(59), "0:59");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(60), "1:00");
        assert_eq!(format_duration(90), "1:30");
        assert_eq!(format_duration(369), "6:09");
        assert_eq!(format_duration(3600), "60:00");
    }

    #[test]
    fn play_queue_next_and_previous() {
        let mut q = PlayQueue::new();
        q.tracks = vec![
            RustTrack {
                id: "1".into(),
                title: "A".into(),
                artist: "X".into(),
                duration: 10,
                url: "".into(),
                source: TrackSource::YouTube,
                thumbnail: "".into(),
            },
            RustTrack {
                id: "2".into(),
                title: "B".into(),
                artist: "X".into(),
                duration: 10,
                url: "".into(),
                source: TrackSource::YouTube,
                thumbnail: "".into(),
            },
            RustTrack {
                id: "3".into(),
                title: "C".into(),
                artist: "X".into(),
                duration: 10,
                url: "".into(),
                source: TrackSource::YouTube,
                thumbnail: "".into(),
            },
        ];
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("1"));

        assert_eq!(q.next(), Some(1));
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("2"));

        assert_eq!(q.previous(), Some(0));
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("1"));

        assert_eq!(q.previous(), None);
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("1"));

        assert_eq!(q.next(), Some(1));
        assert_eq!(q.next(), Some(2));
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("3"));
        assert_eq!(q.next(), None);
    }

    #[test]
    fn play_queue_empty() {
        let q = PlayQueue::new();
        assert!(q.current().is_none());
    }
}

pub fn to_slint_track(
    t: &RustTrack,
    registry: &DownloadRegistry,
    selected: bool,
    downloading: bool,
) -> Track {
    let thumb = if t.source == TrackSource::YouTube {
        let cached = crate::thumbnails::thumbnail_path(&t.id);
        if cached.exists() {
            cached.to_string_lossy().to_string()
        } else if !t.thumbnail.is_empty() {
            t.thumbnail.clone()
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    Track {
        id: t.id.clone().into(),
        title: t.title.clone().into(),
        artist: t.artist.clone().into(),
        duration: t.duration as i32,
        duration_text: format_duration(t.duration).into(),
        url: t.url.clone().into(),
        source: match t.source {
            TrackSource::YouTube => "youtube".into(),
            TrackSource::Local => "local".into(),
        },
        is_downloaded: registry.contains(&t.url),
        is_downloading: downloading,
        is_selected: selected,
        thumbnail: thumb.into(),
    }
}

impl Backend {
    pub fn new(config: Config, result_tx: mpsc::Sender<BackendResult>) -> Self {
        let volume = config.volume;
        let (event_tx, event_rx) = mpsc::channel();
        let cache_max_mb = config.cache_max_size_mb;

        debug!(
            "Initializing Backend with volume={}, cache_max_mb={}",
            volume, cache_max_mb
        );

        Self {
            audio: AudioPlayer::new(volume),
            stream_cache: StreamCache::new(cache_max_mb),
            pending_cache_id: None,
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
            playlists: PlaylistStore::load(),
            show_queue: false,
            selected_playlist: None,
            selected_playlist_name: String::new(),
            playlist_create_name: String::new(),
            show_playlist_picker: None,
            nav_history: vec![View::Search],
            nav_history_pos: 0,
            result_tx,
            event_tx,
            event_rx,
            ui: slint::Weak::default(),
            audio_timer: None,
            focus_lost_ticks: 0,
            last_filtered_history: Vec::new(),
            selected_indices: Vec::new(),
            clipboard: Vec::new(),
            last_click_index: None,
            last_click_time: std::time::Instant::now(),
            search_model_handle: None,
            radio_model_handle: None,
            playlist_model_handle: None,
            queue_model_handle: None,
        }
    }

    pub fn get_track_at(&self, index: usize) -> Option<RustTrack> {
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

    pub fn get_current_tracks(&self) -> Vec<RustTrack> {
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
        let video_id = track.id.clone();
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
                        error!("Failed to read local file: {}", e);
                        self.download_registry.remove(&track_url);
                        self.cache_or_stream(&track_url, duration, &video_id);
                    }
                }
                return;
            } else {
                self.download_registry.remove(&track_url);
            }
        }
        self.cache_or_stream(&track_url, duration, &video_id);
    }

    fn cache_or_stream(&mut self, track_url: &str, duration: f32, video_id: &str) {
        if self.stream_cache.contains(video_id) {
            let cached_path = self.stream_cache.path_for(video_id);
            if cached_path.exists()
                && cached_path
                    .metadata()
                    .map(|m| m.len() > 4096)
                    .unwrap_or(false)
            {
                debug!("Playing from cache: {:?}", cached_path);
                self.track_loading = false;
                self.audio.play_cached(cached_path, duration);
                self.is_playing = true;
                self.stream_cache.record_access(video_id);
                return;
            }
            debug!("Cache too small or missing, removing and re-streaming");
            self.stream_cache.remove(video_id);
        }
        self.play_stream_with_cache(track_url, duration, video_id);
    }

    fn play_stream_with_cache(&mut self, track_url: &str, duration: f32, video_id: &str) {
        let cache_path = self.stream_cache.path_for(video_id);
        self.pending_cache_id = Some(video_id.to_string());
        self.audio
            .play_stream_cache(track_url, duration, cache_path);
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
        self.sync_current_track_ui();
    }

    pub fn process_result(&mut self, result: BackendResult) {
        match result {
            BackendResult::SearchResults(tracks) => {
                self.search_results = tracks;
                self.search_offset = self.search_results.len();
                self.loading = false;
                self.sync_search_model();
                self.clear_notification();
                if let Some(window) = self.ui.upgrade() {
                    window.global::<SearchState>().set_loading(false);
                }
            }
            BackendResult::SearchResultsAppend(tracks) => {
                self.search_offset += tracks.len();
                self.search_results.extend(tracks);
                self.loading = false;
                self.sync_search_model();
                self.clear_notification();
            }
            BackendResult::RadioResults(label, tracks) => {
                self.radio_label = label;
                self.radio_tracks = tracks;
                self.loading = false;
                self.current_view = View::Radio;
                self.sync_radio_model();
                self.update_nav_ui();
                self.save_session();
                if let Some(window) = self.ui.upgrade() {
                    window.global::<SearchState>().set_loading(false);
                }
            }
            BackendResult::DownloadComplete(url, path) => {
                self.downloading_index = None;
                self.download_registry.register(&url, &path);
                self.sync_search_model();
                self.sync_radio_model();
                self.sync_playlist_content();
                self.notify("Download complete!".into());
            }
            BackendResult::DownloadError(msg) => {
                self.downloading_index = None;
                error!("Download error: {}", msg);
                self.notify_error(msg);
            }
            BackendResult::SearchError(msg) => {
                self.loading = false;
                self.clear_notification();
                if let Some(window) = self.ui.upgrade() {
                    window.global::<SearchState>().set_loading(false);
                }
                self.notify_error(msg);
            }
            BackendResult::ThumbnailsReady => {
                self.sync_search_model();
                self.sync_radio_model();
                self.sync_playlist_content();
            }
        }
    }
}

pub fn spawn_thumbnail_download_thread(
    tracks: &[RustTrack],
    result_tx: mpsc::Sender<BackendResult>,
) {
    let entries: Vec<(String, String)> = tracks
        .iter()
        .filter(|t| t.source == TrackSource::YouTube)
        .map(|t| (t.id.clone(), t.thumbnail.clone()))
        .collect();
    if entries.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        for (id, thumb) in &entries {
            crate::thumbnails::download(id, thumb);
            let _ = result_tx.send(BackendResult::ThumbnailsReady);
        }
    });
}

impl Backend {
    pub fn spawn_thumbnail_downloads(&self, tracks: &[RustTrack]) {
        spawn_thumbnail_download_thread(tracks, self.result_tx.clone());
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
        let state = crate::session::SessionState {
            current_view: self.current_view,
            queue: self.queue.clone(),
            is_playing: self.is_playing,
            selected_playlist: self.selected_playlist,
            selected_playlist_name: self.selected_playlist_name.clone(),
            show_queue: self.show_queue,
        };
        state.save();
    }

    pub fn restore_session(&mut self) {
        let state = crate::session::SessionState::load();
        self.current_view = state.current_view;
        self.queue = state.queue;
        self.is_playing = state.is_playing;
        self.selected_playlist = state.selected_playlist;
        self.selected_playlist_name = state.selected_playlist_name;
        self.show_queue = state.show_queue;
        self.nav_history = vec![self.current_view];
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
}
