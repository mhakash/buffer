# Buffer

Personal work app (formerly **Workdesk**; data and install migrate automatically).

Desktop app for custom work tooling, built on [GPUI](https://gpui.rs/) (Zed's UI framework) via
[GPUI Kit](https://gpui-kit.com) (GPUI + ready-made components, Lucide icons, JSON themes).
Aurora-styled UI (drifting glow, springy nav, animated progress) with a task list, a daily
dashboard (greeting, streak, 7-day chart, progress ring), standalone Markdown notes and a pomodoro
Focus timer.

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

`~/Library/Application Support/Buffer/` (Settings → Data → Show in Finder). A pre-rename
`Workdesk` folder is moved there on first launch.

| File | Contents |
|---|---|
| `tasks.json` | Index: task titles, priority, order, categories, dates + the category list |
| `tasks/<id>/task.json` | That task's subtasks, links, work log |
| `tasks/<id>/notes.md` | That task's notes (plain Markdown; edit it in any editor while the app is closed) |
| `notes.json` | Notes index: ids, created/edited times, tags, properties |
| `notes/<id>.md` | One standalone note (plain Markdown) |
| `settings.json` | Theme, your name (greeting), ambient motion on/off. |
| `focus.json` | Focus length + log of finished focus sessions. |

Every file is written atomically; editing a task rewrites only its own folder (notes: only `notes.md`).
Pre-0.7 data (everything inline in `tasks.json`) is split into folders on first launch, after copying
the original to `tasks.v1-backup.json`.

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
opens the task as a full page (Notion-style); ← Tasks or × closes it. Esc steps back: notes editor →
side panel → closed. The folder button reveals the task's files in Finder.

| Section | What it holds |
|---|---|
| Title | Done checkbox + inline-editable title; "Edited …" time in the toolbar |
| Properties | Status, priority (click to cycle), categories (+ menu toggles any number), due date (overdue shows red), created |
| Notes | Markdown, rendered: headings, lists, tables, code blocks, links. Edit/Preview toggle; ticking a `- [ ]` box in the preview updates the source |
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
(Enter jumps to the body); the body is rendered Markdown — Edit, or click the space below it, to
write; Esc closes a property field, then the editor, then full view. The panel button hides the
list (full view). In the file the title is the first line (`# Title`). A note left empty (no
text, tags or properties) is discarded when you leave it; the trash button deletes (with
confirmation).

Properties under the title: Created, Edited, Tags (the task categories; + toggles), and any
number of your own text properties ("Add a property" → name, Enter → value). Click a value to
edit it; URL values get an open button; hover a row to remove it.

## Markdown extras

Task notes and notes render the same Markdown, plus:

| Syntax | Renders as |
|---|---|
| ```` ```mermaid ```` fence | Diagram (flowchart, sequence, class, state, ER, gantt, pie, …), rendered natively in the app's palette; invalid syntax shows the error and source |
| Leading `---` YAML block | Property rows (lists as pills) |
| `![](https://…)` | Remote image (fetched by the app) |
| `![](file.png)`, `~/…`, `/abs/…`, `file://…` | Local image; relative paths resolve against the note's folder (`notes/` or `tasks/<id>/`) |

Click a diagram, or an image on its own line, to open it full-window; click or Esc closes.

## Look

Palette lives in `themes/buffer.json`; `chart.1`–`chart.5` drive the brand gradient, glows and
the ambient backdrop. Settings → Appearance → Ambient motion turns off the drifting glow and
pulsing indicators (saves battery). macOS "Reduce motion" is honoured by every animation.

## Shortcuts

| Keys | Action |
|---|---|
| ⌘N / ⌘⇧N | New task (focus composer) / new note |
| ⌘1 / ⌘2 / ⌘3 / ⌘, | Tasks / Focus / Notes / Settings |
| ⌘⇧F | Start/pause focus timer |
| Enter / Esc | Add or save / cancel edit |
| ⌘⇧T | Toggle light/dark |
| ⌘W / ⌘Q | Close window (app stays in Dock) / Quit |

## Layout

| Path | Purpose |
|---|---|
| `src/main.rs` | App entry, window, Dock reopen |
| `src/actions.rs` | Actions, key bindings, menu bar |
| `src/storage.rs` | JSON load/save in the data dir |
| `src/prefs.rs` | `settings.json` (appearance, name, ambient) as a global |
| `src/theme.rs` | Light/dark appearance over gpui-kit's `Theme`; loads `themes/` |
| `src/focus/` | `FocusTimer` entity (pomodoro state, `focus.json`) |
| `src/notes/` | `NoteStore` entity (`notes.json` + `notes/<id>.md`) |
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
