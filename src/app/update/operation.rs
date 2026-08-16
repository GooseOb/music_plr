//! A custom widget `Operation` that captures scrollable geometry during the
//! layout/operate pass — without relying on `on_scroll` events.
//!
//! `on_scroll` only fires on actual scroll interaction, so it never reports
//! geometry on first layout or when content fits (no scrollbar). The
//! `Operation::scrollable` callback, by contrast, is visited for every
//! scrollable on every operate pass and hands us the viewport `bounds`, the
//! content `bounds` (total height, for max-scroll math), and the current
//! `translation` — even when the list doesn't overflow.
//!
//! For drop-target hit-testing we also need each *row's* measured geometry,
//! not a hard-coded row height. `Operation::container` is visited for every
//! `Container` tagged with an `Id`; we set `current` to the list whose
//! scrollable is being visited and record each row `Container`'s absolute
//! `bounds` into that list's `ListGeometry.rows`. Because a scrollable's rows
//! are visited contiguously right after its own `scrollable` callback, the
//! `current` flag is correctly scoped — and the only id'd `Container`s in the
//! tree are list rows (track-list, playlist, and library rows), so nothing
//! else is captured.

use crate::app::ui::{QUEUE_LIST_ID, TRACK_LIST_ID};
use crate::app::Message;
use iced::widget::Id;
use iced::Rectangle;
use iced_core::widget::operation::{Operation, Outcome, Scrollable};

#[derive(Debug, Clone)]
pub struct ListGeometry {
    pub bounds: Rectangle,
    pub translation_y: f32,
    /// Total content height (from `scrollable`'s `content_bounds`). Replaces
    /// `count * ROW_HEIGHT` for autoscroll max-scroll clamping.
    pub content_height: f32,
    /// Absolute `bounds` of every row `Container`, in DOM order. Replaces the
    /// hard-coded row height in drop-index math.
    pub rows: Vec<Rectangle>,
}

pub const SIDEBAR_LIST_ID: Id = Id::new("sidebar_playlist_list");
pub const LIBRARY_LIST_ID: Id = Id::new("sidebar_library_list");

/// Whether `id` is one of the scrollable lists whose row geometry we capture.
fn is_tracked_list(id: &Id) -> bool {
    *id == SIDEBAR_LIST_ID || *id == LIBRARY_LIST_ID || *id == QUEUE_LIST_ID || *id == TRACK_LIST_ID
}

#[derive(Default, Clone, Debug)]
pub struct CaptureBounds {
    pub sidebar: Option<ListGeometry>,
    pub library: Option<ListGeometry>,
    pub queue: Option<ListGeometry>,
    pub track: Option<ListGeometry>,
    current: Option<Id>,
}

impl CaptureBounds {
    pub fn new() -> Self {
        Self::default()
    }

    fn geo_mut(&mut self, id: &Id) -> &mut Option<ListGeometry> {
        if *id == SIDEBAR_LIST_ID {
            &mut self.sidebar
        } else if *id == LIBRARY_LIST_ID {
            &mut self.library
        } else if *id == QUEUE_LIST_ID {
            &mut self.queue
        } else {
            // Callers only pass tracked ids (`current` is always one, and
            // `scrollable` bails on untracked ids before reaching here).
            &mut self.track
        }
    }

    /// The list (by `Id`) whose bounds contain `point`, if any. Iteration order
    /// matches `scrollable` priority; lists don't overlap so the first hit wins.
    pub fn get_containing(&self, point: iced::Point) -> Option<(Id, &ListGeometry)> {
        for (id, geo) in [
            (QUEUE_LIST_ID.clone(), &self.queue),
            (TRACK_LIST_ID.clone(), &self.track),
            (SIDEBAR_LIST_ID.clone(), &self.sidebar),
            (LIBRARY_LIST_ID.clone(), &self.library),
        ] {
            if let Some(g) = geo {
                if g.bounds.contains(point) {
                    return Some((id, g));
                }
            }
        }
        None
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
        content_bounds: Rectangle,
        translation: iced::Vector,
        _state: &mut dyn Scrollable,
    ) {
        let Some(id) = id else {
            self.current = None;
            return;
        };
        if !is_tracked_list(id) {
            self.current = None;
            return;
        }
        self.current = Some(id.clone());
        *self.geo_mut(id) = Some(ListGeometry {
            bounds,
            translation_y: translation.y,
            content_height: content_bounds.height,
            rows: Vec::new(),
        });
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id.is_none() {
            return;
        }
        let Some(target) = self.current.clone() else {
            return;
        };
        if let Some(g) = self.geo_mut(&target) {
            g.rows.push(bounds);
        }
    }

    fn finish(&self) -> Outcome<Message> {
        Outcome::Some(Message::ListBoundsCaptured(self.clone()))
    }
}
