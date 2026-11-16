//! Programmatic blocks: a ```` ```block <name> ```` fence in a note runs that block type's script
//! and shows its Markdown output. Block types live in the workspace (`blocks/<name>/`); results
//! are cached (`.cache/blocks/`) and re-run when older than the type's `ttl`, or on refresh.
//!
//! | File | Concern |
//! |---|---|
//! | `model` | block type, fence config, output (pure, tested) |
//! | `store` | files on disk |
//! | `runner` | running a script (blocking, tested) |
//!
//! Scripts run only in workspaces that allowed them (`workspaces::blocks_trusted`); until then
//! cards show cached output and an Allow button.
//!
//! State is the [`Blocks`] global, reloaded per workspace by [`load`]. Runs happen on a thread;
//! a finished run updates the global and repaints every window, so renderers just read [`status`].

pub mod model;
pub mod runner;
pub mod store;

use std::{
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

use chrono::Utc;
use futures::channel::oneshot;
use gpui_kit::{App, Global, SharedString, assets::IconName};

use crate::{storage, vault, workspaces};
use model::{BlockType, Config, Output};
use runner::Job;

/// Bumped by [`load`]: runs started in an earlier workspace don't land in this one.
static GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
pub struct Blocks {
    types: Vec<BlockType>,
    /// What `types` was read from, to notice folders added, removed or edited on disk.
    signature: store::Signature,
    /// Results by key; `None` = looked on disk, nothing cached.
    results: HashMap<String, Option<Output>>,
    running: HashSet<String>,
    /// Failed blocks whose stderr is expanded.
    details: HashSet<String>,
}

impl Global for Blocks {}

/// What a renderer shows for one block.
pub struct Status {
    pub output: Option<Output>,
    pub running: bool,
}

/// (Re)loads block types from the active workspace; drops results of the previous one.
pub fn load(cx: &mut App) {
    GENERATION.fetch_add(1, Ordering::Relaxed);
    cx.set_global(Blocks {
        signature: store::signature(),
        types: store::load_all(),
        ..Default::default()
    });
}

/// Re-reads block types after one was saved or removed; results stay (keys carry the revision).
pub fn reload_types(cx: &mut App) {
    let blocks = cx.default_global::<Blocks>();
    blocks.signature = store::signature();
    blocks.types = store::load_all();
    cx.refresh_windows();
}

/// Picks up block folders added, removed or edited outside the app (cheap: a directory listing
/// and one stat per block). Called on window activation and the workspace's periodic tick.
pub fn reload_if_changed(cx: &mut App) {
    let current = cx.try_global::<Blocks>().map(|b| &b.signature);
    if current.is_some_and(|s| *s != store::signature()) {
        reload_types(cx);
    }
}

/// Runs a block once for the Blocks page's test run: no cache, no dedup, and no trust check
/// (the user is running the script they are editing).
pub fn run_once(ty: &BlockType, config: &Config, cx: &App) -> oneshot::Receiver<Output> {
    let job = job(ty, config, cx);
    let (tx, rx) = oneshot::channel();
    thread::spawn(move || {
        let _ = tx.send(match job {
            Ok(job) => runner::run(&job),
            Err(problem) => Output::failed(problem),
        });
    });
    rx
}

/// What to run. `secret:NAME` / `config:NAME` Environment values are resolved from the vault
/// here; one that isn't set fails the run with that message.
fn job(ty: &BlockType, config: &Config, cx: &App) -> Result<Job, String> {
    let workspace = workspaces::get(cx).active().id;
    let vault = vault::get(cx);
    let mut env = vault::script_env(workspace);
    for (key, value) in &ty.env {
        let value = match vault::reference(value) {
            Some(name) => vault
                .get(workspace, name)
                .map(|item| item.value.clone())
                .ok_or_else(|| {
                    format!("{key}: “{name}” isn't set in Settings → Secrets & config")
                })?,
            None => value.clone(),
        };
        env.push((key.clone(), value));
    }
    Ok(Job {
        shell: runner::login_shell(),
        dir: storage::path(&store::folder(&ty.name)),
        command: Job::command(&ty.interpreter, &ty.script),
        input: ty.input(config),
        env,
        timeout: Duration::from_secs(ty.timeout),
    })
}

pub fn types(cx: &App) -> &[BlockType] {
    cx.try_global::<Blocks>()
        .map(|b| b.types.as_slice())
        .unwrap_or_default()
}

/// Asset path of a block type's icon: any Lucide name (`list-checks`), else a default.
pub fn icon_path(icon: &str, cx: &App) -> SharedString {
    let name = icon.trim();
    let path = format!("icons/{name}.svg");
    if !name.is_empty() && matches!(cx.asset_source().load(&path), Ok(Some(_))) {
        path.into()
    } else {
        IconName::Zap.path()
    }
}

pub fn find<'a>(name: &str, cx: &'a App) -> Option<&'a BlockType> {
    types(cx).iter().find(|t| t.name == name)
}

/// The cached result (read from disk the first time) and whether a run is in flight.
pub fn status(ty: &BlockType, config: &Config, cx: &mut App) -> Status {
    let key = ty.key(config);
    let blocks = cx.default_global::<Blocks>();
    let output = blocks
        .results
        .entry(key.clone())
        .or_insert_with(|| store::cached(&key))
        .clone();
    Status {
        output,
        running: blocks.running.contains(&key),
    }
}

/// Runs the block unless a fresh result exists, it is already running, or the workspace hasn't
/// allowed block scripts.
pub fn ensure(ty: &BlockType, config: &Config, cx: &mut App) {
    if !workspaces::blocks_trusted(cx) {
        return;
    }
    let fresh = status(ty, config, cx)
        .output
        .is_some_and(|o| o.is_fresh(ty.ttl, Utc::now()));
    if !fresh {
        start(ty, config, cx);
    }
}

/// Runs the block now (unless it is already running or block scripts aren't allowed).
pub fn refresh(ty: &BlockType, config: &Config, cx: &mut App) {
    if workspaces::blocks_trusted(cx) {
        start(ty, config, cx);
    }
}

fn start(ty: &BlockType, config: &Config, cx: &mut App) {
    let key = ty.key(config);
    if !cx.default_global::<Blocks>().running.insert(key.clone()) {
        return;
    }
    let generation = GENERATION.load(Ordering::Relaxed);
    // Paths resolved now: the workspace may switch before the run ends.
    let cache = storage::path(&store::cache_file(&key));
    let job = job(ty, config, cx);
    let previous = cx
        .default_global::<Blocks>()
        .results
        .get(&key)
        .cloned()
        .flatten();
    let (tx, rx) = oneshot::channel();
    thread::spawn(move || {
        let output = match job {
            Ok(job) => runner::run(&job),
            Err(problem) => Output::failed(problem),
        }
        .keeping(previous.as_ref());
        if let Err(err) = storage::save_at(&cache, &output) {
            eprintln!("couldn't cache block output: {err:#}");
        }
        let _ = tx.send(output);
    });
    cx.spawn(async move |cx| {
        if let Ok(output) = rx.await {
            cx.update(|cx| finish(generation, key, output, cx));
        }
    })
    .detach();
}

pub fn details_open(key: &str, cx: &App) -> bool {
    cx.try_global::<Blocks>()
        .is_some_and(|b| b.details.contains(key))
}

pub fn toggle_details(key: &str, cx: &mut App) {
    let details = &mut cx.default_global::<Blocks>().details;
    if !details.remove(key) {
        details.insert(key.to_string());
    }
    cx.refresh_windows();
}

fn finish(generation: u64, key: String, output: Output, cx: &mut App) {
    if generation != GENERATION.load(Ordering::Relaxed) {
        return;
    }
    let blocks = cx.default_global::<Blocks>();
    blocks.running.remove(&key);
    blocks.results.insert(key, Some(output));
    cx.refresh_windows();
}
