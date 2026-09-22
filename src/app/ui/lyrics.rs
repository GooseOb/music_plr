use iced::{
    alignment,
    widget::{
        scrollable, text, text_editor::Binding, Button, Column, Container, Id, MouseArea, Row,
        Space,
    },
    Color, Element, Length,
};

pub fn lyrics_scroll_id(pane: PaneId) -> Id {
    Id::from(format!("lyrics_scroll:{pane}"))
}

use super::{
    shared_components::{empty_state, loading_state, scope_button, scope_tab_row},
    styles::{
        bg_secondary, button_style_danger, button_style_panel_item, button_style_primary,
        fg_secondary,
    },
    theme, Message, MusicPlayer,
};
use crate::{
    app::{pane::PaneId, LyricsState, LyricsViewMode},
    load_state::LoadState,
    theme::AppTheme,
};

pub(super) fn view_lyrics<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    lyrics_state: &'a LyricsState,
) -> Element<'a, Message, AppTheme> {
    if lyrics_state.editing {
        return view_custom_editor(player, pane, lyrics_state);
    }
    let track = player.queue.current();

    let lyrics_ready = matches!(&lyrics_state.lyrics, LoadState::Ready(_));

    let body: Element<'a, Message, AppTheme> = if lyrics_state.mode == LyricsViewMode::Selectable
        && lyrics_ready
    {
        view_select_editor(pane, &lyrics_state.editor)
    } else {
        let mode = lyrics_state.mode;
        let scrolled = lyrics_state.scrolled_to;
        let picked = lyrics_state.picked_line;
        let lyrics_state = &lyrics_state.lyrics;
        match (track, lyrics_state) {
            (Some(_), LoadState::Ready(lyrics))
                if lyrics.has_timed() && mode == LyricsViewMode::Synced =>
            {
                view_synced(pane, lyrics, scrolled)
            }
            (Some(_), LoadState::Ready(lyrics)) if lyrics.has_notes() => {
                view_plain_notes(pane, lyrics, picked)
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
        .on_right_press(Message::CopyLyrics(pane))
        .into();

    let note_idx = lyrics_state.note_target();
    let note_visible = match (&lyrics_state.lyrics, note_idx) {
        (LoadState::Ready(lyrics), Some(idx)) => lyrics.lines.get(idx).is_some_and(|line| {
            !line.description.is_empty()
                || (lyrics_state.note_line == Some(idx)
                    && lyrics_state.note_editor.text().trim_end() != line.description)
        }),
        _ => false,
    };

    let mut children: Vec<Element<'a, Message, AppTheme>> = Vec::with_capacity(4);
    if track.is_some() {
        if let Some(name) = &lyrics_state.selected_custom {
            children.push(view_edit_custom_row(player, pane, name));
        }
    }
    children.push(Container::new(body).height(Length::Fill).into());
    if note_visible {
        children.push(view_note_block(pane, lyrics_state));
    }
    children.push(view_bottom_controls(player, pane, lyrics_state).into());
    Column::with_children(children)
        .spacing(theme::SPACING_MD)
        .into()
}

fn view_note_block(pane: PaneId, lyrics_state: &LyricsState) -> Element<'_, Message, AppTheme> {
    let editor = iced::widget::text_editor(&lyrics_state.note_editor)
        .on_action(move |a| Message::LyricNoteAction(pane, a))
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
        .padding([theme::SPACING_XS, theme::SPACING_MD])
        .height(Length::Shrink);

    Container::new(editor)
        .style(bg_secondary())
        .padding(theme::SPACING_XS)
        .into()
}

fn view_edit_custom_row<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    name: &'a str,
) -> Element<'a, Message, AppTheme> {
    Button::new(Container::new(text(player.strings.edit_lyrics)).center_x(Length::Fill))
        .width(Length::Fill)
        .padding([theme::SPACING_XS, theme::SPACING_MD])
        .on_press(Message::EditCustomLyrics(pane, name.to_string()))
        .into()
}

fn view_custom_editor<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    lyrics_state: &'a LyricsState,
) -> Element<'a, Message, AppTheme> {
    let editor = iced::widget::text_editor(&lyrics_state.edit_content)
        .on_action(move |a| Message::CustomLyricsEditorAction(pane, a))
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
        .on_press(Message::SaveCustomLyrics(pane));

    let cancel_btn =
        Button::new(Container::new(text(player.strings.cancel)).center_x(Length::Fill))
            .padding(theme::SPACING_SM)
            .on_press(Message::CancelCustomLyricsEdit(pane));

    let mut buttons: Vec<Element<'a, Message, AppTheme>> = vec![cancel_btn.into(), save_btn.into()];
    if lyrics_state.editing_custom_name.is_some() {
        buttons.push(
            Button::new(Container::new(text(player.strings.delete)).center_x(Length::Fill))
                .padding(theme::SPACING_SM)
                .style(button_style_danger())
                .on_press(Message::DeleteCustomLyrics(pane))
                .into(),
        );
    }

    Column::with_children([
        Container::new(
            iced::widget::text_input(player.strings.lyrics_name, &lyrics_state.edit_name)
                .on_input(move |s| Message::CustomLyricsNameChanged(pane, s))
                .padding([theme::SPACING_SM, theme::SPACING_MD]),
        )
        .padding([0.0, theme::SPACING_XS])
        .into(),
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
    pane: PaneId,
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
            .on_press_maybe(available.then_some(Message::SetLyricsViewMode(pane, mode)))
            .into()
    }))
    .spacing(theme::SPACING_XS);

    let selected_provider = lyrics_state.provider;
    let selected_custom = lyrics_state.selected_custom.as_deref();
    let provider_row = scope_tab_row(
        crate::lyrics::LyricsProvider::all()
            .iter()
            .map(|provider| {
                (
                    provider.name().to_string(),
                    selected_custom.is_none() && *provider == selected_provider,
                    Message::SelectLyricsProvider(pane, *provider),
                )
            })
            .chain(lyrics_state.custom_names.iter().map(|name| {
                (
                    name.clone(),
                    selected_custom == Some(name.as_str()),
                    Message::SelectCustomLyrics(pane, name.clone()),
                )
            }))
            .chain(std::iter::once((
                player.strings.add_custom.to_string(),
                false,
                Message::StartCustomLyricsEdit(pane),
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
    pane: PaneId,
    editor_content: &iced::widget::text_editor::Content,
) -> Element<'_, Message, AppTheme> {
    iced::widget::text_editor(editor_content)
        .on_action(move |a| Message::LyricsEditorAction(pane, a))
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

fn view_plain_notes(
    pane: PaneId,
    lyrics: &crate::lyrics::Lyrics,
    picked: Option<usize>,
) -> Element<'_, Message, AppTheme> {
    let rows = lyrics.lines.iter().enumerate().map(|(i, line)| {
        let centered = Container::new(
            text(&line.text)
                .size(theme::TEXT_SIZE_LG)
                .style(fg_secondary()),
        )
        .center(Length::Fill);

        Button::new(centered)
            .padding([theme::SPACING_SM, theme::SPACING_LG])
            .style(button_style_panel_item(picked == Some(i)))
            .on_press(Message::SelectLyricLine(pane, i))
            .into()
    });

    scrollable(
        Column::with_children(rows)
            .spacing(theme::SPACING_SM)
            .padding(theme::SPACING_LG),
    )
    .into()
}

fn view_synced(
    pane: PaneId,
    lyrics: &crate::lyrics::Lyrics,
    scrolled: Option<usize>,
) -> Element<'_, Message, AppTheme> {
    let lines = lyrics.lines.iter().enumerate().filter_map(|(i, line)| {
        let secs = line.time?;
        let is_active = scrolled == Some(i);

        let centered =
            Container::new(text(&line.text).size(theme::TEXT_SIZE_XL)).center(Length::Fill);

        Some(
            Button::new(centered)
                .padding([theme::SPACING_SM, theme::SPACING_LG])
                .style(button_style_panel_item(is_active))
                .on_press(Message::LyricsLineClicked(secs))
                .into(),
        )
    });

    scrollable(
        Column::with_children(lines)
            .spacing(theme::SPACING_SM)
            .padding(theme::SPACING_LG),
    )
    .id(lyrics_scroll_id(pane))
    .on_scroll(move |vp| Message::LyricsScrolled {
        pane,
        translation_y: vp.absolute_offset().y,
        viewport_h: vp.bounds().height,
        content_h: vp.content_bounds().height,
    })
    .into()
}
