# AGENTS.md — Buffer

Personal macOS desktop app (Rust + [GPUI Kit](https://gpui-kit.com) `=0.9.1`, which re-exports its pinned GPUI fork `gpui-pre 0.3.12`). User-facing features, shortcuts, data files → [README.md](README.md). This file: how to change the code safely.

## Commands

| Task | Command |
|---|---|
| Build / check | `cargo build` (first build ~3 min; incremental ~30 s) |
| Tests | `cargo test` |
| Format | `cargo fmt` (run before committing) |
| Run dev, throwaway data | `BUFFER_DATA_DIR=/tmp/wd cargo run` |
| Dev loop (rebuild + relaunch on save) | `make dev` (`bacon.toml`, data in `/tmp/buffer-dev`); theme JSON edits apply live without restart |
| Ship to user | `make install` → rebuilds release, replaces `~/Applications/Buffer.app` (deletes stale `/Applications` and pre-rename `Workdesk.app` copies), relaunches |
| Regenerate icon | `make icon` (edits `scripts/make_icon.swift` first) |

Done = `cargo fmt` + `cargo build` with **zero warnings** + `cargo test` green. Ignore the `block`/`proc-macro-error2` future-incompat note (upstream).

## Delivering a feature

1. Bump `version` in `Cargo.toml` (minor for features, patch for fixes) — shown in status bar + Info.plist; the user checks it to confirm an update.
2. Update [README.md](README.md) if user-visible behaviour, shortcuts, or data format changed.
3. `make install` only when the user wants the installed app updated (it quits and relaunches their running app).
4. Commit only after the user confirms; on `main`, conventional style (`feat:`, `fix:`, `chore:`, `docs:`). Repo is local-only (no remote).

## Code map

Module map, state/theme/style/motion conventions and persistence rules → [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Read it before changing structure, data formats or UI.

- Never touch the user's real data dir in tests or manual checks — always set `BUFFER_DATA_DIR`.

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
| Big text in `Input`/`Textarea` ignores parent `text_*` | kit pins height/size/padding; override on the input (`.h() .text_size() .px_0()`); Textarea padding comes from `with_size` (see `pages/notes.rs`) |
| `on_reopen` isn't on `App` | call on `Application` before `.run()` (see `main.rs`) |
| Closure in `.map()` needs `cx` → "captured variable cannot escape `FnMut`" | take `cx: &Context<Self>` (not `&mut`) in render helpers; `cx.listener` only needs `&` |
| Borrow conflict reading theme then calling `cx.listener` | `let theme = cx.theme().clone();` at top of render |
| Store read + later `cx` use | copy data out of `self.store.read(cx)` first (`to_vec()`, `cloned()`) |
| Intercept keys a focused `Textarea` would handle (Enter, Backspace, arrows, Tab) | `.capture_action` on an ancestor + `cx.stop_propagation()`; caret display row via `state.display_map().buffer_pos_to_display_pos` (see `editor/editing.rs`) |
| Our binding loses to a kit `Input` binding on the same keys | bind with context `"YourContext > Input"` (same depth, registered later, wins; see `editor/keymap.rs`) |
| Click on a child (handle, checkbox, button) also fires the parent row's `on_click` | `cx.stop_propagation()` in the child's handler |
| Selectable text (`TextView`) inside a clickable parent: stopping the click also swallows the mouse-up that ends the text selection (it keeps following the pointer) | don't stop it; claim it and let the parent check (`components::take_claimed_click`, used by block output in the editor) |
| `canvas` used to measure an element can report a shifted origin | prefer the element's own geometry (e.g. `InputState::text_bounds()`); a drag/drop target must contain the pointer (`on_drag_move` bounds, `on_drop` hitbox), so widen the root rather than letting children overflow it |
| Caret at a click on text that becomes an input | after it lays out (`text_bounds()` changed; retry `on_next_frame`), `window.dispatch_event` a MouseDown/Up clamped into it (`editor::replay_click`) |
| Hover-only row actions | parent `.group(name)`, child `.opacity(0.).group_hover(name, \|s\| s.opacity(1.))`; group name unique per row |
| Element IDs | tuple form `("prefix", u64)`; keep prefixes unique per view |
| Mermaid look | `merman` (Mermaid.js port, as in Zed); config from the app theme in `diagram_config`, one brand color per node in `accent_nodes`, root size + rounded boxes in `polish_svg`; labels measured with GPUI's text system in `DIAGRAM_FONT` (usvg can't see the system font). usvg rejects a duplicate attribute and then draws nothing: check a blank diagram with resvg 0.46 |
| Box with `w_full` + `max_w` + `aspect_ratio` is far too tall | Taffy takes height from the unclamped width; also set `max_h` (see Mermaid in `components/markdown.rs`) |
| SVG `img` shown larger than natural size is blurry (rasterized at natural × 2) | re-emit the SVG with scaled `width`/`height` (`scale_svg`) |
| Synthetic Esc (CGEvent) ignored in UI checks | macOS routes Esc through the input context; set the event's unicode string to `\u{1b}` |
| Enter in a single-line `Input` inside a kit dialog closes the dialog (the input propagates `Enter` → `Confirm`) | `.on_action(\|_: &Enter, …\|)` on a wrapper and don't propagate (see `components/file_editor.rs`) |
| Global shortcuts | bind with `secondary-` (cmd on mac, ctrl elsewhere), `None` context; handle on `Workspace` root via `.on_action` |
| `cx.notify()` on a non-view entity (e.g. `RichTextState`) doesn't repaint the view that draws it | the view `cx.observe(&entity, \|_, _, cx\| cx.notify())` |
| Press on rendered Markdown (`markdown_view`) also starts its own selection and takes focus | take the press with `capture_any_mouse_down` + `cx.stop_propagation()` (see `render_body`) |
| Focus after page switch | `Workspace::navigate` must focus something rendered (composer or workspace handle) or actions stop dispatching |

## Verifying UI

Screen recording + Accessibility are granted to the host app, so verify visual changes yourself. Helpers in `scripts/ui/` (compile once: `swiftc -O scripts/ui/X.swift -o <scratch>/X`):

| Tool | Use |
|---|---|
| `list` | on-screen windows front→back: owner, PID, window id, bounds |
| `top X Y` | owner of the topmost window at a point — check before **every** click |
| `move PID X Y` | move that app's window (AX) |
| `mouse move\|click\|rclick X Y`, `mouse drag X Y X2 Y2` | post mouse events (global points) |
| `key text "abc"`, `key code N` | type into the focused app (36 return, 48 tab, 51 delete, 53 esc) |
| `frames IN.mov DIR t…` / `sheet OUT.png COLS imgs…` | pull frames from a recording / contact sheet |

1. Launch: `(BUFFER_DATA_DIR=<scratch>/wd perl -e 'alarm 600; exec @ARGV' ./target/debug/buffer &)` — the alarm guarantees it quits.
2. The user's installed Buffer opens at the **same position**; `move` the test window clear of it (activation alone won't raise an unbundled binary) and confirm with `top` before clicking — never click into the real app. `make dev` (bacon) may also be running a `buffer` that relaunches on every build: pick your window by the PID you launched (`$!`, perl `exec` keeps it), never by owner name, and check `list`'s first row is that PID before each click/key. Restore any window you moved.
3. Click the title bar to activate (hover states need an active window), then drive with `mouse`.
Screenshots are in pixels (2× on Retina): global point = window origin + pixel / 2.
4. Stills: `screencapture -x -o -l<window id> out.png` (works while occluded). Animations: `screencapture -x -v -V 3 -R x,y,w,h rec.mov` in the background, act, then `frames` + `sheet` (stills are too slow to catch <300 ms motion).
5. `osascript` has no assistive access: use it only to read the frontmost process. Kill the test app and clean up afterwards.

## Don't

- Don't add other UI libraries alongside gpui-kit, a direct `gpui` dependency, or git dependencies without asking — they must match the kit's pinned GPUI.
- Don't edit `Cargo.lock` by hand; don't commit `target/`.
- Don't add clutter: the user explicitly wants a minimal UI. Secondary actions go on hover, not always-visible.
- Don't build features for one service or vendor (Jira, GitLab, …) into the app when the user names one as an example: build the general mechanism (e.g. custom blocks) and leave specific integrations to user content.
