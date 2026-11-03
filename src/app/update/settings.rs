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
}

impl MusicPlayer {
    /// Apply `f` to the config, persist it, and keep dependent runtime state
    /// in sync. The single write path for every settings field.
    fn set_config(&mut self, f: impl FnOnce(&mut config::Config)) {
        f(&mut self.config);
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
        }
    }

    pub fn handle_settings_reset_defaults(&mut self) {
        self.config = config::Config::default();
        self.set_config(|_| {});
        crate::deps::set_cookie_browser(self.config.cookie_browser.clone());
        self.stream_cache
            .set_max_size_mb(self.config.cache_max_size_mb);
        self.app_theme = AppTheme::new(&self.config.theme_kind.palette());
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

    pub fn handle_custom_provider_save(&mut self) {
        let Some(form) = self.custom_provider_form.take() else {
            return;
        };
        if form.validated().is_none() {
            self.custom_provider_form = Some(form);
            self.notify_error(self.strings.custom_provider_required);
            return;
        }
        let id = form
            .id
            .unwrap_or_else(|| self.custom_providers.allocate_id());
        self.custom_providers.upsert(form.into_provider(id));
        self.custom_providers.save();
        custom_providers::set_registry(&self.custom_providers.providers);
        self.reconcile_custom_provider_refs();
        self.notify(self.strings.custom_provider_saved);
    }

    pub fn handle_custom_provider_delete(&mut self, id: u64) {
        if self.custom_providers.remove(id).is_none() {
            return;
        }
        self.custom_providers.save();
        custom_providers::set_registry(&self.custom_providers.providers);
        if self
            .custom_provider_form
            .as_ref()
            .is_some_and(|form| form.id == Some(id))
        {
            self.custom_provider_form = None;
        }
        self.reconcile_custom_provider_refs();
        self.notify(self.strings.custom_provider_deleted);
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

    /// Keep config and pane state pointing at providers that still exist.
    /// Custom providers can be deleted while the default provider or a
    /// pane's search bar still names them, so fall back to a usable
    /// replacement after startup, save and delete.
    pub(crate) fn reconcile_custom_provider_refs(&mut self) {
        let default_caps = self.config.default_provider.capabilities();
        if !(default_caps.stream && default_caps.download) {
            let fallback = ProviderId::defaultable()
                .into_iter()
                .find(|provider| {
                    let caps = provider.capabilities();
                    caps.stream && caps.download
                })
                .unwrap_or(ProviderId::YouTube);
            self.config.default_provider = fallback;
            self.config.save();
        }

        let fallback_search = ProviderId::searchable()
            .into_iter()
            .find(|provider| provider.capabilities().search)
            .unwrap_or(ProviderId::YouTube);
        for pane in self.panes.values_mut() {
            if !pane.search_provider.capabilities().search {
                pane.search_provider = fallback_search;
            }
        }
    }
}
