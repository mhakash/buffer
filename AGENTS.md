# AGENTS.md — Workdesk

Personal macOS desktop app (Rust + [GPUI](https://gpui.rs/) 0.2.2 from crates.io). User-facing features, shortcuts, data files → [README.md](README.md). This file: how to change the code safely.

## Commands

| Task | Command |
|---|---|
| Build / check | `cargo build` (first build ~3 min; incremental ~30 s) |
| Tests | `cargo test` |
| Format | `cargo fmt` (run before committing) |
| Run dev, throwaway data | `WORKDESK_DATA_DIR=/tmp/wd cargo run` |
| Ship to user | `make install` → rebuilds release, replaces `/Applications/Workdesk.app`, relaunches |
| Regenerate icon | `make icon` (edits `scripts/make_icon.swift` first) |

Done = `cargo fmt` + `cargo build` with **zero warnings** + `cargo test` green. Ignore the `block`/`proc-macro-error2` future-incompat note (upstream).

## Delivering a feature

1. Bump `version` in `Cargo.toml` (minor for features, patch for fixes) — shown in status bar + Info.plist; the user checks it to confirm an update.
2. Update [README.md](README.md) if user-visible behaviour, shortcuts, or data format changed.
3. `make install` only when the user wants the installed app updated (it quits and relaunches their running app).
4. Commit on `main`, conventional style (`feat:`, `fix:`, `chore:`, `docs:`). Repo is local-only (no remote).

## Architecture

```
main.rs        Application setup, window, Dock reopen
actions.rs     actions!(), key bindings, menu bar
storage.rs     load/save JSON in data_dir()
theme.rs       Theme global + persisted Appearance
workspace.rs   root view: sidebar | active page | status bar; owns Entity<TaskStore>
tasks/         model.rs (Task, Category, Priority, TaskData, parse_quick_add) · store.rs (TaskStore entity)
pages/         one Render view per sidebar page; Page enum lists them
components/    stateless RenderOnce/fn helpers + TextInput entity
```

- **State**: one `Entity<TaskStore>` created in `Workspace::new`, cloned into pages. Pages `cx.observe(&store, …)` to re-render. All mutations go through `TaskStore` methods, which end in `commit()` (save + `cx.notify()`). Never mutate task data elsewhere.
- **New page**: steps in README "Add a page". New feature with its own data → own module like `tasks/` with its own JSON file via `storage::{load, save}`.
- **Theme**: read colors via `cx.theme()` (`ActiveTheme` trait). No hard-coded colors in views except data-driven ones (priority/category hex in `tasks/model.rs`).
- **Text entry**: `components::TextInput` (single-line). Subscribe to `TextInputEvent::{Submit, Cancel}`; use `text()`, `set_text()`, `clear()`. Its key bindings are scoped to the `"TextInput"` context in `text_input::init`.

## Persistence rules

- Data dir: `storage::data_dir()` (`~/Library/Application Support/Workdesk`, or `$WORKDESK_DATA_DIR`).
- Format changes must stay backward compatible: new fields get `#[serde(default)]`; never rename/remove fields without a migration in `TaskData`. Old user files must still load.
- `storage::load` quarantines unparseable files; don't replace that with a silent default.
- Never touch the user's real data dir in tests or manual checks — always set `WORKDESK_DATA_DIR`.

## GPUI 0.2.2 gotchas

API source of truth: `~/.cargo/registry/src/*/gpui-0.2.2/{src,examples}`. Docs online often describe newer Zed-internal APIs — verify there before using.

| Gotcha | Fix |
|---|---|
| macOS build needs full Xcode `metal` | `runtime_shaders` feature already enabled in `Cargo.toml`; keep it |
| `on_reopen` isn't on `App` | call on `Application` before `.run()` (see `main.rs`) |
| Closure in `.map()` needs `cx` → "captured variable cannot escape `FnMut`" | take `cx: &Context<Self>` (not `&mut`) in render helpers; `cx.listener` only needs `&` |
| Borrow conflict reading theme then calling `cx.listener` | `let theme = cx.theme().clone();` at top of render |
| Store read + later `cx` use | copy data out of `self.store.read(cx)` first (`to_vec()`, `cloned()`) |
| Hover-only row actions | parent `.group(name)`, child `.opacity(0.).group_hover(name, \|s\| s.opacity(1.))`; group name unique per row |
| Element IDs | tuple form `("prefix", u64)`; keep prefixes unique per view |
| Global shortcuts | bind with `secondary-` (cmd on mac, ctrl elsewhere), `None` context; handle on `Workspace` root via `.on_action` |
| Focus after page switch | `Workspace::navigate` must focus something rendered (composer or workspace handle) or actions stop dispatching |

## Verifying UI

- Screenshots (`screencapture`) fail: no screen-recording permission. Ask the user to eyeball visual changes.
- Keystroke automation via `osascript`/System Events works but types into **whatever app is frontmost** — the user is often working. Only send keys after asserting the frontmost process is `workdesk`; abort otherwise. Prefer AX actions that don't need focus (e.g. clicking the window close button by subrole `AXCloseButton`).
- Launch for tests with `open --env WORKDESK_DATA_DIR=/tmp/wd --stderr /tmp/wd.log target/release/Workdesk.app` (a backgrounded binary from the agent shell is killed when the command ends). Check results by reading `/tmp/wd/tasks.json`.
- Quit with `osascript -e 'tell application id "com.workdesk.desktop" to quit'`. Clean up `/tmp/wd*` after.

## Don't

- Don't add a UI component library (e.g. gpui-component) or switch GPUI to a git dependency without asking — version coupling is fragile.
- Don't edit `Cargo.lock` by hand; don't commit `target/`.
- Don't add clutter: the user explicitly wants a minimal UI. Secondary actions go on hover, not always-visible.
