use std::{sync::mpsc, thread};

use iced::Task;
use tracing::{error, warn};

use crate::{
    app::{
        interaction::{TrackListKind, TrackPos},
        message::{BackendResult, Message},
        view_data::ViewData,
        MusicPlayer,
    },
    media_controls::{self, MediaControlEvent, MediaUpdate},
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
pub use settings::SettingsChange;
mod tick;
mod translate;

mod updates;
pub use updates::{
    cleanup_stale_update, spawn_update_download, spawn_version_check, UpdateStatus,
    VersionCheckOutcome, APP_VERSION,
};

const DOUBLE_CLICK_MS: u128 = 300;

/// Insert position for prepending tracks to a playlist.
pub(crate) const PREPEND: usize = 0;

/// Download thumbnails for the given `(provider, id, url)` triples. `id`
/// names the cache file inside the provider's directory; `url` is the source
/// (empty falls back to the default `YouTube` still).
pub fn spawn_thumbnail_download(
    entries: Vec<(crate::providers::ProviderId, String, String)>,
    tx: &mpsc::Sender<BackendResult>,
) {
    tracing::debug!(
        "Spawning thumbnail download threads for {} entries",
        entries.len()
    );
    for (provider, id, thumb) in entries {
        let tx = tx.clone();
        thread::spawn(move || {
            crate::data::thumbnails::download(provider, &id, &thumb);
            let _ = tx.send(BackendResult::ThumbnailDownloaded(provider, id));
        });
    }
}
