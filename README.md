# Buffer

Personal work app (formerly **Workdesk**; data and install migrate automatically).

Desktop app for custom work tooling, built on [GPUI](https://gpui.rs/) (Zed's UI framework) via
[GPUI Kit](https://gpui-kit.com) (GPUI + ready-made components, Lucide icons, JSON themes).
Aurora-styled UI (drifting glow, springy nav, animated progress) with a task list, a daily
dashboard (greeting, streak, 7-day chart, progress ring), standalone Markdown notes with custom
script-driven blocks and a pomodoro Focus timer.

## Install / update

```sh
make install     # builds release, installs ~/Applications/Buffer.app, relaunches it
```

Re-run `make install` after any code change to update. The Dock pin and your data survive updates.
The rename from Workdesk is the exception: the old app is removed, so pin Buffer once.
Pin once: right-click the Dock icon → Options → Keep in Dock.

Always installs to `~/Applications` (no admin needed) and removes any old `/Applications/Buffer.app` and pre-rename `Workdesk.app`,
so there's only ever one copy (two copies = two Dock icons).

## Develop

```sh
make dev         # rebuild + relaunch on every save, throwaway data in /tmp/buffer-dev (brew install bacon)
make run         # cargo run (debug, your real data)
make test        # unit tests
make icon        # regenerate assets/AppIcon.icns from scripts/make_icon.swift
BUFFER_DATA_DIR=/tmp/wd cargo run   # use throwaway data
```

Colors, radius and fonts live in `themes/buffer.json` (gpui-kit theme format). In debug builds the
running app reloads them on save, no rebuild needed; release builds embed the file.

macOS needs only Xcode Command Line Tools: gpui-kit enables GPUI's `runtime_shaders`, which compiles Metal shaders at startup instead of requiring the full Xcode `metal` compiler.

Bump `version` in `Cargo.toml` with each feature; it shows in the status bar and the app's Info.plist.

## Data

### Workspaces

A workspace is a root folder holding all content (tasks, notes, focus history, images). Switch with
the brand block at the top of the sidebar (click → pick one, **New workspace…**, **Open folder…**)
or in Settings → Workspaces (click a row to switch; edit the active one's name in place; hover a
row to reveal it in Finder or remove it from the list — its files stay on disk). Switching reloads
every page; a running focus timer is dropped.

| Location | Contents |
|---|---|
| App dir: `~/Library/Application Support/Buffer/` (or `$BUFFER_DATA_DIR`) | `settings.json` + `workspaces.json` + `vault.json` (secrets & config, mode 0600) + `bin/buffer-get` (app-level, shared by all workspaces); also the root of the default workspace (**Personal**), so pre-0.16 data loads unchanged. A pre-rename `Workdesk` folder is moved here on first launch. |
| `<app dir>/Workspaces/<name>/` | Root of a workspace made with **New workspace** |
| any folder | Root of a workspace added with **Open folder…** (e.g. a synced folder) |

`workspaces.json`: `{"active": <id>, "workspaces": [{"id", "name", "path", "blocks_trusted"}]}`; no `path` =
the app dir; `blocks_trusted` (omitted = false) lets custom blocks run their scripts.
A missing file means just the default workspace.

### Files in a workspace root

Settings → Data → Show in Finder opens the active one.

| File | Contents |
|---|---|
| `tasks.json` | Index: task titles, priority, order, categories, dates + the category list |
| `tasks/<id>/task.json` | That task's subtasks, links, work log |
| `tasks/<id>/notes.md` | That task's notes (plain Markdown; edit it in any editor while the app is closed) |
| `notes.json` | Notes index: ids, created/edited times, tags, properties |
| `notes/<id>.md` | One standalone note (plain Markdown) |
| `focus.json` | Focus length + log of finished focus sessions. |
| `blocks/<name>/block.json` + script | A custom block type (see Custom blocks) |
| `.cache/blocks/<key>.json` | Last output of each custom block (safe to delete) |

Every file is written atomically; editing a task rewrites only its own folder (notes: only `notes.md`).
Pre-0.7 data (everything inline in `tasks.json`) is split into folders on first launch, after copying
the original to `tasks.v1-backup.json`. App-level: `settings.json` = theme, your name (greeting),
ambient motion on/off, detail-panel width.

An unreadable file is renamed `*.corrupt-<timestamp>.json`, never overwritten. When changing the
format, add fields with `#[serde(default)]` (`src/tasks/model.rs`) so old files still load.

## Tasks

Type in the composer and press Enter. Inline tokens (anywhere in the text):

| Token | Effect |
|---|---|
| `#name` | Category, repeatable (`#work #review`); matched loosely (`#follow-up` = "Follow-up"), created if new |
| `!urgent` `!high` `!medium` `!low` (or `!1`–`!4`) | Priority |
| `https://…` | Attached as a link (Jira/GitLab/GitHub recognised); a link alone becomes the title, e.g. `PAY-88` |
| `\#` `\!` `\\` | Literal `#`, `!`, or `\` in the title (prefix the word, e.g. `\#channel`) |

Without tokens, the priority and category pills at the end of the input apply (each opens a menu; categories toggle, pick several). Hover a task for actions:
drag the handle to reorder within a priority group (preview while dragging; saved on release), cycle priority, edit, delete (Undo pill stays 8 s).
Every delete/remove ✕ (tasks, subtasks, links, log entries, categories) opens a confirmation dialog first.
Tick the checkbox to complete (row slides out, a toast cheers you on); Done groups by day.
The hero card above shows today's progress ring, your streak (consecutive days with a completion)
and the last 7 days.

## Task details

Click a task's title to open it in a side panel. Drag the panel's left edge to resize it (width is
remembered). Wide windows dock it beside the list; narrower ones float it over the page. The ⤢ button
opens the task as a full page (Notion-style); ← Tasks or × closes it. Esc steps back: notes block or source →
side panel → closed. The folder button reveals the task's files in Finder.

| Section | What it holds |
|---|---|
| Title | Done checkbox + inline-editable title; "Edited …" time in the toolbar |
| Properties | Status, priority (click to cycle), categories (+ menu toggles any number), due date (overdue shows red), created |
| Notes | Same block editor as the Notes page (see Block editor), compact in the side panel; Markdown / Rich text toggles the source |
| Subtasks | Checklist with progress bar; Enter adds |
| Links | Paste URLs, Enter adds. Jira → `PAY-88`, GitLab MR → `!412`, issues → `#7`, GitHub PRs, Confluence pages; click opens in browser |
| Work log | Timestamped entries (newest first) for what you did |

List rows show the due date and badges for subtasks (`2/5`), links (label or count), notes and log
entries; more than two categories collapse into `+n`.
Focus → "Up next" opens the same panel.

## Focus

Pomodoro timer: pick 15/25/45/60 min, Start (⌘⇧F from anywhere). Finished focus sessions are
logged and followed by a 5-min break (15 min every 4th). A chime plays at the end of each phase.
The sidebar shows the countdown while a session is active. "Up next" = your top open task.

## Notes

Standalone Markdown notes for anything that isn't a task (⌘3; ⌘⇧N creates one from anywhere),
laid out like Notion across the whole content area: a list pane (newest edit first, search filters
by content and property values) and the open note as a document. The title is always editable
(Enter jumps to the body). The body is a Notion-style block editor (below); the Markdown /
Rich text button switches to editing the raw source. Esc closes a property field, then the block
being edited, then full view. The panel button hides the
list (full view). In the file the title is the first line (`# Title`). A note left empty (no
text, tags or properties) is discarded when you leave it; the trash button deletes (with
confirmation).

Properties under the title: Created, Edited, Tags (the task categories; + toggles), and any
number of your own text properties ("Add a property" → name, Enter → value). Click a value to
edit it; URL values get an open button; hover a row to remove it.

### Block editor

Notes and task notes share one editor. Every paragraph, heading, list item, to-do, quote, code
fence and divider is a block. Blocks show
formatted; click one to edit it (its inline Markdown — `**bold**`, links — shows while editing).
Untouched blocks are saved byte for byte, so the file stays yours.

| Input | Effect |
|---|---|
| `/` in an empty block | Block menu (type to filter, ↑↓ Enter); also the + handle on hover |
| `# ` `## ` `### ` · `- ` · `1. ` · `[] ` · `> ` · ```` ``` ```` · `---` | Turn the block into heading · bullet · numbered · to-do · quote · code · divider |
| Enter / ⇧Enter | New block (lists continue; Enter on an empty item leaves the list) / line break |
| Backspace at start | Turn into text, then join with the block above |
| ↑ ↓ ← → at an edge · Tab / ⇧Tab | Move between blocks · nest / un-nest list items |
| ⌘B ⌘I ⌘E ⌘⇧X ⌘K | Bold, italic, code, strikethrough, link |
| ⌘Enter in code | Leave the code block |
| Esc while editing · ⇧↑/⇧↓ at a block's edge | Select the block (then ↑↓ move, ⇧↑↓ extend, Enter edits, Esc leaves) |
| Drag from one block onto another · shift-click · ⌘A twice | Select a run of blocks |
| ⌘C ⌘X ⌘V on selected blocks | Copy / cut as Markdown, paste after; Markdown with several blocks pasted into a block splits it |
| Backspace · ⌘D · ⌘⇧↑/↓ · Tab | Delete · duplicate · move up/down · nest (selected blocks, or the block being edited) |
| ⌘Z / ⌘⇧Z | Undo / redo (typing in the open block first, then block changes) |
| Drag ⋮⋮ | Move the block (or the whole selection); a line shows where it lands |
| Click ⋮⋮ or right-click a block | Turn into, copy, duplicate, move up/down, delete |

## Markdown extras

Task notes and notes render the same Markdown, plus:

| Syntax | Renders as |
|---|---|
| ```` ```mermaid ```` fence | Diagram (flowchart, sequence, class, state, ER, gantt, pie, …), rendered natively in the app's palette; invalid syntax shows the error and source |
| Leading `---` YAML block | Property rows (lists as pills) |
| `![](https://…)` | Remote image (fetched by the app) |
| `![](file.png)`, `~/…`, `/abs/…`, `file://…` | Local image; relative paths resolve against the note's folder (`notes/` or `tasks/<id>/`) |

Click a diagram, or an image on its own line, to open it full-window; click or Esc closes.

## Custom blocks

A custom block runs a script and shows the Markdown it prints. Make one on the **Blocks** page (⌘4):
+ starts a block with a Python starter script; fill in the form, Save. Use it in a note by typing `/`
and picking it under Custom, or by writing a fence:

````markdown
```block report
since: 7d
limit: 5
```
````

| Part | Behaviour |
|---|---|
| Config | `key: value` lines in the fence; params not given use their defaults. Editing the block shows the params it takes |
| Script | Runs in `blocks/<name>/` through your login shell (Homebrew tools work). Gets params as JSON on stdin and as `BLOCK_<NAME>` env vars; stdout = the Markdown; non-zero exit = failed (stderr shown, last output kept) |
| Environment | Per block type. Values can be literal, `secret:NAME` / `config:NAME` (from Secrets & config, below), or `keychain:<service>` (login keychain, `security add-generic-password -s <service> -a $USER -w`). A reference that isn't set fails the run with a message |
| `buffer-get NAME` | On every block script's PATH: prints NAME from Secrets & config for the block's workspace (exit 1 if not set), so scripts can read values they don't declare |
| Refresh | Output is cached; a block older than its **Fresh for** seconds re-runs when shown (0 = never); a failed run retries after 30 s. **Timeout** kills the script and anything it started |
| Look | Output renders flat, in the note's own text style; a muted line under it (block name, last update, ↻, ✎ Edit) shows on hover, or always while running, failed or off |
| Clicks | In the output, clicks select text, copy and follow links as usual; **Edit** on that line, or **Edit config** in the block's ⋮⋮ menu, opens the block's config |
| Trust | Off per workspace until you click **Allow…** on a block (or Settings → Workspaces → Run block scripts). Test runs on the Blocks page always run |
| Saving | Explicit (Save). Every save re-runs that type's blocks; editing the script outside the app doesn't, so use ↻ |
| On disk | Block folders added, removed or whose `block.json` changed outside the app are picked up when you switch back to Buffer, or within 15 s — no restart |

### Secrets & config

Settings → Secrets & config holds named values for block scripts: **All workspaces**, plus per-workspace
overrides (**Only "<workspace>"**; an override wins). Mark a value **Secret** to keep it out of the UI
(shown as dots; edit = replace). Everything lives in `vault.json` in the app dir, readable only by you;
it's never written into a workspace folder. Shape:
`{"global": [{"name", "value", "secret"}], "workspaces": {"<id>": [ … ]}}`. Outside a block,
`BUFFER_WORKSPACE_ID=<id> Buffer.app/Contents/MacOS/buffer get NAME` does the same lookup.

## Look

Palette lives in `themes/buffer.json`; `chart.1`–`chart.5` drive the brand gradient, glows and
the ambient backdrop. Settings → Appearance → Ambient motion turns off the drifting glow and
pulsing indicators (saves battery). macOS "Reduce motion" is honoured by every animation.

## Shortcuts

| Keys | Action |
|---|---|
| ⌘N / ⌘⇧N | New task (focus composer) / new note |
| ⌘1 / ⌘2 / ⌘3 / ⌘4 / ⌘, | Tasks / Focus / Notes / Blocks / Settings |
| ⌘⇧F | Start/pause focus timer |
| Enter / Esc | Add or save / cancel edit |
| ⌘⇧T | Toggle light/dark |
| ⌘W / ⌘Q | Close window (app stays in Dock) / Quit |

## Layout

| Path | Purpose |
|---|---|
| `src/main.rs` | App entry, window, Dock reopen |
| `src/actions.rs` | Actions, key bindings, menu bar |
| `src/storage.rs` | JSON load/save; the only place paths are resolved: `data_dir()` = active workspace root (content), `app_dir()` = app config |
| `src/workspaces.rs` | Workspace registry (`workspaces.json`) as a global: create, open folder, switch, rename, remove |
| `src/prefs.rs` | `settings.json` (appearance, name, ambient) as a global |
| `src/theme.rs` | Light/dark appearance over gpui-kit's `Theme`; loads `themes/` |
| `src/focus/` | `FocusTimer` entity (pomodoro state, `focus.json`) |
| `src/notes/` | `NoteStore` entity (`notes.json` + `notes/<id>.md`) |
| `src/editor/` | Block editor for notes and task notes; `mod.rs` lists its files |
| `src/blocks/` | Custom blocks: model + fence config, store, script runner, cache/trust, starter script; `mod.rs` lists its files |
| `src/net.rs` | HTTP client GPUI uses for remote images |
| `src/quotes.rs` | Daily sidebar line + time-of-day greeting |
| `themes/buffer.json` | Light + dark palettes (hot-reloaded in debug builds) |
| `src/workspace.rs` | Root view: aurora backdrop, title bar clock, sidebar, page transitions |
| `src/tasks/` | Task model, quick-add parser, link classifier (`links.rs`), `TaskStore` entity (index + per-task folders, v1 migration) |
| `src/pages/` | One file per page; `Page` enum drives the sidebar |
| `src/components/` | Thin wrappers over gpui-kit (`chip`, `icon_button`, `category_tag`, `card`), icon `Assets`, `markdown_view` (Mermaid, frontmatter, image paths), `lightbox` (full-window image view), `property_row`, `fx` (glow, gradients, ring, enter/pulse animations) |
| `bacon.toml` | `make dev` rebuild/relaunch job |
| `scripts/` | Icon generation, `.app` bundling, install; `ui/` = window/mouse/recording helpers for checking the UI |

## Add a page

1. Create `src/pages/<name>.rs` with a struct implementing `Render`.
2. Add a variant to `Page` (+ `ALL`, `label`, `icon`) in `src/pages/mod.rs`.
3. Hold an `Entity<YourPage>` in `Workspace` and map it in `active_view` / `navigate`.
4. Optional: add a `Show<Name>` action + key binding in `src/actions.rs`.

## References

- [GPUI Kit docs](https://gpui-kit.com) · [repo](https://github.com/longbridge/gpui-kit)
- [GPUI README](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [examples](https://github.com/zed-industries/zed/tree/main/crates/gpui/examples)
- [Lucide icons](https://lucide.dev/icons) (names map to `gpui_kit::assets::IconName`)
