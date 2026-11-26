# Tasks

## Quick add

Type in the composer and press Enter. Inline tokens (anywhere in the text):

| Token | Effect |
|---|---|
| `#name` | Category, repeatable (`#work #review`); matched loosely (`#follow-up` = "Follow-up"), created if new |
| `!urgent` `!high` `!medium` `!low` (or `!1`–`!4`) | Priority |
| `https://…` | Added to the task's notes as a labelled link (Jira/GitLab/GitHub recognised); a link alone becomes the title, e.g. `PAY-88` |
| `\#` `\!` `\\` | Literal `#`, `!`, or `\` in the title (prefix the word, e.g. `\#channel`) |

Without tokens, the priority and category pills at the end of the input apply (each opens a
menu; categories toggle, pick several).

## The list

Hover a task for actions: drag the handle to reorder within a priority group (preview while
dragging; saved on release), status and priority menus, edit, delete (Undo pill stays 8 s).
Every delete/remove ✕ (tasks, categories, properties) opens a confirmation dialog first.
Tick the checkbox to complete (row slides out, a toast cheers you on); Done groups by day.

Tabs: **To do**, **Follow up** (tasks waiting on someone; set via the status menu,
longest-waiting first, row shows how long; tasks added on this tab start there), **Done**.
The hero card above shows today's progress ring, your streak (consecutive days with a
completion) and the last 7 days.

List rows show the due date and badges read from the notes: checklist progress (`2/5`), links
(label or count) and a notes icon; more than two categories collapse into `+n`.

## Task details

Click a task's title to open it in a side panel. Drag the panel's left edge to resize it (width
is remembered). Wide windows dock it beside the list; narrower ones float it over the page. The
⤢ button opens the task as a full page (Notion-style); ← Tasks or × closes it. Esc steps back:
notes block or source → side panel → closed. The folder button reveals the task's files in
Finder.

| Section | What it holds |
|---|---|
| Title | Done checkbox + inline-editable title; "Edited …" time in the toolbar |
| Properties | Status (To do / Follow up / Done) and priority as menus, categories (+ menu toggles any number), due date (overdue shows red), created |
| Notes | Same block editor, outline and file list as the Notes page (see [Notes](notes.md)), compact in the side panel; Markdown / Rich text toggles the source |

Subtasks, links and the work log are part of the notes: a checklist (`/todo`), pasted links and
log lines (`/log`), available in any note. Tasks from before 0.27 had them as separate lists; on
first launch each task's lists are appended to its notes (`## Subtasks`, `## Links`,
`## Work log`, newest entry first) and the old `task.json` is kept as `task.v2-backup.json`.
Links typed in quick add land in the notes too.

## Focus

Pomodoro timer: pick 15/25/45/60 min, Start (⌘⇧F from anywhere). Finished focus sessions are
logged and followed by a 5-min break (15 min every 4th). A chime plays at the end of each phase.
The sidebar shows the countdown while a session is active. "Up next" = your top open task;
clicking it opens the same task panel.
