//! Persistence layer: every on-disk store the app owns.
//!
//! All stores share the same shape — read a JSON file out of the platform
//! config/cache directory, mutate it in memory, write it back — so the
//! read/write halves live in [`JsonStore`] and the path construction lives in
//! [`config_path`] / [`cache_path`].

use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;

pub mod cache;
pub mod config;
pub mod downloads;
pub mod lyrics_cache;
pub mod playlists;
pub mod search_history;
pub mod session;
pub mod thumbnails;

/// The app's platform directories. Falls back to the current directory when
/// the OS can't provide them (e.g. a stripped-down container).
fn project_dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("", "", "music_plr")
}

/// Absolute path to `file` inside the config directory, or a bare relative
/// path when the platform directories are unavailable.
pub fn config_path(file: &str) -> PathBuf {
    project_dirs().map_or_else(|| PathBuf::from(file), |d| d.config_dir().join(file))
}

/// Absolute path to `sub` inside the cache directory, or a bare relative path
/// when the platform directories are unavailable.
pub fn cache_path(sub: &str) -> PathBuf {
    project_dirs().map_or_else(|| PathBuf::from(sub), |d| d.cache_dir().join(sub))
}

/// A store persisted as a single pretty-printed JSON file under the config
/// directory.
///
/// Implementors only declare [`FILE`](JsonStore::FILE); `load` and `save` are
/// provided. Every failure mode is non-fatal by design: a missing, corrupt, or
/// unwritable file degrades to `Default::default()` rather than taking the app
/// down, since none of this data is critical to playback.
///
/// Under `cfg(test)` `save` is a no-op so unit tests never touch the real
/// user config directory.
pub trait JsonStore: Serialize + DeserializeOwned + Default {
    /// File name (not path) within the config directory.
    const FILE: &'static str;

    fn path() -> PathBuf {
        config_path(Self::FILE)
    }

    fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    #[cfg(not(test))]
    fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, s);
        }
    }

    #[cfg(test)]
    fn save(&self) {}
}
