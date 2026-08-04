use crate::icons;
use crate::theme;
use crate::types::View;
use iced::{
    alignment,
    widget::{
        self, button, container, image, scrollable, slider, text, text_input, Column, Container,
        Row, Stack,
    },
    Color, Element, Length,
};

use super::{ContextMenuState, DragTargetList, Message, MusicPlayer};
use crate::theme::Palette;

mod content;
mod overlays;
mod playbar;
mod playlist;
mod queue;
mod search;
mod sidebar;
mod track_list;

use track_list::view_track_list;

const fn scrollable_id(is_queue: bool) -> widget::Id {
    if is_queue {
        widget::Id::new("queue_list")
    } else {
        widget::Id::new("track_list")
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

fn button_style_primary(
    p: &Palette,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    button_style(p.accent, p.accent_hover, Color::BLACK)
}

fn button_style_secondary(
    p: &Palette,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    button_style(p.button, p.button_hover, p.fg)
}

fn button_style_queue(
    enabled: bool,
    p: &Palette,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    if enabled {
        // primary
        button_style(p.accent, p.accent_hover, Color::BLACK)
    } else {
        // secondary
        button_style(p.button, p.button_hover, p.fg)
    }
}

fn button_style_danger(
    p: &Palette,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    button_style(p.danger, p.danger_hover, Color::WHITE)
}

fn button_style_nav(
    enabled: bool,
    p: &Palette,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    button_style(
        if enabled { p.button } else { p.bg },
        if enabled { p.button_hover } else { p.bg },
        if enabled { p.fg } else { p.fg_muted },
    )
}

fn slider_style(
    accent: Color,
    bg_secondary: Color,
) -> impl Fn(&iced::Theme, slider::Status) -> slider::Style + 'static {
    move |_, status| {
        let color = match status {
            slider::Status::Active | slider::Status::Hovered | slider::Status::Dragged => accent,
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

fn text_input_style(
    p: &Palette,
) -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style + 'static {
    let p = *p;
    move |_, status| {
        let border_color = match status {
            text_input::Status::Hovered => p.fg_muted,
            text_input::Status::Focused { is_hovered: _ } => p.accent,
            _ => Color::TRANSPARENT,
        };
        text_input::Style {
            background: p.bg_tertiary.into(),
            border: iced::border::rounded(theme::RADIUS_MD)
                .color(border_color)
                .width(1),
            icon: p.fg_muted,
            placeholder: p.fg_muted,
            value: p.fg,
            selection: p.bg_selected,
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

fn drop_indicator(color: Color) -> widget::Rule<'static> {
    widget::rule::horizontal(theme::DROP_LINE_HEIGHT).style(move |_| widget::rule::Style {
        color,
        radius: iced::border::Radius::new(0),
        fill_mode: widget::rule::FillMode::Full,
        snap: true,
    })
}

pub fn view(player: &MusicPlayer) -> Element<'_, Message> {
    let p = &player.palette;

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
