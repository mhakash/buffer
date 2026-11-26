<p align="center">
  <img src="assets/AppIcon.png" width="112" alt="Buffer icon">
</p>

<h1 align="center">Buffer</h1>

<p align="center">
  A calm, native macOS workspace for tasks, notes and focus, with Markdown blocks your own scripts fill.
</p>

---

Buffer is a personal desktop app for day-to-day work: a quick task list, Notion-style Markdown
notes, a pomodoro timer and custom blocks that run your scripts and show their output right in
a note. Everything is stored as plain JSON and Markdown in a folder you choose, so your data
stays readable and yours.

It is written in Rust on [GPUI](https://gpui.rs/) (the UI framework behind the Zed editor) via
[GPUI Kit](https://gpui-kit.com), so it starts instantly and stays light.

## Features

**Tasks**
- Quick add with inline tokens: `#category`, `!high`, pasted links become labelled notes
- To do, Follow up and Done tabs; drag to reorder; due dates and priorities
- Daily dashboard: progress ring, streak and a 7-day chart
- Task details in a resizable side panel or as a full page, with rich notes

**Notes**
- Notion-style block editor over plain Markdown: edit in place, drag blocks, slash menu, tables
- Mermaid diagrams, images, YAML properties and a heading outline
- Files live in the content: upload, link or create text files, or drop them from Finder
- Search across content and properties; tags shared with tasks

**Custom blocks**
- A ```` ```block name ```` fence runs your script (any language) and renders the Markdown it prints or returns
- Interactive output: scripts can print inputs (text, number, toggle, dropdown, button) that re-run the block
- Cached results, refresh intervals, timeouts and per-workspace trust
- Secrets & config vault and keychain references for API tokens

**Focus**
- Pomodoro timer with breaks, chime and a session log; start it from anywhere with ⌘⇧F

**Workspaces & look**
- Several workspaces, each a plain folder (a synced folder works too)
- Light and dark themes from a JSON palette, ambient aurora backdrop, Reduce Motion respected

## Requirements

- macOS with the Xcode Command Line Tools (`xcode-select --install`); the full Xcode is not needed
- [Rust](https://rustup.rs/) (stable, 2024 edition)

## Install

```sh
git clone <repo-url> buffer && cd buffer
make install    # builds a release and installs ~/Applications/Buffer.app
```

Run `make install` again to update. Your data and Dock pin survive updates.

## Development

```sh
make dev        # rebuild and relaunch on every save, throwaway data (needs `brew install bacon`)
make test       # unit tests
BUFFER_DATA_DIR=/tmp/wd cargo run   # run against a throwaway data folder
```

More in [docs/development.md](docs/development.md).

## Documentation

| Guide | Covers |
|---|---|
| [Tasks](docs/guide/tasks.md) | Quick add, tabs, task details, Focus timer |
| [Notes](docs/guide/notes.md) | Notes, the block editor, files, Markdown extras |
| [Custom blocks](docs/guide/custom-blocks.md) | Writing block scripts, inputs, secrets & config |
| [Workspaces & data](docs/guide/data.md) | Workspaces, where files live, their formats |
| [Shortcuts & appearance](docs/guide/shortcuts.md) | Keyboard shortcuts, themes, motion settings |
| [Development](docs/development.md) | Building, installing, themes, references |
| [Architecture](docs/ARCHITECTURE.md) | Code layout and conventions for contributors |

How to keep these up to date: [docs/README.md](docs/README.md).

## Built with

[GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) ·
[GPUI Kit](https://gpui-kit.com) ·
[Lucide icons](https://lucide.dev/icons) ·
[merman](https://crates.io/crates/merman) (Mermaid diagrams)
