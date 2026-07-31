use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub download_dir: String,
    pub volume: f32,
    pub music_dir: String,
    pub max_search_history_visible: usize,
    pub max_search_history_stored: usize,
    pub search_history: Vec<String>,
    pub last_search_query: String,
    pub cache_max_size_mb: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            download_dir: default_download_dir(),
            volume: 0.8,
            music_dir: default_music_dir(),
            max_search_history_visible: 10,
            max_search_history_stored: 100,
            search_history: Vec::new(),
            last_search_query: String::new(),
            cache_max_size_mb: 1024,
        }
    }
}

fn default_music_dir() -> String {
    if let Some(dirs) = directories::UserDirs::new() {
        dirs.home_dir().join("Music").to_string_lossy().to_string()
    } else {
        String::new()
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

pub fn fuzzy_match(query: &str, text: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    let text = text.to_lowercase();
    let mut qi = query.chars().peekable();
    for c in text.chars() {
        if qi.peek() == Some(&c) {
            qi.next();
        }
    }
    qi.peek().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_match_exact() {
        assert!(fuzzy_match("hello", "hello"));
    }

    #[test]
    fn fuzzy_match_subsequence() {
        assert!(fuzzy_match("hlo", "hello"));
        assert!(fuzzy_match("hlo", "Hello World"));
    }

    #[test]
    fn fuzzy_match_empty_query() {
        assert!(fuzzy_match("", "anything"));
    }

    #[test]
    fn fuzzy_match_no_match() {
        assert!(!fuzzy_match("xyz", "hello"));
    }

    #[test]
    fn fuzzy_match_case_insensitive() {
        assert!(fuzzy_match("HELLO", "hello"));
        assert!(fuzzy_match("HeLlO", "HELLO"));
    }

    #[test]
    fn fuzzy_match_partial() {
        assert!(fuzzy_match("ell", "hello"));
        assert!(!fuzzy_match("leh", "hello"));
    }
}
