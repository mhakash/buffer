//! Block types (`blocks/<name>/block.json` + script), the config a note gives a block (the body
//! of a ```` ```block <name> ```` fence), and a run's cached output. Pure; tested below.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Fence info string prefix: ```` ```block jira ````.
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

impl BlockType {
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

/// The block type a fence info string names (`block jira` → `jira`).
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

impl Output {
    /// Fresh = ran less than `ttl` seconds ago; `ttl` 0 never goes stale (refresh by hand).
    pub fn is_fresh(&self, ttl: u64, now: DateTime<Utc>) -> bool {
        ttl == 0 || (now - self.ran_at).num_seconds() < ttl as i64
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

    fn jira() -> BlockType {
        BlockType {
            name: "jira".into(),
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
        assert_eq!(fence_type("block jira"), Some("jira"));
        assert_eq!(fence_type("  block   my-list_2  extra"), Some("my-list_2"));
        assert_eq!(fence_type("block"), None);
        assert_eq!(fence_type("blockjira"), None);
        assert_eq!(fence_type("block Jira"), None);
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
    fn input_converts_params_and_keeps_extra_keys() {
        let config = parse_config("limit: 5\ndone: yes\nextra: z");
        let input = Value::Object(jira().input(&config));
        assert_eq!(
            input,
            json!({"jql": "assignee = currentUser()", "limit": 5, "done": true, "extra": "z"})
        );
        let input = Value::Object(jira().input(&parse_config("limit: lots")));
        assert_eq!(input["limit"], json!("lots"));
        assert_eq!(input["done"], json!(false));
    }

    #[test]
    fn starter_lists_defaults() {
        assert_eq!(
            jira().starter(),
            "jql: assignee = currentUser()\nlimit: 10\ndone: no"
        );
    }

    #[test]
    fn keys_are_stable_and_distinguish() {
        let ty = jira();
        let a = parse_config("limit: 5");
        assert_eq!(ty.key(&a), ty.key(&parse_config("# c\nlimit:5")));
        assert_ne!(ty.key(&a), ty.key(&parse_config("limit: 6")));
        let bumped = BlockType {
            revision: 1,
            ..jira()
        };
        assert_ne!(ty.key(&a), bumped.key(&a));
        // Pinned: cache file names must not change between builds.
        assert_eq!(ty.key(&Config::new()), "e6da28df2b298b83");
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
