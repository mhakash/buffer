# Custom blocks

A custom block runs a script and shows the Markdown it prints. Make one on the **Blocks** page
(⌘4): + starts a block with a Python starter script; fill in the form, Save. Use it in a note by
typing `/` and picking it under Custom, or by writing a fence:

````markdown
```block report
since: 7d
limit: 5
```
````

Block types live in the workspace as `blocks/<name>/block.json` plus the script
(see [data](data.md)).

| Part | Behaviour |
|---|---|
| Config | `key: value` lines in the fence; params not given use their defaults. Editing the block shows the params it takes |
| Script | Runs in `blocks/<name>/` through your login shell (Homebrew tools work). Gets params as JSON on stdin, in `BUFFER_BLOCK_INPUT`, and as `BLOCK_<NAME>` env vars; stdout = the Markdown; non-zero exit = failed (stderr shown, last output kept) |
| Return | Instead of printing (or as well), a Python script can define `run(params)` and a Node script can export `run` or a default function (sync or async): what it returns is appended to stdout. Picked by the Interpreter's name (`python…`, `node`); other interpreters only print |
| Environment | Per block type. Values can be literal, `secret:NAME` / `config:NAME` (from [Secrets & config](#secrets--config)), or `keychain:<service>` (login keychain, `security add-generic-password -s <service> -a $USER -w`). A reference that isn't set fails the run with a message |
| `buffer-get NAME` | On every block script's PATH: prints NAME from Secrets & config for the block's workspace (exit 1 if not set), so scripts can read values they don't declare |
| Refresh | Output is cached; a block older than its **Fresh for** seconds re-runs when shown (0 = never); a failed run retries after 30 s. **Timeout** kills the script and anything it started |
| Look | Output renders flat, in the note's own text style; a muted line under it (block name, last update, ↻, ✎ Edit) shows on hover, or always while running, failed or off |
| Clicks | In the output, clicks select text, copy and follow links as usual; **Edit** on that line, or **Edit config** in the block's ⋮⋮ menu, opens the block's config; that menu also has **Refresh** |
| Inputs | An ```` ```input ```` fence in the output becomes a row of controls ([below](#inputs-in-block-output)). Changing one writes `name: value` into the note's fence (⌘Z undoes it) and re-runs the block; the old output stays up while it runs |
| Trust | Off per workspace until you click **Allow…** on a block (or Settings → Workspaces → Run block scripts). Test runs on the Blocks page always run |
| Saving | Explicit (Save). Every save re-runs that type's blocks; editing the script outside the app doesn't, so use ↻ |
| On disk | Block folders added, removed or whose `block.json` changed outside the app are picked up when you switch back to Buffer, or within 15 s — no restart |

## Print or return

```python
# print style
import json, sys
p = json.load(sys.stdin)
print(f"Hello **{p['who']}**")

# return style
def run(params):
    return f"Hello **{params['who']}**"
```

```js
// Node: a named export or a default function
export async function run(params) { return `Hello **${params.who}**`; }
```

## Inputs in block output

A script makes its output interactive by printing controls, one per line:

````markdown
```input
text q placeholder="Filter title"
select status label=Status options="Any|Open|Done" default=Any
number limit label=Limit default=5
toggle mine label="Only mine"
button sync label=Sync
```
````

| Kind | Shows | Sends |
|---|---|---|
| `text`, `number` | Field; commits on Enter or when it loses focus | `name: <text>` |
| `toggle` | Switch | `name: true` / `false` |
| `select` | Dropdown of `options` (`\|`-separated) | `name: <option>` |
| `button` | Button | One run with `action: <name>` in the input (not saved; the result replaces the output) |

Line format: `<kind> <name> key=value…`, keys `label`, `placeholder`, `default`, `options`; quote
values with spaces (`\"` inside quotes). A control's value is the fence's `name`, else `default`,
so the next run gets it like any config key (stdin JSON, `BLOCK_<NAME>`). Controls are inert on
the Blocks page's test run and while block scripts are off.

## Secrets & config

Settings → Secrets & config holds named values for block scripts: **All workspaces**, plus
per-workspace overrides (**Only "<workspace>"**; an override wins). Mark a value **Secret** to
keep it out of the UI (shown as dots; edit = replace). Everything lives in `vault.json` in the
app dir, readable only by you; it's never written into a workspace folder. Shape:
`{"global": [{"name", "value", "secret"}], "workspaces": {"<id>": [ … ]}}`. Outside a block,
`BUFFER_WORKSPACE_ID=<id> Buffer.app/Contents/MacOS/buffer get NAME` does the same lookup.
