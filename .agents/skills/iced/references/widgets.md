# Widgets, Layout & Alignment

Iced has **no unified layout or styling engine**. Each widget implements its own `layout()`
and exposes a builder API. You compose them with layout containers.

## Built-in widget inventory

Import from `iced::widget::{ ... }`. Helper macros (`column!`, `row!`, `stack!`, `grid!`,
`center!`, `text!`, `tooltip!`) accept `impl Into<Element>` children.

**Layout / containers**
- `column!` / `Column` — vertical stack. `.spacing(n)`, `.padding(n)`, `.align_x(Center)`, `.width(Fill)`.
- `row!` / `Row` — horizontal stack. `.align_y(Center)`, `.wrap()`, `.spacing(n)`.
- `stack!` / `Stack` — overlapping layers (z-index); used for modals/overlays.
- `grid!` / `Grid` — 2D grid (`row!` of `column!`s or explicit cells). `.column_spacing`, `.row_spacing`.
- `container!` / `Container` — single child; position/align/pad/style. `.padding`, `.center_x(Fill)`, `.style(container::rounded_box)`.
- `center!` / `center_x!` / `center_y!` — convenience centering containers.
- `space!` / `space::horizontal()` / `space::vertical()` — spacer (sized `Fill` or fixed).
- `scrollable!` / `Scrollable` — scroll viewport. `.height(Fill)`, `.width(Fill)`, `.auto_scroll(true)`.
- `responsive!` / `Responsive` — rebuild child on container size change (great for adaptive layouts).
- `mouse_area!` / `MouseArea` — capture mouse events over a child (`on_press`, `on_move`...).
- `opaque!` / `Opaque` — force a subtree to capture events (block pass-through, e.g. modal backdrops).
- `pin!` / `Pin` — keep a widget at a fixed position regardless of scroll (floating UI).
- `sensor!` / `Sensor` — listen to resize/position of a region.
- `themer!` / `Themer` — swap appearance based on active theme.

**Display**
- `text!` / `Text` — `text(value).size(20).align_x(Center).color(c).shaping(text::Shaping::Advanced)`.
- `image!` / `Image`, `svg!` / `Svg`, `qr_code!` / `QRCode`, `rule::horizontal(n)` / `rule::vertical(n)`.

**Inputs**
- `button!` / `Button` — `.on_press(msg)`, `.on_press_maybe(Option<msg>)`, `.padding`, `.style(...)`.
- `text_input!` / `TextInput` — `.on_input(Message::X)`, `.on_submit(msg)`, `.id("...")`, `.secure(true)`, `.password()`.
- `checkbox!` / `Checkbox` — `checkbox(value).label("X").on_toggle(Message::X)`.
- `radio!` / `Radio` — `radio("Label", value, selected, Message::X)`.
- `toggler!` / `Toggler` — `toggler(value).label("X").on_toggle(Message::X)`.
- `slider!` / `Slider`, `vertical_slider!` / `VerticalSlider` — `slider(0.0..=100.0, val, Message::X).step(0.01)`.
- `pick_list!` / `PickList` — `pick_list(selected, options, |s| s.to_string()).on_select(Message::X)`.
- `combo_box!` / `ComboBox` — searchable text + dropdown.
- `progress_bar!` / `ProgressBar` — `progress_bar(0.0..=100.0, value)`.

**Advanced**
- `pane_grid!` / `PaneGrid` — dockable/splittable panes (`pane_grid::State`, `TitleBar`, `Controls`).
- `table!` / `Table` — columnar data table.
- `keyed_column!` / `keyed_column` — stable identity per row so state survives reordering (use with `Uuid`/id keys).
- `lazy!` / `lazy(key, |_| view)` — memoize a subtree by `key`; rebuilds only when `key` changes (see `examples/lazy`).
- `tooltip!` / `Tooltip`, `canvas!` / `Canvas`, `shader!` / `Shader`, `markdown!` / `Markdown`, `text_editor!` / `TextEditor`, `float!` / `Float`.

## Layout essentials

- **Sizing** uses `Length`: `Fill`, `Shrink`, `FillPortion(n)`, `Fit`, `Pixels(n)`.
  Most widgets default to `Shrink` but inherit `Fill` from children. Prefer `width(Fill)` /
  `height(Fill)` on top-level containers so the UI fills the window.
- **Alignment**: `align_x(Center)`, `align_y(Center)` on `Column`/`Row`; `center_x(Fill)`/
  `center_y(Fill)` on `Container`. Re-exported helpers: `Center`, `Left`, `Right`, `Top`, `Bottom`.
- **Spacing/padding**: `.spacing(n)` between children, `.padding(n)` inside a container.
- **Responsive width**: `width(Fit.max(800))` centers content and caps its width (common pattern
  in `examples/todos`).
- **Building manually**: `Column::new().push(child).push(another).spacing(10).into()` is
  equivalent to the macro and is handy in loops/functions returning `Element`.

## Common view patterns

```rust
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Center, Element, Fill, Fit};

fn view(state: &State) -> Element<'_, Message> {
    let content = column![
        text("Title").size(24),
        text_input("Type...", &state.input).on_input(Message::InputChanged),
        row![
            button("Save").on_press(Message::Save).style(button::primary),
            button("Cancel").on_press(Message::Cancel).style(button::text),
        ]
        .spacing(10)
        .align_y(Center),
    ]
    .spacing(20)
    .width(Fit.max(600));

    scrollable(center_x(content).padding(40))
        .height(Fill)
        .into()
}
```

### Modals (stack + opaque + mouse_area)

```rust
use iced::widget::{center, container, mouse_area, opaque, stack};
use iced::{Color, Element};

fn modal<'a, Message>(
    base: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
    on_blur: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    stack![
        base.into(),
        opaque(
            mouse_area(center(opaque(content)))
                .style(|_| container::Style {
                    background: Some(Color { a: 0.8, ..Color::BLACK }.into()),
                    ..container::Style::default()
                })
                .on_press(on_blur)
        )
    ]
    .into()
}
```

### Dynamic child lists

When mapping a collection, each child becomes an `Element<ChildMsg>` then `.map(Message::Child)`:

```rust
keyed_column(tasks.iter().enumerate().map(|(i, task)| {
    (task.id, task.view(i).map(Message::TaskMessage.with(i)))
}))
.spacing(10)
.into()
```

`Message::TaskMessage.with(i)` is sugar for `move |m| Message::TaskMessage(i, m)` — see
`references/tasks-subscriptions.md`. For non-keyed lists prefer `Column::with_children(iter)`.
