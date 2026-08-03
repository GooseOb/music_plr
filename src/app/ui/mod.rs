use crate::icons;
use crate::theme;
use crate::types::View;
use iced::{
    alignment,
    widget::{
        self, button, container, image, scrollable, slider, text, Column, Container, Row, Stack,
    },
    Color, Element, Length,
};

use super::{ContextMenuState, Message, MusicPlayer};

mod content;
mod overlays;
mod playbar;
mod sidebar;
mod track_list;

use track_list::view_track_list;

fn scrollable_id(is_queue: bool) -> iced::widget::Id {
    if is_queue {
        iced::widget::Id::new("queue_list")
    } else {
        iced::widget::Id::new("track_list")
    }
}

fn bg(color: Color) -> impl Fn(&iced::Theme) -> container::Style + 'static {
    move |_| container::Style {
        background: Some(color.into()),
        ..Default::default()
    }
}

fn button_style(
    bg: Color,
    bg_hover: Color,
    text_color: Color,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    move |_, status| {
        let bg_color = match status {
            button::Status::Hovered | button::Status::Pressed => bg_hover,
            _ => bg,
        };
        button::Style {
            background: Some(bg_color.into()),
            text_color,
            border: iced::border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

fn button_style_accent() -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    button_style(
        Color::from_rgb8(0x2a, 0x2a, 0x34),
        Color::from_rgb8(0x3a, 0x3a, 0x44),
        Color::WHITE,
    )
}

fn button_style_green() -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    button_style(
        Color::from_rgb8(0x14, 0xc8, 0x84),
        Color::from_rgba8(0x14, 0xc8, 0x84, 0.8),
        Color::BLACK,
    )
}

fn slider_style(
    accent: Color,
    bg_secondary: Color,
) -> impl Fn(&iced::Theme, slider::Status) -> slider::Style + 'static {
    move |_, status| {
        let color = match status {
            slider::Status::Active => accent,
            slider::Status::Hovered => accent,
            slider::Status::Dragged => accent,
        };
        slider::Style {
            rail: slider::Rail {
                backgrounds: (color.into(), bg_secondary.into()),
                width: 4.0,
                border: iced::border::rounded(2.0),
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Circle { radius: 7.0 },
                background: color.into(),
                border_color: Color::TRANSPARENT,
                border_width: 0.0,
            },
        }
    }
}

fn thumbnail<'a>(
    track: &'a crate::types::Track,
    p: &theme::Palette,
    size: f32,
) -> Element<'a, Message> {
    let thumb_path = crate::thumbnails::thumbnail_path(&track.id);
    let fallback_color = p.fg_muted;
    if thumb_path.exists() {
        image(iced::widget::image::Handle::from_path(thumb_path))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        icons::icon("music.svg", fallback_color, size).into()
    }
}

fn drop_indicator(color: Color) -> Container<'static, Message> {
    Container::new(Row::new())
        .width(Length::Fill)
        .height(Length::Fixed(crate::theme::DROP_LINE_HEIGHT))
        .style(move |_| container::Style {
            background: Some(color.into()),
            ..Default::default()
        })
}

pub fn view(player: &MusicPlayer) -> Element<'_, Message> {
    let p = &player.palette;

    let main_content = content::view_main_content(player);
    let sidebar = sidebar::view_sidebar(player);
    let queue = if player.show_queue {
        track_list::view_queue_panel(player)
    } else {
        Container::new(Row::new()).width(Length::Fixed(0.0)).into()
    };

    let body = Row::with_children(vec![sidebar, main_content, queue])
        .height(Length::Fill)
        .align_y(alignment::Vertical::Top);

    let layout = Column::with_children(vec![
        view_notification(player),
        body.into(),
        playbar::view_playbar(player),
    ])
    .spacing(0);

    let main = Container::new(layout)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(bg(p.bg));

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

fn view_notification(player: &MusicPlayer) -> Element<'_, Message> {
    if let Some(msg) = &player.notification {
        return Container::new(
            text(msg)
                .size(theme::TEXT_SIZE_DEFAULT)
                .color(Color::WHITE)
                .center(),
        )
        .width(Length::Fill)
        .padding([theme::SPACING_XS, theme::SPACING_XL])
        .style(bg(player.palette.warning))
        .into();
    }
    Container::new(Row::new())
        .width(Length::Fill)
        .height(Length::Fixed(0.0))
        .into()
}