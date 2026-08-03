use super::*;
use crate::types::TrackSource;
use std::thread;

mod actions;
mod drag;
mod input;
mod navigation;
mod playback;
mod playlists;
mod search;
mod session;
mod tick;

const DOUBLE_CLICK_MS: u128 = 300;

pub fn spawn_thumbnail_download_thread(tracks: &[Track]) {
    let entries: Vec<(String, String)> = tracks
        .iter()
        .filter(|t| t.source == TrackSource::YouTube)
        .map(|t| (t.id.clone(), t.thumbnail.clone()))
        .collect();
    if entries.is_empty() {
        return;
    }
    thread::spawn(move || {
        for (id, thumb) in &entries {
            crate::thumbnails::download(id, thumb);
        }
    });
}
