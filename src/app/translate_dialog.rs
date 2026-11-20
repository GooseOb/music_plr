//! Translate-with-AI popup state: which pane requested it and the persisted
//! target language and prompt drafts. The request itself lives on the player
//! so hiding the popup doesn't abandon it.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use super::pane::PaneId;

#[derive(Debug, Clone)]
pub struct TranslateDialog {
    pub pane: PaneId,
    pub language: String,
    pub prompt: iced::widget::text_editor::Content,
}

impl TranslateDialog {
    pub fn new(pane: PaneId, language: String, prompt: &str) -> Self {
        Self {
            pane,
            language,
            prompt: iced::widget::text_editor::Content::with_text(prompt),
        }
    }
}

/// One in-flight AI translation. Its `rid` routes the reply back to exactly
/// this request, so a superseded one's late result is dropped instead of
/// overwriting the editor or closing a newer popup.
#[derive(Debug)]
pub struct TranslationRequest {
    pub rid: u64,
    pub pane: PaneId,
    pub track_id: String,
    pub language: String,
    cancelled: Arc<AtomicBool>,
}

impl TranslationRequest {
    pub fn new(rid: u64, pane: PaneId, track_id: String, language: String) -> Self {
        Self {
            rid,
            pane,
            track_id,
            language,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// The flag the request thread checks before delivering its result.
    pub fn cancelled(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }
}
