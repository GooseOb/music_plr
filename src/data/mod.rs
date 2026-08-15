//! Persistence layer: every on-disk store the app owns.
//!
//! All stores share the same shape — read a JSON file out of a platform base
//! directory, mutate it in memory, write it back — so the read/write halves
//! live in [`JsonStore`] and the path construction lives in [`config_path`] /
//! [`data_path`] / [`cache_path`]. Each store declares which base directory it
//! belongs in via [`JsonStore::LOCATION`]: user *settings* go in the config
//! dir, persistent user *data* in the data dir, and regenerable *caches* in
//! the cache dir.

use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;

pub mod cache;
pub mod config;
pub mod downloads;
pub mod library;
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

/// Absolute path to `file` inside the data directory, or a bare relative path
/// when the platform directories are unavailable.
pub fn data_path(file: &str) -> PathBuf {
    project_dirs().map_or_else(|| PathBuf::from(file), |d| d.data_local_dir().join(file))
}

/// Which XDG base directory a [`JsonStore`] lives in.
pub enum StoreLocation {
    /// User-specific *settings* (`~/.config/music_plr`).
    Config,
    /// Persistent user *data* (`~/.local/share/music_plr`).
    Data,
    /// Regenerable *cache* (`~/.cache/music_plr`).
    Cache,
}

/// A store persisted as a single pretty-printed JSON file.
///
/// Implementors declare [`FILE`](JsonStore::FILE) and, if they are not plain
/// settings, [`LOCATION`](JsonStore::LOCATION); `load` and `save` are provided.
/// Every failure mode is non-fatal by design: a missing, corrupt, or
/// unwritable file degrades to `Default::default()` rather than taking the app
/// down, since none of this data is critical to playback.
///
/// Under `cfg(test)` `save` is a no-op so unit tests never touch the real
/// user directories.
pub trait JsonStore: Serialize + DeserializeOwned + Default {
    /// File name (not path) within the chosen base directory.
    const FILE: &'static str;

    /// Which base directory the store is persisted under. Defaults to
    /// [`StoreLocation::Config`]; persistent user data overrides to `Data` and
    /// regenerable caches to `Cache`.
    const LOCATION: StoreLocation = StoreLocation::Config;

    fn path() -> PathBuf {
        match Self::LOCATION {
            StoreLocation::Config => config_path(Self::FILE),
            StoreLocation::Data => data_path(Self::FILE),
            StoreLocation::Cache => cache_path(Self::FILE),
        }
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
