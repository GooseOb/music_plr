//! A custom widget `Operation` that captures scrollable geometry during the
//! layout/operate pass — without relying on `on_scroll` events.
//!
//! `on_scroll` only fires on actual scroll interaction, so it never reports
//! geometry on first layout or when content fits (no scrollbar). The
//! `Operation::scrollable` callback, by contrast, is visited for every
//! scrollable on every operate pass and hands us the viewport `bounds` and the
//! current `translation` — even when the list doesn't overflow. We run this op
//! from `update` (on cursor movement and drag) and emit a single
//! `Message::ListBoundsCaptured` carrying the geometry of every list we care
//! about, replacing the old `on_scroll` plumbing.

use crate::app::ui::{QUEUE_LIST_ID, TRACK_LIST_ID};
use crate::app::Message;
use iced::widget::Id;
use iced::Rectangle;
use iced_core::widget::operation::{Operation, Outcome, Scrollable};

/// Viewport + scroll state for one list, captured by [`CaptureBounds`].
#[derive(Debug, Clone, Copy)]
pub struct ListGeometry {
    /// Visible viewport rectangle (absolute window coordinates).
    pub bounds: Rectangle,
    /// Current scroll translation (`.y` is the vertical offset).
    pub translation_y: f32,
}

impl ListGeometry {
    /// The vertical scroll offset, matching what `on_scroll`'s
    /// `absolute_offset().y` reported.
    pub fn scroll_offset(&self) -> f32 {
        self.translation_y
    }
}

/// Sidebar playlist list id, defined here to keep all captured ids together.
pub const SIDEBAR_LIST_ID: Id = Id::new("sidebar_playlist_list");

/// Captures `ListGeometry` for each known scrollable by id, then emits a
/// `Message::ListBoundsCaptured` carrying the results on `finish`.
pub struct CaptureBounds {
    sidebar: Option<ListGeometry>,
    queue: Option<ListGeometry>,
    track: Option<ListGeometry>,
}

impl CaptureBounds {
    pub fn new() -> Self {
        Self {
            sidebar: None,
            queue: None,
            track: None,
        }
    }

    fn record(&mut self, id: Option<&Id>, bounds: Rectangle, translation_y: f32) {
        let Some(id) = id else {
            return;
        };
        let geo = ListGeometry {
            bounds,
            translation_y,
        };
        if id == &SIDEBAR_LIST_ID {
            self.sidebar = Some(geo);
        } else if id == &QUEUE_LIST_ID {
            self.queue = Some(geo);
        } else if id == &TRACK_LIST_ID {
            self.track = Some(geo);
        }
    }
}

impl Operation<Message> for CaptureBounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Message>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _content_bounds: Rectangle,
        translation: iced::Vector,
        _state: &mut dyn Scrollable,
    ) {
        self.record(id, bounds, translation.y);
    }

    fn finish(&self) -> Outcome<Message> {
        Outcome::Some(Message::ListBoundsCaptured {
            sidebar: self.sidebar,
            queue: self.queue,
            track: self.track,
        })
    }
}
