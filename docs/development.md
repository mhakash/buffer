# Development

## Commands

| Command | Does |
|---|---|
| `make install` | Builds a release, installs `~/Applications/Buffer.app`, relaunches it |
| `make dev` | Rebuilds and relaunches on every save, throwaway data in `/tmp/buffer-dev` (`brew install bacon`; job in `bacon.toml`) |
| `make run` | `cargo run` (debug build, **your real data**) |
| `make test` | Unit tests |
| `make icon` | Regenerates `assets/AppIcon.icns` from `scripts/make_icon.swift` |
| `BUFFER_DATA_DIR=/tmp/wd cargo run` | Debug build against a throwaway data folder |

macOS needs only the Xcode Command Line Tools: gpui-kit enables GPUI's `runtime_shaders`, which
compiles Metal shaders at startup instead of requiring the full Xcode `metal` compiler.

## Installing

`make install` always installs to `~/Applications` (no admin needed) and removes any old
`/Applications/Buffer.app` and pre-rename `Workdesk.app`, so there's only ever one copy (two
copies = two Dock icons). Re-run it after any code change. The Dock pin and your data survive
updates; pin once with right-click on the Dock icon → Options → Keep in Dock. The rename from
Workdesk is the exception: the old app is removed, so pin Buffer once.

Bump `version` in `Cargo.toml` with each feature; it shows in the status bar and the app's
Info.plist.

## Themes

Colors, radius and fonts live in `themes/buffer.json` (gpui-kit theme format); `chart.1`–`chart.5`
drive the brand gradient, glows and the ambient backdrop. In debug builds the running app
reloads the file on save, no rebuild needed; release builds embed it.

## Code

Module map and conventions: [ARCHITECTURE.md](ARCHITECTURE.md). Rules for coding agents (and a
good checklist for humans): [AGENTS.md](../AGENTS.md).

## References

- [GPUI Kit docs](https://gpui-kit.com) · [repo](https://github.com/longbridge/gpui-kit)
- [GPUI README](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [examples](https://github.com/zed-industries/zed/tree/main/crates/gpui/examples)
- [Lucide icons](https://lucide.dev/icons) (names map to `gpui_kit::assets::IconName`)
