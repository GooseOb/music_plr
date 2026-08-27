//! Self-animating arc spinner.
//!
//! The spinner is themed through a dedicated [`Catalog`] so its color comes
//! from the [`AppTheme`] palette by default (`palette.fg_secondary`) instead
//! of being threaded through every caller. Build with [`spinner`] for the
//! default style; the arc color is resolved from [`Style`] at draw time, so a
//! different default can be supplied via the [`Catalog`] implementation.

use std::{marker::PhantomData, time::Duration};

use iced::{
    advanced, advanced::graphics::geometry::Renderer as _, widget::canvas, window, Color, Element,
    Length, Size, Vector,
};
use iced_core::Renderer as _;

use crate::{
    app::Message,
    theme::{self, AppTheme},
};

/// Visual style of the [`Spinner`] — currently just its arc color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub color: Color,
}

/// Styling function: maps a theme to a [`Spinner`] [`Style`].
pub type StyleFn<'a> = Box<dyn Fn(&AppTheme) -> Style + 'a>;

/// Theming interface for the [`Spinner`] widget, parameterized over a theme.
pub trait Catalog {
    type Class<'a>;

    fn default<'a>() -> Self::Class<'a>;

    fn style(theme: &Self, class: &Self::Class<'_>) -> Style;
}

impl Catalog for AppTheme {
    type Class<'a> = StyleFn<'a>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(|theme| Style {
            color: theme.palette.fg_secondary,
        })
    }

    fn style(theme: &Self, class: &Self::Class<'_>) -> Style {
        class(theme)
    }
}

/// Self-animating arc spinner. Rotation is derived from the wall clock at
/// draw time; `update` keeps the redraw loop alive while mounted.
pub struct Spinner<'a, Message> {
    size: f32,
    class: <AppTheme as Catalog>::Class<'a>,
    _message: PhantomData<Message>,
}

impl<Message> Spinner<'_, Message> {
    pub fn new(size: f32) -> Self {
        Self {
            size,
            class: AppTheme::default(),
            _message: PhantomData,
        }
    }
}

impl<Message> advanced::Widget<Message, AppTheme, iced::Renderer> for Spinner<'_, Message> {
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
        theme: &AppTheme,
        _style: &advanced::renderer::Style,
        layout: advanced::Layout<'_>,
        _cursor: iced::mouse::Cursor,
        _viewport: &iced::Rectangle,
    ) {
        let bounds = layout.bounds();
        if bounds.width < 1.0 || bounds.height < 1.0 {
            return;
        }
        let color = AppTheme::style(theme, &self.class).color;

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
                    .with_color(color)
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

/// Build a spinner using the theme's default [`Catalog`] style.
pub fn spinner(size: f32) -> Element<'static, Message, AppTheme> {
    Element::new(Spinner::new(size))
}
