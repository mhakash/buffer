//! Block types on disk, in the active workspace: `blocks/<name>/block.json` + its script.
//! Cached results: `.cache/blocks/<key>.json`.

use super::model::{BlockType, Output, valid_name};
use crate::storage;

const DIR: &str = "blocks";
const FILE: &str = "block.json";
const CACHE: &str = ".cache/blocks";

fn file(name: &str) -> String {
    format!("{DIR}/{name}/{FILE}")
}

/// The block's folder, relative to the workspace root.
pub fn folder(name: &str) -> String {
    format!("{DIR}/{name}")
}

pub fn cache_file(key: &str) -> String {
    format!("{CACHE}/{key}.json")
}

/// Every folder under `blocks/` with a valid name and a `block.json`.
pub fn load_all() -> Vec<BlockType> {
    storage::list_dirs(DIR)
        .into_iter()
        .filter(|name| valid_name(name) && storage::path(&file(name)).is_file())
        .map(|name| {
            let mut ty: BlockType = storage::load(&file(&name));
            ty.name = name;
            ty
        })
        .collect()
}

pub fn script(ty: &BlockType) -> String {
    storage::load_text(&format!("{}/{}", folder(&ty.name), ty.script))
}

/// Writes `block.json` and the script, bumping the revision so earlier results go stale.
pub fn save(ty: &mut BlockType, script: &str) -> anyhow::Result<()> {
    anyhow::ensure!(valid_name(&ty.name), "invalid block name {:?}", ty.name);
    anyhow::ensure!(
        !ty.script.is_empty() && !ty.script.contains(['/', '\\']) && ty.script != "..",
        "invalid script file name {:?}",
        ty.script
    );
    ty.revision += 1;
    storage::save(&file(&ty.name), ty)?;
    storage::save_text(&format!("{}/{}", folder(&ty.name), ty.script), script)
}

pub fn remove(name: &str) -> anyhow::Result<()> {
    anyhow::ensure!(valid_name(name), "invalid block name {name:?}");
    storage::remove_dir(&folder(name))
}

/// The cached result, if any. Unparseable caches are quarantined by `storage::load`.
pub fn cached(key: &str) -> Option<Output> {
    storage::path(&cache_file(key))
        .is_file()
        .then(|| storage::load::<Option<Output>>(&cache_file(key)))
        .flatten()
}
