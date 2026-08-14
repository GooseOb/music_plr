# Tasks & Subscriptions

These are iced's two ways of interacting with the runtime / outside world.

## `Task<Message>` — commands returned from `update`

`update(&mut self, message: Message) -> Task<Message>`.

- `Task::none()` — do nothing (the common case).
- `Task::perform(future, Message::Done)` — run an async fn, map its output to a message.
- `Task::sip(stream, Update::Chunk, Update::Done)` — stream chunks + a final result
  (great for progress). Has `.abortable() -> (Task, Handle)`; `handle.abort_on_drop()`.
- `Task::stream(stream)` — emit a message per stream item.
- `Task::batch([a, b, c])` — run several tasks together.
- `task.map(f)` — lift a sub-task's message into the parent enum.
- `task.then(f)` — chain once the first task finishes (monadic). `task.chain(other)` runs after.
- `Task::done(value)`, `Task::oneshot(f)`.

### Async fetch (the canonical pattern)

```rust
async fn fetch_weather() -> Weather { /* reqwest / tokio / wasm fetch */ unimplemented!() }

enum Message { Fetch, WeatherFetched(Weather) }

fn update(&mut self, message: Message) -> Task<Message> {
    match message {
        Message::Fetch => Task::perform(fetch_weather(), Message::WeatherFetched),
        Message::WeatherFetched(w) => { self.weather = Some(w); Task::none() }
    }
}
```

### Lifting sub-component messages / ids

```rust
// .map lifts a child's message type into the parent enum
contacts.view().map(Message::Contacts)

// .with(i) is sugar for `move |m| Variant(i, m)`
task.map(Message::DownloadUpdated.with(index))

// focus a widget by id (operation::* tasks)
operation::focus("new-task")
operation::focus_next()
operation::focus_previous()
```

### Runtime / window tasks (module helpers)

- `window::open(Settings)` → `(window::Id, Task)`. `window::close(id)`, `window::resize`,
  `window::set_mode(id, window::Mode::Fullscreen)`, `window::latest()`.
- `clipboard::write(...)`, `clipboard::read_text()`.
- `iced::exit()` — quit the app.
- See `examples/multi_window`, `examples/download_progress`.

## `Subscription<Message>` — passive data sources

Declarative, like `view`: the `subscription(&self)` function fully dictates active streams.
A subscription must **not end on its own** — only `subscription` may change it.

- `Subscription::none()` — no subscriptions.
- `time::every(duration)` — tick every interval (`iced::time::{milliseconds, seconds}`).
- `keyboard::listen()` / `event::listen()` / `mouse` / `touch` — native event streams.
- `window::resize_events()` — window resize.
- `stream::channel` / `Subscription::run(recipe)` — build your own (`iced::advanced::subscription`).

Pattern: `stream.filter_map(|event| match event { ... => Some(msg), _ => None })`.
Use `#[derive(Hash, Eq, PartialEq)]` on the message if you build subscriptions from keys
(so iced can dedupe/hash them).

```rust
fn subscription(&self) -> Subscription<Message> {
    if self.playing {
        time::every(milliseconds(1000 / self.speed as u64)).map(|_| Message::Tick)
    } else {
        Subscription::none()
    }
}

fn keyboard(&self) -> Subscription<Message> {
    use iced::keyboard::{key, Event, Key, Modifiers};
    keyboard::listen().filter_map(|event| match event {
        Event::KeyPressed { key: Key::Named(named), modifiers, .. } => match (named, modifiers) {
            (key::Named::Tab, _) => Some(Message::TabPressed { shift: modifiers.shift() }),
            _ => None,
        },
        _ => None,
    })
}
```

### `event::listen()` vs `keyboard::listen()`

- `event::listen()` gives the broad `Event` enum (keyboard + mouse + window + ...). Use for global
  hotkeys / escape-to-close. Map with `Message::Event(event)` and handle in `update`.
- `keyboard::listen()` / `mouse::` / `touch::` are focused sub-streams — prefer them when you only
  care about one input domain. See `examples/modal` and `examples/styling`.

## Executors & features

`iced` needs an async executor feature: `tokio`, `thread-pool` (default, uses `futures`),
or `smol`. `Task::perform`/subscriptions only work with one enabled. For network I/O in
`tokio`, enable the `tokio` feature and use `tokio::fs`/`reqwest` inside async fns.
