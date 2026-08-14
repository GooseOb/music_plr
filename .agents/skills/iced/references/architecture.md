# Architecture & Project Structure

Iced scales by **composition**. The `State` + `Message` + `update` + `view` quartet composes
seamlessly via functor methods (`Element::map`, `Task::map`, `Subscription::map`).

## Screen / component composition

Give each screen its own module with its own `State`, `Message`, `update`, `view`. The parent
holds an enum of screens and wraps child messages:

```rust
mod contacts { /* pub struct Contacts; pub enum Message; pub enum Action { None, Run(Task<Message>), Open(Contact) } */ }
mod conversation { /* ... */ }

struct App { screen: Screen }
enum Screen { Contacts(contacts::Contacts), Conversation(conversation::Conversation) }
enum Message { Contacts(contacts::Message), Conversation(conversation::Message) }

fn update(&mut self, message: Message) -> Task<Message> {
    match message {
        Message::Contacts(msg) => {
            if let Screen::Contacts(c) = &mut self.screen {
                match c.update(msg) {
                    contacts::Action::None => Task::none(),
                    contacts::Action::Run(t) => t.map(Message::Contacts),
                    contacts::Action::Open(contact) => {
                        let (conv, t) = conversation::Conversation::new(contact);
                        self.screen = Screen::Conversation(conv);
                        t.map(Message::Conversation)
                    }
                }
            } else { Task::none() }
        }
        Message::Conversation(msg) => { /* symmetric */ Task::none() }
    }
}

fn view(&self) -> Element<'_, Message> {
    match &self.screen {
        Screen::Contacts(c) => c.view().map(Message::Contacts),
        Screen::Conversation(c) => c.view().map(Message::Conversation),
    }
}
```

The child returns an **`Action` enum** (`None` / `Run(Task)` / transition payload) so the parent
stays in control of screen transitions and task execution. This "tells a story" connecting screens
type-safely — exactly the pattern in the `iced` lib.rs Pocket Guide and `examples/todos`.

Confirmed in production: **icebreaker** uses `pub enum Action { None, Run(Task<Message>) }` and its
screens' `update` returns `Action`; **Halloy** generalizes this with cross-cutting `Event`s the app
interprets centrally. See `references/real-world.md` for the full analysis of both.

## Message design guidelines

- **Namespace by domain**: `Message::TaskMessage(usize, TaskMessage)`, `Message::Grid(version, grid::Message)`.
- **Carry IDs/versions** so `update` can route without re-querying: `TaskMessage(i, msg)`,
  `Message::DownloadUpdated(index, update)`.
- **Use `with(i)` sugar**: `task.map(Message::X.with(i))` ≡ `move |m| Message::X(i, m)`.
- **Version keys for memoization**: keep a `version: usize` bumped on structural change and pass
  it to `lazy(version, ...)` so expensive subtrees rebuild only when needed (`examples/lazy`).
- **Keep messages `Clone`** — the runtime clones freely. For big payloads use `Arc` or emit a key.

## Persistent / async state (load → save)

Classic shape (from `examples/todos`):
- On `new()` return `(Self::Loading, Command::perform(SavedState::load(), Message::Loaded))`.
- `update` matches `Message::Loaded(Ok/Err)` to swap `Loading` → `Loaded(State)`.
- Mutations mark `state.dirty = true`; a guard batches a debounced `Command::perform(save, Message::Saved)`.
- `Command::batch([focus, move_cursor])` to drive the UI after load.
- Native vs wasm: branch `#[cfg(not(target_arch = "wasm32"))]` (tokio fs + `directories`)
  vs `#[cfg(target_arch = "wasm32")]` (`web_sys::Storage` + `wasmtimer`).

## Presets (debugging)

`iced::application(...).presets([Preset::new("Name", || (state, Task::none()))])` registers
launchable presets — handy for tests/demos (`examples/todos` registers "Empty" and "Carl Sagan").

## Suggested crate / module layout

Avoid one giant `main.rs`. Split the four TEA entities per concern:

```
src/
  main.rs            // iced::application(new, update, view).run()
  app.rs             // App state, top-level Message, update, view, theme, subscription
  style.rs           // shared style fns & palette-driven colors
  screens/
    home.rs          // Home state/message/update/view + Action enum
    settings.rs
  widgets/           // custom widgets (each its own module/file)
    circle.rs
  components/        // reusable sub-views (search_bar, task_row, ...)
  messages.rs        // shared Message types (Filter, Plan, TaskMessage...)
  persistence.rs     // load/save logic (cfg-gated native/wasm)
```

Rules of thumb:
- One **screen per file/module**; each owns its `Message` enum.
- A top-level `Message` is mostly a sum of child messages + global events
  (`Event`, `WindowResized`, `TabPressed`).
- Shared enums (`Filter`, `Plan`, `TaskMessage`) live in a common module and implement `Display`
  (for `pick_list`/`radio`) and `Serialize`/`Deserialize` (for persistence) as needed.
- Keep `view` thin and declarative; push layout helpers into `components/` returning `Element`.
- Large apps to study: [Halloy](https://github.com/squidowl/halloy) (chat client, strong
  component structure) and the official `examples/` (each is a self-contained, well-factored app).

## Daemon (multi-window / headless)

For multiple windows or no default window, use `iced::daemon(new, update, view)` and manage
`window::Id`s in a `BTreeMap`. See `examples/multi_window`.
