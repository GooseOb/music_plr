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
    /// Provider-namespaced `slug:id` track key (see [`crate::types::Track::cache_key`]).
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
    pub selected_translation: Option<String>,
    pub loading_translation: Option<String>,
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
            selected_translation: None,
            loading_translation: None,
            custom_names: Vec::new(),
            picked_line: None,
            note_editor: iced::widget::text_editor::Content::default(),
            note_line: None,
        }
    }

    /// The lyrics currently on screen: the selected translation when it is
    /// loaded, otherwise the original. `None` while loading or failed.
    pub fn displayed_lyrics(&self) -> Option<&Lyrics> {
        let LoadState::Ready(lyrics) = &self.lyrics else {
            return None;
        };
        if let Some(language) = self.selected_translation.as_deref() {
            if let Some(translation) = lyrics
                .translations
                .iter()
                .find(|t| t.language == language && t.is_loaded())
            {
                return Some(&translation.lyrics);
            }
        }
        Some(lyrics)
    }

    pub fn displayed_lyrics_mut(&mut self) -> Option<&mut Lyrics> {
        let language = self.selected_translation.clone();
        let LoadState::Ready(lyrics) = &mut self.lyrics else {
            return None;
        };
        match language {
            Some(language) => {
                let pos = lyrics
                    .translations
                    .iter()
                    .position(|t| t.language == language && t.is_loaded());
                match pos {
                    Some(pos) => Some(&mut lyrics.translations[pos].lyrics),
                    None => Some(lyrics),
                }
            }
            None => Some(lyrics),
        }
    }

    pub fn reset_translation_state(&mut self) {
        self.selected_translation = None;
        self.loading_translation = None;
    }

    pub fn mode_available(&self, mode: LyricsViewMode) -> bool {
        let Some(lyrics) = self.displayed_lyrics() else {
            return false;
        };
        match mode {
            LyricsViewMode::Selectable => lyrics.has_timed() || !lyrics.plain.is_empty(),
            LyricsViewMode::Synced => lyrics.has_timed(),
            LyricsViewMode::Plain => !lyrics.plain.is_empty(),
        }
    }

    pub fn note_target(&self) -> Option<usize> {
        let lyrics = self.displayed_lyrics()?;
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
