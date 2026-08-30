use std::{sync::mpsc, thread};

use iced::Task;
use tracing::{error, warn};

use crate::{
    app::{
        interaction::{ContextMenuState, TrackListKind, TrackPos},
        message::{BackendResult, Message},
        view_data::ViewData,
        MusicPlayer,
    },
    mpris::{self, MprisCommand, MprisUpdate},
    types::Track,
};

mod actions;
mod artist;
mod dispatch;
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
        "Spawning thumbnail download threads for {} entries",
        entries.len()
    );
    for (id, thumb) in entries {
        let tx = tx.clone();
        thread::spawn(move || {
            crate::data::thumbnails::download(&id, &thumb);
            let _ = tx.send(BackendResult::ThumbnailDownloaded(id));
        });
    }
}
