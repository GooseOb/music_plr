use iced::{
    alignment,
    widget::{
        scrollable, text, text_editor::Binding, Button, Column, Container, MouseArea, Row, Space,
    },
    Color, Element, Length,
};

use crate::{
    app::{LyricsState, LyricsViewMode},
    load_state::LoadState,
    theme::AppTheme,
};

use super::{
    shared_components::{empty_state, scope_button, scope_tab_row},
    styles::{button_style_panel_item, fg_secondary},
    theme, Message, MusicPlayer,
};

pub(super) fn view_lyrics<'a>(
    player: &'a MusicPlayer,
    lyrics_state: &'a LyricsState,
) -> Element<'a, Message, AppTheme> {
    let track = player.queue.current();

    let lyrics_ready = matches!(&lyrics_state.lyrics, LoadState::Ready(_));

    let body: Element<'a, Message, AppTheme> = if lyrics_state.mode == LyricsViewMode::Selectable
        && lyrics_ready
    {
        view_select_editor(&lyrics_state.editor)
    } else {
        let lyrics_state = &lyrics_state.lyrics;
        match (track, lyrics_state) {
            (Some(_), LoadState::Ready(lyrics)) if !lyrics.timed.is_empty() => {
                view_synced(player, lyrics)
            }
            (Some(_), LoadState::Ready(lyrics)) => Container::new(
                text(lyrics.plain.clone())
                    .size(theme::TEXT_SIZE_LG)
                    .center()
                    .style(fg_secondary())
                    .width(Length::Fill),
            )
            .padding(theme::SPACING_LG)
            .into(),
            (Some(_), LoadState::Loading) => empty_state("Looking up lyrics…"),
            (Some(_), LoadState::Failed(e)) => empty_state(format!("Couldn't load lyrics: {e}")),
            (None, _) => empty_state("Play a track to see its lyrics."),
        }
    };

    let body: Element<'a, Message, AppTheme> = MouseArea::new(body)
        .on_right_press(Message::CopyLyrics)
        .into();

    Column::with_children([
        Container::new(body).height(Length::Fill).into(),
        view_bottom_controls(player, lyrics_state).into(),
    ])
    .spacing(theme::SPACING_MD)
    .into()
}

fn view_bottom_controls<'a>(
    player: &'a MusicPlayer,
    lyrics_state: &'a LyricsState,
) -> Row<'a, Message, AppTheme> {
    const MODES: [(LyricsViewMode, &str); 3] = [
        (LyricsViewMode::Selectable, "Selectable"),
        (LyricsViewMode::Synced, "Synced"),
        (LyricsViewMode::Plain, "Plain"),
    ];

    let picker = Row::with_children(MODES.iter().map(|&(mode, label)| {
        let selected = lyrics_state.mode == mode;
        let available = lyrics_state.mode_available(mode);
        scope_button(label, selected)
            .on_press_maybe(available.then_some(Message::SetLyricsViewMode(mode)))
            .into()
    }))
    .spacing(theme::SPACING_XS);

    let selected_provider = player.lyrics_client.selected();
    let provider_row = scope_tab_row(crate::lyrics::LyricsProvider::all().iter().map(|provider| {
        (
            provider.name().to_string(),
            *provider == selected_provider,
            Message::SelectLyricsProvider(*provider),
        )
    }));

    Row::with_children([
        provider_row,
        Space::new().width(Length::Fill).into(),
        picker.into(),
    ])
    .padding(theme::SPACING_SM)
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
