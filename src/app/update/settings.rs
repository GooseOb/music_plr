use super::MusicPlayer;
use crate::data::{config, JsonStore};

impl MusicPlayer {
    pub fn handle_settings_download_dir(&mut self, dir: &str) {
        let dir = dir.trim();
        if dir.is_empty() {
            return;
        }
        self.config.download_dir = dir.to_string();
        self.config.save();
    }

    pub fn handle_settings_max_history_visible(&mut self, v: &str) {
        if let Ok(n) = v.trim().parse::<usize>() {
            self.config.max_search_history_visible = n.max(1);
            self.config.save();
        }
    }

    pub fn handle_settings_max_history_stored(&mut self, v: &str) {
        if let Ok(n) = v.trim().parse::<usize>() {
            self.config.max_search_history_stored = n.max(1);
            self.config.save();
        }
    }

    pub fn handle_settings_cache_max_size(&mut self, v: &str) {
        if let Ok(n) = v.trim().parse::<u64>() {
            self.config.cache_max_size_mb = n;
            self.stream_cache.set_max_size_mb(n);
            self.config.save();
        }
    }

    pub fn handle_settings_max_recently_played(&mut self, v: &str) {
        if let Ok(n) = v.trim().parse::<usize>() {
            self.config.max_recently_played = n.max(1);
            self.config.save();
        }
    }

    pub fn handle_settings_reset_defaults(&mut self) {
        self.config = config::Config::default();
        self.stream_cache
            .set_max_size_mb(self.config.cache_max_size_mb);
        self.config.save();
    }

    pub fn handle_settings_volume_normalization(&mut self, enabled: bool) {
        self.config.volume_normalization = enabled;
        self.config.save();
    }

    pub fn handle_settings_default_provider(&mut self, provider: crate::provider::ProviderId) {
        // Only providers that support both streaming and downloading are valid
        // defaults; ignore others defensively.
        if provider.capabilities().stream && provider.capabilities().download {
            self.config.default_provider = provider;
            self.config.save();
        }
    }
}
