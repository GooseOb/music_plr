use super::{BackendResult, RustTrack};
use crate::youtube;

impl super::Backend {
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
}
