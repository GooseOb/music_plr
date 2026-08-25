use std::time::Duration;

use iced::{
    advanced, alignment,
    widget::{canvas, container, image, text, text_input, Button, Column, Container, Id, Row},
    window, Color, Element, Length, Size, Vector,
};

use crate::{
    app::Message,
    icons,
    theme::{self, AppTheme, Palette},
};

use super::styles::{button_style_primary, button_style_scope, fg_secondary};
use iced::advanced::graphics::geometry::Renderer as _;
use iced_core::Renderer as _;

pub fn thumbnail<'a>(
    p: &Palette,
    size: f32,
    thumb: Option<&'a std::path::PathBuf>,
) -> Element<'a, Message, AppTheme> {
    if let Some(path) = thumb {
        image(image::Handle::from_path(path))
            .width(size)
            .height(size)
            .border_radius(size / 4.0)
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        icons::icon(icons::MUSIC_ICON, p.fg_muted, size).into()
    }
}

pub fn inner_row_layout<'a>(
    leading: Element<'a, Message, AppTheme>,
    thumbnail: Element<'a, Message, AppTheme>,
    title: &'a str,
    subtitle: Element<'a, Message, AppTheme>,
    trailing: Element<'a, Message, AppTheme>,
) -> Row<'a, Message, AppTheme> {
    Row::with_children([
        leading,
        thumbnail,
        Column::with_children([
            text(title)
                .size(theme::TEXT_SIZE_MD)
                .width(Length::Fill)
                .into(),
            subtitle,
        ])
        .spacing(theme::SPACING_2XS)
        .into(),
        trailing,
    ])
    .spacing(theme::SPACING_SM)
    .align_y(alignment::Vertical::Center)
    .padding([theme::SPACING_XS, theme::SPACING_SM])
}

/// Wraps row content in a container with an optional fixed height and
/// background color. `id` (when `Some`) tags the container so the bounds
/// `Operation` can capture its measured geometry for drop-target
/// hit-testing.
pub fn track_row<'a>(
    content: impl Into<Element<'a, Message, AppTheme>>,
    bg: Color,
    id: Option<Id>,
    border: Option<Color>,
) -> Container<'a, Message, AppTheme> {
    let container = Container::new(content)
        .height(theme::ROW_HEIGHT)
        .style(move |_: &AppTheme| container::Style {
            background: Some(bg.into()),
            border: border.map_or(iced::border::Border::default(), |c| iced::border::Border {
                width: 1.0,
                color: c,
                radius: 0.0.into(),
            }),
            ..Default::default()
        });
    match id {
        Some(id) => container.id(id),
        None => container,
    }
}

pub fn empty_state<'a>(msg: impl text::IntoFragment<'a>) -> Element<'a, Message, AppTheme> {
    Container::new(text(msg).style(fg_secondary()))
        .center(Length::Fill)
        .into()
}

/// Animated loading indicator: spinner above a status line. The widget
/// drives its own animation by rescheduling redraws on every window
/// `RedrawRequested` event, so it needs no subscription or messages; it
/// stops automatically when unmounted.
pub fn loading_state<'a>(
    p: &'a Palette,
    msg: impl text::IntoFragment<'a>,
) -> Element<'a, Message, AppTheme> {
    Container::new(
        Column::with_children([
            spinner(p.fg_secondary, theme::SPINNER_SIZE),
            text(msg).style(fg_secondary()).into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_x(alignment::Horizontal::Center),
    )
    .center(Length::Fill)
    .into()
}

pub fn spinner(color: Color, size: f32) -> Element<'static, Message, AppTheme> {
    Element::new(Spinner { color, size })
}

/// Self-animating arc spinner. Rotation is derived from the wall clock at
/// draw time; `update` keeps the redraw loop alive while mounted.
struct Spinner {
    color: Color,
    size: f32,
}

impl<Message> advanced::Widget<Message, AppTheme, iced::Renderer> for Spinner {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.size), Length::Fixed(self.size))
    }

    fn layout(
        &mut self,
        _tree: &mut advanced::widget::Tree,
        _renderer: &iced::Renderer,
        limits: &advanced::layout::Limits,
    ) -> advanced::layout::Node {
        advanced::layout::Node::new(limits.resolve(self.size, self.size, Size::ZERO))
    }

    fn update(
        &mut self,
        _tree: &mut advanced::widget::Tree,
        event: &iced::Event,
        _layout: advanced::Layout<'_>,
        _cursor: iced::mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn advanced::Clipboard,
        shell: &mut advanced::Shell<'_, Message>,
        _viewport: &iced::Rectangle,
    ) {
        if let iced::Event::Window(window::Event::RedrawRequested(now)) = event {
            shell.request_redraw_at(*now + Duration::from_millis(theme::SPINNER_FRAME_MS));
        }
    }

    fn draw(
        &self,
        _tree: &advanced::widget::Tree,
        renderer: &mut iced::Renderer,
        _theme: &AppTheme,
        _style: &advanced::renderer::Style,
        layout: advanced::Layout<'_>,
        _cursor: iced::mouse::Cursor,
        _viewport: &iced::Rectangle,
    ) {
        let bounds = layout.bounds();
        if bounds.width < 1.0 || bounds.height < 1.0 {
            return;
        }
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let stroke_width = (self.size / 10.0).max(2.0);
        let radius = bounds.width / 2.0 - stroke_width;
        if radius > 0.0 {
            let frac = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_millis()) as f32)
                / 1000.0;
            let start_angle: iced::Radians = (frac * std::f32::consts::TAU).into();
            let mut builder = canvas::path::Builder::new();
            builder.arc(canvas::path::Arc {
                center: frame.center(),
                radius,
                start_angle,
                end_angle: (start_angle.0 + std::f32::consts::TAU * 0.75).into(),
            });
            frame.stroke(
                &builder.build(),
                canvas::Stroke::default()
                    .with_color(self.color)
                    .with_width(stroke_width)
                    .with_line_cap(canvas::LineCap::Round),
            );
        }
        let geometry = frame.into_geometry();
        renderer.with_translation(Vector::new(bounds.x, bounds.y), |renderer| {
            renderer.draw_geometry(geometry);
        });
    }
}

pub fn scope_button(
    label: impl text::IntoFragment<'static>,
    selected: bool,
) -> Button<'static, Message, AppTheme> {
    Button::new(text(label).size(theme::TEXT_SIZE_SM))
        .padding([theme::SPACING_XS, theme::SPACING_SM])
        .style(button_style_scope(selected))
}

/// Build a segmented row of scope/provider chips. Each item is
/// `(label, selected, on_press)`; `pad_x`/`pad_y` set the button padding.
pub fn scope_tab_row<I, S>(items: I) -> Element<'static, Message, AppTheme>
where
    I: IntoIterator<Item = (S, bool, Message)>,
    S: text::IntoFragment<'static>,
{
    let tabs = items
        .into_iter()
        .map(|(label, selected, on_press)| scope_button(label, selected).on_press(on_press).into());
    Row::with_children(tabs)
        .spacing(theme::SPACING_XS)
        .wrap()
        .into()
}

pub fn play_pause_button(is_playing: bool) -> Button<'static, Message, AppTheme> {
    Button::new(icons::icon(
        if is_playing {
            icons::PAUSE_ICON
        } else {
            icons::PLAY_ICON
        },
        Color::BLACK,
        theme::ICON_SIZE_LG,
    ))
    .style(button_style_primary())
}

pub fn toggle_bookmark_button(p: &Palette, is_saved: bool) -> Button<'static, Message, AppTheme> {
    Button::new(icons::icon(
        icons::BOOKMARK_ICON,
        if is_saved { Color::BLACK } else { p.fg_muted },
        theme::ICON_SIZE_SM,
    ))
    .padding(theme::SPACING_SM)
    .style(button_style_scope(is_saved))
}

pub fn subtitle_artist(
    name: &str,
    size: u32,
    artist_target: Option<(String, crate::providers::ProviderId)>,
) -> Element<'_, Message, AppTheme> {
    let artist = text(name).size(size);
    if let Some((id, source)) = artist_target {
        Button::new(artist)
            .padding(0)
            .style(super::styles::button_style_album())
            .on_press(Message::OpenArtist {
                id,
                name: name.to_string(),
                source,
            })
            .into()
    } else {
        artist.style(fg_secondary()).into()
    }
}

pub fn disabled_text_input_row<'a>(label: &'a str, value: &str) -> Element<'a, Message, AppTheme> {
    Column::with_children([
        Container::new(text(label))
            .padding([0.0, theme::SPACING_XS])
            .into(),
        text_input("", value)
            .padding([theme::SPACING_SM, theme::SPACING_MD])
            .into(),
    ])
    .spacing(theme::SPACING_XS)
    .into()
}

pub fn text_input_row<'a>(
    label: &'a str,
    value: &str,
    placeholder: &'a str,
    on_input: fn(String) -> Message,
) -> Element<'a, Message, AppTheme> {
    Column::with_children([
        Container::new(text(label))
            .padding([0.0, theme::SPACING_XS])
            .into(),
        text_input(placeholder, value)
            .on_input(on_input)
            .padding([theme::SPACING_SM, theme::SPACING_MD])
            .into(),
    ])
    .spacing(theme::SPACING_XS)
    .into()
}
