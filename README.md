# Workdesk

Desktop app for custom work tooling, built on [GPUI](https://gpui.rs/) (Zed's UI framework).
First feature: a task list with categories, priorities and a done log.

## Install / update

```sh
make install     # builds release, installs /Applications/Workdesk.app, relaunches it
```

Re-run `make install` after any code change to update. The Dock pin and your data survive updates.
Pin once: right-click the Dock icon → Options → Keep in Dock.

Falls back to `~/Applications` if `/Applications` isn't writable.

## Develop

```sh
make run         # cargo run (debug)
make test        # unit tests
make icon        # regenerate assets/AppIcon.icns from scripts/make_icon.swift
WORKDESK_DATA_DIR=/tmp/wd cargo run   # use throwaway data
```

macOS needs only Xcode Command Line Tools: the `runtime_shaders` feature (see `Cargo.toml`) compiles Metal shaders at startup instead of requiring the full Xcode `metal` compiler.

Bump `version` in `Cargo.toml` with each feature; it shows in the status bar and the app's Info.plist.

## Data

`~/Library/Application Support/Workdesk/` (Settings → Data → Show in Finder)

| File | Contents |
|---|---|
| `tasks.json` | Tasks + categories. Saved on every change, atomically. |
| `settings.json` | Theme. |

An unreadable file is renamed `*.corrupt-<timestamp>.json`, never overwritten. When changing the
format, add fields with `#[serde(default)]` (`src/tasks/model.rs`) so old files still load.

## Tasks

Type in the composer and press Enter. Inline tokens (anywhere in the text):

| Token | Effect |
|---|---|
| `#name` | Category; matched loosely (`#follow-up` = "Follow-up"), created if new |
| `!urgent` `!high` `!medium` `!low` (or `!1`–`!4`) | Priority |

Without tokens, the priority/category chips under the input apply. Hover a task for actions:
cycle priority (●), edit (✎), delete (✕, with Undo). Click the circle to complete; Done groups by day.

## Shortcuts

| Keys | Action |
|---|---|
| ⌘N | New task (focus composer) |
| ⌘1 / ⌘, | Tasks / Settings |
| Enter / Esc | Add or save / cancel edit |
| ⌘⇧T | Toggle light/dark |
| ⌘W / ⌘Q | Close window (app stays in Dock) / Quit |

## Layout

| Path | Purpose |
|---|---|
| `src/main.rs` | App entry, window, Dock reopen |
| `src/actions.rs` | Actions, key bindings, menu bar |
| `src/storage.rs` | JSON load/save in the data dir |
| `src/theme.rs` | Light/dark `Theme` global (`cx.theme()`), persisted |
| `src/workspace.rs` | Root view: sidebar, page content, status bar |
| `src/tasks/` | Task model, quick-add parser, `TaskStore` entity |
| `src/pages/` | One file per page; `Page` enum drives the sidebar |
| `src/components/` | `Button`, `card`, `chip`, `icon` / `icon_button` (SVGs in `assets/icons/`, embedded), `TextInput` |
| `scripts/` | Icon generation, `.app` bundling, install |

## Add a page

1. Create `src/pages/<name>.rs` with a struct implementing `Render`.
2. Add a variant to `Page` (+ `ALL`, `label`, `icon`) in `src/pages/mod.rs`.
3. Hold an `Entity<YourPage>` in `Workspace` and map it in `active_view` / `navigate`.
4. Optional: add a `Show<Name>` action + key binding in `src/actions.rs`.

## References

- [GPUI README](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [examples](https://github.com/zed-industries/zed/tree/main/crates/gpui/examples)
- [gpui-component](https://github.com/longbridge/gpui-component): optional ready-made widgets
