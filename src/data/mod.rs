//! Persistence layer: every on-disk store the app owns.
//!
//! All stores share the same shape — read a JSON file out of a platform base
//! directory, mutate it in memory, write it back — so the read/write halves
//! live in [`JsonStore`] and the path construction lives in [`config_path`] /
//! [`data_path`] / [`cache_path`]. Each store declares which base directory it
//! belongs in via [`JsonStore::LOCATION`]: user *settings* go in the config
//! dir, persistent user *data* in the data dir, and regenerable *caches* in
//! the cache dir.

use std::path::PathBuf;

use serde::{de::DeserializeOwned, Serialize};

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
fn project_dirs() -> Option<&'static directories::ProjectDirs> {
    static DIRS: std::sync::OnceLock<Option<directories::ProjectDirs>> = std::sync::OnceLock::new();
    DIRS.get_or_init(|| directories::ProjectDirs::from("", "", "goosemusic"))
        .as_ref()
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
    /// User-specific *settings* (`~/.config/goosemusic`).
    Config,
    /// Persistent user *data* (`~/.local/share/goosemusic`).
    Data,
    /// Regenerable *cache* (`~/.cache/goosemusic`).
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
            if let Err(e) = std::fs::create_dir_all(dir) {
                tracing::warn!("Failed to create {}: {e}", dir.display());
                return;
            }
        }
        let Ok(s) = serde_json::to_string_pretty(self) else {
            tracing::warn!("Failed to serialize {}", path.display());
            return;
        };
        let tmp = path.with_extension("tmp");
        if let Err(e) = std::fs::write(&tmp, s) {
            tracing::warn!("Failed to write {}: {e}", tmp.display());
            return;
        }
        if let Err(e) = std::fs::rename(&tmp, &path) {
            tracing::warn!("Failed to rename {}: {e}", tmp.display());
            let _ = std::fs::remove_file(&tmp);
        }
    }

    #[cfg(test)]
    fn save(&self) {}
}
