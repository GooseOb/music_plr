//! Lyrics overlay state: which track's lyrics are shown, the loaded lyrics,
//! the active view mode, and the edit buffer.

use crate::{
    load_state::LoadState,
    lyrics::{Lyrics, LyricsProvider},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LyricsViewMode {
    Selectable,
    Synced,
    Plain,
}

#[derive(Debug, Clone, Copy)]
pub struct LyricsViewport {
    pub offset_y: f32,
    pub height: f32,
    pub content_h: f32,
}

#[derive(Debug, Clone)]
pub struct LyricsState {
    pub provider: LyricsProvider,
    pub track_id: Option<String>,
    pub lyrics: LoadState<Lyrics>,
    pub mode: LyricsViewMode,
    pub editor: iced::widget::text_editor::Content,
    pub scrolled_to: Option<usize>,
    pub viewport: Option<LyricsViewport>,
    pub editing: bool,
    pub edit_content: iced::widget::text_editor::Content,
    pub edit_name: String,
    pub editing_custom_name: Option<String>,
    pub selected_custom: Option<String>,
    pub custom_names: Vec<String>,
    pub picked_line: Option<usize>,
    pub note_editor: iced::widget::text_editor::Content,
    pub note_line: Option<usize>,
}

impl LyricsViewMode {
    pub fn for_lyrics(lyrics: &Lyrics) -> Self {
        if lyrics.has_timed() {
            Self::Synced
        } else {
            Self::Plain
        }
    }
}

impl LyricsState {
    pub(crate) fn new(provider: LyricsProvider) -> Self {
        Self {
            provider,
            track_id: None,
            lyrics: LoadState::Loading,
            mode: LyricsViewMode::Selectable,
            editor: iced::widget::text_editor::Content::default(),
            scrolled_to: None,
            viewport: None,
            editing: false,
            edit_content: iced::widget::text_editor::Content::default(),
            edit_name: String::new(),
            editing_custom_name: None,
            selected_custom: None,
            custom_names: Vec::new(),
            picked_line: None,
            note_editor: iced::widget::text_editor::Content::default(),
            note_line: None,
        }
    }

    pub fn mode_available(&self, mode: LyricsViewMode) -> bool {
        let LoadState::Ready(lyrics) = &self.lyrics else {
            return false;
        };
        match mode {
            LyricsViewMode::Selectable => lyrics.has_timed() || !lyrics.plain.is_empty(),
            LyricsViewMode::Synced => lyrics.has_timed(),
            LyricsViewMode::Plain => !lyrics.plain.is_empty(),
        }
    }

    pub fn note_target(&self) -> Option<usize> {
        let LoadState::Ready(lyrics) = &self.lyrics else {
            return None;
        };
        let idx = match self.mode {
            LyricsViewMode::Synced => self.scrolled_to.or(self.picked_line),
            LyricsViewMode::Plain => self.picked_line,
            LyricsViewMode::Selectable => None,
        }?;
        lyrics.lines.get(idx).map(|_| idx)
    }

    pub fn reset_note_state(&mut self) {
        self.picked_line = None;
        self.note_editor = iced::widget::text_editor::Content::default();
        self.note_line = None;
    }
}
