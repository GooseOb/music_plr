use std::sync::mpsc;

use crate::audio::AudioPlayer;
use crate::config::Config;
use crate::downloads::DownloadRegistry;
use crate::mpris::{MprisCommand, MprisUpdate};
use crate::playlists::PlaylistStore;
use crate::types::{Track as RustTrack, TrackSource};

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
    pub focus_lost_ticks: u32,
    pub pending_search_text: Option<String>,
    pub last_filtered_history: Vec<String>,
    pub selected_indices: Vec<usize>,
    pub clipboard: Vec<RustTrack>,
    pub last_click_index: Option<usize>,
    pub last_click_time: std::time::Instant,
    pub search_model_handle: Option<std::rc::Rc<slint::VecModel<Track>>>,
    pub radio_model_handle: Option<std::rc::Rc<slint::VecModel<Track>>>,
    pub playlist_model_handle: Option<std::rc::Rc<slint::VecModel<Track>>>,
}

fn format_dur(dur: i32) -> String {
    if dur > 0 {
        format!("{}:{:02}", dur / 60, dur % 60)
    } else {
        "--:--".to_string()
    }
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
}
