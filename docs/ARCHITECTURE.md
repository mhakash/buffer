# Buffer — architecture

Module map, state, UI conventions and persistence rules. Features and data files → [guide](guide/).

## Modules

```
main.rs        gpui_kit::application(), gpui_kit::open_window (wraps view in kit Root), Dock reopen
actions.rs     actions!(), key bindings (bind()), menu bar
storage.rs     load/save JSON; sole path resolver: data_dir() = active workspace root, app_dir() = app config
workspaces.rs  workspace registry (workspaces.json in app_dir) as a Global; switching re-points storage, Workspace rebuilds stores + pages
vault.rs       vault.json (app dir, 0600): secrets & config for block scripts, all-workspace + per-workspace overrides, as a Global; `buffer get NAME` headless mode (main.rs) backs the `buffer-get` helper the runner puts on PATH
prefs.rs       settings.json (appearance, name, ambient) as a Global; prefs::update saves + repaints
theme.rs       Appearance → kit Theme; loads themes/buffer.json (watched in debug)
workspace.rs   `Shell` (window root: bg, `components::aurora::Aurora` view, cached `Workspace` view, dialog scrim) · `Workspace`: TitleBar | custom sidebar | active page; owns TaskStore + FocusTimer
tasks/         model.rs (Task; legacy Subtask/Link/LogEntry only read to fold into notes via `fold_legacy`; checklist/link badges derived from notes; parse_quick_add, streak stats) · links.rs (URL → Jira/MR/… label) · store.rs (TaskStore entity)
pages/task_detail.rs  properties + the shared notes editor and `components::file_list` (same feature set as Notes); side panel / full page (`full_page`); Workspace sizes it (`prefs.detail_width`, `ResizePanel` drag)
focus/         FocusTimer entity (pomodoro, focus.json)
notes/         NoteStore entity (notes.json index + notes/<id>.md; first line = title via split/compose); pages/notes.rs = list + document panes, fills the content area itself (Workspace skips the max-width wrapper)
blocks/        programmatic blocks: ```` ```block <name> ```` fence → script in `blocks/<name>/` (workspace) → Markdown, cached in `.cache/blocks/`; `Blocks` global (reloaded in `Content::load`; `reload_if_changed` on window activation + Workspace's 15 s tick picks up on-disk edits), runs on a thread via login shell; table in `blocks/mod.rs`. ```` ```input ```` fences in output render as controls (`components/block_controls.rs`); they write values back through a `BlockHost` the editor passes to `hosted_markdown_view` (`BlockEditor::set_block_param`), and the editor's captured keys stand aside while one has focus (`editor/keys.rs` `guarded`)
editor/        Notion-style block editor used by notes and task notes (`load(md, image dir)`, `set_compact` for the side panel); one file per concern, table in `editor/mod.rs`. `model.rs` = Markdown ⇄ blocks (untouched blocks keep their source); owns its key bindings (`editor::init`); `rich/` = the in-place rich-text input (source Markdown is the model, markers hidden, `inline.rs` maps display ⇄ source offsets); blocks not being edited draw through the same styling (`rich_text_view`, Markdown renderer only for images) so opening one never restyles it
pages/         one Render view per sidebar page; Page enum lists them (`owns_panes`: Notes/Blocks fill the content area themselves). blocks.rs = custom block editor (explicit Save; every save bumps the type's revision). task_detail.rs = right panel owned by Workspace; pages `cx.emit(OpenTask(id))`, Workspace subscribes and opens it
components/    thin fn wrappers over kit components + icon Assets; fx.rs = glow/gradient/ring/animations; markdown.rs = `markdown_view` (always use it for Markdown: Mermaid + frontmatter + clickable-image plugins, image resolver); outline.rs = heading outline over a scrolled editor (notes, task notes); lightbox.rs = full-window image view (Global; Workspace renders it last and routes keys via `capture_key`)
files.rs      files in content are plain Markdown links (no index): copy/create/rename next to the note, `local_links` reads them back; editor/files.rs = `/upload` `/link` `/txt` + Finder drops; components/file_editor.rs = text-file dialog; local link clicks go through `markdown::open_link`
net.rs        HttpClient for GPUI (ureq on a thread); without it remote images never load
quotes.rs     daily sidebar line + time-of-day greeting
themes/       buffer.json: light + dark palettes (hot-reloaded in debug builds)
scripts/      icon generation, .app bundling, install; ui/ = window/mouse/recording helpers for checking the UI
bacon.toml    `make dev` rebuild/relaunch job
```

- **State**: one `Entity<TaskStore>` created in `Workspace::new`, cloned into pages. Pages `cx.observe(&store, …)` to re-render. All mutations go through `TaskStore` methods, which end in `commit()` (save + `cx.notify()`). Never mutate task data elsewhere.
- **New page**: steps in [Add a page](#add-a-page). New feature with its own data → own module like `tasks/` with its own JSON file via `storage::{load, save}`.
- **Imports**: `gpui_kit::*` is GPUI; components under `gpui_kit::component::*`; never add `gpui` as a direct dependency.
- **Components**: use kit ones (`Button`, `Checkbox`, `Input`, `TabBar`, `Tag`, `Sidebar`, `Icon`, `h_flex`/`v_flex`, tooltips…) before hand-rolling. Kit source: `~/.cargo/registry/src/*/gpui-component-0.9.1/src`.
- **Theme**: colors via `cx.theme()` (`gpui_kit::component::ActiveTheme`); our mapping: `group_box` = card/surface (translucent so the aurora shows), `secondary` = row hover, `secondary_active` = selected, `primary` = accent, `muted_foreground` = secondary text, `chart_1`–`chart_5` = brand gradient/glow/stat accents (`fx::brand_gradient`, `fx::wash`). Change the palette in `themes/buffer.json`, not in code. No hard-coded colors in views except data-driven ones (priority/category hex in `tasks/model.rs`). Don't mark our themes `is_default` (breaks hot reload).
- **Style**: translucent surfaces with a hairline `theme.border`; no drop shadows, only colored glows via `fx::glow` (kit `"shadow": false`); `rounded_2xl` cards, `rounded_xl` rows, pill (`ButtonRounded::Size(px(999.))`) buttons/chips.
- **Deletes**: never delete on one click — call `components::confirm::confirm_delete` (exception: blocks inside the editor, which ⌘Z restores) (kit `AlertDialog` via `WindowExt::open_alert_dialog`; user rejected inline "Delete?" pills). Dialog dim = Workspace's fading `dialog-scrim`; theme `overlay` stays transparent because the kit backdrop can't fade. Kit motion is tuned in `theme::tune_motion`.
- **Motion**: use `components::fx` (`enter` stagger, `ring`/`progress_bar` springs, `pulse`). No decorative looping motion on icons (user rejected bobbing icons as slop); repeating animations must be gated on `fx::ambient(cx)` (setting + reduce motion + window focused) and set a low `with_max_fps` (pulses 10) — each frame re-renders the animating view **and every ancestor view**, i.e. the whole `Workspace`. The backdrop escapes this only because `Aurora` is a sibling of the cached `Workspace` in `Shell`; keep it that way. `Workspace` is cached: state it reads must notify it (observe the entity) or `refresh_windows`, else it shows stale. `with_animation` state is keyed by element id: an id that disappears for a frame replays on return (used for page/row entrances).
- **Icons**: `Icon::new(gpui_kit::assets::IconName::X).text_color(c)` — never text glyphs. `components::Assets` serves `icon_assets!` extras, then the kit's ~100 component icons, then the full Lucide catalog (`AllAssets`, so block types can name any icon); still list app-used icons in `icon_assets!` + the test so they don't depend on the fallback.
- **Text entry**: `Entity<InputState>` (`InputState::new(window, cx).placeholder(..)`) rendered with `Input::new(&state)`. `cx.subscribe_in(&state, window, …)` for `InputEvent::PressEnter`; `value()`, `set_value(text, window, cx)`. Esc propagates as `component::input::Escape` — handle with `.on_action` on a parent.

## Persistence

- App dir: `storage::app_dir()` (`~/Library/Application Support/Buffer`, or `$BUFFER_DATA_DIR`; legacy `WORKDESK_DATA_DIR` still honoured) holds app-level config (`settings.json`, `workspaces.json`; `storage::{load_app, save_app}`) and is the default workspace's root. The app was **Workdesk** before 0.11: `storage::migrate_legacy_dir` renames the old folder on first launch, and `app_dir` falls back to it if that failed.
- Content dir: `storage::data_dir()` = active workspace root (`storage::set_root`, called only by `workspaces`). Content paths are always relative strings through `storage::{load, save, path, …}` — never build one from another base. Stores are rebuilt on switch (`workspace::Content`), so nothing may cache a root.
- Tasks are split: list fields in `tasks.json` (index, `DATA_VERSION` 2); details in `tasks/<id>/task.json` + `notes.md`. Detail fields on `Task` are `#[serde(default, skip_serializing)]` (read from v1 for migration, never written to the index). Save details with `commit_task`, notes with `commit_notes`, index with `commit`.
- Format changes must stay backward compatible: new fields get `#[serde(default)]`; never rename/remove fields without a migration in `TaskData`. Old user files must still load.
- `storage::load` quarantines unparseable files; don't replace that with a silent default.

## Add a page

1. Create `src/pages/<name>.rs` with a struct implementing `Render`.
2. Add a variant to `Page` (+ `ALL`, `label`, `icon`) in `src/pages/mod.rs`.
3. Hold an `Entity<YourPage>` in `Workspace` and map it in `active_view` / `navigate`.
4. Optional: add a `Show<Name>` action + key binding in `src/actions.rs`.
