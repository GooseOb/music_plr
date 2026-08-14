# Real-World Patterns (from production iced apps)

These patterns are distilled from studying large, well-architected iced projects:
- **[Halloy](https://github.com/squidowl/halloy)** — the official showcase; a full IRC client.
  Huge `src/` split into `screen/`, `widget/`, `appearance/theme/`, `buffer/`, `modal/`.
- **[icebreaker](https://github.com/hecrj/icebreaker)** — AI chat client by iced's author.
  Clean `screen/` + `Action` enum, streaming boot with `Task::sip`.
- **iced_receipts**, **iced_web** — smaller reference apps.

Use these when an app outgrows the single-struct example shape.

## 1. `Screen` enum + per-screen modules (the dominant scaling pattern)

Both Halloy and icebreaker model navigation as a sum of screens, each owning its own
`Message` / `update` / `view`:

```rust
// icebreaker/src/screen.rs
pub enum Screen {
    Loading,
    Search(Search),
    Conversation(Conversation),
    Settings(Settings),
}
```

Each screen module (`screen/conversation.rs`, `screen/settings.rs`) is self-contained:
its own `struct`, `enum Message`, `fn update(&mut self, ...) -> Action`, `fn view(&self, ...)`.
The top-level `update`/`view` match on `Screen` and delegate, lifting child messages with `.map`.

## 2. The `Action` enum returned by screens

icebreaker's screens return a small `Action` instead of `Task<Message>` directly, so the parent
stays in control of transitions and task execution:

```rust
pub enum Action {
    None,
    Run(Task<Message>),
}

pub fn update(&mut self, library: &Library, message: Message) -> Action {
    match message {
        Message::Booted(Ok(assistant)) => { /* swap state, return Action::None */ }
        // ...
    }
}
```

Halloy generalizes this further: screens return richer actions (e.g. `Event` for cross-cutting
effects like opening URLs, quitting, reloads) that the app interprets centrally. This "tell a
story" composition is the canonical way to connect screens type-safely.

## 3. Multi-window via `iced::daemon` + `window::Id` map

Halloy is a **daemon** (`iced::daemon(new, update, view)`), not `application`, because it supports
pop-out panes and multiple windows. It tracks `window::Id`s in collections and renders per-window
in `view(id: window::Id)`. Use `iced::daemon` whenever you need >1 window or no default window.

## 4. Shared type aliases (kill the boilerplate)

Halloy defines app-wide aliases once (in `widget.rs`) and reuses them everywhere:

```rust
pub type Renderer = iced::Renderer;
pub type Element<'a, Message> = iced::Element<'a, Message, Theme, Renderer>;
pub type Column<'a, Message>  = iced::widget::Column<'a, Message, Theme, Renderer>;
pub type Row<'a, Message>     = iced::widget::Row<'a, Message, Theme, Renderer>;
pub type Text<'a>             = iced::widget::Text<'a, Theme, Renderer>;
pub type Container<'a, Message> = iced::widget::Container<'a, Message, Theme, Renderer>;
```

Do this in any non-trivial app — it makes signatures readable and keeps `Message`/`Theme`/`Renderer`
consistent across modules.

## 5. Production custom-widget theming: the `Catalog` trait

Halloy has ~20 custom widgets (`selectable_text`, `context_menu`, `tooltip`, `color_picker`,
`combo_box`, `modal`, `anchored_overlay`, ...). Each pairs a widget with a **`Catalog`** so it
participates in theming like built-ins:

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

// widget carries `class: Theme::Class<'a>` + a `.style(impl Fn(&Theme) -> Style)` builder.
```

Then a dedicated style module implements `Catalog` for the **app's** `Theme`:

```rust
// appearance/theme/selectable_text.rs
use crate::widget::selectable_text::{Catalog, Style, StyleFn};

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;
    fn default<'a>() -> Self::Class<'a> { Box::new(default) }
    fn style(&self, class: &Self::Class<'_>) -> Style { class(self) }
}

pub fn default(theme: &Theme) -> Style {
    Style { color: None, selection_color: theme.styles().buffer.selection }
}
pub fn secondary(theme: &Theme) -> Style {
    Style { color: Some(theme.styles().text.secondary.color), ..default(theme) }
}
```

**Takeaways for your own widgets:**
- Define a `Catalog` trait + `Style`/`StyleFn` so your widget is themeable like built-ins.
- Put style helper fns (`default`, `secondary`, `danger`, ...) in a matching
  `appearance/theme/<widget>.rs` module — never hardcode colors.
- Read colors from a shared `Styles`/`Palette` struct (Halloy's `theme.styles().*`) so every
  widget stays coherent across themes.

## 6. Layered `appearance/theme/` modules per widget

Halloy's `appearance/theme/` has one file per widget (`button.rs`, `container.rs`, `text_input.rs`,
`scrollable.rs`, `pane_grid.rs`, `selectable_text.rs`, ...). Each implements `Catalog` for the app
`Theme` and exposes semantic style helpers. This is the production-grade analogue of the simpler
`style` module shown in the official `pane_grid` example.

## 7. Dynamic theming from a system subscription

Halloy subscribes to OS color-scheme changes and swaps themes at runtime:

```rust
// appearance.rs — a subscription::Recipe<Output = Mode> (Dark/Light/Unspecified)
// theme(state) picks light/dark from the user's selected pair.
```

So `theme(&self) -> Theme` (passed to `iced::daemon(...).theme(...)`) is fully dynamic — the app
reacts to system preferences and user choice. Let the theme function read state; don't hardcode it.

## 8. Async boot / streaming with `Task::sip`

icebreaker boots an AI assistant by streaming progress then a final result, keeping a handle to
abort on drop:

```rust
let (boot, handle) = Task::sip(
    Assistant::boot(library.directory().clone(), file.clone(), backend),
    Message::Booting,        // per-progress chunk
    Message::Booted,         // final output
).abortable();

self.state = State::Booting { _task: handle.abort_on_drop(), .. };
// update batches boot + other startup tasks:
Task::batch([boot, Task::perform(Chat::list(), Message::ChatsListed)])
```

Pattern: long/streaming work → `Task::sip` with a `_task: task::Handle` stored in state and
`.abort_on_drop()` so it cancels if the screen is dropped.

## 9. `pane_grid` as the primary layout for complex UIs

Halloy's whole UI is one `PaneGrid` (dockable/splittable). It maps `self.panes.main` to
`PaneGrid::new(...)` with a closure building `Content`/`TitleBar` per pane, then `.map`s pane
messages into the parent `Message::Pane(window_id, pane::Message)`. `pane_grid` is the right tool
for IDE-like, multi-region layouts (see `examples/pane_grid` for the basics).

## 10. `Message` is rich and namespaced

Real apps have large, domain-namespaced messages:
`Message::Pane(window::Id, pane::Message)`, `Message::Sidebar(sidebar::Message)`,
`Message::FileTransfer(file_transfer::task::Update)`, `Message::Client(client::Message)`.
Child messages are wrapped (not flattened), keeping `update` a clean per-domain `match`. Pair this
with the `Action`/`Event` enums for cross-cutting effects.

## Project layout that scales (synthesize from Halloy + icebreaker)

```
src/
  main.rs                 // iced::daemon / application(...).run()
  screen.rs               // pub enum Screen { ... } + module decls
  screen/
    dashboard.rs          // self-contained: struct, Message, update->Action, view
    conversation.rs
    settings.rs
  widget.rs               // shared aliases: Element/Column/Row/Row + pub use custom widgets
  widget/                 // custom widgets, each: <name>.rs (+ maybe selection.rs etc.)
    selectable_text.rs
    context_menu.rs
    modal.rs
  appearance.rs           // Mode (Dark/Light), system subscription recipe
  appearance/
    theme.rs              // app Theme, shared Styles, TEXT_SIZE/ICON_SIZE consts
    theme/                // one file per widget: button.rs, container.rs, ...
  modal.rs                // app-level modal state/coordination
  buffer.rs / system.rs   // domain data & side-effecting helpers
```

**Principles**
- One screen per file; each owns its `Message` enum and returns `Action`.
- Top-level `Message` = sum of child messages + global `Event`s.
- Custom widgets get a `Catalog` + a `appearance/theme/<widget>.rs` style module.
- Never hardcode colors; read from a shared palette/`Styles`.
- Use `Element`/`Column`/`Row` aliases to keep signatures clean.
- Reach for `daemon` (multi-window), `pane_grid` (complex layout), `Task::sip` (streaming),
  and dynamic `theme`/`subscription` (reactive theming).
