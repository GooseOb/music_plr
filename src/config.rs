use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub download_dir: String,
    pub volume: f32,
    pub max_search_history_visible: usize,
    pub max_search_history_stored: usize,
    pub search_history: Vec<String>,
    pub cache_max_size_mb: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            download_dir: default_download_dir(),
            volume: 0.8,
            max_search_history_visible: 10,
            max_search_history_stored: 100,
            search_history: Vec::new(),
            cache_max_size_mb: 1024,
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

pub fn load_config() -> Config {
    confy::load::<Config>("music_plr", "config").unwrap_or_else(|_| {
        let cfg = Config::default();
        let _ = confy::store("music_plr", "config", &cfg);
        cfg
    })
}

pub fn save_config(cfg: &Config) {
    let _ = confy::store("music_plr", "config", cfg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_values() {
        let cfg = Config::default();
        assert_eq!(cfg.volume, 0.8);
        assert_eq!(cfg.max_search_history_visible, 10);
        assert_eq!(cfg.max_search_history_stored, 100);
        assert_eq!(cfg.cache_max_size_mb, 1024);
        assert!(cfg.search_history.is_empty());
    }

    #[test]
    fn config_round_trip() {
        let cfg = Config {
            search_history: vec!["test".to_string(), "query".to_string()],
            volume: 0.5,
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let restored: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.search_history, cfg.search_history);
        assert_eq!(restored.volume, cfg.volume);
    }
}
