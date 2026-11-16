//! Secrets and config for block scripts, managed in Settings → Secrets & config.
//!
//! One file, `vault.json` in the app dir (never in a workspace folder, which may be synced),
//! written 0600: values for all workspaces plus per-workspace overrides. A secret is a value the
//! UI never shows. Scripts get them two ways:
//! - a block type's Environment value `secret:NAME` / `config:NAME` (resolved by `blocks`);
//! - `buffer-get NAME` from any block script: a helper on the script's PATH that runs this
//!   binary headless (`buffer get NAME`, see [`cli_get`]) for the workspace in
//!   `BUFFER_WORKSPACE_ID`.

use std::{collections::BTreeMap, path::PathBuf};

use gpui_kit::{App, BorrowAppContext, Global};
use serde::{Deserialize, Serialize};

use crate::{storage, workspaces};

pub const FILE: &str = "vault.json";
/// Env var naming the workspace a block script runs for (read by [`cli_get`]).
pub const WORKSPACE_ENV: &str = "BUFFER_WORKSPACE_ID";
/// Env var with this binary's path (the helper execs it).
pub const BIN_ENV: &str = "BUFFER_BIN";
/// Env var with the folder holding `buffer-get`; the runner puts it first on PATH.
pub const HELPERS_ENV: &str = "BUFFER_HELPERS";
const HELPER: &str = "buffer-get";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub name: String,
    #[serde(default)]
    pub value: String,
    /// Hidden in the UI (shown as dots, only replaceable).
    #[serde(default)]
    pub secret: bool,
}

/// On-disk shape of `vault.json`. New fields need `#[serde(default)]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vault {
    /// Values for every workspace.
    #[serde(default)]
    pub global: Vec<Item>,
    /// Overrides by workspace id.
    #[serde(default)]
    pub workspaces: BTreeMap<String, Vec<Item>>,
}

impl Global for Vault {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    Workspace(workspaces::Id),
}

/// Names work as env-var-like identifiers: letters, digits, `_`, `-`, `.`; not starting with a digit.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// The vault name an Environment value refers to: `secret:NAME` or `config:NAME`.
pub fn reference(value: &str) -> Option<&str> {
    let value = value.trim();
    value
        .strip_prefix("secret:")
        .or_else(|| value.strip_prefix("config:"))
        .map(str::trim)
}

impl Vault {
    pub fn items(&self, scope: Scope) -> &[Item] {
        match scope {
            Scope::All => &self.global,
            Scope::Workspace(id) => self
                .workspaces
                .get(&id.to_string())
                .map(Vec::as_slice)
                .unwrap_or_default(),
        }
    }

    fn items_mut(&mut self, scope: Scope) -> &mut Vec<Item> {
        match scope {
            Scope::All => &mut self.global,
            Scope::Workspace(id) => self.workspaces.entry(id.to_string()).or_default(),
        }
    }

    /// The value scripts in `workspace` see: its override, else the all-workspaces value.
    pub fn get(&self, workspace: workspaces::Id, name: &str) -> Option<&Item> {
        self.items(Scope::Workspace(workspace))
            .iter()
            .chain(&self.global)
            .find(|i| i.name == name)
    }

    /// Adds `item`, or replaces the one with its name (keeping its place).
    pub fn set(&mut self, scope: Scope, item: Item) {
        let items = self.items_mut(scope);
        match items.iter_mut().find(|i| i.name == item.name) {
            Some(existing) => *existing = item,
            None => items.push(item),
        }
    }

    pub fn remove(&mut self, scope: Scope, name: &str) {
        self.items_mut(scope).retain(|i| i.name != name);
        self.workspaces.retain(|_, items| !items.is_empty());
    }

    pub fn forget_workspace(&mut self, id: workspaces::Id) {
        self.workspaces.remove(&id.to_string());
    }
}

pub fn init(cx: &mut App) {
    cx.set_global(storage::load_app::<Vault>(FILE));
}

pub fn get(cx: &App) -> &Vault {
    cx.global::<Vault>()
}

/// Mutates, saves (0600) and repaints.
pub fn update(cx: &mut App, f: impl FnOnce(&mut Vault)) {
    cx.update_global::<Vault, _>(|v, _| {
        f(v);
        if let Err(err) = storage::save_app_private(FILE, v) {
            eprintln!("failed to save {FILE}: {err:#}");
        }
    });
    cx.refresh_windows();
}

/// Environment for a block script: `BUFFER_*` vars so `buffer-get` works. Writes the helper on
/// first use; without it (unwritable app dir) scripts still get declared values.
pub fn script_env(workspace: workspaces::Id) -> Vec<(String, String)> {
    let mut env = vec![(WORKSPACE_ENV.to_string(), workspace.to_string())];
    if let Ok(bin) = std::env::current_exe() {
        env.push((BIN_ENV.into(), bin.display().to_string()));
        if let Some(dir) = helper_dir() {
            env.push((HELPERS_ENV.into(), dir.display().to_string()));
        }
    }
    env
}

/// `<app dir>/bin` holding the `buffer-get` shell shim (rewritten if it differs).
fn helper_dir() -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let dir = storage::app_dir().join("bin");
    let path = dir.join(HELPER);
    let script = format!(
        "#!/bin/sh\n# Prints a value from Buffer's Secrets & config.\nexec \"${BIN_ENV}\" get \"$@\"\n"
    );
    if std::fs::read_to_string(&path).ok().as_deref() != Some(script.as_str()) {
        let written = std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(&path, &script))
            .and_then(|_| std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)));
        if let Err(err) = written {
            eprintln!("couldn't write {}: {err}", path.display());
            return None;
        }
    }
    Some(dir)
}

/// `buffer get NAME`: prints the value (exit 0), or explains on stderr (exit 1). Runs headless,
/// reading `vault.json` directly, for the workspace in `BUFFER_WORKSPACE_ID` (else all-workspace
/// values only).
pub fn cli_get(args: &[String]) -> i32 {
    let [name] = args else {
        eprintln!("usage: buffer-get NAME");
        return 2;
    };
    let vault: Vault = storage::load_app(FILE);
    let workspace = std::env::var(WORKSPACE_ENV)
        .ok()
        .and_then(|id| id.parse().ok());
    let item = match workspace {
        Some(id) => vault.get(id, name),
        None => vault.global.iter().find(|i| &i.name == name),
    };
    match item {
        Some(item) => {
            println!("{}", item.value);
            0
        }
        None => {
            eprintln!("“{name}” isn't set in Buffer → Settings → Secrets & config");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, value: &str) -> Item {
        Item {
            name: name.into(),
            value: value.into(),
            secret: false,
        }
    }

    #[test]
    fn workspace_overrides_win() {
        let mut v = Vault::default();
        v.set(Scope::All, item("URL", "global"));
        v.set(Scope::All, item("TOKEN", "t"));
        v.set(Scope::Workspace(3), item("URL", "ws"));
        assert_eq!(v.get(3, "URL").unwrap().value, "ws");
        assert_eq!(v.get(3, "TOKEN").unwrap().value, "t");
        assert_eq!(v.get(1, "URL").unwrap().value, "global");
        assert!(v.get(1, "NOPE").is_none());
    }

    #[test]
    fn set_replaces_in_place_and_remove_prunes() {
        let mut v = Vault::default();
        v.set(Scope::All, item("A", "1"));
        v.set(Scope::All, item("B", "2"));
        v.set(Scope::All, item("A", "3"));
        assert_eq!(v.global, vec![item("A", "3"), item("B", "2")]);
        v.set(Scope::Workspace(2), item("A", "x"));
        v.remove(Scope::Workspace(2), "A");
        assert!(v.workspaces.is_empty());
        v.set(Scope::Workspace(2), item("A", "x"));
        v.forget_workspace(2);
        assert!(v.workspaces.is_empty());
    }

    #[test]
    fn references_and_names() {
        assert_eq!(reference("secret:TOKEN"), Some("TOKEN"));
        assert_eq!(reference(" config: URL "), Some("URL"));
        assert_eq!(reference("keychain:x"), None);
        assert_eq!(reference("plain"), None);
        assert!(valid_name("JIRA_TOKEN") && valid_name("api.url") && valid_name("_x"));
        assert!(!valid_name("") && !valid_name("1A") && !valid_name("A B") && !valid_name("A=B"));
    }

    #[test]
    fn old_or_missing_file_loads() {
        let v: Vault = serde_json::from_str("{}").unwrap();
        assert_eq!(v, Vault::default());
        let v: Vault = serde_json::from_str(r#"{"global": [{"name": "A"}]}"#).unwrap();
        assert_eq!(v.global, vec![item("A", "")]);
    }
}
