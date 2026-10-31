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

    let mut list: Vec<Element<'a, Message, AppTheme>> = Vec::new();
    let mut group = None;
    for (pos, row) in rows.iter().enumerate() {
        if group != Some(row.group) {
            list.push(group_header(row.group, player));
            group = Some(row.group);
        }
        list.push(row_view(player, palette, pos, row));
    }

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

    let mut children: Vec<Element<'a, Message, AppTheme>> = Vec::new();
    if let PaletteMode::AddToPlaylist(add) = &palette.mode {
        let title = if add.indices.len() > 1 {
            (strings.ctx_add_to_playlist_n)(add.indices.len())
        } else {
            strings.ctx_add_to_playlist.to_string()
        };
        children.push(text(title).size(theme::TEXT_SIZE_LG).into());
    }
    children.push(input.into());
    children.push(list);

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
    let mut children: Vec<Element<'a, Message, AppTheme>> = vec![label.into()];
    if let Some(detail) = &row.detail {
        children.push(
            text(detail.clone())
                .size(theme::TEXT_SIZE_MD)
                .wrapping(iced::widget::text::Wrapping::None)
                .style(fg_secondary())
                .into(),
        );
    }
    let content = dir
        .row(children)
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
