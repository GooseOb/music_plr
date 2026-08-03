use iced::{
    alignment,
    widget::{text, text_input, Button, Column, Container, Row},
    Color, Length,
};

use super::{
    bg, button, button_style_accent, button_style_nav, icons, scrollable, theme, widget, Element,
    Message, MusicPlayer, View,
};

pub(super) fn view_sidebar<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;

    let can_back = player.can_navigate_back();
    let can_forward = player.can_navigate_forward();

    let nav_buttons = Row::with_children(vec![
        Button::new(
            Container::new(icons::icon(
                "back.svg",
                if can_back { p.fg } else { p.fg_muted },
                theme::ICON_SIZE_MD,
            ))
            .center(Length::Fill),
        )
        .padding(6)
        .style(button_style_nav(can_back, p))
        .width(Length::Fill)
        .height(Length::Fixed(theme::BUTTON_HEIGHT))
        .on_press_maybe(if can_back {
            Some(Message::NavigateBack)
        } else {
            None
        })
        .into(),
        Button::new(
            Container::new(icons::icon(
                "forward.svg",
                if can_forward { p.fg } else { p.fg_muted },
                theme::ICON_SIZE_MD,
            ))
            .center(Length::Fill),
        )
        .padding(6)
        .style(button_style_nav(can_forward, p))
        .width(Length::Fill)
        .height(Length::Fixed(theme::BUTTON_HEIGHT))
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

    let nav_items: Vec<Element<'a, Message>> = vec![
        sidebar_nav_item("Search", View::Search, player, p),
        sidebar_nav_item("Downloads", View::Downloads, player, p),
    ];

    let playlist_items: Vec<Element<'a, Message>> = player
        .playlists
        .playlists
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            let is_selected = matches!(player.current_view, View::Playlist)
                && player.selected_playlist == Some(i);
            let bg_color = if is_selected {
                p.bg_current
            } else {
                p.bg_secondary
            };
            let icon_color = if is_selected { p.accent } else { p.fg_muted };
            let text_color = if is_selected { p.fg } else { p.fg_secondary };
            let bg_hover = p.bg_hover;
            let is_hover = player.drag.sidebar_hover_playlist == Some(i);

            Button::new(
                Row::with_children(vec![
                    icons::icon("music.svg", icon_color, theme::ICON_SIZE_SM).into(),
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
                let bg = if is_selected {
                    bg_color
                } else if is_hover {
                    p.accent
                } else {
                    match status {
                        button::Status::Hovered | button::Status::Pressed => bg_hover,
                        _ => bg_color,
                    }
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color,
                    border: iced::border::rounded(theme::RADIUS_SM),
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
                .padding([theme::SPACING_XS, theme::SPACING_SM])
                .size(theme::TEXT_SIZE_DEFAULT),
        )
        .width(Length::Fill)
        .into(),
        Button::new(icons::icon("folder.svg", Color::WHITE, theme::ICON_SIZE_SM))
            .padding(6)
            .style(button_style_accent())
            .on_press(Message::CreatePlaylist)
            .into(),
    ])
    .align_y(alignment::Vertical::Center)
    .spacing(6)
    .padding([theme::SPACING_SM, theme::SPACING_MD]);

    let import_btn = Button::new(
        Row::with_children(vec![
            icons::icon("folder.svg", Color::WHITE, theme::ICON_SIZE_SM).into(),
            text("Local Music")
                .size(theme::TEXT_SIZE_DEFAULT)
                .color(Color::WHITE)
                .into(),
        ])
        .spacing(6)
        .align_y(alignment::Vertical::Center),
    )
    .padding(theme::SPACING_SM)
    .width(Length::Fill)
    .style(button_style_accent())
    .on_press(Message::AddLocalMusic);

    let sidebar_content = Column::with_children(vec![
        nav_buttons.into(),
        Container::new(widget::rule::horizontal(1))
            .width(Length::Fill)
            .padding([theme::SPACING_SM, 0.0])
            .into(),
        Column::with_children(nav_items).spacing(2).into(),
        Container::new(widget::rule::horizontal(1))
            .width(Length::Fill)
            .padding([theme::SPACING_SM, 0.0])
            .into(),
        scrollable(
            Column::with_children(playlist_items)
                .spacing(0)
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
        Container::new(widget::rule::horizontal(1))
            .width(Length::Fill)
            .padding([theme::SPACING_SM, 0.0])
            .into(),
        create_row.into(),
        import_btn.into(),
    ]);

    Container::new(sidebar_content)
        .width(Length::Fixed(theme::SIDEBAR_WIDTH))
        .height(Length::Fill)
        .style(bg(p.bg_secondary))
        .into()
}

fn sidebar_nav_item<'a>(
    name: &'a str,
    view: View,
    player: &'a MusicPlayer,
    p: &theme::Palette,
) -> Element<'a, Message> {
    let is_active = player.current_view == view;
    let bg_color = if is_active {
        p.bg_current
    } else {
        p.bg_secondary
    };
    let icon_color = if is_active { p.accent } else { p.fg_muted };
    let text_color = if is_active { p.fg } else { p.fg_secondary };
    let bg_hover = p.bg_hover;
    let icon_name: &'a str = match &view {
        View::Search => "search.svg",
        View::SongRadio | View::ArtistRadio => "radio.svg",
        View::Playlist => "music.svg",
        View::Downloads => "download.svg",
    };

    Button::new(
        Row::with_children(vec![
            icons::icon(icon_name, icon_color, theme::ICON_SIZE_MD).into(),
            text(name)
                .size(theme::TEXT_SIZE_DEFAULT)
                .color(text_color)
                .into(),
        ])
        .spacing(10)
        .padding([theme::SPACING_LG, theme::SPACING_XL])
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
            border: iced::border::rounded(theme::RADIUS_LG),
            ..Default::default()
        }
    })
    .on_press(Message::NavigateTo(view))
    .into()
}
