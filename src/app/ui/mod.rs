use crate::icons;
use crate::theme;
use crate::theme::AppTheme;
use crate::types::View;
use iced::{
    alignment,
    widget::{self, button, container, image, text, Column, Container, Row, Stack},
    Color, Element, Length,
};

use super::{ContextMenuState, DragTargetList, Message, MusicPlayer};

mod content;
mod overlays;
mod playbar;
mod playlist;
mod queue;
mod search;
mod sidebar;
mod track_list;

use track_list::view_track_list;

fn bg(color: Color) -> impl Fn(&AppTheme) -> container::Style + 'static {
    move |_| container::Style {
        background: Some(color.into()),
        ..Default::default()
    }
}

fn button_style_primary() -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    move |theme, status| {
        let p = &theme.palette;
        let bg_color = match status {
            button::Status::Hovered | button::Status::Pressed => p.accent_hover,
            _ => p.accent,
        };
        button::Style {
            background: Some(bg_color.into()),
            text_color: Color::BLACK,
            border: iced::border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

fn button_style_queue(
    enabled: bool,
) -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    move |theme, status| {
        let p = &theme.palette;
        let bg_color = match status {
            button::Status::Hovered | button::Status::Pressed => {
                if enabled {
                    p.accent_hover
                } else {
                    p.button_hover
                }
            }
            _ => {
                if enabled {
                    p.accent
                } else {
                    p.button
                }
            }
        };
        button::Style {
            background: Some(bg_color.into()),
            text_color: if enabled { Color::BLACK } else { p.fg },
            border: iced::border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

fn button_style_danger() -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    move |theme, status| {
        let p = &theme.palette;
        let bg_color = match status {
            button::Status::Hovered | button::Status::Pressed => p.danger_hover,
            _ => p.danger,
        };
        button::Style {
            background: Some(bg_color.into()),
            text_color: Color::WHITE,
            border: iced::border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

fn button_style_nav(
    enabled: bool,
) -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    move |theme, status| {
        let p = &theme.palette;
        let bg_color = match status {
            button::Status::Hovered | button::Status::Pressed => {
                if enabled {
                    p.button_hover
                } else {
                    p.bg
                }
            }
            _ => {
                if enabled {
                    p.button
                } else {
                    p.bg
                }
            }
        };
        let text_color = if enabled { p.fg } else { p.fg_muted };
        button::Style {
            background: Some(bg_color.into()),
            text_color,
            border: iced::border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

fn thumbnail<'a>(
    track: &'a crate::types::Track,
    p: &'a theme::Palette,
    size: f32,
) -> Element<'a, Message, AppTheme> {
    let thumb_path = crate::thumbnails::thumbnail_path(&track.id);
    let fallback_color = p.fg_muted;
    if thumb_path.exists() {
        image(widget::image::Handle::from_path(thumb_path))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .border_radius(size / 4.0)
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        icons::icon("music.svg", fallback_color, size).into()
    }
}

fn drop_indicator(color: Color) -> widget::Rule<'static, AppTheme> {
    widget::rule::horizontal(theme::DROP_LINE_HEIGHT).style(move |_| widget::rule::Style {
        color,
        radius: iced::border::Radius::new(0),
        fill_mode: widget::rule::FillMode::Full,
        snap: true,
    })
}

pub fn view(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let main_content = content::view_main_content(player);
    let sidebar = sidebar::view_sidebar(player);
    let queue = if player.show_queue {
        queue::view_queue_panel(player)
    } else {
        Container::new(Row::new()).width(Length::Fixed(0.0)).into()
    };

    let body = Row::with_children(vec![sidebar, main_content, queue])
        .height(Length::Fill)
        .align_y(alignment::Vertical::Top);

    let layout = Column::with_children(vec![body.into(), playbar::view_playbar(player)]).spacing(0);

    let main = Container::new(layout)
        .width(Length::Fill)
        .height(Length::Fill);

    let mut stack = Stack::new()
        .width(Length::Fill)
        .height(Length::Fill)
        .push(main);

    if player.show_playlist_picker.is_some() {
        stack = stack.push(overlays::view_playlist_picker(player));
    } else if player.show_delete_confirm {
        stack = stack.push(overlays::view_delete_confirm(player));
    } else if let Some(menu) = &player.context_menu {
        if menu.visible {
            stack = stack.push(overlays::view_context_menu(menu, &player.palette));
        }
    }

    stack.into()
}

fn view_notification(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    if let Some(msg) = &player.notification {
        return Container::new(
            text(msg)
                .size(theme::TEXT_SIZE_DEFAULT)
                .color(player.palette.fg)
                .center(),
        )
        .width(Length::Fill)
        .padding([theme::SPACING_XS, theme::SPACING_XL])
        .into();
    }
    Container::new(Row::new())
        .width(Length::Fill)
        .height(Length::Fixed(0.0))
        .into()
}
