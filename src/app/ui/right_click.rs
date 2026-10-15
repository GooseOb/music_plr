use iced::{
    advanced::{
        layout, mouse, overlay, renderer,
        widget::{tree, Operation, Tree},
        Clipboard, Layout, Shell, Widget,
    },
    Element, Event, Length, Rectangle, Size, Vector,
};

use super::Message;
use crate::app::interaction::TrackPos;

/// Reports right-clicks with the cursor position. `MouseArea::on_right_press`
/// carries no position and the global cursor subscription runs only while
/// dragging, so a wrapper that observes the press in place is the only quiet
/// way to anchor the context menu at the cursor.
///
/// Transparent pass-through like `Container`: tag/state/children/diff are the
/// content's own, so widget state and tree shape are unchanged. Right presses
/// are observed before delegating and never captured, so hover, press, focus,
/// and scroll behavior are unchanged.
pub struct RightClickArea<'a> {
    content: Element<'a, Message, crate::theme::AppTheme, iced::Renderer>,
    pos: TrackPos,
}

impl<'a> RightClickArea<'a> {
    pub fn new(
        content: impl Into<Element<'a, Message, crate::theme::AppTheme, iced::Renderer>>,
        pos: TrackPos,
    ) -> Self {
        Self {
            content: content.into(),
            pos,
        }
    }
}

impl Widget<Message, crate::theme::AppTheme, iced::Renderer> for RightClickArea<'_> {
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
        ) {
            if let Some(point) = cursor.position_over(layout.bounds()) {
                shell.publish(Message::CursorMoved(point));
                shell.publish(Message::TrackRightClicked(self.pos));
            }
        }

        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &crate::theme::AppTheme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, crate::theme::AppTheme, iced::Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

impl<'a> From<RightClickArea<'a>> for Element<'a, Message, crate::theme::AppTheme, iced::Renderer> {
    fn from(area: RightClickArea<'a>) -> Self {
        Element::new(area)
    }
}
