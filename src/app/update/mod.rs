use std::thread;

use super::{
    error, mpris, mpsc, warn, BackendResult, ContextMenuState, Message, MprisCommand, MprisUpdate,
    MusicPlayer, Task, Track, TrackListKind, TrackPos, ViewData,
};

mod actions;
mod artist;
mod drag;
mod input;
mod navigation;
pub mod operation;
mod playback;
mod playlists;
mod search;
mod selection;
mod session;
pub mod settings;
mod tick;

const DOUBLE_CLICK_MS: u128 = 300;

/// Download thumbnails for the given `(id, url)` pairs. `id` names the cache
/// file; `url` is the source (empty falls back to the default `YouTube` still).
pub fn spawn_thumbnail_download(entries: Vec<(String, String)>, tx: mpsc::Sender<BackendResult>) {
    tracing::debug!(
        "Spawning thumbnail download thread for {} entries",
        entries.len()
    );
    if entries.is_empty() {
        return;
    }
    thread::spawn(move || {
        let mut downloaded = Vec::with_capacity(entries.len());
        for (id, thumb) in &entries {
            crate::data::thumbnails::download(id, thumb);
            downloaded.push(id.clone());
        }
        let _ = tx.send(BackendResult::ThumbnailsDownloaded(downloaded));
    });
}
