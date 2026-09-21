use iced::{
    alignment,
    widget::{
        scrollable, text, text_editor::Binding, Button, Column, Container, Id, MouseArea, Row,
        Space,
    },
    Color, Element, Length,
};

pub const LYRICS_SCROLL_ID: Id = Id::new("lyrics_scroll");

use super::{
    shared_components::{empty_state, loading_state, scope_button, scope_tab_row, text_input_row},
    styles::{button_style_danger, button_style_panel_item, button_style_primary, fg_secondary},
    theme, Message, MusicPlayer,
};
use crate::{
    app::{LyricsState, LyricsViewMode},
    load_state::LoadState,
    theme::AppTheme,
};

pub(super) fn view_lyrics<'a>(
    player: &'a MusicPlayer,
    lyrics_state: &'a LyricsState,
) -> Element<'a, Message, AppTheme> {
    if lyrics_state.editing {
        return view_custom_editor(player, lyrics_state);
    }
    let track = player.queue.current();

    let lyrics_ready = matches!(&lyrics_state.lyrics, LoadState::Ready(_));

    let body: Element<'a, Message, AppTheme> = if lyrics_state.mode == LyricsViewMode::Selectable
        && lyrics_ready
    {
        view_select_editor(&lyrics_state.editor)
    } else {
        let mode = lyrics_state.mode;
        let scrolled = lyrics_state.scrolled_to;
        let lyrics_state = &lyrics_state.lyrics;
        match (track, lyrics_state) {
            (Some(_), LoadState::Ready(lyrics))
                if !lyrics.timed.is_empty() && mode == LyricsViewMode::Synced =>
            {
                view_synced(lyrics, scrolled)
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
            (Some(_), LoadState::Loading) => loading_state(player.strings.looking_up_lyrics),
            (Some(_), LoadState::Failed(e)) => empty_state((player.strings.couldnt_load_lyrics)(e)),
            (None, _) => empty_state(player.strings.play_a_track_for_lyrics),
        }
    };

    let body: Element<'a, Message, AppTheme> = MouseArea::new(body)
        .on_right_press(Message::CopyLyrics)
        .into();

    let mut children: Vec<Element<'a, Message, AppTheme>> = Vec::with_capacity(3);
    if track.is_some() {
        if let Some(name) = &lyrics_state.selected_custom {
            children.push(view_edit_custom_row(player, name));
        }
    }
    children.push(Container::new(body).height(Length::Fill).into());
    children.push(view_bottom_controls(player, lyrics_state).into());
    Column::with_children(children)
        .spacing(theme::SPACING_MD)
        .into()
}

fn view_edit_custom_row<'a>(
    player: &'a MusicPlayer,
    name: &'a str,
) -> Element<'a, Message, AppTheme> {
    Button::new(Container::new(text(player.strings.edit_lyrics)).center_x(Length::Fill))
        .width(Length::Fill)
        .padding([theme::SPACING_XS, theme::SPACING_MD])
        .on_press(Message::EditCustomLyrics(name.to_string()))
        .into()
}

fn view_custom_editor<'a>(
    player: &'a MusicPlayer,
    lyrics_state: &'a LyricsState,
) -> Element<'a, Message, AppTheme> {
    let editor = iced::widget::text_editor(&lyrics_state.edit_content)
        .on_action(Message::CustomLyricsEditorAction)
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
        .padding(theme::SPACING_LG)
        .height(Length::Fill);

    let save_btn = Button::new(Container::new(text(player.strings.save)).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .style(button_style_primary())
        .on_press(Message::SaveCustomLyrics);

    let cancel_btn =
        Button::new(Container::new(text(player.strings.cancel)).center_x(Length::Fill))
            .padding(theme::SPACING_SM)
            .on_press(Message::CancelCustomLyricsEdit);

    let mut buttons: Vec<Element<'a, Message, AppTheme>> = vec![cancel_btn.into(), save_btn.into()];
    if lyrics_state.editing_custom_name.is_some() {
        buttons.push(
            Button::new(Container::new(text(player.strings.delete)).center_x(Length::Fill))
                .padding(theme::SPACING_SM)
                .style(button_style_danger())
                .on_press(Message::DeleteCustomLyrics)
                .into(),
        );
    }

    Column::with_children([
        text_input_row(
            player.strings.lyrics_name,
            &lyrics_state.edit_name,
            player.strings.lyrics_name,
            Message::CustomLyricsNameChanged,
        ),
        text(player.strings.lyrics_editor_hint)
            .size(theme::TEXT_SIZE_SM)
            .style(fg_secondary())
            .into(),
        Container::new(editor).height(Length::Fill).into(),
        Row::with_children(buttons)
            .spacing(theme::SPACING_SM)
            .align_y(alignment::Vertical::Center)
            .into(),
    ])
    .spacing(theme::SPACING_MD)
    .padding(theme::SPACING_MD)
    .into()
}

fn view_bottom_controls<'a>(
    player: &'a MusicPlayer,
    lyrics_state: &'a LyricsState,
) -> Row<'a, Message, AppTheme> {
    const MODES: [LyricsViewMode; 3] = [
        LyricsViewMode::Selectable,
        LyricsViewMode::Synced,
        LyricsViewMode::Plain,
    ];
    let labels = [
        player.strings.lyrics_selectable,
        player.strings.lyrics_synced,
        player.strings.lyrics_plain,
    ];

    let picker = Row::with_children(MODES.iter().zip(labels).map(|(&mode, label)| {
        let selected = lyrics_state.mode == mode;
        let available = lyrics_state.mode_available(mode);
        scope_button(label, selected)
            .on_press_maybe(available.then_some(Message::SetLyricsViewMode(mode)))
            .into()
    }))
    .spacing(theme::SPACING_XS);

    let selected_provider = player.lyrics_client.selected();
    let selected_custom = lyrics_state.selected_custom.as_deref();
    let provider_row = scope_tab_row(
        crate::lyrics::LyricsProvider::all()
            .iter()
            .map(|provider| {
                (
                    provider.name().to_string(),
                    selected_custom.is_none() && *provider == selected_provider,
                    Message::SelectLyricsProvider(*provider),
                )
            })
            .chain(lyrics_state.custom_names.iter().map(|name| {
                (
                    name.clone(),
                    selected_custom == Some(name.as_str()),
                    Message::SelectCustomLyrics(name.clone()),
                )
            }))
            .chain(std::iter::once((
                player.strings.add_custom.to_string(),
                false,
                Message::StartCustomLyricsEdit,
            ))),
    );

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
        .padding(theme::SPACING_LG)
        .into()
}

fn view_synced(
    lyrics: &crate::lyrics::Lyrics,
    scrolled: Option<usize>,
) -> Element<'_, Message, AppTheme> {
    let lines = lyrics.timed.iter().enumerate().map(|(i, (secs, line))| {
        let is_active = scrolled == Some(i);

        let centered = Container::new(text(line).size(theme::TEXT_SIZE_XL)).center(Length::Fill);

        Button::new(centered)
            .padding([theme::SPACING_SM, theme::SPACING_LG])
            .style(button_style_panel_item(is_active))
            .on_press(Message::LyricsLineClicked(*secs))
            .into()
    });

    scrollable(
        Column::with_children(lines)
            .spacing(theme::SPACING_SM)
            .padding(theme::SPACING_LG),
    )
    .id(LYRICS_SCROLL_ID)
    .on_scroll(|vp| Message::LyricsScrolled {
        translation_y: vp.absolute_offset().y,
        viewport_h: vp.bounds().height,
        content_h: vp.content_bounds().height,
    })
    .into()
}
