use iced::{
    alignment,
    widget::{scrollable, text, text_editor::Binding, Button, Column, Container, Row},
    Color, Element, Length,
};

use crate::{lyrics::LyricsProvider, theme::AppTheme};

use super::{
    styles::{button_style_panel_item, button_style_scope, fg_secondary},
    theme,
    track_list::empty_state,
    Message, MusicPlayer,
};

pub(super) fn view_lyrics<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let track = player.queue.current();
    let lyrics_state = player.lyrics.as_ref();
    let is_select_mode = lyrics_state.is_some_and(|s| s.select_mode);

    let body: Element<'a, Message, AppTheme> = if is_select_mode {
        lyrics_state
            .and_then(|state| state.editor.as_ref())
            .map_or_else(|| empty_state("No lyrics available."), view_select_editor)
    } else {
        let lyrics = lyrics_state.and_then(|state| state.lyrics.as_ref());
        match (track, lyrics) {
            (Some(_), Some(lyrics)) if !lyrics.timed.is_empty() => view_synced(player, lyrics),
            (Some(_), Some(lyrics)) => Container::new(
                text(lyrics.plain.clone())
                    .size(theme::TEXT_SIZE_LG)
                    .center()
                    .style(fg_secondary())
                    .width(Length::Fill),
            )
            .padding(theme::SPACING_LG)
            .into(),
            (Some(_), None) if lyrics_state.is_some_and(|s| s.loading) => {
                empty_state("Looking up lyrics…")
            }
            (Some(_), None) => empty_state("No lyrics available."),
            (None, _) => empty_state("Play a track to see its lyrics."),
        }
    };

    Column::with_children([
        Container::new(body).height(Length::Fill).into(),
        view_bottom_controls(player, is_select_mode).into(),
    ])
    .spacing(theme::SPACING_MD)
    .into()
}

fn view_bottom_controls(player: &MusicPlayer, is_select_mode: bool) -> Row<'_, Message, AppTheme> {
    let select_toggle = Button::new(
        text(if is_select_mode { "Deselect" } else { "Select" }).size(theme::TEXT_SIZE_SM),
    )
    .padding([theme::SPACING_XS, theme::SPACING_MD])
    .style(button_style_scope(is_select_mode))
    .on_press(Message::ToggleLyricsSelectMode);

    let provider_row = view_provider_selector(player.lyrics_client.selected());

    Row::with_children([select_toggle.into(), provider_row])
        .padding(theme::SPACING_SM)
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
}

fn view_select_editor(
    editor_content: &iced::widget::text_editor::Content,
) -> Element<'_, Message, AppTheme> {
    iced::widget::text_editor(editor_content)
        .on_action(Message::LyricsEditorAction)
        .key_binding(|press| {
            let binding = Binding::from_key_press(press)?;
            match binding {
                Binding::Copy
                | Binding::Move(_)
                | Binding::Select(_)
                | Binding::SelectWord
                | Binding::SelectLine
                | Binding::SelectAll
                | Binding::Unfocus => Some(binding),
                _ => None,
            }
        })
        .style(|theme: &AppTheme, _| {
            let p = &theme.palette;
            iced::widget::text_editor::Style {
                background: Color::TRANSPARENT.into(),
                border: iced::Border::default(),
                placeholder: Color::TRANSPARENT,
                value: p.fg_secondary,
                selection: p.accent.scale_alpha(0.4),
            }
        })
        .into()
}

fn view_provider_selector<'a>(selected_provider: LyricsProvider) -> Element<'a, Message, AppTheme> {
    let chips = crate::lyrics::LyricsProvider::all().iter().map(|provider| {
        let is_active = *provider == selected_provider;
        Button::new(text(provider.name()).size(theme::TEXT_SIZE_SM))
            .padding([theme::SPACING_XS, theme::SPACING_MD])
            .style(button_style_scope(is_active))
            .on_press(Message::SelectLyricsProvider(*provider))
            .into()
    });

    Row::with_children(chips)
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .into()
}

fn view_synced<'a>(
    player: &'a MusicPlayer,
    lyrics: &'a crate::lyrics::Lyrics,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let position = player.progress * player.duration;
    let active = lyrics.active_index(position);

    let lines = lyrics.timed.iter().enumerate().map(|(i, (secs, line))| {
        let is_active = active == Some(i);
        let fg = if is_active { p.fg } else { p.fg_secondary };

        let centered = Container::new(text(line).size(theme::TEXT_SIZE_XL)).center(Length::Fill);

        Button::new(centered)
            .padding([theme::SPACING_SM, theme::SPACING_LG])
            .style(button_style_panel_item(is_active, fg))
            .on_press(Message::LyricsLineClicked(*secs))
            .into()
    });

    scrollable(
        Column::with_children(lines)
            .spacing(theme::SPACING_SM)
            .padding(theme::SPACING_LG),
    )
    .into()
}
