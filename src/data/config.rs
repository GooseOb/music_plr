use serde::{Deserialize, Serialize};

use crate::{
    data::JsonStore, i18n::Language, lyrics::LyricsProvider, providers::ProviderId,
    theme::ThemeKind,
};

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
    /// Provider a newly opened lyrics pane starts with. Reconciled to an
    /// enabled one whenever the configured provider is switched off.
    #[serde(default)]
    pub default_lyrics_provider: LyricsProvider,
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
    /// Whether the app queries crates.io for a newer release at startup.
    #[serde(default = "default_true")]
    pub check_updates_on_startup: bool,
    /// Providers the user switched off in Settings. They disappear from every
    /// picker, submenu and server-playlist group, while tracks that already
    /// carry their identity keep playing.
    #[serde(default)]
    pub disabled_providers: std::collections::BTreeSet<ProviderId>,
    /// Lyrics providers the user switched off in Settings. They disappear from
    /// the lyrics provider picker, and open lyrics panes move to the default.
    #[serde(default)]
    pub disabled_lyrics_providers: std::collections::BTreeSet<LyricsProvider>,
}

fn default_true() -> bool {
    true
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
            default_lyrics_provider: LyricsProvider::default(),
            language: Language::from_system_locale().unwrap_or_default(),
            theme_kind: ThemeKind::Dark,
            cookie_browser: None,
            translation_base_url: default_translation_base_url(),
            translation_api_key: String::new(),
            translation_model: default_translation_model(),
            translation_language: String::new(),
            translation_prompt: default_translation_prompt(),
            check_updates_on_startup: true,
            disabled_providers: std::collections::BTreeSet::new(),
            disabled_lyrics_providers: std::collections::BTreeSet::new(),
        }
    }
}

impl Config {
    pub fn lyrics_provider_enabled(&self, provider: LyricsProvider) -> bool {
        !self.disabled_lyrics_providers.contains(&provider)
    }

    /// Enabled lyrics providers in the order the lyrics view lists them.
    pub fn enabled_lyrics_providers(&self) -> Vec<LyricsProvider> {
        LyricsProvider::all()
            .iter()
            .copied()
            .filter(|provider| self.lyrics_provider_enabled(*provider))
            .collect()
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
        assert!(cfg.check_updates_on_startup);
        assert_eq!(cfg.default_lyrics_provider, LyricsProvider::LrcLib);
        assert!(cfg.disabled_lyrics_providers.is_empty());
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
            default_lyrics_provider: LyricsProvider::NetEase,
            language: Language::Pl,
            theme_kind: ThemeKind::Light,
            cookie_browser: Some("firefox".into()),
            translation_base_url: "https://api.openai.com/v1".into(),
            translation_api_key: "sk-test".into(),
            translation_model: "gpt-4o-mini".into(),
            translation_language: "Spanish".into(),
            translation_prompt: "translate".into(),
            check_updates_on_startup: false,
            disabled_providers: [ProviderId::LastFm].into_iter().collect(),
            disabled_lyrics_providers: [LyricsProvider::Genius].into_iter().collect(),
        };

        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(serde_json::from_str::<Config>(&json).unwrap(), cfg);
    }

    #[test]
    fn enabled_lyrics_providers_skip_disabled_ones() {
        let cfg = Config {
            disabled_lyrics_providers: [LyricsProvider::LrcLib, LyricsProvider::Genius]
                .into_iter()
                .collect(),
            ..Config::default()
        };

        assert!(!cfg.lyrics_provider_enabled(LyricsProvider::LrcLib));
        assert!(cfg.lyrics_provider_enabled(LyricsProvider::NetEase));
        assert_eq!(
            cfg.enabled_lyrics_providers(),
            vec![
                LyricsProvider::LrcMux,
                LyricsProvider::NetEase,
                LyricsProvider::LyricsOvh
            ]
        );
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
        assert!(cfg.check_updates_on_startup);
        assert!(cfg.disabled_providers.is_empty());
        assert_eq!(cfg.default_lyrics_provider, LyricsProvider::LrcLib);
        assert!(cfg.disabled_lyrics_providers.is_empty());
    }
}
