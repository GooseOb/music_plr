# Styling & Theming

Iced has **no single styling system**; every widget exposes a `style` method taking a closure:
`|theme: &Theme, status| -> Style`. Built-in widgets in the same `Theme` all read from a shared
`Palette`, which keeps a coherent look.

## Themes

- `iced::Theme` has many built-ins: `Dark`, `Light`, `TokyoNight`, `Catppuccin`, `Solarized`,
  `Gruvbox`, etc. Iterate with `Theme::ALL`.
- Switch theme via the `Application` builder or a `theme(&self) -> Theme` function:

```rust
pub fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .theme(App::theme)   // fn(&App) -> Theme  (can be dynamic!)
        .run()
}
```

- A theme function can read state, so theming can be user-driven (`examples/styling` lets the
  user pick a `Theme` from a `pick_list` and stores `Option<Theme>`).
- Custom themes: `Theme::custom(name, palette)` from a `theme::Palette` (background/primary/
  success/warning/danger text+strong+weak colors). Define a full custom set with
  `theme::Custom::new(palette, pair_fn)` for per-widget styles.

## Palette

`theme.palette()` returns a `theme::Palette` with semantic colors:
`background.{base,weak,strong}` (each with `.color` and `.text`), `primary`, `secondary`,
`success`, `warning`, `danger`, `text`, `is_dark`. Always derive colors from the palette rather
than hardcoding hex — that's what keeps iced apps theme-coherent.

## Styling widgets

Closures receive `(&Theme, Status)` where `Status` is the widget's interaction state
(`Active`, `Hovered`, `Pressed`, `Disabled`, ...). Use the provided style helpers in each module
(`button::primary`, `button::danger`, `button::text`, `container::rounded_box`,
`container::bordered_box`, `text::danger`, ...) — they already key off the palette.

```rust
use iced::widget::button;
use iced::{Element, Theme};

fn view(state: &State) -> Element<'_, Message> {
    button("Go")
        .on_press(Message::Go)
        .padding(10)
        .style(|theme: &Theme, status| {
            let p = theme.palette();
            match status {
                button::Status::Active => button::Style::default()
                    .with_background(p.success.strong.color),
                _ => button::primary(theme, status),
            }
        })
        .into()
}
```

### Container / custom element styling

Containers and many widgets have a plain `style(|theme| -> Style)` (no `Status`):

```rust
fn pane_active(theme: &Theme) -> container::Style {
    let p = theme.palette();
    container::Style {
        background: Some(p.background.weak.color.into()),
        border: iced::Border { width: 2.0, color: p.background.strong.color, ..Default::default() },
        ..Default::default()
    }
}
// usage: container(...).style(pane_active)
```

### Inline styles (one-off)

Passing a closure directly also works for `container` backdrops (e.g., modal dimming):

```rust
mouse_area(center(opaque(content)))
    .style(|_| container::Style {
        background: Some(Color { a: 0.8, ..Color::BLACK }.into()),
        ..container::Style::default()
    })
    .on_press(on_blur)
```

## Fonts & text

- Load custom fonts with `.font(include_bytes!("x.ttf"))` on the `Application` builder, then
  reference by family name (`text(unicode).font("Iced-Icons")`). The `todos` example embeds an
  icon font this way.
- `text::Shaping::Advanced` enables ligatures/emoji/`\u{...}` glyphs; `Basic` is faster.
- `text(value)` accepts anything `Display` (`text(self.value)`, `text!("Radius: {r:.2}", ...)`).

## Best practices

1. **Never hardcode colors** — pull from `theme.palette()`. Your widget then works in every theme.
2. **Reuse the `*::primary/secondary/danger/...` helpers** before writing a custom closure.
3. **Put shared style fns in a `style` module** (see `examples/pane_grid`'s `mod style`) so they're
   reusable and testable.
4. Let the **theme function be dynamic** — base it on app state so users can switch at runtime.
