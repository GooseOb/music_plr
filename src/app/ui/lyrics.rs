use iced::{
    alignment,
    widget::{button, scrollable, text, text_editor::Binding, Button, Column, Container, Row},
    Color, Element, Length,
};

use crate::theme::AppTheme;

use super::{
    styles::{button_style_panel_item, fg_secondary},
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

    Column::with_children(vec![
        Container::new(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        view_bottom_controls(player, is_select_mode).into(),
    ])
    .spacing(theme::SPACING_MD)
    .padding(theme::SPACING_LG)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn view_bottom_controls(player: &MusicPlayer, is_select_mode: bool) -> Row<'_, Message, AppTheme> {
    let select_toggle = Button::new(
        text(if is_select_mode { "Deselect" } else { "Select" }).size(theme::TEXT_SIZE_SM),
    )
    .padding([theme::SPACING_XS, theme::SPACING_MD])
    .style(move |theme: &AppTheme, _| {
        let p = &theme.palette;
        button::Style {
            background: Some(
                if is_select_mode {
                    p.accent
                } else {
                    p.bg_secondary
                }
                .into(),
            ),
            text_color: if is_select_mode {
                Color::BLACK
            } else {
                p.fg_secondary
            },
            border: iced::border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    })
    .on_press(Message::ToggleLyricsSelectMode);

    let provider_row = view_provider_selector(player);

    Row::with_children(vec![select_toggle.into(), provider_row])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .width(Length::Fill)
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
                background: iced::Background::Color(Color::TRANSPARENT),
                border: iced::Border::default(),
                placeholder: Color::TRANSPARENT,
                value: p.fg_secondary,
                selection: p.accent.scale_alpha(0.4),
            }
        })
        .height(Length::Fill)
        .into()
}

fn view_provider_selector<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let selected = player.lyrics_client.selected();
    let chips: Vec<Element<'a, Message, AppTheme>> = crate::lyrics::LyricsProvider::all()
        .iter()
        .map(|provider| {
            let is_active = *provider == selected;
            let fg = if is_active { p.accent } else { p.fg_secondary };
            Button::new(text(provider.name()).size(theme::TEXT_SIZE_SM).color(fg))
                .padding([theme::SPACING_XS, theme::SPACING_MD])
                .style(move |_, _| button::Style {
                    background: Some(
                        if is_active {
                            p.bg_current
                        } else {
                            p.bg_secondary
                        }
                        .into(),
                    ),
                    text_color: fg,
                    border: iced::border::rounded(theme::RADIUS_SM),
                    ..Default::default()
                })
                .on_press(Message::SelectLyricsProvider(*provider))
                .into()
        })
        .collect();

    Row::with_children(chips)
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .width(Length::Fill)
        .into()
}

fn view_synced<'a>(
    player: &'a MusicPlayer,
    lyrics: &'a crate::lyrics::Lyrics,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let position = player.progress * player.duration;
    let active = lyrics.active_index(position);

    let lines: Vec<Element<'a, Message, AppTheme>> = lyrics
        .timed
        .iter()
        .enumerate()
        .map(|(i, (secs, line))| {
            let is_active = active == Some(i);
            let fg = if is_active { p.fg } else { p.fg_secondary };

            let centered =
                Container::new(text(line).size(theme::TEXT_SIZE_XL)).center(Length::Fill);

            Button::new(centered)
                .width(Length::Fill)
                .padding([theme::SPACING_SM, theme::SPACING_LG])
                .style(button_style_panel_item(is_active, fg))
                .on_press(Message::LyricsLineClicked(*secs))
                .into()
        })
        .collect();

    scrollable(
        Column::with_children(lines)
            .spacing(theme::SPACING_SM)
            .padding(theme::SPACING_LG)
            .width(Length::Fill),
    )
    .into()
}
