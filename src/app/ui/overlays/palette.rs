//! The command palette (`F1` / `?` / `Ctrl+P`): fuzzy search over commands, views, saved
//! library entries and playlists. `PaletteMode::Playlists` is the `Ctrl+K`
//! picker, and `PaletteMode::AddToPlaylist` narrows to the add-to-playlist
//! picker.

use iced::{
    alignment,
    widget::{container, scrollable, text, text_input, Column, Container, MouseArea},
    Element, Length,
};

use super::{theme, view_dialog, Message, MusicPlayer};
use crate::{
    app::{
        palette::{Palette, PaletteGroup, PaletteMode, PaletteRow},
        ui::styles::{fg_secondary, scroll_padding},
    },
    theme::AppTheme,
};

pub fn palette_input_id() -> iced::widget::Id {
    iced::widget::Id::new("palette_input")
}

pub fn palette_list_id() -> iced::widget::Id {
    iced::widget::Id::new("palette_list")
}

pub(super) fn view_palette<'a>(
    player: &'a MusicPlayer,
    palette: &'a Palette,
) -> Element<'a, Message, AppTheme> {
    let strings = player.strings;
    let dir = strings.direction;
    let rows = palette.rows(player);

    let mut group = None;
    let list = rows.iter().enumerate().flat_map(|(pos, row)| {
        let new_group = group != Some(row.group);
        group = Some(row.group);
        new_group
            .then(|| group_header(row.group, player))
            .into_iter()
            .chain([row_view(player, palette, pos, row)])
    });

    let list: Element<'_, Message, AppTheme> = if rows.is_empty() {
        Container::new(
            text(strings.no_results_found)
                .style(fg_secondary())
                .width(Length::Fill),
        )
        .padding([theme::SPACING_XS, theme::SPACING_MD])
        .into()
    } else {
        scrollable(
            Column::with_children(list)
                .spacing(theme::SPACING_XS)
                .padding(scroll_padding()),
        )
        .id(palette_list_id())
        .on_scroll(|viewport| Message::PaletteViewport(viewport.bounds().height))
        .height(Length::Fill)
        .into()
    };

    let placeholder = if palette.mode == PaletteMode::All {
        strings.palette_hint
    } else {
        strings.find_in_list
    };
    let input = text_input(placeholder, &palette.query)
        .on_input(Message::PaletteInput)
        .align_x(dir.align())
        .padding(theme::SPACING_SM)
        .id(palette_input_id())
        .width(Length::Fill);

    let title = match &palette.mode {
        PaletteMode::AddToPlaylist(add) => {
            let title = if add.indices.len() > 1 {
                (strings.ctx_add_to_playlist_n)(add.indices.len())
            } else {
                strings.ctx_add_to_playlist.to_string()
            };
            Some(text(title).size(theme::TEXT_SIZE_LG).into())
        }
        _ => None,
    };
    let children = title.into_iter().chain([input.into(), list]);

    view_dialog(
        Column::with_children(children)
            .align_x(alignment::Horizontal::Center)
            .spacing(theme::SPACING_SM)
            .width(theme::DIALOG_WIDTH_LG)
            .height(player.window_size.height * 0.6)
            .padding(theme::SPACING_MD)
            .into(),
        Message::CloseDialog,
    )
}

fn group_header(group: PaletteGroup, player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    Container::new(
        text(group.title(player.strings))
            .size(theme::TEXT_SIZE_SM)
            .style(fg_secondary()),
    )
    .padding([theme::SPACING_XS, theme::SPACING_MD])
    .height(theme::PALETTE_HEADER_HEIGHT)
    .width(Length::Fill)
    .align_x(player.strings.direction.align())
    .align_y(alignment::Vertical::Center)
    .into()
}

fn row_view<'a>(
    player: &'a MusicPlayer,
    palette: &'a Palette,
    pos: usize,
    row: &PaletteRow,
) -> Element<'a, Message, AppTheme> {
    let dir = player.strings.direction;
    // Labels are pinned to one line and clipped inside their own box: the row
    // height is fixed (so the scroll offset can be computed), and a long title
    // must not wrap into the next row or run under the detail.
    let label = Container::new(
        text(row.label.clone())
            .size(theme::TEXT_SIZE_MD)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .align_x(dir.align())
    .clip(true);
    let content = dir
        .row(
            [label.into()]
                .into_iter()
                .chain(row.detail.as_ref().map(|detail| {
                    text(detail.clone())
                        .size(theme::TEXT_SIZE_MD)
                        .wrapping(iced::widget::text::Wrapping::None)
                        .style(fg_secondary())
                        .into()
                })),
        )
        .spacing(theme::SPACING_MD)
        .align_y(alignment::Vertical::Center)
        .width(Length::Fill);
    let padded = Container::new(content)
        .padding([theme::SPACING_SM, theme::SPACING_MD])
        .height(theme::PALETTE_ROW_HEIGHT)
        .align_y(alignment::Vertical::Center)
        .width(Length::Fill);
    let selected = pos == palette.selected;
    let highlighted = Container::new(
        MouseArea::new(padded)
            .on_press(Message::PaletteRun(pos))
            .on_enter(Message::PaletteHover(pos))
            .interaction(iced::mouse::Interaction::Pointer),
    )
    .style(move |theme: &AppTheme| container::Style {
        background: selected.then(|| theme.palette.bg_current.into()),
        border: iced::border::rounded(theme::RADIUS_SM),
        ..Default::default()
    });
    highlighted.into()
}
