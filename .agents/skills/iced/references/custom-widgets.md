# Custom Widgets, Operations & Overlays

Enable the `advanced` feature to access `iced::advanced::{Widget, layout, renderer, mouse, Shell, ...}`.
A widget is anything implementing `Widget<Message, Theme, Renderer>`. The `custom_widget` example
draws a circle — read it alongside this.

## The `Widget` trait (what you must implement)

```rust
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Widget};
use iced::border;
use iced::mouse;
use iced::{Color, Element, Length, Rectangle, Size};

pub struct Circle { radius: f32 }

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Circle
where
    Renderer: renderer::Renderer,
{
    // 1. Intrinsic size in Lengths.
    fn size(&self) -> Size<Length> {
        Size { width: Length::Shrink, height: Length::Shrink }
    }

    // 2. Layout: produce a layout::Node within `limits`.
    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(Size::new(self.radius * 2.0, self.radius * 2.0))
    }

    // 3. Draw using the renderer.
    fn draw(
        &self,
        _tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        renderer.fill_quad(
            renderer::Quad {
                bounds: layout.bounds(),
                border: border::rounded(self.radius),
                ..renderer::Quad::default()
            },
            Color::BLACK,
        );
    }
}

// 4. Bridge into the Element type so `Into<Element>` works.
impl<Message, Theme, Renderer> From<Circle> for Element<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn from(circle: Circle) -> Self { Self::new(circle) }
}
```

### Optional trait methods (only implement what you need)
- `state()` / `tag()` — for **stateful** widgets that need internal mutable state. Return
  `tree::State::Some(Box::new(MyState::default()))` and a matching `tree::Tag::of::<MyState>()`.
  `diff()` then reconciles old/new state across `view` rebuilds.
- `update(&mut self, tree, event, layout, cursor, renderer, shell, viewport)` — handle
  `Event`s (mouse/keyboard), produce messages via `shell.publish(msg)`, request redraw with
  `shell.request_redraw()`.
- `mouse_interaction(...)` — return `mouse::Interaction` (Pointer/Text/Grab etc.).
- `operate(...)` — participate in `Operation`s (focus, scroll, custom queries).
- `overlay(...)` — return a floating layer (menus, tooltips, popups).

## Renderers & drawing

- `Renderer: renderer::Renderer` is the bound for generic widgets. Concrete renderers are
  `iced_wgpu` (Vulkan/Metal/DX12) and `iced_tiny_skia` (software fallback).
- Low-level primitives: `renderer.fill_quad(quad, color)`, `renderer.fill_text(...)`,
  `renderer.fill_geometry(...)` (via `iced::advanced::graphics::geometry` / `Mesh2D`).
- For **2D vector drawing** prefer the `Canvas` widget (`iced::widget::canvas`) with a
  `Program<Message>` implementing `draw` + `update` — you get caching, hit-testing, and events
  for free. See `examples/bezier_tool`, `examples/solar_system`, `examples/game_of_life`.
- For **GPU shaders** use `Shader` (`iced::widget::shader`) + a `wgpu` program
  (`examples/custom_shader`, `examples/geometry`).

## Operations — query/update widget state from outside

`iced::advanced::widget::Operation<T>` traverses the tree. Built-ins: `operation::focus(id)`,
`operation::focus_next()`, `operation::scroll_to(...)`, `operation::text_input::select_all(id)`.

```rust
// custom operation to read a widget's value by Id
struct ReadValue(Option<Id>, Option<String>);
impl Operation<String> for ReadValue {
    fn container(&mut self, id: Option<&Id>, _b: Rectangle) { self.0 = id.cloned(); }
    fn text(&mut self, _id: Option<&Id>, _b: Rectangle, text: &str) { self.1 = Some(text.into()); }
    fn finish(&self) -> Outcome<String> { Outcome::Some(self.1.clone().unwrap_or_default()) }
}
// run it: widget::operate(&mut tree, layout, renderer, &mut op)
```

Sub-components expose `operation` tasks so parents can drive them (focus an input after opening a
modal, select-all on edit, etc. — exactly what `examples/todos` and `examples/modal` do).

## Overlays

Return `Some(overlay::Element)` from `Widget::overlay` for popups that must render above siblings
(menus, combobox dropdowns, tooltips). The `overlay` module (`iced::overlay`) mirrors `widget`
for overlay-specific elements.

## Theming a custom widget: the `Catalog` pattern

For production-grade, themeable widgets (as used by Halloy's ~20 custom widgets), define a
`Catalog` trait so your widget is styled like a built-in and reads from the app `Theme`:

```rust
// widget/selectable_text.rs
pub trait Catalog: Sized {
    type Class<'a>;
    fn default<'a>() -> Self::Class<'a>;
    fn style(&self, item: &Self::Class<'_>) -> Style;
}

pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme) -> Style + 'a>;

impl Catalog for iced::Theme {
    type Class<'a> = StyleFn<'a, Self>;
    fn default<'a>() -> Self::Class<'a> { Box::new(|_| Style::default()) }
    fn style(&self, class: &Self::Class<'_>) -> Style { class(self) }
}
```

The widget stores `class: Theme::Class<'a>` and exposes `.style(impl Fn(&Theme) -> Style)`.
Then a dedicated style module implements `Catalog` for your **app** `Theme` and exposes semantic
helpers (`default`, `secondary`, `danger`, ...). **Never hardcode colors** — read from a shared
palette/`Styles` (Halloy's `theme.styles().*`). See `references/real-world.md` §5–6 for the full
Halloy layout (`appearance/theme/<widget>.rs` per widget).

## Checklist for a new widget

1. Define the struct + a `fn new` / helper `fn circle(...)`.
2. Implement `size`, `layout`, `draw`. Add `state`/`tag`/`diff` only if stateful.
3. Implement `update` + `mouse_interaction` if interactive; `operate` if it should be focusable/scrollable.
4. Implement `From<W> for Element` (or wrap with `Element::new`).
5. For a reusable widget, define a `Catalog` trait + `Style`/`StyleFn` and a matching
   `appearance/theme/<widget>.rs` style module so it themes like a built-in.
   (For built-ins, follow `iced::widget::container` as the reference implementation.)
