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
//! tree are list rows (sidebar playlist, library, and search-history rows), so
//! nothing else is captured. The track/queue/recent lists are virtualized and
//! uniform-height, so they use `ROW_HEIGHT` for drop math instead and don't
//! need `rows`.
//! The global search `text_input` (id `SEARCH_INPUT_ID`) is captured via the
//! `text_input` callback so its bounds can hit-test the search-history
//! dropdown.
//!
//! The search-history dropdown rows are captured by a *separate* operation,
//! `CaptureSearchHistoryRows`, so they can be refreshed on every search-input
//! message without re-walking the whole widget tree (sidebar/library/queue/
//! track/recent + search input). See that struct for details.

use iced::{widget::Id, Rectangle, Task};
use iced_core::widget::operation::{Operation, Outcome, Scrollable};

use crate::{
    app::{
        ui::{
            QUEUE_LIST_ID, QUEUE_RECENT_LIST_ID, SEARCH_HISTORY_LIST_ID, SEARCH_INPUT_ID,
            TRACK_LIST_ID,
        },
        Message,
    },
    theme,
};

#[derive(Debug, Clone)]
pub struct ListGeometry {
    pub bounds: Rectangle,
    pub translation_y: f32,
    pub rows: Vec<Rectangle>,
}

pub const SIDEBAR_LIST_ID: Id = Id::new("sidebar_playlist_list");
pub const LIBRARY_LIST_ID: Id = Id::new("sidebar_library_list");

/// Whether `id` is one of the scrollable lists whose row geometry we capture.
fn is_tracked_list(id: &Id) -> bool {
    *id == SIDEBAR_LIST_ID
        || *id == LIBRARY_LIST_ID
        || *id == QUEUE_LIST_ID
        || *id == QUEUE_RECENT_LIST_ID
        || *id == TRACK_LIST_ID
}

#[derive(Default, Clone, Debug)]
pub struct CaptureBounds {
    pub sidebar: Option<ListGeometry>,
    pub library: Option<ListGeometry>,
    pub queue: Option<ListGeometry>,
    pub track: Option<ListGeometry>,
    pub recent: Option<ListGeometry>,
    /// Captured separately by [`CaptureSearchHistoryRows`] (see that struct).
    pub search_history: Option<ListGeometry>,
    pub search_input: Option<Rectangle>,
    /// Captured by [`CaptureContextMenu`] when the context menu opens.
    pub context_menu: Option<ContextMenuGeometry>,
    current: Option<Id>,
}

impl From<CaptureBounds> for Task<Message> {
    fn from(bounds: CaptureBounds) -> Self {
        iced_runtime::task::widget(bounds)
    }
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
        } else if *id == QUEUE_RECENT_LIST_ID {
            &mut self.recent
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
            (QUEUE_RECENT_LIST_ID.clone(), &self.recent),
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
        _content_bounds: Rectangle,
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
            rows: Vec::new(),
        });
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        let Some(target) = self.current.clone() else {
            return;
        };
        if target == QUEUE_LIST_ID || target == TRACK_LIST_ID || target == QUEUE_RECENT_LIST_ID {
            return;
        }
        if id.is_some() {
            if let Some(g) = self.geo_mut(&target) {
                g.rows.push(bounds);
            }
        }
    }

    fn text_input(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _state: &mut dyn iced_core::widget::operation::TextInput,
    ) {
        if id == Some(&SEARCH_INPUT_ID) {
            self.search_input = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<Message> {
        Outcome::Some(Message::ListBoundsCaptured(Box::new(self.clone())))
    }
}

#[derive(Default, Clone, Debug)]
pub struct CaptureSearchHistoryRows {
    geo: Option<ListGeometry>,
    current: bool,
}

impl From<CaptureSearchHistoryRows> for Task<Message> {
    fn from(capture: CaptureSearchHistoryRows) -> Self {
        iced_runtime::task::widget(capture)
    }
}

impl CaptureSearchHistoryRows {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Operation<Message> for CaptureSearchHistoryRows {
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
        if id == Some(&SEARCH_HISTORY_LIST_ID) {
            self.current = true;
            self.geo = Some(ListGeometry {
                bounds,
                translation_y: translation.y,
                rows: Vec::new(),
            });
        } else {
            self.current = false;
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if self.current && id.is_some() {
            if let Some(g) = &mut self.geo {
                g.rows.push(bounds);
            }
        }
    }

    fn finish(&self) -> Outcome<Message> {
        match self.geo.clone() {
            Some(mut geo) => {
                if !geo.rows.is_empty() {
                    let last = theme::SEARCH_DROPDOWN_MAX_ITEMS.min(geo.rows.len()) - 1;
                    geo.bounds.height = geo.rows[last].y + geo.rows[last].height - geo.bounds.y;
                }
                Outcome::Some(Message::SearchHistoryBoundsCaptured(geo))
            }
            None => Outcome::None,
        }
    }
}

/// Measured context-menu layout, derived from captured bounds. The view and
/// the position-flip logic both read this instead of raw rects.
#[derive(Debug, Clone)]
pub struct ContextMenuGeometry {
    pub panel: Rectangle,
    /// Main-menu row tops relative to the panel top, in display order.
    pub row_offsets: Vec<f32>,
    /// True once two consecutive captures agree, i.e. the measured width was
    /// not clipped by the window edge; rows only stretch to full width then.
    pub stable: bool,
}

/// Captures the context-menu panel and each main-menu row's bounds. Run once
/// when the menu opens; the geometry drives submenu alignment and the
/// flip-inside-window placement done in the handler.
#[derive(Default, Clone, Debug)]
pub struct CaptureContextMenu {
    panel: Option<Rectangle>,
    rows: Vec<Rectangle>,
}

impl From<CaptureContextMenu> for Task<Message> {
    fn from(capture: CaptureContextMenu) -> Self {
        iced_runtime::task::widget(capture)
    }
}

impl Operation<Message> for CaptureContextMenu {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Message>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        use crate::app::ui::{CONTEXT_MENU_PANEL_ID, CONTEXT_MENU_ROW_ID};
        match id {
            Some(id) if *id == CONTEXT_MENU_ROW_ID => self.rows.push(bounds),
            Some(id) if *id == CONTEXT_MENU_PANEL_ID => self.panel = Some(bounds),
            _ => {}
        }
    }

    fn finish(&self) -> Outcome<Message> {
        let Some(panel) = self.panel else {
            return Outcome::None;
        };
        Outcome::Some(Message::ContextMenuBoundsCaptured {
            panel,
            row_offsets: self.rows.iter().map(|r| r.y - panel.y).collect(),
        })
    }
}
