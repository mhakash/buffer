# Docs

| File | Audience | Holds |
|---|---|---|
| [`../README.md`](../README.md) | Visitors on GitHub | What Buffer is, feature list, install, quick dev commands, links here |
| [`guide/`](guide/) | Users | How each feature behaves: inputs, keys, settings, files on disk |
| [`development.md`](development.md) | Builders | Commands, installing, themes, references |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Contributors | Module map, conventions, persistence rules, recipes (Add a page) |
| [`../AGENTS.md`](../AGENTS.md) | Coding agents | How to change code safely: done criteria, GPUI gotchas, UI checks |

## Where a change goes

| You changed | Update |
|---|---|
| A new feature or a big change to one | One bullet in README **Features** (under its area) + details in the matching `guide/` page |
| Behaviour, a menu, a setting, a slash command | The matching `guide/` page only |
| A keyboard shortcut | `guide/shortcuts.md` (app-wide) or the page's own key table (editor keys → `guide/notes.md`) |
| A data file or its format | `guide/data.md` (what it holds) + `ARCHITECTURE.md` → Persistence (rules for code) |
| A build/install step or a `make` target | `development.md`; README **Install** / **Development** only if the everyday commands change |
| A module, folder or convention | `ARCHITECTURE.md` |
| A GPUI/kit pitfall | `AGENTS.md` → gotchas |
| A whole new area (a page) | New `guide/<area>.md` + a row in README **Documentation** and in this file's table |

## Writing rules

- **README stays short.** Intro, features as one-line bullets, install, a few commands, links. No
  tables of keys, file formats or edge cases there: those go in `guide/`.
- **Each fact lives once.** Link to it (`[Notes](guide/notes.md#block-editor)`) instead of
  repeating it. When something changes, fix it where it lives and delete stale copies.
- **Guide pages describe behaviour, not code.** Say what the user sees and does; module and
  type names belong in `ARCHITECTURE.md`.
- **Lead with the common case.** A short paragraph, then a table for keys, inputs or options
  (two or three columns), then edge cases and migrations last.
- **Name UI exactly as it appears**, in bold (**Allow…**, Settings → Workspaces); keys as
  symbols (⌘⇧F); files, fields and code in backticks.
- **Relative links only**, so they work on GitHub and in an editor. Anchors are the heading in
  lowercase with spaces as `-` (`#secrets--config` for "Secrets & config").
- Check links after moving text: every `](…md…)` target must exist.
