//! Lyrics overlay state: which track's lyrics are shown, the loaded lyrics,
//! the active view mode, and the edit buffer.

use crate::{load_state::LoadState, lyrics::Lyrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LyricsViewMode {
    Selectable,
    Synced,
    Plain,
}

#[derive(Debug, Clone)]
pub struct LyricsState {
    pub track_id: Option<String>,
    pub lyrics: LoadState<Lyrics>,
    pub mode: LyricsViewMode,
    pub editor: iced::widget::text_editor::Content,
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
