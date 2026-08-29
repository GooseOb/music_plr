use serde::{Deserialize, Serialize};

use crate::{data::JsonStore, i18n::Language, providers::ProviderId, theme::ThemeKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub download_dir: String,
    pub max_search_history_visible: usize,
    pub max_search_history_stored: usize,
    pub cache_max_size_mb: u64,
    pub max_recently_played: usize,
    pub volume_normalization: bool,
    /// The provider used to stream/download when a track lacks a streamable
    /// id or when playing a search-only (e.g. `MusicBrainz`) result. Constrained
    /// at the UI level to providers that support both streaming and
    /// downloading.
    pub default_provider: ProviderId,
    pub language: Language,
    pub theme_kind: ThemeKind,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            download_dir: default_download_dir(),
            max_search_history_visible: 10,
            max_search_history_stored: 100,
            cache_max_size_mb: 1024,
            max_recently_played: 50,
            volume_normalization: false,
            default_provider: ProviderId::YouTube,
            language: Language::default(),
            theme_kind: ThemeKind::Dark,
        }
    }
}

fn default_download_dir() -> String {
    if let Some(dirs) = directories::UserDirs::new() {
        dirs.home_dir()
            .join("Music")
            .join("honkhorn")
            .to_string_lossy()
            .to_string()
    } else {
        "downloads".to_string()
    }
}

impl JsonStore for Config {
    const FILE: &'static str = "config.json";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_values() {
        let cfg = Config::default();
        assert_eq!(cfg.max_search_history_visible, 10);
        assert_eq!(cfg.max_search_history_stored, 100);
        assert_eq!(cfg.cache_max_size_mb, 1024);
        assert_eq!(cfg.max_recently_played, 50);
    }

    #[test]
    fn config_round_trip() {
        let cfg = Config {
            download_dir: "/tmp/music".into(),
            max_search_history_visible: 3,
            max_search_history_stored: 7,
            cache_max_size_mb: 42,
            max_recently_played: 9,
            volume_normalization: true,
            default_provider: ProviderId::SoundCloud,
            language: Language::Pl,
            theme_kind: ThemeKind::Light,
        };

        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(serde_json::from_str::<Config>(&json).unwrap(), cfg);
    }
}
