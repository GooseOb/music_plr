use iced::Task;

use super::{BackendResult, Message, MusicPlayer};
use crate::{
    app::{dialog::Dialog, pane::PaneId, TranslateDialog, TranslationRequest},
    data::JsonStore,
    load_state::LoadState,
};

impl MusicPlayer {
    /// The track whose lyrics this pane shows plus its text, captured together
    /// so a translation request always matches the lyrics it was started from.
    fn translate_source(&self, pane: PaneId) -> Option<(String, String)> {
        let state = self.pane(pane).lyrics.as_ref()?;
        let track_id = state.track_id.clone()?;
        let text = state.displayed_lyrics()?.to_edit_text();
        (!text.trim().is_empty()).then_some((track_id, text))
    }

    fn translate_dialog_mut(&mut self) -> Option<&mut TranslateDialog> {
        match &mut self.dialog {
            Some(Dialog::Translate(dialog)) => Some(dialog),
            _ => None,
        }
    }

    pub fn translation_in_flight(&self, pane: PaneId) -> bool {
        self.translation_requests
            .iter()
            .any(|request| request.pane == pane)
    }

    fn take_translation(&mut self, rid: u64) -> Option<TranslationRequest> {
        let index = self
            .translation_requests
            .iter()
            .position(|request| request.rid == rid)?;
        Some(self.translation_requests.swap_remove(index))
    }

    /// Stop waiting for `pane`'s translation: the thread drops its result, and
    /// a reply that slipped out before the flag was read is ignored because
    /// the request is no longer tracked.
    pub(super) fn cancel_translation_for(&mut self, pane: PaneId) {
        if let Some(index) = self
            .translation_requests
            .iter()
            .position(|request| request.pane == pane)
        {
            self.translation_requests.swap_remove(index).cancel();
        }
    }

    pub fn open_translate_dialog(&mut self, pane: PaneId) -> Task<Message> {
        if self.translate_source(pane).is_none() {
            self.notify_error(self.strings.no_lyrics_to_translate);
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

    /// Close the popup without touching the request, so a refresh of the
    /// lyrics view or a track change can happen while it runs.
    pub fn hide_translate_dialog(&mut self) {
        self.save_translate_prefs();
        self.dialog = None;
    }

    /// Cancel the popup's translation (if one is running) and close it.
    pub fn cancel_translation(&mut self) {
        if let Some(Dialog::Translate(dialog)) = &self.dialog {
            self.cancel_translation_for(dialog.pane);
        }
        self.hide_translate_dialog();
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
        if self.translation_in_flight(pane) {
            return Task::none();
        }
        if language.is_empty() {
            self.notify_error(self.strings.translation_language_empty);
            return Task::none();
        }
        let Some((track_id, source)) = self.translate_source(pane) else {
            self.notify_error(self.strings.no_lyrics_to_translate);
            return Task::none();
        };
        if self.config.translation_base_url.trim().is_empty()
            || self.config.translation_model.trim().is_empty()
        {
            self.notify_error(self.strings.translation_not_configured);
            return Task::none();
        }
        self.save_translate_prefs();
        if let Some(dialog) = self.translate_dialog_mut() {
            dialog.language.clone_from(&language);
        }
        let rid = self.request_ids.next();
        let request = TranslationRequest::new(rid, pane, track_id.clone(), language.clone());
        let cancelled = request.cancelled();
        self.translation_requests.push(request);
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
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            match result {
                Ok(text) => {
                    let _ = tx.send(BackendResult::TranslationDone {
                        rid,
                        pane,
                        track_id,
                        language,
                        text,
                    });
                }
                Err(message) => {
                    let _ = tx.send(BackendResult::TranslationError { rid, message });
                }
            }
        });
        Task::none()
    }

    pub fn process_translation_done(
        &mut self,
        rid: u64,
        pane: PaneId,
        track_id: &str,
        language: &str,
        text: &str,
    ) -> Task<Message> {
        if self.take_translation(rid).is_none() {
            return Task::none();
        }
        if !self.session.panes.contains_key(&pane) {
            return Task::none();
        }
        let text = text.trim();
        if text.is_empty() {
            self.notify_error((self.strings.translation_failed)("empty response"));
            return Task::none();
        }
        if self
            .pane(pane)
            .lyrics
            .as_ref()
            .and_then(|s| s.track_id.as_deref())
            != Some(track_id)
        {
            self.notify_error((self.strings.translation_failed)("track changed"));
            return Task::none();
        }
        if matches!(&self.dialog, Some(Dialog::Translate(dialog)) if dialog.pane == pane) {
            self.dialog = None;
        }
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

    pub fn process_translation_error(&mut self, rid: u64, message: &str) -> Task<Message> {
        if self.take_translation(rid).is_some() {
            self.notify_error((self.strings.translation_failed)(message));
        }
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

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    use super::*;
    use crate::{app::LyricsState, lyrics::LyricsProvider};

    const TRACK: &str = "youtube:track1";

    fn player() -> (MusicPlayer, PaneId) {
        let mut player = MusicPlayer::new_with(crate::data::config::Config::default());
        let pane = player.session.focused;
        let mut lyrics = LyricsState::new(LyricsProvider::LrcLib);
        lyrics.track_id = Some(TRACK.to_string());
        lyrics.lyrics = LoadState::Ready(Arc::new(crate::lyrics::Lyrics {
            lines: Vec::new(),
            plain: "line".into(),
            provider: LyricsProvider::LrcLib,
            translations: Vec::new(),
        }));
        player.pane_mut(pane).lyrics = Some(lyrics);
        (player, pane)
    }

    fn track_request(player: &mut MusicPlayer, pane: PaneId) -> (u64, Arc<AtomicBool>) {
        let rid = player.request_ids.next();
        let request = TranslationRequest::new(rid, pane, TRACK.to_string(), "German".into());
        let cancelled = request.cancelled();
        player.translation_requests.push(request);
        (rid, cancelled)
    }

    #[test]
    fn stale_translation_reply_is_dropped() {
        let (mut player, pane) = player();
        let (rid, _) = track_request(&mut player, pane);
        let _ = player.process_translation_done(rid + 1, pane, TRACK, "German", "Hallo");
        assert!(!player.pane(pane).lyrics.as_ref().unwrap().editing);
        assert!(player.translation_in_flight(pane));

        let _ = player.process_translation_done(rid, pane, TRACK, "German", "Hallo");
        let state = player.pane(pane).lyrics.as_ref().unwrap();
        assert!(state.editing);
        assert_eq!(state.edit_name, "German");
        assert!(!player.translation_in_flight(pane));
    }

    #[test]
    fn translation_reply_for_another_track_is_dropped() {
        let (mut player, pane) = player();
        let (rid, _) = track_request(&mut player, pane);
        let _ = player.process_translation_done(rid, pane, "youtube:other", "German", "Hallo");
        assert!(!player.pane(pane).lyrics.as_ref().unwrap().editing);
        assert!(!player.translation_in_flight(pane));
    }

    #[test]
    fn cancelling_translation_flags_the_request_and_closes_the_popup() {
        let (mut player, pane) = player();
        let (_, cancelled) = track_request(&mut player, pane);
        player.dialog = Some(Dialog::Translate(TranslateDialog::new(
            pane,
            "German".into(),
            "prompt",
        )));
        player.cancel_translation();
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(!player.translation_in_flight(pane));
        assert!(player.dialog.is_none());
    }
}
