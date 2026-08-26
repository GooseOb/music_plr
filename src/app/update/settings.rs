use super::MusicPlayer;
use crate::{
    data::{config, JsonStore},
    theme::AppTheme,
};

/// One settings change from the Settings view. Every variant applies a field
/// of [`config::Config`] and persists the store.
#[derive(Debug, Clone)]
pub enum SettingsChange {
    DownloadDir(String),
    MaxHistoryVisible(String),
    MaxHistoryStored(String),
    CacheMaxSize(String),
    MaxRecentlyPlayed(String),
    VolumeNormalization(bool),
    DefaultProvider(crate::providers::ProviderId),
    Language(crate::i18n::Language),
    Theme(crate::theme::ThemeKind),
}

impl MusicPlayer {
    /// Apply `f` to the config, persist it, and keep dependent runtime state
    /// in sync. The single write path for every settings field.
    fn set_config(&mut self, f: impl FnOnce(&mut config::Config)) {
        f(&mut self.config);
        self.stream_cache
            .set_max_size_mb(self.config.cache_max_size_mb);
        self.config.save();
    }

    pub fn handle_settings_change(&mut self, change: SettingsChange) {
        match change {
            SettingsChange::DownloadDir(dir) => {
                let dir = dir.trim();
                if !dir.is_empty() {
                    let dir = dir.to_string();
                    self.set_config(|c| c.download_dir = dir);
                }
            }
            SettingsChange::MaxHistoryVisible(v) => {
                if let Ok(n) = v.trim().parse::<usize>() {
                    self.set_config(|c| c.max_search_history_visible = n.max(1));
                }
            }
            SettingsChange::MaxHistoryStored(v) => {
                if let Ok(n) = v.trim().parse::<usize>() {
                    self.set_config(|c| c.max_search_history_stored = n.max(1));
                }
            }
            SettingsChange::CacheMaxSize(v) => {
                if let Ok(n) = v.trim().parse::<u64>() {
                    self.set_config(|c| c.cache_max_size_mb = n);
                }
            }
            SettingsChange::MaxRecentlyPlayed(v) => {
                if let Ok(n) = v.trim().parse::<usize>() {
                    self.set_config(|c| c.max_recently_played = n.max(1));
                }
            }
            SettingsChange::VolumeNormalization(enabled) => {
                self.set_config(|c| c.volume_normalization = enabled);
            }
            SettingsChange::Language(language) => {
                self.set_config(|c| c.language = language);
                self.strings = language.strings();
            }
            SettingsChange::DefaultProvider(provider) => {
                // Only providers that support both streaming and downloading are
                // valid defaults; ignore others defensively.
                if provider.capabilities().stream && provider.capabilities().download {
                    self.set_config(|c| c.default_provider = provider);
                }
            }
            SettingsChange::Theme(kind) => {
                self.set_config(|c| c.theme_kind = kind);
                self.app_theme = AppTheme::new(kind.palette());
            }
        }
    }

    pub fn handle_settings_reset_defaults(&mut self) {
        self.config = config::Config::default();
        self.set_config(|_| {});
        self.app_theme = AppTheme::new(self.config.theme_kind.palette());
    }
}
