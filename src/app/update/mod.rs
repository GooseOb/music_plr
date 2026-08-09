use super::{
    error, format_duration, mpris, mpsc, warn, BackendResult, ContextMenuState, DragTargetList,
    Message, MprisCommand, MprisUpdate, MusicPlayer, Task, Track, ViewData,
};
use crate::types::TrackSource;
use std::thread;

mod actions;
mod drag;
mod input;
mod navigation;
pub mod operation;
mod playback;
mod playlists;
mod search;
mod selection;
mod session;
mod tick;

const DOUBLE_CLICK_MS: u128 = 300;

pub fn spawn_thumbnail_download_thread(tracks: &[Track], tx: mpsc::Sender<BackendResult>) {
    let entries: Vec<(String, String)> = tracks
        .iter()
        .filter(|t| t.source == TrackSource::YouTube)
        .map(|t| (t.id.clone(), t.thumbnail.clone()))
        .collect();
    spawn_thumbnail_download(entries, tx);
}

/// Download thumbnails for the given `(id, url)` pairs. `id` names the cache
/// file; `url` is the source (empty falls back to the default YouTube still).
pub fn spawn_thumbnail_download(entries: Vec<(String, String)>, tx: mpsc::Sender<BackendResult>) {
    tracing::debug!(
        "Spawning thumbnail download thread for {} entries",
        entries.len()
    );
    if entries.is_empty() {
        return;
    }
    thread::spawn(move || {
        for (id, thumb) in &entries {
            crate::data::thumbnails::download(id, thumb);
        }
        let _ = tx.send(BackendResult::ThumbnailsDownloaded);
    });
}
