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
    /// Browser `yt-dlp` reads cookies from for age-restricted videos (`None`
    /// disables). `serde(default)` keeps pre-existing `config.json` files
    /// parsing after the upgrade instead of resetting the whole config.
    pub cookie_browser: Option<String>,
    #[serde(default = "default_translation_base_url")]
    pub translation_base_url: String,
    #[serde(default)]
    pub translation_api_key: String,
    #[serde(default = "default_translation_model")]
    pub translation_model: String,
    #[serde(default)]
    pub translation_language: String,
    #[serde(default = "default_translation_prompt")]
    pub translation_prompt: String,
}

fn default_translation_base_url() -> String {
    crate::translate::DEFAULT_BASE_URL.to_string()
}

fn default_translation_model() -> String {
    crate::translate::DEFAULT_MODEL.to_string()
}

fn default_translation_prompt() -> String {
    crate::translate::DEFAULT_PROMPT.to_string()
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
            language: Language::from_system_locale().unwrap_or_default(),
            theme_kind: ThemeKind::Dark,
            cookie_browser: None,
            translation_base_url: default_translation_base_url(),
            translation_api_key: String::new(),
            translation_model: default_translation_model(),
            translation_language: String::new(),
            translation_prompt: default_translation_prompt(),
        }
    }
}

fn default_download_dir() -> String {
    if let Some(dirs) = directories::UserDirs::new() {
        dirs.home_dir()
            .join("Music")
            .join("goosemusic")
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
        assert_eq!(cfg.cookie_browser, None);
        assert_eq!(cfg.translation_base_url, crate::translate::DEFAULT_BASE_URL);
        assert_eq!(cfg.translation_model, crate::translate::DEFAULT_MODEL);
        assert_eq!(cfg.translation_prompt, crate::translate::DEFAULT_PROMPT);
        assert!(cfg.translation_api_key.is_empty());
        assert!(cfg.translation_language.is_empty());
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
            cookie_browser: Some("firefox".into()),
            translation_base_url: "https://api.openai.com/v1".into(),
            translation_api_key: "sk-test".into(),
            translation_model: "gpt-4o-mini".into(),
            translation_language: "Spanish".into(),
            translation_prompt: "translate".into(),
        };

        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(serde_json::from_str::<Config>(&json).unwrap(), cfg);
    }

    #[test]
    fn config_without_translation_fields_keeps_defaults() {
        let json = r#"{
            "download_dir": "/tmp/music",
            "max_search_history_visible": 10,
            "max_search_history_stored": 100,
            "cache_max_size_mb": 1024,
            "max_recently_played": 50,
            "volume_normalization": false,
            "default_provider": "YouTube",
            "language": "En",
            "theme_kind": "dark",
            "cookie_browser": null
        }"#;
        let cfg = serde_json::from_str::<Config>(json).unwrap();
        assert_eq!(cfg.translation_base_url, crate::translate::DEFAULT_BASE_URL);
        assert_eq!(cfg.translation_model, crate::translate::DEFAULT_MODEL);
        assert_eq!(cfg.translation_prompt, crate::translate::DEFAULT_PROMPT);
    }
}
