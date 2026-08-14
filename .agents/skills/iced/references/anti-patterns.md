# Anti-Patterns & Gotchas

Common mistakes when writing iced code and how to avoid them.

## 1. Monolithic `main.rs`
**Smell:** every `Message`, `update` arm, and `view` branch lives in one file; it grows to
thousands of lines fast.
**Fix:** split into screens/components (see `references/architecture.md`). Each screen owns its
`Message` enum; the parent composes via `Message::Screen(msg).map(...)`.

## 2. Mutating state inside `view`
**Smell:** `view` takes `&mut self`, records something, or calls async.
**Fix:** `view(&self)` is a pure projection of state → `Element`. All mutation belongs in
`update`. The runtime calls `view` whenever it wants to redraw.

## 3. Forgetting `.into()` / wrong `Message` type
**Smell:** `column![...]` returned directly; or a child's `Element<ChildMsg>` doesn't match the
parent `Element<Message>`.
**Fix:** end layout chains with `.into()`. Lift child messages with `.map(Message::Child)`.
Every `Element`/`Task`/`Subscription` in a branch must share the **same `Message`** type.

## 4. Hardcoding colors instead of using the palette
**Smell:** `Color::from_rgb8(38,139,210)` sprinkled through views.
**Fix:** `theme.palette()` + the `button::primary/danger/...` helpers. Your UI then adapts to any
`Theme` and stays coherent. (Exception: one-off overlay backdrops.)

## 5. `Message` not `Clone` / carrying huge payloads by value
**Smell:** compile errors about `Message: Clone`, or huge structs cloned on every event.
**Fix:** derive `Clone` on `Message` (and `Debug` where useful). Wrap large data in `Arc`/emit an
id and read it from state in `update`. The runtime clones messages freely.

## 6. Rebuilding expensive subtrees every frame
**Smell:** a heavy list/graph re-renders on unrelated state changes, janky UI.
**Fix:** use `lazy(key, |_| view)` keyed on a bumped `version` (or `keyed_column` for stable row
identity). Memoize with `responsive` only when you truly need size-driven rebuilds.

## 7. Subscriptions that end on their own
**Smell:** a subscription stream completes and stops firing.
**Fix:** subscriptions are *declarative and must not end*. Drive them from
`subscription(&self)` (enable/disable based on state, e.g. `time::every` only while `playing`).
Never return a one-shot future as a subscription.

## 8. Blocking `update` with sync I/O
**Smell:** `std::fs::read` / `reqwest::blocking` inside `update`.
**Fix:** do async work via `Task::perform(async { ... }, Message::Done)` with an enabled executor
(`tokio`/`thread-pool`/`smol`). For CPU-heavy work, run it on a background thread and stream
results with `Task::sip` (see `examples/download_progress`, `examples/game_of_life`).

## 9. Ignoring the executor / display-server features
**Smell:** `compile_error!` about "No futures executor" or "No Unix display server backend".
**Fix:** `cargo add iced --features tokio,wgpu` (or `thread-pool`/`smol`; `x11`/`wayland` on
Linux; wasm32 needs an executor feature).

## 10. Manual event wiring instead of `on_press`/`on_input`
**Smell:** re-implementing button clicks through `event::listen()` in `update`.
**Fix:** prefer declarative widget callbacks: `button(...).on_press(msg)`,
`text_input(...).on_input(msg).on_submit(msg)`, `checkbox(...).on_toggle(msg)`. Use
`event::listen()` only for global hotkeys / escape-to-close (see `examples/modal`).

## 11. Losing focus/scroll state across rebuilds
**Smell:** focus jumps, scroll position resets after typing.
**Fix:** assign stable `widget::Id`s (`text_input("...", &v).id("new-task")`) and drive focus with
`operation::focus(id)` / `focus_next()` tasks. Use `keyed_column` so rows keep identity when
reordered (`examples/todos`).

## 12. Custom widget without `From<W> for Element`
**Smell:** can't use the widget in `column!` / `.into()`.
**Fix:** implement `From<YourWidget> for Element<'_, Message, Theme, Renderer>` (or wrap with
`Element::new`). Ensure `size`/`layout`/`draw` are consistent (bounds from `layout` == drawn area).

## 13. Borrow-checker fights in `view` with `self`
**Smell:** `self.field.clone()` everywhere, or "cannot borrow `self` as mutable".
**Fix:** `view` is `&self`, so clone cheaply or restructure `match` to destructure `&self`
fields (`match self { State::Loaded(State { input_value, .. }) => ... }`). Keep `Message`/`State`
cheaply copyable; for expensive parts store `Arc`/indices.

## 14. Not testing the UI
**Smell:** only unit-testing pure logic, shipping blind.
**Fix:** use `iced_test::simulator(view)` for headless interaction tests and
`snapshot.matches_hash(path)` for visual regression (see `examples/todos` `mod tests`).
