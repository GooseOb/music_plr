use serde::{Deserialize, Serialize};

use crate::data::JsonStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub download_dir: String,
    pub max_search_history_visible: usize,
    pub max_search_history_stored: usize,
    pub cache_max_size_mb: u64,
    pub max_recently_played: usize,
    pub volume_normalization: bool,
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
        }
    }
}

fn default_download_dir() -> String {
    if let Some(dirs) = directories::UserDirs::new() {
        dirs.home_dir()
            .join("Music")
            .join("music_plr")
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
        let cfg = Config::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let restored: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(
            restored.max_search_history_stored,
            cfg.max_search_history_stored
        );
    }
}
