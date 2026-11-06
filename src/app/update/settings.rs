use super::MusicPlayer;
use crate::{
    app::{custom_provider::ConnectionTest, message::BackendResult},
    data::{config, custom_providers, custom_providers::CustomProviderKind, JsonStore},
    providers::ProviderId,
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
    CookieBrowser(Option<String>),
    TranslationBaseUrl(String),
    TranslationApiKey(String),
    TranslationModel(String),
    CheckUpdatesOnStartup(bool),
    /// Switch a provider off (or back on) in Settings. Disabled providers are
    /// hidden from every picker, submenu and server-playlist group.
    ProviderEnabled(crate::providers::ProviderId, bool),
}

impl MusicPlayer {
    /// Apply `f` to the config, persist it, and keep dependent runtime state
    /// in sync. The single write path for every settings field: the disabled
    /// provider mirror is refreshed here so no caller can forget it.
    fn set_config(&mut self, f: impl FnOnce(&mut config::Config)) {
        f(&mut self.config);
        self.config.save();
        crate::providers::set_disabled_providers(
            self.config.disabled_providers.iter().copied().collect(),
        );
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
                    self.stream_cache.set_max_size_mb(n);
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
                self.app_theme = AppTheme::new(&kind.palette());
            }
            SettingsChange::CookieBrowser(browser) => {
                crate::deps::set_cookie_browser(browser.clone());
                self.set_config(|c| c.cookie_browser = browser);
            }
            SettingsChange::TranslationBaseUrl(url) => {
                let url = url.trim().to_string();
                if !url.is_empty() {
                    self.set_config(|c| c.translation_base_url = url);
                }
            }
            SettingsChange::TranslationApiKey(key) => {
                self.set_config(|c| c.translation_api_key = key.trim().to_string());
            }
            SettingsChange::TranslationModel(model) => {
                let model = model.trim().to_string();
                if !model.is_empty() {
                    self.set_config(|c| c.translation_model = model);
                }
            }
            SettingsChange::CheckUpdatesOnStartup(enabled) => {
                self.set_config(|c| c.check_updates_on_startup = enabled);
                if enabled && self.update_status == crate::app::update::UpdateStatus::Unchecked {
                    self.check_for_updates();
                }
            }
            SettingsChange::ProviderEnabled(provider, enabled) => {
                self.set_config(|c| {
                    if enabled {
                        c.disabled_providers.remove(&provider);
                    } else {
                        c.disabled_providers.insert(provider);
                    }
                });
                self.reconcile_provider_refs();
                self.save_session();
                if enabled && provider.is_custom() {
                    // A server's playlist index is hidden while it is off, so
                    // fetch it again as soon as it comes back.
                    let _ = self.refresh_server_playlists(Some(provider));
                }
            }
        }
    }

    pub fn handle_settings_reset_defaults(&mut self) {
        self.config = config::Config::default();
        self.set_config(|_| {});
        crate::deps::set_cookie_browser(self.config.cookie_browser.clone());
        self.stream_cache
            .set_max_size_mb(self.config.cache_max_size_mb);
        self.app_theme = AppTheme::new(&self.config.theme_kind.palette());
        self.reconcile_provider_refs();
        self.save_session();
    }

    pub fn handle_custom_provider_add(&mut self) {
        self.custom_provider_form = Some(crate::app::CustomProviderForm::default());
    }

    pub fn handle_custom_provider_edit(&mut self, id: u64) {
        if let Some(provider) = self
            .custom_providers
            .providers
            .iter()
            .find(|provider| provider.id == id)
        {
            self.custom_provider_form =
                Some(crate::app::CustomProviderForm::from_provider(provider));
        }
    }

    pub fn handle_custom_provider_field(
        &mut self,
        field: crate::app::CustomProviderField,
        value: String,
    ) {
        if let Some(form) = &mut self.custom_provider_form {
            form.set_field(field, value);
        }
    }

    pub fn handle_custom_provider_kind(&mut self, kind: CustomProviderKind) {
        if let Some(form) = &mut self.custom_provider_form {
            form.set_kind(kind);
        }
    }

    pub fn handle_custom_provider_cancel(&mut self) {
        self.custom_provider_form = None;
    }

    pub fn handle_custom_provider_save(&mut self) -> iced::Task<super::Message> {
        let Some(form) = self.custom_provider_form.take() else {
            return iced::Task::none();
        };
        if form.validated().is_none() {
            self.custom_provider_form = Some(form);
            self.notify_error(self.strings.custom_provider_required);
            return iced::Task::none();
        }
        let id = form
            .id
            .unwrap_or_else(|| self.custom_providers.allocate_id());
        let kind = form.kind;
        // An edit that switches the backend re-identifies the server, so the
        // state filed under the old identity has to go with it.
        let previous_kind = form.id.and_then(|id| {
            self.custom_providers
                .providers
                .iter()
                .find(|provider| provider.id == id)
                .map(|provider| provider.kind)
        });
        self.custom_providers.upsert(form.into_provider(id));
        self.custom_providers.save();
        custom_providers::set_registry(&self.custom_providers.providers);
        if let Some(previous_kind) = previous_kind.filter(|previous| *previous != kind) {
            self.forget_provider_data(crate::providers::ProviderId::Custom(previous_kind, id));
        }
        self.prune_disabled_providers();
        self.reconcile_provider_refs();
        self.notify(self.strings.custom_provider_saved);
        // A fresh server's playlists belong in the sidebar right away.
        let provider = crate::providers::ProviderId::Custom(kind, id);
        if provider.enabled() {
            return self.refresh_server_playlists(Some(provider));
        }
        iced::Task::none()
    }

    pub fn handle_custom_provider_delete(&mut self, id: u64) {
        let Some(removed) = self.custom_providers.remove(id) else {
            return;
        };
        self.custom_providers.save();
        custom_providers::set_registry(&self.custom_providers.providers);
        self.prune_disabled_providers();
        if self
            .custom_provider_form
            .as_ref()
            .is_some_and(|form| form.id == Some(id))
        {
            self.custom_provider_form = None;
        }
        // Its playlists went away with the server, and its id is never reused.
        self.forget_provider_data(crate::providers::ProviderId::Custom(removed.kind, id));
        self.reconcile_provider_refs();
        self.notify(self.strings.custom_provider_deleted);
    }

    /// Drop the indexed playlists and cached feed rows of a server identity
    /// that no longer exists. Tracks that still name it are left alone: they
    /// simply stop resolving, so nothing is silently repointed at a new server.
    fn forget_provider_data(&mut self, provider: crate::providers::ProviderId) {
        self.server_playlists
            .entries
            .retain(|entry| entry.provider != provider);
        self.server_playlists.save();
        self.home_cache.forget_provider(provider);
    }

    pub fn handle_custom_provider_test(&mut self) {
        let rid = self.request_ids.next();
        let Some(form) = &mut self.custom_provider_form else {
            return;
        };
        if form.server_url.trim().is_empty() || form.username.trim().is_empty() {
            self.notify_error(self.strings.custom_provider_required);
            return;
        }
        let kind = form.kind;
        let server_url = form.server_url.trim().to_string();
        let username = form.username.trim().to_string();
        let password = form.password.clone();
        form.test = ConnectionTest::Testing;
        form.test_rid = rid;

        let tx = self.result_tx.clone();
        let strings = self.strings;
        std::thread::spawn(move || {
            let result =
                crate::providers::test_custom_provider(kind, &server_url, &username, &password)
                    .map_err(|error| {
                        let error = crate::providers::ProviderError::from(error);
                        strings.error_reason(&error).to_string()
                    });
            let _ = tx.send(BackendResult::CustomProviderTested { rid, result });
        });
    }

    pub fn process_custom_provider_tested(&mut self, rid: u64, result: Result<String, String>) {
        let Some(form) = &mut self.custom_provider_form else {
            return;
        };
        if form.test_rid != rid {
            return;
        }
        form.test_rid = 0;
        form.test = match result {
            Ok(detail) => ConnectionTest::Ok(detail),
            Err(error) => ConnectionTest::Failed(error),
        };
    }

    /// Drop disabled-provider entries that no longer name a registered custom
    /// server (deleted, or edited into another kind). `set_config` re-syncs the
    /// mirror the static provider lists read.
    fn prune_disabled_providers(&mut self) {
        let known = crate::data::custom_providers::all_provider_ids();
        self.set_config(|config| {
            config
                .disabled_providers
                .retain(|provider| !provider.is_custom() || known.contains(provider));
        });
    }

    /// Keep config and pane state pointing at providers that are usable and
    /// switched on. A custom provider can be deleted, and any provider can be
    /// turned off, while the default provider or a pane's search bar still
    /// names it, so fall back to a usable replacement after startup, save,
    /// delete and every toggle.
    pub(crate) fn reconcile_provider_refs(&mut self) {
        // The config is the source of truth here; the static lists already
        // filter through the mirror, but checking both keeps this correct no
        // matter when the mirror was last synced.
        let off = self.config.disabled_providers.clone();
        let default_caps = self.config.default_provider.capabilities();
        let default_ok = default_caps.stream
            && default_caps.download
            && !off.contains(&self.config.default_provider);
        if !default_ok {
            let fallback = ProviderId::defaultable().into_iter().find(|provider| {
                let caps = provider.capabilities();
                caps.stream && caps.download && !off.contains(provider)
            });
            if let Some(fallback) = fallback {
                self.config.default_provider = fallback;
                self.config.save();
            }
        }

        let fallback_search = ProviderId::searchable()
            .into_iter()
            .find(|provider| provider.capabilities().search);
        let Some(fallback_search) = fallback_search else {
            return;
        };
        for pane in self.panes.values_mut() {
            if !pane.search_provider.capabilities().search || off.contains(&pane.search_provider) {
                pane.search_provider = fallback_search;
                if !fallback_search
                    .supported_scopes()
                    .contains(&pane.search_scope)
                {
                    pane.search_scope = fallback_search.supported_scopes()[0];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard};

    use super::*;
    use crate::{
        app::ViewData,
        data::config,
        providers::{ArtistSectionKind, ProviderId},
    };

    /// The provider lists read a process-global mirror, so the tests that
    /// switch a provider off and inspect those lists must not overlap.
    static PROVIDERS: Mutex<()> = Mutex::new(());

    fn lock() -> MutexGuard<'static, ()> {
        PROVIDERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn player() -> MusicPlayer {
        let mut player = MusicPlayer::new_with(config::Config::default());
        player.reset_test_pane(vec![ViewData::default()]);
        player
    }

    #[test]
    fn switching_a_provider_off_takes_it_out_of_the_offered_lists() {
        let _guard = lock();
        let mut player = player();
        player.handle_settings_change(SettingsChange::ProviderEnabled(ProviderId::Local, false));

        assert!(!ProviderId::Local.enabled());
        assert!(!ProviderId::all().contains(&ProviderId::Local));
        // Cache and migration lookups still see every known id.
        assert!(ProviderId::known().contains(&ProviderId::Local));

        player.handle_settings_change(SettingsChange::ProviderEnabled(ProviderId::Local, true));
        assert!(ProviderId::Local.enabled());
        assert!(ProviderId::all().contains(&ProviderId::Local));
    }

    #[test]
    fn switching_a_provider_off_hides_it_from_every_picker() {
        let _guard = lock();
        let mut player = player();
        player.config.default_provider = ProviderId::SoundCloud;
        player.stage_search_provider(0, ProviderId::SoundCloud);

        player.handle_settings_change(SettingsChange::ProviderEnabled(
            ProviderId::SoundCloud,
            false,
        ));

        assert!(player
            .config
            .disabled_providers
            .contains(&ProviderId::SoundCloud));
        assert!(!ProviderId::searchable().contains(&ProviderId::SoundCloud));
        assert!(!ProviderId::defaultable().contains(&ProviderId::SoundCloud));
        // Both the default provider and the pane move to one that is still on.
        assert_eq!(player.config.default_provider, ProviderId::YouTube);
        assert_eq!(player.pane(0).search_provider, ProviderId::YouTube);

        player.handle_settings_change(SettingsChange::ProviderEnabled(
            ProviderId::SoundCloud,
            true,
        ));
        assert!(ProviderId::searchable().contains(&ProviderId::SoundCloud));
    }

    #[test]
    fn a_disabled_provider_keeps_its_tracks_playable() {
        let _guard = lock();
        let mut player = player();
        player.handle_settings_change(SettingsChange::ProviderEnabled(ProviderId::Bandcamp, false));

        // Switching a provider off only takes it out of the pickers; its
        // capabilities (and so playback of its tracks) are unchanged.
        let caps = ProviderId::Bandcamp.capabilities();
        assert!(caps.stream && caps.download);
        assert!(!ProviderId::searchable().contains(&ProviderId::Bandcamp));
        assert!(ProviderId::known().contains(&ProviderId::Bandcamp));

        player.handle_settings_change(SettingsChange::ProviderEnabled(ProviderId::Bandcamp, true));
        assert!(ProviderId::searchable().contains(&ProviderId::Bandcamp));
    }

    #[test]
    fn resetting_defaults_switches_every_provider_back_on() {
        let _guard = lock();
        let mut player = player();
        player.handle_settings_change(SettingsChange::ProviderEnabled(ProviderId::LastFm, false));
        assert!(!ProviderId::searchable().contains(&ProviderId::LastFm));

        player.handle_settings_reset_defaults();
        assert!(player.config.disabled_providers.is_empty());
        assert!(ProviderId::searchable().contains(&ProviderId::LastFm));
    }

    #[test]
    fn artist_section_pickers_drop_switched_off_providers() {
        let _guard = lock();
        let mut player = player();
        player.handle_settings_change(SettingsChange::ProviderEnabled(
            ProviderId::SoundCloud,
            false,
        ));

        let albums = ArtistSectionKind::Albums.providers(ProviderId::YouTube);
        assert!(albums.contains(&ProviderId::YouTube));
        assert!(!albums.contains(&ProviderId::SoundCloud));

        // The page's own provider stays offered even after it is switched off,
        // because the sections it opened still name it.
        let from_soundcloud = ArtistSectionKind::Albums.providers(ProviderId::SoundCloud);
        assert!(from_soundcloud.contains(&ProviderId::SoundCloud));

        player.handle_settings_change(SettingsChange::ProviderEnabled(
            ProviderId::SoundCloud,
            true,
        ));
        assert!(ArtistSectionKind::Albums
            .providers(ProviderId::YouTube)
            .contains(&ProviderId::SoundCloud));
    }
}
