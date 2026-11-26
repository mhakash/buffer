//! Block types (`blocks/<name>/block.json` + script), the config a note gives a block (the body
//! of a ```` ```block <name> ```` fence), and a run's cached output. Pure; tested below.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Fence info string prefix: ```` ```block report ````.
pub const FENCE: &str = "block";

/// A programmatic block: a script whose stdout (Markdown) is shown in place of the fence.
/// On-disk shape of `block.json`; new fields need a default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlockType {
    /// Folder name under `blocks/`; also the name used in fences. Not stored in the file.
    #[serde(skip)]
    pub name: String,
    pub label: String,
    /// Lucide icon name (kebab-case); empty = default.
    pub icon: String,
    /// Program that runs the script (`python3`, `node`, `bash`, …); empty = run the script itself.
    pub interpreter: String,
    /// Script file name inside the block's folder.
    pub script: String,
    /// Seconds a result stays fresh; a block opened after that re-runs. 0 = only on refresh.
    pub ttl: u64,
    /// Seconds before a run is killed.
    pub timeout: u64,
    pub params: Vec<Param>,
    /// Extra environment. A value `keychain:<service>` is read from the login keychain at run time.
    pub env: BTreeMap<String, String>,
    /// Bumped on every save so results from before an edit aren't treated as fresh.
    pub revision: u64,
}

impl Default for BlockType {
    fn default() -> Self {
        Self {
            name: String::new(),
            label: String::new(),
            icon: String::new(),
            interpreter: "python3".into(),
            script: "run.py".into(),
            ttl: 300,
            timeout: 30,
            params: Vec::new(),
            env: BTreeMap::new(),
            revision: 0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Param {
    pub name: String,
    pub kind: ParamKind,
    /// As written in a fence; converted by `kind` when passed to the script.
    pub default: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamKind {
    #[default]
    Text,
    Number,
    Bool,
}

impl ParamKind {
    fn value(self, raw: &str) -> Value {
        match self {
            ParamKind::Text => Value::String(raw.into()),
            ParamKind::Number => raw
                .parse::<i64>()
                .map(Value::from)
                .or_else(|_| raw.parse::<f64>().map(Value::from))
                .unwrap_or_else(|_| Value::String(raw.into())),
            ParamKind::Bool => match raw.to_ascii_lowercase().as_str() {
                "true" | "yes" | "on" | "1" => Value::Bool(true),
                "false" | "no" | "off" | "0" | "" => Value::Bool(false),
                _ => Value::String(raw.into()),
            },
        }
    }
}

/// Script a new block type starts with (Python; uses the `who` param of [`BlockType::blank`]).
pub const STARTER_SCRIPT: &str = include_str!("starter.py");

impl BlockType {
    /// A new block type: [`STARTER_SCRIPT`] and the one param it uses.
    pub fn blank() -> Self {
        Self {
            params: vec![Param {
                name: "who".into(),
                kind: ParamKind::Text,
                default: "world".into(),
            }],
            ..Default::default()
        }
    }

    pub fn label(&self) -> &str {
        if self.label.is_empty() {
            &self.name
        } else {
            &self.label
        }
    }

    /// The script's input: every param (fence value, else its default) converted by kind, plus any
    /// other fence keys as text, so scripts can take ad-hoc options.
    pub fn input(&self, config: &Config) -> Map<String, Value> {
        let mut input: Map<String, Value> = config
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        for param in &self.params {
            let raw = config.get(&param.name).unwrap_or(&param.default);
            input.insert(param.name.clone(), param.kind.value(raw));
        }
        input
    }

    /// The fence body a new block of this type starts with: every param at its default.
    pub fn starter(&self) -> String {
        self.params
            .iter()
            .map(|p| format!("{}: {}", p.name, p.default))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Identifies a result: same type, revision and config → same cached output.
    pub fn key(&self, config: &Config) -> String {
        let mut h = Fnv::default();
        h.write(self.name.as_bytes());
        h.write(&[0]);
        h.write(&self.revision.to_le_bytes());
        for (k, v) in config {
            h.write(k.as_bytes());
            h.write(&[0]);
            h.write(v.as_bytes());
            h.write(&[0]);
        }
        format!("{:016x}", h.0)
    }
}

/// A block type name is a folder name and part of a fence info string: `[a-z0-9_-]+`.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// The block type a fence info string names (`block report` → `report`).
pub fn fence_type(info: &str) -> Option<&str> {
    let rest = info.trim().strip_prefix(FENCE)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let name = rest.split_whitespace().next()?;
    valid_name(name).then_some(name)
}

/// A block's config: `key: value` lines from the fence body. Sorted, so it hashes stably.
pub type Config = BTreeMap<String, String>;

/// Blank lines, `#` comments and lines without `:` are skipped; values may be quoted.
pub fn parse_config(body: &str) -> Config {
    body.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once(':')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            Some((key.to_string(), unquote(value.trim()).to_string()))
        })
        .collect()
}

fn unquote(s: &str) -> &str {
    for q in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(q).and_then(|s| s.strip_suffix(q)) {
            return inner;
        }
    }
    s
}

/// `body` with `key: value` set: the first `key:` line is rewritten in place, else the line is
/// appended. Values that `parse_config` would trim or unquote are quoted.
pub fn set_param(body: &str, key: &str, value: &str) -> String {
    let value = value.replace(['\n', '\r'], " ");
    let value = if value != value.trim() || value.starts_with(['"', '\'']) {
        let q = if value.contains('"') { '\'' } else { '"' };
        format!("{q}{value}{q}")
    } else {
        value
    };
    let line = format!("{key}: {value}");
    let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
    let existing = lines.iter().position(|l| {
        let l = l.trim();
        !l.starts_with('#') && l.split_once(':').is_some_and(|(k, _)| k.trim() == key)
    });
    match existing {
        Some(ix) => lines[ix] = line,
        None => {
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
            lines.push(line);
        }
    }
    lines.join("\n")
}

/// Fence info string of controls a script prints in its output: ```` ```input ````.
pub const INPUT_FENCE: &str = "input";

/// Input key a button run gets: the button's name. Never written to the fence.
pub const ACTION: &str = "action";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    Text,
    Number,
    Toggle,
    Select,
    Button,
}

/// One control from an ```` ```input ```` fence in a block's output. Its value is the fence
/// config's `name`, else `default`; changing it writes `name: value` into the note's fence.
#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    pub kind: ControlKind,
    pub name: String,
    pub label: String,
    pub placeholder: String,
    pub default: String,
    pub options: Vec<String>,
}

impl Control {
    pub fn value(&self, config: &Config) -> String {
        config.get(&self.name).unwrap_or(&self.default).clone()
    }

    pub fn checked(&self, config: &Config) -> bool {
        ParamKind::Bool.value(&self.value(config)) == Value::Bool(true)
    }

    /// The label, else the name.
    pub fn title(&self) -> &str {
        if self.label.is_empty() {
            &self.name
        } else {
            &self.label
        }
    }
}

/// One control per line: `<kind> <name> key=value key="quoted value"…`, kinds `text`,
/// `number`, `toggle`, `select`, `button`; keys `label`, `placeholder`, `default`, `options`
/// (`|`-separated). Blank lines, `#` comments and unknown kinds are skipped.
pub fn parse_controls(body: &str) -> Vec<Control> {
    body.lines()
        .filter_map(|line| {
            let mut words = split_words(line.trim()).into_iter();
            let kind = match words.next()?.as_str() {
                "text" => ControlKind::Text,
                "number" => ControlKind::Number,
                "toggle" => ControlKind::Toggle,
                "select" => ControlKind::Select,
                "button" => ControlKind::Button,
                _ => return None,
            };
            let name = words.next().filter(|n| !n.contains('=') && n != ACTION)?;
            let mut control = Control {
                kind,
                name,
                label: String::new(),
                placeholder: String::new(),
                default: String::new(),
                options: Vec::new(),
            };
            for word in words {
                let Some((key, value)) = word.split_once('=') else {
                    continue;
                };
                match key {
                    "label" => control.label = value.into(),
                    "placeholder" => control.placeholder = value.into(),
                    "default" => control.default = value.into(),
                    "options" => {
                        control.options = value
                            .split('|')
                            .map(|o| o.trim().to_string())
                            .filter(|o| !o.is_empty())
                            .collect()
                    }
                    _ => {}
                }
            }
            if kind == ControlKind::Select && control.default.is_empty() {
                control.default = control.options.first().cloned().unwrap_or_default();
            }
            Some(control)
        })
        .collect()
}

/// Splits on whitespace; double quotes group (and are dropped), `\"` is a literal quote.
fn split_words(line: &str) -> Vec<String> {
    let (mut words, mut word, mut quoted, mut started) = (Vec::new(), String::new(), false, false);
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' if quoted => word.extend(chars.next()),
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// A run's result; cached in `.cache/blocks/<key>.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Output {
    /// Stdout: the Markdown to show.
    pub markdown: String,
    /// Stderr, trimmed and capped; shown on failure.
    #[serde(default)]
    pub stderr: String,
    pub ok: bool,
    pub ran_at: DateTime<Utc>,
    #[serde(default)]
    pub duration_ms: u64,
}

/// A failed run is retried after this many seconds (or the type's `ttl`, if shorter), so a fixed
/// secret or a network blip doesn't leave the error up for the whole refresh window.
pub const RETRY_AFTER: u64 = 30;

impl Output {
    /// Fresh = ran less than `ttl` seconds ago (failed runs: [`RETRY_AFTER`]); `ttl` 0 never goes
    /// stale (refresh by hand).
    pub fn is_fresh(&self, ttl: u64, now: DateTime<Utc>) -> bool {
        let window = if self.ok { ttl } else { ttl.min(RETRY_AFTER) };
        ttl == 0 || (now - self.ran_at).num_seconds() < window as i64
    }

    /// A run that couldn't start (e.g. a missing secret).
    pub fn failed(problem: impl Into<String>) -> Self {
        Self {
            markdown: String::new(),
            stderr: problem.into(),
            ok: false,
            ran_at: Utc::now(),
            duration_ms: 0,
        }
    }

    /// A failed run keeps the last good Markdown so the block doesn't go blank on a hiccup.
    pub fn keeping(mut self, previous: Option<&Output>) -> Self {
        if !self.ok
            && self.markdown.trim().is_empty()
            && let Some(prev) = previous
        {
            self.markdown = prev.markdown.clone();
        }
        self
    }
}

/// "just now", "3m ago", "2h ago", "4d ago".
pub fn ago(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let s = (now - then).num_seconds().max(0);
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{}m ago", s / 60),
        3600..86400 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86400),
    }
}

/// FNV-1a 64: stable across builds, unlike `DefaultHasher`, so cache file names survive upgrades.
struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> BlockType {
        BlockType {
            name: "report".into(),
            params: vec![
                Param {
                    name: "jql".into(),
                    kind: ParamKind::Text,
                    default: "assignee = currentUser()".into(),
                },
                Param {
                    name: "limit".into(),
                    kind: ParamKind::Number,
                    default: "10".into(),
                },
                Param {
                    name: "done".into(),
                    kind: ParamKind::Bool,
                    default: "no".into(),
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn fence_types() {
        assert_eq!(fence_type("block report"), Some("report"));
        assert_eq!(fence_type("  block   my-list_2  extra"), Some("my-list_2"));
        assert_eq!(fence_type("block"), None);
        assert_eq!(fence_type("blockreport"), None);
        assert_eq!(fence_type("block Report"), None);
        assert_eq!(fence_type("block ../x"), None);
        assert_eq!(fence_type("mermaid"), None);
    }

    #[test]
    fn parses_config() {
        let config = parse_config(
            "jql: status = \"Open\" AND x: y\n\n# comment\nlimit: 5\nno colon\n: empty\nq: 'hi'\n",
        );
        assert_eq!(
            config,
            Config::from([
                ("jql".into(), "status = \"Open\" AND x: y".into()),
                ("limit".into(), "5".into()),
                ("q".into(), "hi".into()),
            ])
        );
    }

    #[test]
    fn sets_params_in_place_or_appends() {
        let body = "# filters\nlimit: 5\nq: old\n\n";
        assert_eq!(
            set_param(body, "q", "new one"),
            "# filters\nlimit: 5\nq: new one\n"
        );
        assert_eq!(
            set_param(body, "done", "true"),
            "# filters\nlimit: 5\nq: old\ndone: true"
        );
        assert_eq!(set_param("", "q", "x"), "q: x");
        assert_eq!(set_param("#q: no", "q", "x"), "#q: no\nq: x");
        for value in [" padded ", "\"quoted\"", "say \"hi\" ", "a: b", ""] {
            let config = parse_config(&set_param("q: old", "q", value));
            assert_eq!(config["q"], value, "{value:?}");
        }
        assert_eq!(parse_config(&set_param("", "q", "a\nb"))["q"], "a b");
    }

    #[test]
    fn parses_controls() {
        let controls = parse_controls(
            "text q label=Search placeholder=\"e.g. \\\"bug\\\" in title\"\n\
             # comment\n\
             number limit default=10\n\
             toggle mine label=\"Only mine\"\n\
             select status options=\"Open | In progress|Done\"\n\
             button sync label=Sync\n\
             slider x\n\
             text\n\
             button action\n",
        );
        let kinds: Vec<_> = controls.iter().map(|c| (c.kind, c.name.as_str())).collect();
        assert_eq!(
            kinds,
            [
                (ControlKind::Text, "q"),
                (ControlKind::Number, "limit"),
                (ControlKind::Toggle, "mine"),
                (ControlKind::Select, "status"),
                (ControlKind::Button, "sync"),
            ]
        );
        assert_eq!(controls[0].label, "Search");
        assert_eq!(controls[0].placeholder, "e.g. \"bug\" in title");
        assert_eq!(controls[1].title(), "limit");
        assert_eq!(controls[2].title(), "Only mine");
        assert_eq!(controls[3].options, ["Open", "In progress", "Done"]);
        assert_eq!(controls[3].default, "Open");

        let config = parse_config("limit: 3\nmine: yes");
        assert_eq!(controls[1].value(&config), "3");
        assert_eq!(controls[1].value(&Config::new()), "10");
        assert!(controls[2].checked(&config));
        assert!(!controls[2].checked(&Config::new()));
    }

    #[test]
    fn input_converts_params_and_keeps_extra_keys() {
        let config = parse_config("limit: 5\ndone: yes\nextra: z");
        let input = Value::Object(sample().input(&config));
        assert_eq!(
            input,
            json!({"jql": "assignee = currentUser()", "limit": 5, "done": true, "extra": "z"})
        );
        let input = Value::Object(sample().input(&parse_config("limit: lots")));
        assert_eq!(input["limit"], json!("lots"));
        assert_eq!(input["done"], json!(false));
    }

    #[test]
    fn starter_lists_defaults() {
        assert_eq!(
            sample().starter(),
            "jql: assignee = currentUser()\nlimit: 10\ndone: no"
        );
    }

    #[test]
    fn keys_are_stable_and_distinguish() {
        let ty = sample();
        let a = parse_config("limit: 5");
        assert_eq!(ty.key(&a), ty.key(&parse_config("# c\nlimit:5")));
        assert_ne!(ty.key(&a), ty.key(&parse_config("limit: 6")));
        let bumped = BlockType {
            revision: 1,
            ..sample()
        };
        assert_ne!(ty.key(&a), bumped.key(&a));
        // Pinned: cache file names must not change between builds.
        assert_eq!(ty.key(&Config::new()), "2fa63bf2ce805ed1");
    }

    #[test]
    fn freshness() {
        let now = Utc::now();
        let out = Output {
            markdown: String::new(),
            stderr: String::new(),
            ok: true,
            ran_at: now - chrono::Duration::seconds(100),
            duration_ms: 0,
        };
        assert!(out.is_fresh(300, now));
        assert!(!out.is_fresh(60, now));
        assert!(out.is_fresh(0, now));
        let failed = Output { ok: false, ..out };
        assert!(!failed.is_fresh(300, now));
        assert!(failed.is_fresh(0, now));
        let just_failed = Output {
            ran_at: now - chrono::Duration::seconds(5),
            ..failed
        };
        assert!(just_failed.is_fresh(300, now));
    }

    #[test]
    fn failure_keeps_last_markdown() {
        let now = Utc::now();
        let out = |markdown: &str, ok| Output {
            markdown: markdown.into(),
            stderr: String::new(),
            ok,
            ran_at: now,
            duration_ms: 0,
        };
        let good = out("table", true);
        assert_eq!(out("", false).keeping(Some(&good)).markdown, "table");
        assert_eq!(
            out("partial", false).keeping(Some(&good)).markdown,
            "partial"
        );
        assert_eq!(out("", true).keeping(Some(&good)).markdown, "");
    }

    #[test]
    fn relative_times() {
        let now = Utc::now();
        let at = |s| now - chrono::Duration::seconds(s);
        assert_eq!(ago(at(5), now), "just now");
        assert_eq!(ago(at(-5), now), "just now");
        assert_eq!(ago(at(180), now), "3m ago");
        assert_eq!(ago(at(7200), now), "2h ago");
        assert_eq!(ago(at(4 * 86400), now), "4d ago");
    }

    #[test]
    fn block_json_defaults() {
        let ty: BlockType = serde_json::from_str(r#"{"label": "Tickets"}"#).unwrap();
        assert_eq!(ty.label, "Tickets");
        assert_eq!(ty.script, "run.py");
        assert_eq!(ty.ttl, 300);
        let ty: BlockType =
            serde_json::from_str(r#"{"params": [{"name": "n", "kind": "number"}]}"#).unwrap();
        assert_eq!(ty.params[0].kind, ParamKind::Number);
    }
}
