---
name: iced
description: Build cross-platform GUI applications in Rust with the iced framework (The Elm Architecture). Use when writing, reviewing, or architecting iced code — state/message/update/view, widgets, layout, styling, themes, Tasks, subscriptions, custom widgets, and canvas/renderer integration. Based on the iced source (examples, core, widget, runtime), the official book, and idiomatic project structure.
---

# Iced — Rust GUI Framework Skill

Iced is a cross-platform, type-safe GUI library for Rust inspired by **The Elm Architecture (TEA)**.
Every iced interface is split into four connected concepts:

- **State** — application data that persists between interactions (`Model` in Elm).
- **Messages** — meaningful user interactions / events (`Msg` in Elm).
- **Update logic** — reacts to a `Message` and mutates `State` (optionally returning a `Task`).
- **View logic** — turns `State` into `Element`s (widgets) that emit `Message`s.

> Mental model: **Widgets → Messages → State changes → new Widgets**. A feedback loop.

The runtime owns the loop: it takes the `Element` from `view`, lays it out, draws it, turns
native events into `Message`s, feeds them to `update`, and repeats. You never call `view` or
`draw` yourself.

This skill is grounded in the `iced` repository (examples, `core/`, `widget/`, `runtime/`),
the official [book](https://book.iced.rs), and `docs.rs/iced`. Reference files in this
skill expand each topic with runnable patterns:

- `references/widgets.md` — built-in widget inventory + builder-method idioms.
- `references/tasks-subscriptions.md` — `Task`, async work, and `Subscription` patterns.
- `references/styling-theming.md` — `Theme`, `Palette`, widget `style` closures.
- `references/custom-widgets.md` — implementing `Widget`, operations, and overlays.
- `references/architecture.md` — scaling apps, screen composition, project structure.
- `references/real-world.md` — patterns from Halloy & icebreaker (Screen enum, Action, Catalog theming, daemon/multi-window, pane_grid).
- `references/anti-patterns.md` — common mistakes and how to avoid them.

## When to use this skill

- Scaffolding a new iced app or adding a feature to one.
- Writing `update`/`view`, designing `Message` enums, or structuring state.
- Choosing layout widgets (`column`/`row`/`container`/`stack`/`grid`), sizing (`Fill`/`Shrink`), or alignment.
- Theming / styling widgets, or building a custom `Theme`.
- Performing async I/O, background work, or reacting to system events (subscriptions).
- Building a custom widget, canvas drawing, or `wgpu`/custom shader integration.
- Reviewing iced code for idiomatic structure and anti-patterns.

## Quick start (canonical shape)

```rust
use iced::widget::{button, column, text};
use iced::{Center, Element};

pub fn main() -> iced::Result {
    iced::run(Counter::update, Counter::view)
}

#[derive(Default)]
struct Counter { value: i64 }

#[derive(Debug, Clone, Copy)]
enum Message { Increment, Decrement }

impl Counter {
    fn update(&mut self, message: Message) {
        match message {
            Message::Increment => self.value += 1,
            Message::Decrement => self.value -= 1,
        }
    }

    fn view(&self) -> Element<'_, Message> {
        column![
            button("+").on_press(Message::Increment),
            text(self.value).size(50),
            button("-").on_press(Message::Decrement),
        ]
        .padding(20)
        .align_x(Center)
        .into()
    }
}
```

For anything beyond a toy, prefer the builder form `iced::application(new, update, view)`
so you can attach `.theme()`, `.subscription()`, `.window_size()`, `.title()`, etc.

## Core rules of thumb

1. **`Message` carries data, not intent-only.** `Message::InputChanged(String)`,
   `Message::DownloadUpdated(usize, Update)`, `Message::TaskMessage(usize, TaskMessage)` —
   bring the payload you need so `update` stays a pure `match` with no re-querying.
2. **`Message` must be `Clone` + cheap to clone.** The runtime clones messages.
   Wrap large payloads (e.g. `SavedState`, image buffers) behind `Arc`/`Rc`, or emit a token
   and read state in `update`.
3. **`view` is a pure function of `&State`.** No mutation, no I/O, no side effects.
   It returns an `Element<'_, Message>` every call. Keep it declarative.
4. **`update` returns `Task<Message>`** when it needs the runtime (async, focus,
   window ops). Return `Task::none()` otherwise. Mutating state is `&mut self`.
5. **Everything is generic over `Message`.** `view` returns `Element<'_, Message>` and the
   `Message` type must match `update`. Use `Element::map` / `Task::map` to lift sub-component
   messages into the parent enum.
6. **Builder pattern for widgets.** `button("+").on_press(...).padding(10).style(...)`.
   End a layout chain with `.into()` to turn it into an `Element`.
7. **Leverage `Length` for sizing**: `Fill` (take all space), `Shrink` (intrinsic),
   `FillPortion(n)`, `Fit` (like `Shrink` but becomes `Fill` if no constraint), or fixed `Pixels`.
8. **Composition over monolith.** Split large apps into screens/components, each with their
   own `State` + `Message` + `update` + `view`, composed via `Message::Screen(msg).map(...)`.
   See `references/architecture.md`.

## Running / testing

- `cargo add iced --features <executor>,<renderer>` — pick an executor
  (`tokio` / `thread-pool` / `smol`) and a renderer (`wgpu` default, or `tiny_skia`).
  On Unix you must enable `x11` or `wayland`; on wasm32 you need an executor feature.
- `iced_test` provides a `Simulator` for headless UI tests: `ui.click(id)`, `ui.typewrite(...)`,
  `ui.find(...)`, `ui.snapshot(&theme)`, `snapshot.matches_hash(path)`. Great for regression tests.

## Reference index (read when relevant)

| Task | Read |
|------|------|
| Pick/use a built-in widget, layout, alignment | `references/widgets.md` |
| Async fetch, streams, focus, batch tasks, subscriptions | `references/tasks-subscriptions.md` |
| Theme switching, `Palette`, custom widget visuals | `references/styling-theming.md` |
| Implement `Widget`, `Operation`, `overlay`, canvas | `references/custom-widgets.md` |
| Multi-screen apps, `Action` enum, crate layout | `references/architecture.md` |
| Patterns from Halloy/icebreaker, `Catalog` theming, daemon | `references/real-world.md` |
| Avoid confusion / perf cliffs / borrow fights | `references/anti-patterns.md` |
