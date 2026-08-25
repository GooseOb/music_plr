use iced::{
    alignment,
    widget::{scrollable, text, text_input, Button, Column, Container, MouseArea, Row},
    Color, Element, Length,
};

use crate::{
    app::{
        interaction::{DropTarget, HoverTarget, Pressed},
        ViewData, ViewKind,
    },
    data::library::LibraryItem,
    icons,
    theme::AppTheme,
};

use super::{
    shared_components::{thumbnail, toggle_bookmark_button},
    styles::{
        bg_secondary, button_style_list_item, button_style_nav, button_style_panel_item,
        button_style_primary, fg_secondary,
    },
    theme, widget, Message, MusicPlayer,
};

fn playlist_row<'a>(
    player: &'a MusicPlayer,
    index: usize,
    name: &'a str,
    track_count: usize,
    active: bool,
    dragged_over: bool,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let is_dragging_this = matches!(player.drag.pressed, Some(Pressed::Playlist(i)) if i == index);
    let is_hovered = player.drag.hovered_playlist() == Some(index);
    let interacting = active || dragged_over || is_hovered;
    let bg_color = if dragged_over {
        p.bg_current
    } else if active {
        p.bg_current.scale_alpha(0.7)
    } else if is_hovered {
        p.bg_hover
    } else {
        p.bg_secondary
    };
    let icon_color = if interacting { p.accent } else { p.fg_muted };
    let text_color = if interacting { p.fg } else { p.fg_secondary };

    let row = Row::with_children([
        icons::icon(icons::MUSIC_ICON, icon_color, theme::ICON_SIZE_MD).into(),
        text(name).color(text_color).into(),
        iced::widget::right(text(track_count).style(fg_secondary())).into(),
    ])
    .spacing(theme::SPACING_MD)
    .align_y(alignment::Vertical::Center);

    MouseArea::new(
        Container::new(row)
            .padding([theme::SPACING_SM, theme::SPACING_MD])
            .style(move |_| iced::widget::container::Style {
                background: Some(bg_color.into()),
                border: if is_dragging_this {
                    iced::border::rounded(theme::RADIUS_MD)
                        .width(2.0)
                        .color(p.accent)
                } else {
                    iced::border::rounded(theme::RADIUS_MD)
                },
                ..Default::default()
            }),
    )
    .interaction(player.drag.clickable_cursor_interaction())
    .on_press(Message::DragPress(Pressed::Playlist(index)))
    .on_double_click(Message::OpenAndPlayPlaylist(index))
    .on_move(move |_| Message::HoverStart(HoverTarget::Playlist(index)))
    .into()
}

fn library_row<'a>(
    player: &'a MusicPlayer,
    item: &'a LibraryItem,
    index: usize,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let is_active = match &player.view_data().kind {
        ViewKind::Artist(entry) => entry.id == item.id,
        ViewKind::Album(r) => r.id == item.id,
        ViewKind::PlaylistView(r) => r.id == item.id,
        _ => false,
    };
    let text_color = if is_active { p.fg } else { p.fg_secondary };
    let thumb = player.thumbnail_index.get(&item.id);
    let thumb = thumbnail(p, theme::ICON_SIZE_LG + 4.0, thumb);
    let is_hovered = player.drag.is_hovered_library_card(item);
    let hover_item = item.clone();
    let row = Row::with_children([thumb, text(&item.title).color(text_color).into()])
        .spacing(theme::SPACING_MD)
        .align_y(alignment::Vertical::Center)
        .width(Length::Fill);
    let toggle_btn = toggle_bookmark_button(p, true)
        .padding(theme::SPACING_XS)
        .on_press(Message::ToggleLibrarySave(item.clone()));

    let bg = if is_active {
        p.bg_current.scale_alpha(0.7)
    } else if is_hovered {
        p.bg_hover
    } else {
        p.bg_secondary
    };
    MouseArea::new(
        Container::new(
            Row::with_children([row.into(), toggle_btn.into()])
                .spacing(theme::SPACING_MD)
                .align_y(alignment::Vertical::Center),
        )
        .id(iced::widget::Id::from(format!("library:{index}")))
        .padding([theme::SPACING_SM, theme::SPACING_MD])
        .style(move |_| iced::widget::container::Style {
            background: Some(bg.into()),
            ..Default::default()
        }),
    )
    .interaction(player.drag.clickable_cursor_interaction())
    .on_press(Message::DragPress(Pressed::Card(item.clone())))
    .on_move(move |_| Message::HoverStart(HoverTarget::LibraryCard(hover_item.clone())))
    .into()
}

fn view_notification(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    if let Some(msg) = &player.notification {
        return Container::new(text(msg.as_ref()).center())
            .width(Length::Fill)
            .padding([theme::SPACING_XS, theme::SPACING_XL])
            .into();
    }
    Row::new().into()
}

fn sidebar_button(row: Row<'_, Message, AppTheme>) -> Button<'_, Message, AppTheme> {
    Button::new(
        row.spacing(theme::SPACING_MD)
            .align_y(alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .padding([theme::SPACING_SM, theme::SPACING_MD])
}

fn library_collapsed<'a>(msg: impl text::IntoFragment<'a>) -> Element<'a, Message, AppTheme> {
    Container::new(text(msg).style(fg_secondary()).size(theme::TEXT_SIZE_SM))
        .padding([theme::SPACING_XS, theme::SPACING_MD])
        .into()
}

fn nav_icon_button(
    can: bool,
    icon: &'static [u8],
    p: &crate::theme::Palette,
    on_press: Message,
) -> Element<'static, Message, AppTheme> {
    Button::new(
        Container::new(icons::icon(
            icon,
            if can { p.fg } else { p.fg_muted },
            theme::ICON_SIZE_MD,
        ))
        .center(Length::Fill),
    )
    .padding(theme::SPACING_XS)
    .style(button_style_nav(can))
    .height(theme::BUTTON_HEIGHT)
    .on_press_maybe(if can { Some(on_press) } else { None })
    .into()
}

#[allow(clippy::too_many_lines)]
pub(super) fn view_sidebar(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let p = &player.app_theme.palette;

    let can_back = player.can_navigate_back();
    let can_forward = player.can_navigate_forward();

    let nav_buttons = Row::with_children([
        nav_icon_button(can_back, icons::BACK_ICON, p, Message::NavigateBack),
        nav_icon_button(
            can_forward,
            icons::FORWARD_ICON,
            p,
            Message::NavigateForward,
        ),
    ])
    .spacing(theme::SPACING_XS)
    .align_y(alignment::Vertical::Center)
    .padding([theme::SPACING_MD, theme::SPACING_XL]);

    let nav_items: Vec<Element<'_, Message, AppTheme>> = vec![
        sidebar_nav_item(
            "Search",
            ViewData::new_search(String::new(), player.search_provider, player.search_scope),
            player,
        ),
        sidebar_nav_item("Downloads", downloads_view_data(player), player),
        sidebar_nav_item("Settings", ViewData::new_settings(), player),
    ];

    let playlist_items: Vec<Element<'_, Message, AppTheme>> = player
        .playlists
        .playlists
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            let is_active = match &player.view_data().kind {
                ViewKind::Playlist(entry) => entry.index == i,
                _ => false,
            };
            let is_dragged_over = matches!(
                player.drag.drop_target,
                Some(DropTarget::PlaylistAdd(j)) if j == i
            ) || matches!(
                player.drag.drop_target,
                Some(DropTarget::PlaylistReorder { to: j, .. }) if j == i
            );
            let row = playlist_row(
                player,
                i,
                &pl.name,
                pl.tracks.len(),
                is_active,
                is_dragged_over,
            );
            iced::widget::Container::new(row)
                .width(Length::Fill)
                .id(iced::widget::Id::from(format!("sidebar_pl:{i}")))
                .into()
        })
        .collect();

    let library_items: Vec<Element<'_, Message, AppTheme>> = player
        .library
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| library_row(player, item, i))
        .collect();

    let library_section: Element<'_, Message, AppTheme> = if player.library.items.is_empty() {
        library_collapsed("Nothing saved yet")
    } else if player.library_expanded {
        scrollable(Column::with_children(library_items).spacing(theme::SPACING_XS))
            .id(iced::widget::Id::new("sidebar_library_list"))
            .height(Length::Fill)
            .into()
    } else {
        library_collapsed(format!("{} saved", player.library.items.len()))
    };

    let library_header = Button::new(
        Row::with_children([
            icons::icon(icons::BOOKMARK_ICON, p.fg_muted, theme::ICON_SIZE_MD).into(),
            text("Library").style(fg_secondary()).into(),
            iced::widget::right(
                text(if player.library_expanded {
                    "▾"
                } else {
                    "▸"
                })
                .color(p.fg_muted),
            )
            .into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .padding([theme::SPACING_SM, theme::SPACING_MD])
    .style(button_style_list_item(false))
    .on_press(Message::ToggleLibraryExpanded)
    .into();

    let create_row = Row::with_children([
        Container::new(
            text_input("New playlist name", &player.playlist_create_name)
                .on_input(Message::NewPlaylistNameChanged)
                .padding([theme::SPACING_SM, theme::SPACING_SM]),
        )
        .into(),
        Button::new(icons::icon(
            icons::ADD_ICON,
            Color::BLACK,
            theme::ICON_SIZE_SM,
        ))
        .padding(theme::SPACING_MD - 2f32)
        .style(button_style_primary())
        .on_press(Message::CreatePlaylist)
        .into(),
    ])
    .align_y(alignment::Vertical::Center)
    .spacing(theme::SPACING_SM)
    .padding([theme::SPACING_SM, theme::SPACING_MD]);

    let sidebar_content = Column::with_children([
        nav_buttons.into(),
        widget::rule::horizontal(1).into(),
        Column::with_children(nav_items)
            .spacing(theme::SPACING_XS)
            .into(),
        widget::rule::horizontal(1).into(),
        scrollable(Column::with_children(playlist_items).spacing(theme::SPACING_XS))
            .id(iced::widget::Id::new("sidebar_playlist_list"))
            .height(Length::FillPortion(2))
            .into(),
        widget::rule::horizontal(1).into(),
        library_header,
        library_section,
        view_notification(player),
        widget::rule::horizontal(1).into(),
        create_row.into(),
    ])
    .spacing(theme::SPACING_XS)
    .padding([theme::SPACING_SM, theme::SPACING_XS]);

    Container::new(sidebar_content)
        .width(theme::SIDEBAR_WIDTH)
        .style(bg_secondary())
        .into()
}

fn sidebar_nav_item<'a>(
    name: &'a str,
    target: ViewData,
    player: &'a MusicPlayer,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let is_active = player.view_data().same_kind(&target);
    let icon_color = if is_active { p.accent } else { p.fg_muted };
    let text_color = if is_active { p.fg } else { p.fg_secondary };
    let icon_name: &[u8] = match target.kind {
        ViewKind::Search { .. } => icons::SEARCH_ICON,
        ViewKind::Downloads => icons::DOWNLOAD_ICON,
        ViewKind::Settings => icons::SETTINGS_ICON,
        _ => icons::MUSIC_ICON,
    };

    sidebar_button(Row::with_children([
        icons::icon(icon_name, icon_color, theme::ICON_SIZE_MD).into(),
        text(name).color(text_color).into(),
    ]))
    .style(button_style_panel_item(is_active, text_color))
    .on_press(Message::NavigateTo(target))
    .into()
}

fn downloads_view_data(_player: &MusicPlayer) -> ViewData {
    ViewData::new_downloads(Vec::new())
}
