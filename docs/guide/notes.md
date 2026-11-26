# Notes

Standalone Markdown notes for anything that isn't a task (⌘3; ⌘⇧N creates one from anywhere),
laid out like Notion across the whole content area: a list pane (newest edit first, search
filters by content and property values) and the open note as a document. The title is always
editable (Enter jumps to the body). The body is a Notion-style [block editor](#block-editor);
the Markdown / Rich text button switches to editing the raw source. Esc closes a property field,
then the block being edited, then full view. The panel button hides the list (full view). In the
file the title is the first line (`# Title`). A note left empty (no text, tags or properties) is
discarded when you leave it; the trash button deletes (with confirmation).

Properties under the title: Created, Edited, Tags (the task categories; + toggles), and any
number of your own text properties ("Add a property" → name, Enter → value). Click a value to
edit it; URL values get an open button; hover a row to remove it.

A note with headings shows an outline at the document's top right: a dash per heading (longer
for higher levels), the section being read highlighted. Hover it to list the titles; click one
to scroll there.

## Block editor

Notes and task notes share one editor. Every paragraph, heading, list item, to-do, quote, code
fence, table and divider is a block. Blocks show formatted; click one to edit it in place,
formatting and all: Markdown markers stay hidden (the link address shows while the caret is in a
link; Enter there finishes it). Pressing on text puts the caret there and dragging selects it,
like Notion; ⌘C copies plain text (formatting kept when pasted back into a note). A click on a
link opens it (⌘-click while editing that block). Untouched blocks are saved byte for byte, so
the file stays yours.

| Input | Effect |
|---|---|
| `/` in an empty block | Block menu with its own search field (type to filter, ↑↓/Tab Enter; Esc, a space or no match puts `/…` back as text; Backspace on an empty search drops the `/`); also the + handle on hover |
| `# ` `## ` `### ` · `- ` · `1. ` · `[] ` · `> ` · ```` ``` ```` · `---` | Turn the block into heading · bullet · numbered · to-do · quote · code · divider |
| Enter / ⇧Enter | New block (lists continue; Enter on an empty item leaves the list) / line break |
| Backspace at start | Turn into text, then join with the block above |
| ↑ ↓ ← → at an edge · Tab / ⇧Tab | Move between blocks · nest / un-nest list items |
| ⌘B ⌘I ⌘E ⌘⇧X ⌘K | Bold, italic, code, strikethrough, link |
| ⌘Enter in code | Leave the code block |
| Esc while editing · ⇧↑/⇧↓ at a block's edge | Select the block (then ↑↓ move, ⇧↑↓ extend, Enter edits, Esc leaves) |
| Drag inside a block (not being edited) | Select its text as shown, ⌘C copies it; a plain click edits the block |
| Drag from one block onto another · shift-click · ⌘A twice | Select a run of blocks |
| ⌘C ⌘X ⌘V on selected blocks | Copy / cut as Markdown, paste after; Markdown with several blocks pasted into a block splits it |
| Backspace · ⌘D · ⌘⇧↑/↓ · Tab | Delete · duplicate · move up/down · nest (selected blocks, or the block being edited) |
| ⌘Z / ⌘⇧Z | Undo / redo (typing in the open block first, then block changes) |
| Drag ⋮⋮ | Move the block (or the whole selection); a line shows where it lands |
| Click ⋮⋮ or right-click a block | Turn into, copy, duplicate, move up/down, delete |
| Table (`/table`, or any GFM table) | Click a cell to edit it; Tab / ⇧Tab and ←→ at an edge walk cells (Tab past the last adds a row); ↑↓ change rows; Enter goes down (adds a row at the bottom); ⌘Enter leaves. Hover + bars add a row / column; right-click a cell to insert or delete rows and columns. Saved as a padded GFM table, column alignment kept |

## Files

Files are plain links in the content. In the block editor:

| Input | Effect |
|---|---|
| `/upload` (Upload file) | Pick files; each is copied next to the note and linked on its own line |
| `/link` (Link file) | Pick files; linked where they are (nothing copied) |
| `/log` (Log entry) | Bullet starting with the current time (`**Nov 20, 14:05** — `), for a work log |
| A line holding only a URL | Labelled when you leave it: Jira → `[PAY-88](…)`, GitLab MR → `!412`, issues → `#7`, GitHub PRs, Confluence pages |
| `/txt` (Text file) | Makes `untitled.txt`, links it and opens it with the name selected: type a name (`body.json`), Enter, paste the content |
| Drag files from Finder | Copied and linked where they're dropped (a line shows where) |

Copies go in `notes/<id>/` (task notes: `tasks/<id>/files/`) and are linked relatively,
`[spec.pdf](<3/spec.pdf>)`; images are embedded (`![…](…)`). Click a link to open it: text
files (≤ 4 MB) open in an editor dialog (line numbers, saved as you type, copy contents, show in
Finder); anything else opens in its default app. Any local link works the same, written by hand
too (relative to the note's folder, `~/…`, absolute, `file://`).

The end of a note lists every file it links to (size, **Linked**, or **Missing** if gone); click
to open, hover to show in Finder. Renaming a new text file in its dialog rewrites its link.
Deleting a link leaves the copied file on disk; deleting a note deletes its folder.

## Markdown extras

Task notes and notes render the same Markdown, plus:

| Syntax | Renders as |
|---|---|
| ```` ```mermaid ```` fence | Diagram (flowchart, sequence, class, state, ER, gantt, pie, …), rendered natively with Mermaid's own layouts (as on GitHub), in the app's palette with one brand color per node; invalid syntax shows the error and source |
| ```` ```block <name> ```` fence | A [custom block](custom-blocks.md): your script's output |
| Leading `---` YAML block | Property rows (lists as pills) |
| `![](https://…)` | Remote image (fetched by the app) |
| `![](file.png)`, `~/…`, `/abs/…`, `file://…` | Local image; relative paths resolve against the note's folder (`notes/` or `tasks/<id>/`) |

Click a diagram, or an image on its own line, to open it full-window; click or Esc closes.
