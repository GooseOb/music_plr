use std::time::{Duration, Instant};

use iced::{
    advanced::{
        graphics::geometry::Renderer as _,
        layout, mouse, renderer,
        widget::{tree, Tree},
        Clipboard, Layout, Shell, Widget,
    },
    widget::canvas,
    Element, Event, Length, Rectangle, Size, Vector,
};
use iced_core::Renderer as _;

use super::Message;
use crate::theme::AppTheme;

/// Redraw cadence while playing; draw-only, no view rebuilds.
const FRAME_MS: u64 = 100;
const RAIL_HEIGHT: f32 = 4.0;
const HANDLE_RADIUS: f32 = 7.0;
const BAR_HEIGHT: f32 = 20.0;
/// Minimum fraction change that emits a seek while dragging.
const SEEK_QUANTUM: f32 = 0.005;

/// Playback progress bar with the same look as a themed slider (accent fill
/// on `bg_secondary`, round handle) but animated without view rebuilds: the
/// position derives from the anchor plus the wall clock at draw time (like
/// [`super::spinner`]), and a redraw loop runs only while playing. Seeks
/// publish [`Message::Seek`] on press and drag.
pub struct ProgressBar {
    progress: f32,
    progress_at: Instant,
    duration: f32,
    playing: bool,
}

#[derive(Default)]
struct State {
    dragging: bool,
    last_frac: f32,
}

impl ProgressBar {
    pub fn new(progress: f32, progress_at: Instant, duration: f32, playing: bool) -> Self {
        Self {
            progress,
            progress_at,
            duration,
            playing,
        }
    }

    fn displayed(&self) -> f32 {
        if self.playing && self.duration > 0.0 {
            (self.progress + self.progress_at.elapsed().as_secs_f32() / self.duration)
                .clamp(0.0, 1.0)
        } else {
            self.progress.clamp(0.0, 1.0)
        }
    }

    fn frac_at(point_x: f32, bounds: Rectangle) -> f32 {
        let travel = (bounds.width - 2.0 * HANDLE_RADIUS).max(1.0);
        ((point_x - bounds.x - HANDLE_RADIUS) / travel).clamp(0.0, 1.0)
    }

    fn thumb_x(frac: f32, width: f32) -> f32 {
        HANDLE_RADIUS + frac * (width - 2.0 * HANDLE_RADIUS).max(0.0)
    }

    fn seek_to(state: &mut State, x: f32, bounds: Rectangle, shell: &mut Shell<'_, Message>) {
        let frac = Self::frac_at(x, bounds);
        if (frac - state.last_frac).abs() >= SEEK_QUANTUM {
            state.last_frac = frac;
            shell.publish(Message::Seek(frac));
        }
    }
}

impl Widget<Message, AppTheme, iced::Renderer> for ProgressBar {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(BAR_HEIGHT))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fill, Length::Fixed(BAR_HEIGHT), Size::ZERO))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if let Event::Window(iced::window::Event::RedrawRequested(now)) = event {
            if self.playing && self.duration > 0.0 {
                shell.request_redraw_at(*now + Duration::from_millis(FRAME_MS));
            }
            return;
        }
        let state: &mut State = tree.state.downcast_mut();
        let bounds = layout.bounds();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(iced::touch::Event::FingerPressed { .. }) => {
                let Some(point) = cursor.position_over(bounds) else {
                    return;
                };
                state.dragging = true;
                state.last_frac = self.displayed();
                Self::seek_to(state, point.x, bounds, shell);
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(iced::touch::Event::FingerLifted { .. }) => {
                state.dragging = false;
            }
            Event::Mouse(mouse::Event::CursorMoved { position })
            | Event::Touch(iced::touch::Event::FingerMoved { position, .. })
                if state.dragging =>
            {
                Self::seek_to(state, position.x, bounds, shell);
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if tree.state.downcast_ref::<State>().dragging {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &AppTheme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if bounds.width < 1.0 || bounds.height < 1.0 {
            return;
        }
        let p = &theme.palette;
        let frac = self.displayed();
        let cy = bounds.height / 2.0;
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let mut rail = |x: f32, w: f32, color: iced::Color| {
            frame.fill(
                &canvas::Path::rectangle(
                    iced::Point::new(x, cy - RAIL_HEIGHT / 2.0),
                    Size::new(w, RAIL_HEIGHT),
                ),
                color,
            );
            for cx in [x, x + w] {
                frame.fill(
                    &canvas::Path::circle(iced::Point::new(cx, cy), RAIL_HEIGHT / 2.0),
                    color,
                );
            }
        };
        rail(0.0, bounds.width, p.bg_secondary);
        if frac > 0.0 {
            let w = Self::thumb_x(frac, bounds.width);
            rail(0.0, w, p.accent);
            frame.fill(
                &canvas::Path::circle(iced::Point::new(w, cy), HANDLE_RADIUS),
                p.accent,
            );
        }
        let geometry = frame.into_geometry();
        renderer.with_translation(Vector::new(bounds.x, bounds.y), |renderer| {
            renderer.draw_geometry(geometry);
        });
    }
}

impl From<ProgressBar> for Element<'_, Message, AppTheme> {
    fn from(bar: ProgressBar) -> Self {
        Element::new(bar)
    }
}
