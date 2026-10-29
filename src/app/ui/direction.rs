//! Right-to-left layout support.
//!
//! iced lays every [`Row`] out left to right and paragraphs out with whatever
//! alignment they are given, so a right-to-left locale has to mirror the widget
//! tree itself: a horizontal group hands its children over in reading order
//! (its first child ends up on the right), a trailing element is pushed to the
//! left, and filling paragraphs align right. Every helper leaves what a
//! left-to-right locale renders exactly as it was.
//!
//! Two kinds of groups keep their logical order on purpose, because they are
//! not layout but content: horizontally scrolled rows (card shelves, chip rows)
//! and absolutely positioned overlays. iced's scrollable origin is fixed at the
//! left, so reversing a shelf would make it open on its *last* card; overlay
//! offsets are measured from the left edge of the window for the same reason.
//! A scrollable's scrollbar stays on the physical right too, so the padding that
//! keeps content clear of it does not mirror.

use iced::{
    alignment,
    widget::{self, Row},
    Element, Length,
};

use super::{AppTheme, Message};
use crate::i18n::Direction;

impl Direction {
    /// A horizontal group over `children` in reading order: reversed for a
    /// right-to-left locale, so an icon that leads an icon+label pair ends up on
    /// the right.
    pub(crate) fn row<'a>(
        self,
        children: impl IntoIterator<Item = Element<'a, Message, AppTheme>>,
    ) -> Row<'a, Message, AppTheme> {
        Row::with_children(self.ordered(children))
    }

    /// `children` in reading order.
    fn ordered<'a>(
        self,
        children: impl IntoIterator<Item = Element<'a, Message, AppTheme>>,
    ) -> Vec<Element<'a, Message, AppTheme>> {
        let mut children: Vec<_> = children.into_iter().collect();
        if self.is_rtl() {
            children.reverse();
        }
        children
    }

    /// Alignment for a paragraph that fills its container.
    pub(crate) fn align(self) -> alignment::Horizontal {
        match self {
            Direction::Ltr => alignment::Horizontal::Left,
            Direction::Rtl => alignment::Horizontal::Right,
        }
    }

    /// Alignment for content that hugs the end of a row: its right edge
    /// normally, its left edge in a right-to-left locale.
    pub(crate) fn end_align(self) -> alignment::Horizontal {
        match self {
            Direction::Ltr => alignment::Horizontal::Right,
            Direction::Rtl => alignment::Horizontal::Left,
        }
    }

    /// Pushes `element` to the start of a row: the left edge normally, the right
    /// edge in a right-to-left locale.
    pub(crate) fn leading<'a>(
        self,
        element: impl Into<Element<'a, Message, AppTheme>>,
    ) -> Element<'a, Message, AppTheme> {
        widget::container(element)
            .width(Length::Fill)
            .align_x(self.align())
            .into()
    }

    /// Pushes `element` to the far end of a row: the right edge normally, the
    /// left edge in a right-to-left locale.
    pub(crate) fn trailing<'a>(
        self,
        element: impl Into<Element<'a, Message, AppTheme>>,
    ) -> Element<'a, Message, AppTheme> {
        widget::container(element)
            .width(Length::Fill)
            .align_x(self.end_align())
            .into()
    }

    /// `ltr` normally, `rtl` in a right-to-left locale: for glyphs that point
    /// along the reading direction (back/forward, skip), whose meaning is
    /// decided by the side they sit on rather than by the shape of the arrow.
    pub(crate) fn mirror(self, ltr: &'static [u8], rtl: &'static [u8]) -> &'static [u8] {
        match self {
            Direction::Ltr => ltr,
            Direction::Rtl => rtl,
        }
    }
}
