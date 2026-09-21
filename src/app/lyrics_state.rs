//! Lyrics overlay state: which track's lyrics are shown, the loaded lyrics,
//! the active view mode, and the edit buffer.

use crate::{load_state::LoadState, lyrics::Lyrics};

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
}

impl LyricsViewMode {
    pub fn for_lyrics(lyrics: &Lyrics) -> Self {
        if lyrics.timed.is_empty() {
            Self::Plain
        } else {
            Self::Synced
        }
    }
}

impl LyricsState {
    pub(crate) fn new() -> Self {
        Self {
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
        }
    }

    pub fn mode_available(&self, mode: LyricsViewMode) -> bool {
        let LoadState::Ready(lyrics) = &self.lyrics else {
            return false;
        };
        match mode {
            LyricsViewMode::Selectable => !(lyrics.timed.is_empty() && lyrics.plain.is_empty()),
            LyricsViewMode::Synced => !lyrics.timed.is_empty(),
            LyricsViewMode::Plain => !lyrics.plain.is_empty(),
        }
    }
}
