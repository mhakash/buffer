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
//! State is the [`Blocks`] global, reloaded per workspace by [`load`]. Runs happen on a thread;
//! a finished run updates the global and repaints every window, so renderers just read [`status`].

// Renderers and the Blocks page (later phases) are the callers.
#![allow(dead_code)]

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
use gpui_kit::{App, Global};

use crate::storage;
use model::{BlockType, Config, Output};
use runner::Job;

/// Bumped by [`load`]: runs started in an earlier workspace don't land in this one.
static GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
pub struct Blocks {
    types: Vec<BlockType>,
    /// Results by key; `None` = looked on disk, nothing cached.
    results: HashMap<String, Option<Output>>,
    running: HashSet<String>,
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
        types: store::load_all(),
        ..Default::default()
    });
}

pub fn types(cx: &App) -> &[BlockType] {
    cx.try_global::<Blocks>()
        .map(|b| b.types.as_slice())
        .unwrap_or_default()
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

/// Runs the block unless a fresh result exists or it is already running.
pub fn ensure(ty: &BlockType, config: &Config, cx: &mut App) {
    let fresh = status(ty, config, cx)
        .output
        .is_some_and(|o| o.is_fresh(ty.ttl, Utc::now()));
    if !fresh {
        start(ty, config, cx);
    }
}

/// Runs the block now (unless it is already running).
pub fn refresh(ty: &BlockType, config: &Config, cx: &mut App) {
    start(ty, config, cx);
}

fn start(ty: &BlockType, config: &Config, cx: &mut App) {
    let key = ty.key(config);
    if !cx.default_global::<Blocks>().running.insert(key.clone()) {
        return;
    }
    let generation = GENERATION.load(Ordering::Relaxed);
    // Paths resolved now: the workspace may switch before the run ends.
    let cache = storage::path(&store::cache_file(&key));
    let job = Job {
        shell: runner::login_shell(),
        dir: storage::path(&store::folder(&ty.name)),
        command: Job::command(&ty.interpreter, &ty.script),
        input: ty.input(config),
        env: ty.env.clone().into_iter().collect(),
        timeout: Duration::from_secs(ty.timeout),
    };
    let (tx, rx) = oneshot::channel();
    thread::spawn(move || {
        let output = runner::run(&job);
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

fn finish(generation: u64, key: String, output: Output, cx: &mut App) {
    if generation != GENERATION.load(Ordering::Relaxed) {
        return;
    }
    let blocks = cx.default_global::<Blocks>();
    blocks.running.remove(&key);
    blocks.results.insert(key, Some(output));
    cx.refresh_windows();
}
