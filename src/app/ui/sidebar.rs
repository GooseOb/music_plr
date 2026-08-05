use iced::{
    alignment,
    widget::{button, scrollable, text, text_input, Button, Column, Container, Row},
    Color, Element, Length,
};

use crate::{icons, theme::AppTheme};

use super::{
    styles::{bg_secondary, button_style_nav, button_style_primary},
    theme, widget, Message, MusicPlayer, View,
};

fn view_notification(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    if let Some(msg) = &player.notification {
        return Container::new(text(msg).size(theme::TEXT_SIZE_DEFAULT).center())
            .width(Length::Fill)
            .padding([theme::SPACING_XS, theme::SPACING_XL])
            .into();
    }
    Container::new(Row::new())
        .width(Length::Fill)
        .height(Length::Fixed(0.0))
        .into()
}

pub(super) fn view_sidebar(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let p = &player.app_theme.palette;

    let can_back = player.can_navigate_back();
    let can_forward = player.can_navigate_forward();

    let nav_buttons = Row::with_children(vec![
        Button::new(
            Container::new(icons::icon(
                icons::BACK_ICON,
                if can_back { p.fg } else { p.fg_muted },
                theme::ICON_SIZE_MD,
            ))
            .center(Length::Fill),
        )
        .padding(theme::SPACING_XS)
        .style(button_style_nav(can_back))
        .width(Length::Fill)
        .height(theme::BUTTON_HEIGHT)
        .on_press_maybe(if can_back {
            Some(Message::NavigateBack)
        } else {
            None
        })
        .into(),
        Button::new(
            Container::new(icons::icon(
                icons::FORWARD_ICON,
                if can_forward { p.fg } else { p.fg_muted },
                theme::ICON_SIZE_MD,
            ))
            .center(Length::Fill),
        )
        .padding(theme::SPACING_XS)
        .style(button_style_nav(can_forward))
        .width(Length::Fill)
        .height(theme::BUTTON_HEIGHT)
        .on_press_maybe(if can_forward {
            Some(Message::NavigateForward)
        } else {
            None
        })
        .into(),
    ])
    .spacing(theme::SPACING_XS)
    .align_y(alignment::Vertical::Center)
    .padding([theme::SPACING_MD, theme::SPACING_XL])
    .width(Length::Fill);

    let nav_items: Vec<Element<'_, Message, AppTheme>> = vec![
        sidebar_nav_item("Search", View::Search, player, p),
        sidebar_nav_item("Downloads", View::Downloads, player, p),
    ];

    let playlist_items: Vec<Element<'_, Message, AppTheme>> = player
        .playlists
        .playlists
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            let is_active = matches!(player.current_view, View::Playlist)
                && player.selected_playlist == Some(i);
            let is_dragged_over = player.drag.sidebar_hover_playlist == Some(i);
            let is_interacting = is_active || is_dragged_over;

            let bg_color = if is_dragged_over {
                p.bg_selected
            } else if is_active {
                p.bg_current.scale_alpha(0.7)
            } else {
                p.bg_secondary
            };

            let bg_hover = if is_dragged_over {
                p.bg_selected
            } else if is_active {
                p.bg_current
            } else {
                p.bg_hover
            };

            let icon_color = if is_interacting { p.accent } else { p.fg_muted };
            let text_color = if is_interacting { p.fg } else { p.fg_secondary };

            Button::new(
                Row::with_children(vec![
                    icons::icon(icons::MUSIC_ICON, icon_color, theme::ICON_SIZE_MD).into(),
                    text(&pl.name)
                        .size(theme::TEXT_SIZE_DEFAULT)
                        .color(text_color)
                        .into(),
                ])
                .spacing(10)
                .padding([theme::SPACING_SM, theme::SPACING_MD])
                .align_y(alignment::Vertical::Center)
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .padding(0)
            .style(move |_, status| {
                let bg = match status {
                    button::Status::Hovered | button::Status::Pressed => bg_hover,
                    _ => bg_color,
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color,
                    border: iced::border::rounded(theme::RADIUS_MD),
                    ..Default::default()
                }
            })
            .on_press(Message::SelectPlaylist(i))
            .into()
        })
        .collect();

    let create_row = Row::with_children(vec![
        Container::new(
            text_input("New playlist name", &player.playlist_create_name)
                .on_input(Message::NewPlaylistNameChanged)
                .padding([theme::SPACING_SM, theme::SPACING_SM])
                .size(theme::TEXT_SIZE_DEFAULT),
        )
        .width(Length::Fill)
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

    let sidebar_content = Column::with_children(vec![
        nav_buttons.into(),
        widget::rule::horizontal(1).into(),
        Column::with_children(nav_items)
            .spacing(theme::SPACING_XS)
            .into(),
        widget::rule::horizontal(1).into(),
        scrollable(
            Column::with_children(playlist_items)
                .spacing(theme::SPACING_XS)
                .width(Length::Fill),
        )
        .id(iced::widget::Id::new("sidebar_playlist_list"))
        .on_scroll(|vp| Message::SidebarListScrolled {
            offset_y: vp.absolute_offset().y,
            bounds: vp.bounds(),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
        view_notification(player),
        widget::rule::horizontal(1).into(),
        create_row.into(),
    ])
    .spacing(theme::SPACING_XS)
    .padding([theme::SPACING_SM, theme::SPACING_XS]);

    Container::new(sidebar_content)
        .width(Length::Fixed(theme::SIDEBAR_WIDTH))
        .height(Length::Fill)
        .style(bg_secondary())
        .into()
}

fn sidebar_nav_item<'a>(
    name: &'a str,
    view: View,
    player: &'a MusicPlayer,
    p: &theme::Palette,
) -> Element<'a, Message, AppTheme> {
    let is_active = player.current_view == view;
    let bg_color = if is_active {
        p.bg_current
    } else {
        p.bg_secondary
    };
    let icon_color = if is_active { p.accent } else { p.fg_muted };
    let text_color = if is_active { p.fg } else { p.fg_secondary };
    let bg_hover = p.bg_hover;
    let icon_name: &'static [u8] = match &view {
        View::Search => icons::SEARCH_ICON,
        View::SongRadio | View::ArtistRadio => icons::RADIO_ICON,
        View::Playlist => icons::MUSIC_ICON,
        View::Downloads => icons::DOWNLOAD_ICON,
    };

    Button::new(
        Row::with_children(vec![
            icons::icon(icon_name, icon_color, theme::ICON_SIZE_MD).into(),
            text(name)
                .size(theme::TEXT_SIZE_DEFAULT)
                .color(text_color)
                .into(),
        ])
        .spacing(theme::SPACING_MD)
        .padding([theme::SPACING_SM, theme::SPACING_MD])
        .align_y(alignment::Vertical::Center)
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding(0)
    .style(move |_, status| {
        let bg = if is_active {
            bg_color
        } else {
            match status {
                button::Status::Hovered | button::Status::Pressed => bg_hover,
                _ => bg_color,
            }
        };
        button::Style {
            background: Some(bg.into()),
            text_color,
            border: iced::border::rounded(theme::RADIUS_MD),
            ..Default::default()
        }
    })
    .on_press(Message::NavigateTo(view))
    .into()
}
