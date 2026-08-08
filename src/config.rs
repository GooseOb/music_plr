use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub download_dir: String,
    pub volume: f32,
    pub max_search_history_visible: usize,
    pub max_search_history_stored: usize,
    pub cache_max_size_mb: u64,
    pub max_recently_played: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            download_dir: default_download_dir(),
            volume: 0.8,
            max_search_history_visible: 10,
            max_search_history_stored: 100,
            cache_max_size_mb: 1024,
            max_recently_played: 50,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_values() {
        let cfg = Config::default();
        assert!((cfg.volume - 0.8).abs() < f32::EPSILON);
        assert_eq!(cfg.max_search_history_visible, 10);
        assert_eq!(cfg.max_search_history_stored, 100);
        assert_eq!(cfg.cache_max_size_mb, 1024);
        assert_eq!(cfg.max_recently_played, 50);
    }

    #[test]
    fn config_round_trip() {
        let cfg = Config {
            volume: 0.5,
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let restored: Config = serde_json::from_str(&json).unwrap();
        assert!((restored.volume - cfg.volume).abs() < f32::EPSILON);
    }
}
