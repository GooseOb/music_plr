//! Translate-with-AI popup state: which pane requested it, the persisted
//! target language and prompt drafts, and whether a request is in flight.

use super::pane::PaneId;

#[derive(Debug, Clone)]
pub struct TranslateDialog {
    pub pane: PaneId,
    pub language: String,
    pub prompt: iced::widget::text_editor::Content,
    pub translating: bool,
}

impl TranslateDialog {
    pub fn new(pane: PaneId, language: String, prompt: &str) -> Self {
        Self {
            pane,
            language,
            prompt: iced::widget::text_editor::Content::with_text(prompt),
            translating: false,
        }
    }
}
