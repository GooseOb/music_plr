use iced::{
    alignment,
    widget::{
        container, opaque, scrollable, text, text_editor::Binding, Button, Column, Container, Id,
        MouseArea, Row, Stack,
    },
    Border, Color, Element, Length,
};

pub fn lyrics_scroll_id(pane: PaneId) -> Id {
    Id::from(format!("lyrics_scroll:{pane}"))
}

use super::{
    shared_components::{empty_state, loading_state, scope_tab_row_h_scroll},
    styles::{
        bg_secondary, button_style_danger, button_style_panel_item, button_style_primary,
        fg_secondary,
    },
    theme, Message, MusicPlayer,
};
use crate::{
    app::{
        pane::PaneId,
        ui::shared_components::{corner_radius, dialog_button},
        LyricsState, LyricsViewMode,
    },
    load_state::LoadState,
    lyrics::LyricsProvider,
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

    let tr = player.strings;

    if player.session.queue.current().is_none() {
        return empty_state(tr.play_a_track_for_lyrics);
    }

    let displayed = lyrics_state.displayed_lyrics();
    let lyrics_ready = displayed.is_some();

    let body: Element<'a, Message, AppTheme> =
        if lyrics_state.mode == LyricsViewMode::Selectable && lyrics_ready {
            view_select_editor(pane, &lyrics_state.editor)
        } else {
            let mode = lyrics_state.mode;
            let scrolled = lyrics_state.scrolled_to;
            let picked = lyrics_state.picked_line;
            match &lyrics_state.lyrics {
                LoadState::Ready(_) => match displayed {
                    Some(lyrics) if lyrics.has_timed() && mode == LyricsViewMode::Synced => {
                        view_synced(pane, lyrics, scrolled)
                    }
                    Some(_) => view_plain_notes(pane, &lyrics_state.plain_lines, picked),
                    // `displayed` is always `Some` once lyrics are ready.
                    None => empty_state(""),
                },
                LoadState::Loading => loading_state(tr.looking_up_lyrics),
                LoadState::Failed(e) => empty_state((tr.couldnt_load_lyrics)(e)),
            }
        };

    let body: Element<'a, Message, AppTheme> = MouseArea::new(body)
        .on_right_press(Message::CopyLyrics(pane))
        .into();

    let note_idx = lyrics_state.note_target();
    let note_visible = match (displayed, note_idx) {
        (Some(lyrics), Some(idx)) => lyrics.lines.get(idx).is_some_and(|line| {
            !line.description.is_empty()
                || (lyrics_state.note_line == Some(idx)
                    && lyrics_state.note_editor.text().trim_end() != line.description)
        }),
        _ => false,
    };
    let corner_btn_data = if let Some(name) = lyrics_state.selected_custom.as_deref() {
        Some((
            tr.edit_lyrics,
            Message::EditCustomLyrics(pane, name.to_string()),
        ))
    } else if lyrics_state.provider != LyricsProvider::Custom && !lyrics_state.lyrics.is_loading() {
        Some((tr.refresh_lyrics, Message::RefreshLyrics(pane)))
    } else {
        None
    };

    let body: Element<'a, Message, AppTheme> = match corner_btn_data {
        Some((label, msg)) => {
            Stack::with_children([body, view_corner_button(dialog_button(label).on_press(msg))])
                .height(Length::Fill)
                .into()
        }
        None => Container::new(body).height(Length::Fill).into(),
    };

    let children = std::iter::once(body)
        .chain(note_visible.then(|| view_note_block(pane, lyrics_state)))
        .chain(
            view_translation_row(player, pane, lyrics_state)
                .map(|row| Container::new(row).style(bg_secondary()).into()),
        )
        .chain([
            Container::new(view_bottom_controls(player, pane, lyrics_state))
                .style(bg_secondary())
                .into(),
        ]);
    Column::with_children(children).into()
}

fn view_note_block(pane: PaneId, lyrics_state: &LyricsState) -> Element<'_, Message, AppTheme> {
    iced::widget::text_editor(&lyrics_state.note_editor)
        .on_action(move |a| Message::LyricNoteAction(pane, a))
        .style(|theme: &AppTheme, _| {
            let p = &theme.palette;
            iced::widget::text_editor::Style {
                background: p.bg_secondary.into(),
                border: iced::Border::default(),
                placeholder: Color::TRANSPARENT,
                value: p.fg_secondary,
                selection: p.accent.scale_alpha(0.4),
            }
        })
        .padding([theme::SPACING_MD, theme::SPACING_LG])
        .height(Length::Shrink)
        .into()
}

fn view_corner_button<'a>(
    btn: impl Into<Element<'a, Message, AppTheme>>,
) -> Element<'a, Message, AppTheme> {
    Container::new(opaque(
        Container::new(btn)
            .width(Length::Shrink)
            .style(|theme| container::Style {
                background: Some(theme.palette.bg_secondary.into()),
                border: Border {
                    width: 0.0,
                    color: Color::TRANSPARENT,
                    radius: corner_radius(0.0, theme::RADIUS_MD, 0.0, 0.0),
                },
                ..Default::default()
            })
            .padding(theme::SPACING_SM),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(alignment::Horizontal::Left)
    .align_y(alignment::Vertical::Bottom)
    .into()
}

fn view_custom_editor<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    lyrics_state: &'a LyricsState,
) -> Element<'a, Message, AppTheme> {
    let dir = player.strings.direction;
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

    let save_btn = dialog_button(player.strings.save)
        .style(button_style_primary())
        .on_press(Message::SaveCustomLyrics(pane));

    let cancel_btn =
        dialog_button(player.strings.cancel).on_press(Message::CancelCustomLyricsEdit(pane));

    let buttons = [cancel_btn.into(), save_btn.into()].into_iter().chain(
        lyrics_state.editing_custom_name.is_some().then(|| {
            dialog_button(player.strings.delete)
                .style(button_style_danger())
                .on_press(Message::DeleteCustomLyrics(pane))
                .into()
        }),
    );

    Column::with_children([
        Container::new(
            iced::widget::text_input(player.strings.lyrics_name, &lyrics_state.edit_name)
                .on_input(move |s| Message::CustomLyricsNameChanged(pane, s))
                .align_x(player.strings.direction.align())
                .padding([theme::SPACING_SM, theme::SPACING_MD]),
        )
        .padding([0.0, theme::SPACING_XS])
        .into(),
        text(player.strings.lyrics_editor_hint)
            .size(theme::TEXT_SIZE_SM)
            .style(fg_secondary())
            .into(),
        Container::new(editor).height(Length::Fill).into(),
        dir.row(buttons)
            .spacing(theme::SPACING_SM)
            .align_y(alignment::Vertical::Center)
            .into(),
    ])
    .spacing(theme::SPACING_MD)
    .padding(theme::SPACING_MD)
    .align_x(player.strings.direction.align())
    .into()
}

/// Language picker shown below the lyrics when the provider ships
/// translations for this track. Translations lazy-load on selection; the
/// in-flight language is disabled until its fetch settles.
fn view_translation_row<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    lyrics_state: &'a LyricsState,
) -> Option<Element<'a, Message, AppTheme>> {
    let LoadState::Ready(lyrics) = &lyrics_state.lyrics else {
        return None;
    };
    if lyrics.translations.is_empty() {
        return None;
    }
    let selected = lyrics_state.selected_translation.as_deref();
    let loading = lyrics_state.loading_translation.as_deref();
    let original = (
        player.strings.lyrics_original.to_string(),
        selected.is_none(),
        Some(Message::SelectLyricsTranslation(pane, None)),
    );
    let languages = lyrics.translations.iter().map(|translation| {
        let language = translation.language.clone();
        let busy = loading == Some(language.as_str());
        (
            crate::lyrics::language_name(&translation.language),
            selected == Some(language.as_str()) && translation.is_loaded(),
            (!busy).then_some(Message::SelectLyricsTranslation(pane, Some(language))),
        )
    });
    let row = scope_tab_row_h_scroll(std::iter::once(original).chain(languages));
    let dir = player.strings.direction;
    Some(
        dir.row(
            [
                text(player.strings.lyrics_translations)
                    .size(theme::TEXT_SIZE_SM)
                    .style(fg_secondary())
                    .into(),
                dir.trailing(row),
            ]
            .into_iter(),
        )
        .spacing(theme::SPACING_SM)
        .padding([0.0, theme::SPACING_SM])
        .align_y(alignment::Vertical::Center)
        .into(),
    )
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

    let picker = scope_tab_row_h_scroll(MODES.iter().zip(labels).map(|(&mode, label)| {
        (
            label,
            lyrics_state.mode == mode,
            lyrics_state
                .mode_available(mode)
                .then_some(Message::SetLyricsViewMode(pane, mode)),
        )
    }));

    let selected_provider = lyrics_state.provider;
    let selected_custom = lyrics_state.selected_custom.as_deref();
    let provider_row = scope_tab_row_h_scroll(
        player
            .config
            .enabled_lyrics_providers()
            .into_iter()
            .map(|provider| {
                (
                    provider.name().to_string(),
                    selected_custom.is_none() && provider == selected_provider,
                    Some(Message::SelectLyricsProvider(pane, provider)),
                )
            })
            .chain(lyrics_state.custom_names.iter().map(|name| {
                (
                    name.clone(),
                    selected_custom == Some(name.as_str()),
                    Some(Message::SelectCustomLyrics(pane, name.clone())),
                )
            }))
            .chain([
                (
                    player.strings.translate_with_ai.to_string(),
                    false,
                    Some(Message::OpenTranslateDialog(pane)),
                ),
                (
                    player.strings.add_custom.to_string(),
                    false,
                    Some(Message::StartCustomLyricsEdit(pane)),
                ),
            ]),
    );

    let dir = player.strings.direction;
    dir.row([dir.leading(provider_row), dir.trailing(picker)].into_iter())
        .spacing(theme::SPACING_SM)
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
    lines: &[String],
    picked: Option<usize>,
) -> Element<'_, Message, AppTheme> {
    let rows = lines.iter().enumerate().map(|(i, line)| {
        let centered = Container::new(text(line).size(theme::TEXT_SIZE_LG).style(fg_secondary()))
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
