# Workspaces & data

## Workspaces

A workspace is a root folder holding all content (tasks, notes, focus history, images). Switch
with the brand block at the top of the sidebar (click → pick one, **New workspace…**, **Open
folder…**) or in Settings → Workspaces (click a row to switch; edit the active one's name in
place; hover a row to reveal it in Finder or remove it from the list — its files stay on disk).
Switching reloads every page; a running focus timer is dropped.

| Location | Contents |
|---|---|
| App dir: `~/Library/Application Support/Buffer/` (or `$BUFFER_DATA_DIR`) | `settings.json` + `workspaces.json` + `vault.json` (secrets & config, mode 0600) + `bin/buffer-get` (app-level, shared by all workspaces); also the root of the default workspace (**Personal**), so pre-0.16 data loads unchanged. A pre-rename `Workdesk` folder is moved here on first launch. |
| `<app dir>/Workspaces/<name>/` | Root of a workspace made with **New workspace** |
| any folder | Root of a workspace added with **Open folder…** (e.g. a synced folder) |

`workspaces.json`: `{"active": <id>, "workspaces": [{"id", "name", "path", "blocks_trusted"}]}`;
no `path` = the app dir; `blocks_trusted` (omitted = false) lets custom blocks run their scripts.
A missing file means just the default workspace.

App-level `settings.json` = theme, your name (greeting), ambient motion on/off, detail-panel
width.

## Files in a workspace root

Settings → Data → Show in Finder opens the active one.

| File | Contents |
|---|---|
| `tasks.json` | Index: task titles, priority, order, categories, dates (`follow_up_at` while waiting for a follow-up) + the category list |
| `tasks/<id>/task.json` | Reserved for task details (empty since 0.27; subtasks, links and log live in `notes.md`) |
| `tasks/<id>/notes.md` | That task's notes (plain Markdown; edit it in any editor while the app is closed) |
| `tasks/<id>/files/` | Files uploaded to that task's notes or made in them |
| `notes.json` | Notes index: ids, created/edited times, tags, properties |
| `notes/<id>.md` | One standalone note (plain Markdown) |
| `notes/<id>/` | Files uploaded to that note or made in it (linked from its content) |
| `focus.json` | Focus length + log of finished focus sessions |
| `blocks/<name>/block.json` + script | A [custom block](custom-blocks.md) type |
| `.cache/blocks/<key>.json` | Last output of each custom block (safe to delete) |

## Safety and migrations

Every file is written atomically; editing a task rewrites only its own folder (notes: only
`notes.md`). An unreadable file is renamed `*.corrupt-<timestamp>.json`, never overwritten.

Pre-0.7 data (everything inline in `tasks.json`) is split into folders on first launch, after
copying the original to `tasks.v1-backup.json`. The app was called **Workdesk** before 0.11; its
data folder and install migrate automatically.
