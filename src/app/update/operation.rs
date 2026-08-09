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

#[derive(Debug, Clone, Copy)]
pub struct ListGeometry {
    pub bounds: Rectangle,
    pub translation_y: f32,
}

pub const SIDEBAR_LIST_ID: Id = Id::new("sidebar_playlist_list");

#[derive(Default, Clone, Copy, Debug)]
pub struct CaptureBounds {
    pub sidebar: Option<ListGeometry>,
    pub queue: Option<ListGeometry>,
    pub track: Option<ListGeometry>,
}

impl CaptureBounds {
    pub fn new() -> Self {
        Self::default()
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
        Outcome::Some(Message::ListBoundsCaptured(self.clone()))
    }
}
