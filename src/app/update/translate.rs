use iced::Task;

use super::{BackendResult, Message, MusicPlayer};
use crate::{
    app::{dialog::Dialog, pane::PaneId, TranslateDialog},
    data::JsonStore,
    load_state::LoadState,
};

impl MusicPlayer {
    fn translate_source_text(&self, pane: PaneId) -> Option<String> {
        let state = self.pane(pane).lyrics.as_ref()?;
        let lyrics = state.displayed_lyrics()?;
        let text = lyrics.to_edit_text();
        (!text.trim().is_empty()).then_some(text)
    }

    fn translate_dialog_mut(&mut self) -> Option<&mut TranslateDialog> {
        match &mut self.dialog {
            Some(Dialog::Translate(dialog)) => Some(dialog),
            _ => None,
        }
    }

    pub fn open_translate_dialog(&mut self, pane: PaneId) -> Task<Message> {
        if self.translate_source_text(pane).is_none() {
            self.notify_error(self.strings.no_lyrics_to_translate.to_string());
            return Task::none();
        }
        let language = self.config.translation_language.clone();
        let prompt = if self.config.translation_prompt.is_empty() {
            crate::translate::DEFAULT_PROMPT
        } else {
            &self.config.translation_prompt
        };
        self.dialog = Some(Dialog::Translate(TranslateDialog::new(
            pane, language, prompt,
        )));
        Task::none()
    }

    pub fn handle_translate_language_changed(&mut self, value: String) {
        if let Some(dialog) = self.translate_dialog_mut() {
            dialog.language = value;
        }
    }

    pub fn handle_translate_prompt_action(&mut self, action: iced::widget::text_editor::Action) {
        if let Some(dialog) = self.translate_dialog_mut() {
            dialog.prompt.perform(action);
        }
    }

    pub(super) fn save_translate_prefs(&mut self) {
        let (language, prompt) = match &self.dialog {
            Some(Dialog::Translate(dialog)) => (dialog.language.clone(), dialog.prompt.text()),
            _ => return,
        };
        self.config.translation_language = language.trim().to_string();
        if !prompt.trim().is_empty() {
            self.config.translation_prompt = prompt;
        }
        self.config.save();
    }

    pub fn handle_translate_language_submit(&mut self) {
        self.save_translate_prefs();
    }

    pub fn handle_translate_reset_prompt(&mut self) {
        let default = crate::translate::DEFAULT_PROMPT.to_string();
        if let Some(dialog) = self.translate_dialog_mut() {
            dialog.prompt = iced::widget::text_editor::Content::with_text(&default);
        }
        self.config.translation_prompt = default;
        self.config.save();
    }

    pub fn submit_translation(&mut self) -> Task<Message> {
        let (pane, language, prompt) = match &self.dialog {
            Some(Dialog::Translate(dialog)) => (
                dialog.pane,
                dialog.language.trim().to_string(),
                dialog.prompt.text(),
            ),
            _ => return Task::none(),
        };
        if language.is_empty() {
            self.notify_error(self.strings.translation_language_empty.to_string());
            return Task::none();
        }
        let Some(source) = self.translate_source_text(pane) else {
            self.notify_error(self.strings.no_lyrics_to_translate.to_string());
            return Task::none();
        };
        if self.config.translation_base_url.trim().is_empty()
            || self.config.translation_model.trim().is_empty()
        {
            self.notify_error(self.strings.translation_not_configured.to_string());
            return Task::none();
        }
        self.save_translate_prefs();
        if let Some(dialog) = self.translate_dialog_mut() {
            dialog.language.clone_from(&language);
            dialog.translating = true;
        }
        let track_id = self
            .queue
            .current()
            .map(|t| t.primary_id().to_string())
            .unwrap_or_default();
        let base_url = self.config.translation_base_url.clone();
        let api_key = self.config.translation_api_key.clone();
        let model = self.config.translation_model.clone();
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            // `{e:#}` prints the whole anyhow chain, not just the outer
            // context, so the toast names the real cause (refused
            // connection, timeout, HTTP status + server error body, ...).
            let result = crate::translate::translate(
                &base_url, &api_key, &model, &prompt, &language, &source,
            )
            .map_err(|e| format!("{e:#}"));
            match result {
                Ok(text) => {
                    let _ = tx.send(BackendResult::TranslationDone {
                        pane,
                        track_id,
                        language,
                        text,
                    });
                }
                Err(message) => {
                    let _ = tx.send(BackendResult::TranslationError { pane, message });
                }
            }
        });
        Task::none()
    }

    pub fn process_translation_done(
        &mut self,
        pane: PaneId,
        track_id: &str,
        language: &str,
        text: &str,
    ) -> Task<Message> {
        if !self.panes.contains_key(&pane) {
            return Task::none();
        }
        let text = text.trim();
        if text.is_empty() {
            self.notify_error((self.strings.translation_failed)("empty response"));
            return Task::none();
        }
        let current_id = self
            .queue
            .current()
            .map(|t| t.primary_id().to_string())
            .unwrap_or_default();
        if current_id != track_id {
            self.notify_error((self.strings.translation_failed)("track changed"));
            return Task::none();
        }
        self.dialog = None;
        self.flush_note_draft(pane);
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            state.track_id = Some(track_id.to_string());
            state.edit_name = language.to_string();
            state.edit_content = iced::widget::text_editor::Content::with_text(text);
            state.editing = true;
            state.editing_custom_name = None;
            state.viewport = None;
            state.reset_note_state();
        }
        self.notify(self.strings.translation_ready);
        Task::none()
    }

    pub fn process_translation_error(&mut self, pane: PaneId, message: &str) -> Task<Message> {
        if let Some(Dialog::Translate(dialog)) = &mut self.dialog {
            if dialog.pane == pane {
                dialog.translating = false;
            }
        }
        self.notify_error((self.strings.translation_failed)(message));
        Task::none()
    }

    /// Fetch the model list from the configured translation server for the
    /// Settings picker. No-op while a fetch is already in flight.
    pub fn request_translation_models(&mut self) {
        if self
            .translation_models
            .as_ref()
            .is_some_and(LoadState::is_loading)
        {
            return;
        }
        self.translation_models = Some(LoadState::Loading);
        let base_url = self.config.translation_base_url.clone();
        let api_key = self.config.translation_api_key.clone();
        let tx = self.result_tx.clone();
        std::thread::spawn(
            move || match crate::translate::list_models(&base_url, &api_key) {
                Ok(models) => {
                    let _ = tx.send(BackendResult::TranslationModels(models));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::TranslationModelsError(format!("{e:#}")));
                }
            },
        );
    }

    pub fn process_translation_models(&mut self, models: Vec<String>) {
        self.translation_models = Some(LoadState::Ready(models));
    }

    pub fn process_translation_models_error(&mut self, message: &str) {
        self.translation_models = Some(LoadState::Failed(message.to_string()));
        self.notify_error((self.strings.translation_models_failed)(message));
    }
}
