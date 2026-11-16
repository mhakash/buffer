//! Workspaces: each is a root folder holding all content (tasks, notes, focus, images).
//! The registry (`workspaces.json`) is app-level config in [`storage::app_dir`], held as a
//! GPUI global. The default workspace's root is the app dir itself, so data from before
//! workspaces existed loads unchanged. Switching sets [`storage::set_root`] and notifies
//! observers of the global; `Workspace` (the root view) rebuilds its stores and pages.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use gpui_kit::{App, BorrowAppContext, Global, PathPromptOptions};
use serde::{Deserialize, Serialize};

use crate::storage;

const FILE: &str = "workspaces.json";
/// Where "New workspace" puts its folder, inside the app dir.
const NEW_DIR: &str = "Workspaces";
const DEFAULT_NAME: &str = "Personal";

pub type Id = u64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: Id,
    pub name: String,
    /// Absolute root folder; `None` = the app dir (the default workspace).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Custom blocks may run their scripts. Off until the user allows it: a folder opened as a
    /// workspace could come with someone else's `blocks/`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blocks_trusted: bool,
}

impl Entry {
    pub fn root(&self) -> PathBuf {
        self.path.clone().unwrap_or_else(storage::app_dir)
    }

    pub fn is_default(&self) -> bool {
        self.path.is_none()
    }
}

/// On-disk shape of `workspaces.json`. New fields need `#[serde(default)]`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    active: Id,
    #[serde(default)]
    workspaces: Vec<Entry>,
}

impl Global for Registry {}

impl Registry {
    /// Guarantees a default workspace and a valid active one (missing/old file → just the default).
    fn normalized(mut self) -> Self {
        if !self.workspaces.iter().any(Entry::is_default) {
            let id = self.next_id();
            self.workspaces.insert(
                0,
                Entry {
                    id,
                    name: DEFAULT_NAME.into(),
                    path: None,
                    blocks_trusted: false,
                },
            );
        }
        if !self.workspaces.iter().any(|w| w.id == self.active) {
            self.active = self.default_entry().id;
        }
        self
    }

    fn next_id(&self) -> Id {
        self.workspaces.iter().map(|w| w.id + 1).max().unwrap_or(0)
    }

    fn default_entry(&self) -> &Entry {
        self.workspaces
            .iter()
            .find(|w| w.is_default())
            .unwrap_or(&self.workspaces[0])
    }

    pub fn entries(&self) -> &[Entry] {
        &self.workspaces
    }

    pub fn active(&self) -> &Entry {
        self.workspaces
            .iter()
            .find(|w| w.id == self.active)
            .unwrap_or_else(|| self.default_entry())
    }

    fn find_root(&self, root: &Path) -> Option<Id> {
        self.workspaces
            .iter()
            .find(|w| same_dir(&w.root(), root))
            .map(|w| w.id)
    }

    fn add(&mut self, name: String, path: PathBuf) -> Id {
        let id = self.next_id();
        self.workspaces.push(Entry {
            id,
            name,
            path: Some(path),
            blocks_trusted: false,
        });
        id
    }
}

/// Loads the registry and points storage at the active workspace. Call before anything loads.
pub fn init(cx: &mut App) {
    let registry = storage::load_app::<Registry>(FILE).normalized();
    storage::set_root(registry.active().root());
    cx.set_global(registry);
}

pub fn get(cx: &App) -> &Registry {
    cx.global::<Registry>()
}

/// The active workspace lets custom blocks run their scripts.
pub fn blocks_trusted(cx: &App) -> bool {
    cx.try_global::<Registry>()
        .is_some_and(|r| r.active().blocks_trusted)
}

pub fn set_blocks_trusted(trusted: bool, cx: &mut App) {
    update(cx, move |r| {
        let id = r.active().id;
        if let Some(w) = r.workspaces.iter_mut().find(|w| w.id == id) {
            w.blocks_trusted = trusted;
        }
    });
}

/// Makes `id` active; the root view reloads every store and page.
pub fn switch(id: Id, cx: &mut App) {
    if get(cx).active == id || !get(cx).workspaces.iter().any(|w| w.id == id) {
        return;
    }
    update(cx, |r| r.active = id);
}

/// Creates a workspace in a new folder under the app dir and switches to it.
pub fn create(name: &str, cx: &mut App) -> anyhow::Result<()> {
    let name = name.trim();
    let name = if name.is_empty() { "Workspace" } else { name };
    let parent = storage::app_dir().join(NEW_DIR);
    let root = unique_dir(&parent, &slug(name));
    std::fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
    let name = name.to_string();
    update(cx, move |r| r.active = r.add(name, root));
    Ok(())
}

/// Opens an existing folder as a workspace (named after it) and switches to it.
pub fn open_folder(root: PathBuf, cx: &mut App) {
    if let Some(id) = get(cx).find_root(&root) {
        return switch(id, cx);
    }
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string());
    update(cx, move |r| r.active = r.add(name, root));
}

/// Asks for a folder, then [`open_folder`]s it.
pub fn prompt_open_folder(cx: &mut App) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Open as workspace".into()),
    });
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = paths.await
            && let Some(root) = paths.into_iter().next()
        {
            cx.update(|cx| open_folder(root, cx));
        }
    })
    .detach();
}

pub fn rename(id: Id, name: &str, cx: &mut App) {
    let name = name.trim().to_string();
    let unchanged = get(cx)
        .workspaces
        .iter()
        .find(|w| w.id == id)
        .is_none_or(|w| w.name == name);
    if name.is_empty() || unchanged {
        return;
    }
    update(cx, move |r| {
        if let Some(w) = r.workspaces.iter_mut().find(|w| w.id == id) {
            w.name = name;
        }
    });
}

/// Forgets a workspace; its folder and files stay on disk. The default and the active one stay.
pub fn remove(id: Id, cx: &mut App) {
    let registry = get(cx);
    if registry.active == id || registry.default_entry().id == id {
        return;
    }
    update(cx, |r| r.workspaces.retain(|w| w.id != id));
}

/// Mutates, persists, re-points storage and notifies observers (the root view reloads on a
/// root change).
fn update(cx: &mut App, f: impl FnOnce(&mut Registry)) {
    cx.update_global::<Registry, _>(|r, _| {
        f(r);
        storage::set_root(r.active().root());
        if let Err(err) = storage::save_app(FILE, r) {
            eprintln!("failed to save workspaces: {err:#}");
        }
    });
    cx.refresh_windows();
}

fn slug(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "workspace".into()
    } else {
        slug
    }
}

/// `parent/base`, or `parent/base-2`, `-3`, … if taken.
fn unique_dir(parent: &Path, base: &str) -> PathBuf {
    let mut dir = parent.join(base);
    let mut n = 2;
    while dir.exists() {
        dir = parent.join(format!("{base}-{n}"));
        n += 1;
    }
    dir
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    canon(a) == canon(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_are_untrusted_unless_saved_so() {
        let r: Registry =
            serde_json::from_str(r#"{"active": 0, "workspaces": [{"id": 0, "name": "P"}]}"#)
                .unwrap();
        assert!(!r.active().blocks_trusted);
        assert!(
            !serde_json::to_string(&r)
                .unwrap()
                .contains("blocks_trusted")
        );
        let mut r = r;
        r.workspaces[0].blocks_trusted = true;
        let json = serde_json::to_string(&r).unwrap();
        let back: Registry = serde_json::from_str(&json).unwrap();
        assert!(back.active().blocks_trusted);
    }

    #[test]
    fn missing_registry_is_just_the_default_workspace() {
        let r = Registry::default().normalized();
        assert_eq!(r.entries().len(), 1);
        assert!(r.active().is_default());
        assert_eq!(r.active().name, DEFAULT_NAME);
    }

    #[test]
    fn unknown_active_falls_back_to_default() {
        let r: Registry = serde_json::from_str(
            r#"{"active":7,"workspaces":[{"id":3,"name":"Work","path":"/tmp/w"}]}"#,
        )
        .unwrap();
        let r = r.normalized();
        assert_eq!(r.entries().len(), 2);
        assert!(r.active().is_default());
        assert_eq!(r.active().id, 4);
    }

    #[test]
    fn add_gets_fresh_ids() {
        let mut r = Registry::default().normalized();
        let a = r.add("A".into(), "/tmp/a".into());
        let b = r.add("B".into(), "/tmp/b".into());
        assert_ne!(a, b);
        assert_eq!(r.find_root(Path::new("/tmp/b")), Some(b));
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Side Project!"), "side-project");
        assert_eq!(slug("  "), "workspace");
        assert_eq!(slug("Über"), "über");
    }
}
