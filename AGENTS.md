# AGENTS.md — Workdesk

Personal macOS desktop app (Rust + [GPUI Kit](https://gpui-kit.com) `=0.9.1`, which re-exports its pinned GPUI fork `gpui-pre 0.3.12`). User-facing features, shortcuts, data files → [README.md](README.md). This file: how to change the code safely.

## Commands

| Task | Command |
|---|---|
| Build / check | `cargo build` (first build ~3 min; incremental ~30 s) |
| Tests | `cargo test` |
| Format | `cargo fmt` (run before committing) |
| Run dev, throwaway data | `WORKDESK_DATA_DIR=/tmp/wd cargo run` |
| Dev loop (rebuild + relaunch on save) | `make dev` (`bacon.toml`, data in `/tmp/workdesk-dev`); theme JSON edits apply live without restart |
| Ship to user | `make install` → rebuilds release, replaces `~/Applications/Workdesk.app` (deletes stale `/Applications` copy), relaunches |
| Regenerate icon | `make icon` (edits `scripts/make_icon.swift` first) |

Done = `cargo fmt` + `cargo build` with **zero warnings** + `cargo test` green. Ignore the `block`/`proc-macro-error2` future-incompat note (upstream).

## Delivering a feature

1. Bump `version` in `Cargo.toml` (minor for features, patch for fixes) — shown in status bar + Info.plist; the user checks it to confirm an update.
2. Update [README.md](README.md) if user-visible behaviour, shortcuts, or data format changed.
3. `make install` only when the user wants the installed app updated (it quits and relaunches their running app).
4. Commit on `main`, conventional style (`feat:`, `fix:`, `chore:`, `docs:`). Repo is local-only (no remote).

## Architecture

```
main.rs        gpui_kit::application(), gpui_kit::open_window (wraps view in kit Root), Dock reopen
actions.rs     actions!(), key bindings (bind()), menu bar
storage.rs     load/save JSON in data_dir()
prefs.rs       settings.json (appearance, name, ambient) as a Global; prefs::update saves + repaints
theme.rs       Appearance → kit Theme; loads themes/workdesk.json (watched in debug)
workspace.rs   root view: aurora | TitleBar | custom sidebar | active page; owns TaskStore + FocusTimer
tasks/         model.rs (Task, …, parse_quick_add, streak stats) · store.rs (TaskStore entity)
focus/         FocusTimer entity (pomodoro, focus.json)
pages/         one Render view per sidebar page; Page enum lists them
components/    thin fn wrappers over kit components + icon Assets; fx.rs = glow/gradient/ring/animations
```

- **State**: one `Entity<TaskStore>` created in `Workspace::new`, cloned into pages. Pages `cx.observe(&store, …)` to re-render. All mutations go through `TaskStore` methods, which end in `commit()` (save + `cx.notify()`). Never mutate task data elsewhere.
- **New page**: steps in README "Add a page". New feature with its own data → own module like `tasks/` with its own JSON file via `storage::{load, save}`.
- **Imports**: `gpui_kit::*` is GPUI; components under `gpui_kit::component::*`; never add `gpui` as a direct dependency.
- **Components**: use kit ones (`Button`, `Checkbox`, `Input`, `TabBar`, `Tag`, `Sidebar`, `Icon`, `h_flex`/`v_flex`, tooltips…) before hand-rolling. Kit source: `~/.cargo/registry/src/*/gpui-component-0.9.1/src`.
- **Theme**: colors via `cx.theme()` (`gpui_kit::component::ActiveTheme`); our mapping: `group_box` = card/surface (translucent so the aurora shows), `secondary` = row hover, `secondary_active` = selected, `primary` = accent, `muted_foreground` = secondary text, `chart_1`–`chart_5` = brand gradient/glow/stat accents (`fx::brand_gradient`, `fx::wash`). Change the palette in `themes/workdesk.json`, not in code. No hard-coded colors in views except data-driven ones (priority/category hex in `tasks/model.rs`). Don't mark our themes `is_default` (breaks hot reload).
- **Style**: translucent surfaces with a hairline `theme.border`; no drop shadows, only colored glows via `fx::glow` (kit `"shadow": false`); `rounded_2xl` cards, `rounded_xl` rows, pill (`ButtonRounded::Size(px(999.))`) buttons/chips.
- **Motion**: use `components::fx` (`enter` stagger, `ring`/`progress_bar` springs, `pulse`). No decorative looping motion on icons (user rejected bobbing icons as slop); repeating animations must be gated on `prefs::get(cx).ambient` and set `with_max_fps` — each one redraws the whole window. `with_animation` state is keyed by element id: an id that disappears for a frame replays on return (used for page/row entrances).
- **Icons**: `Icon::new(gpui_kit::assets::IconName::X).text_color(c)` — never text glyphs. Kit embeds ~100 component icons; any other Lucide icon must be added to `icon_assets!` in `components/assets.rs` (+ the test list) or it renders blank.
- **Text entry**: `Entity<InputState>` (`InputState::new(window, cx).placeholder(..)`) rendered with `Input::new(&state)`. `cx.subscribe_in(&state, window, …)` for `InputEvent::PressEnter`; `value()`, `set_value(text, window, cx)`. Esc propagates as `component::input::Escape` — handle with `.on_action` on a parent.

## Persistence rules

- Data dir: `storage::data_dir()` (`~/Library/Application Support/Workdesk`, or `$WORKDESK_DATA_DIR`).
- Format changes must stay backward compatible: new fields get `#[serde(default)]`; never rename/remove fields without a migration in `TaskData`. Old user files must still load.
- `storage::load` quarantines unparseable files; don't replace that with a silent default.
- Never touch the user's real data dir in tests or manual checks — always set `WORKDESK_DATA_DIR`.

## GPUI (gpui-pre 0.3.12) / kit gotchas

API source of truth: `~/.cargo/registry/src/*/{gpui-pre-0.3.12,gpui-base-0.9.1,gpui-component-0.9.1}/src`. Online docs track newer versions — verify there before using.

| Gotcha | Fix |
|---|---|
| Upgrading gpui-kit | bumps the pinned GPUI too; expect API breaks, do it as its own change |
| `KeyBinding::new("secondary-…")` panics at startup | use `actions::bind` (`KeyBinding::load` resolves `secondary` per OS) |
| Focus during window construction is dropped | `cx.defer_in(window, …)` (see `Workspace::new`) |
| `FocusHandle::focus` / `InputState::set_value` need `window, cx` | thread `window` through; subscribe with `subscribe_in` |
| macOS build needs full Xcode `metal` | not needed: gpui-kit enables `runtime_shaders` |
| Window chrome | transparent title bar (`TitleBar::title_bar_options` + `TitleBar::window_options`); kit `TitleBar` owns dragging, so keep it rendered at the top |
| Focus state of an `InputState` | `entity.focus_handle(cx)` needs `gpui_kit::Focusable` in scope |
| `BoxShadow` literal | needs `inset: false` |
| `on_reopen` isn't on `App` | call on `Application` before `.run()` (see `main.rs`) |
| Closure in `.map()` needs `cx` → "captured variable cannot escape `FnMut`" | take `cx: &Context<Self>` (not `&mut`) in render helpers; `cx.listener` only needs `&` |
| Borrow conflict reading theme then calling `cx.listener` | `let theme = cx.theme().clone();` at top of render |
| Store read + later `cx` use | copy data out of `self.store.read(cx)` first (`to_vec()`, `cloned()`) |
| Hover-only row actions | parent `.group(name)`, child `.opacity(0.).group_hover(name, \|s\| s.opacity(1.))`; group name unique per row |
| Element IDs | tuple form `("prefix", u64)`; keep prefixes unique per view |
| Global shortcuts | bind with `secondary-` (cmd on mac, ctrl elsewhere), `None` context; handle on `Workspace` root via `.on_action` |
| Focus after page switch | `Workspace::navigate` must focus something rendered (composer or workspace handle) or actions stop dispatching |

## Verifying UI

- Screenshots (`screencapture`) fail: no screen-recording permission, and the window exposes no AX tree. Ask the user to eyeball visual changes.
- Quick check without bundling: run `WORKDESK_DATA_DIR=/tmp/wd ./target/debug/workdesk` as a background shell (block 0), then use the frontmost-guarded keystrokes below.
- Keystroke automation via `osascript`/System Events works but types into **whatever app is frontmost** — the user is often working. Only send keys after asserting the frontmost process is `workdesk`; abort otherwise. Prefer AX actions that don't need focus (e.g. clicking the window close button by subrole `AXCloseButton`).
- Launch for tests with `open --env WORKDESK_DATA_DIR=/tmp/wd --stderr /tmp/wd.log target/release/Workdesk.app` (a backgrounded binary from the agent shell is killed when the command ends). Check results by reading `/tmp/wd/tasks.json`.
- Quit with `osascript -e 'tell application id "com.workdesk.desktop" to quit'`. Clean up `/tmp/wd*` after.

## Don't

- Don't add other UI libraries alongside gpui-kit, a direct `gpui` dependency, or git dependencies without asking — they must match the kit's pinned GPUI.
- Don't edit `Cargo.lock` by hand; don't commit `target/`.
- Don't add clutter: the user explicitly wants a minimal UI. Secondary actions go on hover, not always-visible.
