# iced skill

An [agent skill](https://skills.sh) for building cross-platform GUI applications in Rust with
the [iced](https://github.com/iced-rs/iced) framework (The Elm Architecture).

## What it covers

- The four TEA concepts: **State / Messages / Update / View**
- Built-in widgets, layout (`column`/`row`/`container`/`stack`/`grid`), sizing & alignment
- **Tasks** (`Task::perform`, `Task::sip`, `batch`, `map`, `abortable`) and **Subscriptions**
  (`time::every`, `keyboard::listen`, `event::listen`)
- **Theming & styling** — `Theme`, `Palette`, widget `style` closures, and the production-grade
  `Catalog` pattern for custom widgets
- **Custom widgets** — implementing `Widget`, operations, overlays, `Canvas`, custom shaders
- **Architecture** — screen composition via `Screen`/`Action` enums, `Message` design, project layout
- **Real-world patterns** distilled from [Halloy](https://github.com/squidowl/halloy) and
  [icebreaker](https://github.com/hecrj/icebreaker): `daemon` multi-window, `pane_grid` layouts,
  `Catalog`-based theming, streaming boot with `Task::sip`
- **Anti-patterns** — 14 common mistakes and how to avoid them

## Structure

```
SKILL.md                  # entry point: when-to-use, quick start, rules of thumb, index
references/
  widgets.md              # widget inventory + builder idioms
  tasks-subscriptions.md  # Task + Subscription patterns
  styling-theming.md      # Theme, Palette, style closures
  custom-widgets.md       # implementing Widget, Catalog, operations, overlays
  architecture.md         # scaling apps, screen composition, layout
  real-world.md           # patterns from Halloy & icebreaker
  anti-patterns.md        # common mistakes
```

## Usage

With an agent that supports the skills ecosystem:

```bash
npx skills add <your-owner>/iced-skill
```

Or copy this directory into your agent's skills folder (e.g. `~/.agents/skills/iced/`).

## Sources

Grounded in the `iced` source tree (examples, `core/`, `widget/`, `runtime/`), the official
[book](https://book.iced.rs), `docs.rs/iced`, and the architecture of Halloy and icebreaker.
